//! Dialog — Modal window with OK/Cancel handling.
//!
//! A `Dialog` is a modal window that:
//! - Uses [`FrameType::Dialog`] for visual distinction
//! - Has an `end_state` that breaks the modal loop
//! - Intercepts Escape (→ [`CM_CANCEL`]) and Enter (→ [`CM_OK`])
//! - Only closes on commands < [`INTERNAL_COMMAND_BASE`]
//!
//! # Example
//!
//! ```ignore
//! let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Confirm");
//! dialog.set_focus_to(0); // Focus first child
//!
//! // In event loop:
//! while dialog.is_running() {
//!     dialog.handle_event(&mut event);
//!     if dialog.result() != 0 {
//!         break; // Dialog closed
//!     }
//! }
//!
//! if dialog.result() == CM_OK {
//!     // User confirmed
//! }
//! ```

use crate::command::{CommandId, CM_CANCEL, CM_OK, INTERNAL_COMMAND_BASE};
use crate::frame::FrameType;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, SF_MODAL, SF_VISIBLE};
use crate::window::Window;
use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

// ============================================================================
// Dialog
// ============================================================================

/// Modal window with OK/Cancel handling.
///
/// Dialog wraps a [`Window`] with [`FrameType::Dialog`] and intercepts
/// keyboard shortcuts (Escape, Enter) and commands to close the modal loop.
///
/// The dialog's `result` field tracks the command that ended the dialog:
/// - `0` → dialog is still running
/// - `CM_OK`, `CM_CANCEL`, `CM_YES`, `CM_NO`, etc. → user's choice
#[allow(clippy::module_name_repetitions)]
pub struct Dialog {
    /// The wrapped window (provides frame, interior, drag/resize).
    window: Window,
    /// Base view state (holds `SF_MODAL` and other flags).
    base: ViewBase,
    /// The command that ended the dialog (0 = still running).
    result: CommandId,
}

impl Dialog {
    /// Create a new dialog with the given bounds and title.
    ///
    /// The dialog:
    /// - Uses [`FrameType::Dialog`] for visual distinction
    /// - Is not resizable by default
    /// - Has [`SF_MODAL`] and [`SF_VISIBLE`] flags set
    #[must_use]
    pub fn new(bounds: Rect, title: &str) -> Self {
        let mut window = Window::with_frame_type(bounds, title, FrameType::Dialog);
        window.set_resizable(false);

        let mut base = ViewBase::new(bounds);
        // Set SF_MODAL (SF_VISIBLE is already set by ViewBase::new)
        base.set_state(base.state() | SF_MODAL);

        Self {
            window,
            base,
            result: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Configuration
    // -----------------------------------------------------------------------

    /// Enable or disable resizing.
    ///
    /// Dialogs are not resizable by default.
    pub fn set_resizable(&mut self, resizable: bool) {
        self.window.set_resizable(resizable);
    }

    /// Get the dialog title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.window.title()
    }

    // -----------------------------------------------------------------------
    // Child management (delegates to window.interior)
    // -----------------------------------------------------------------------

    /// Add a child view to the dialog's interior group.
    pub fn add(&mut self, child: Box<dyn View>) -> ViewId {
        self.window.add(child)
    }

    /// Get the number of child views in the interior.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.window.child_count()
    }

    /// Get a reference to the interior group.
    #[must_use]
    pub fn interior(&self) -> &crate::group::Group {
        self.window.interior()
    }

    /// Get a mutable reference to the interior group.
    pub fn interior_mut(&mut self) -> &mut crate::group::Group {
        self.window.interior_mut()
    }

    // -----------------------------------------------------------------------
    // Modal result
    // -----------------------------------------------------------------------

    /// Get the command that ended the dialog.
    ///
    /// Returns `0` if the dialog is still running.
    #[must_use]
    pub fn result(&self) -> CommandId {
        self.result
    }

    /// Check if the dialog is still running.
    ///
    /// Returns `true` if `result() == 0`.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.result == 0
    }

    /// End the modal loop with the given command.
    ///
    /// Sets the `result` field. Call this in response to button clicks
    /// or keyboard shortcuts.
    pub fn end_modal(&mut self, command: CommandId) {
        self.result = command;
    }

    // -----------------------------------------------------------------------
    // Geometry
    // -----------------------------------------------------------------------

    /// Get the interior area (bounds minus frame border).
    #[must_use]
    pub fn interior_rect(&self) -> Rect {
        self.window.interior_rect()
    }
}

// ============================================================================
// View implementation
// ============================================================================

impl View for Dialog {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.window.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.window.set_bounds(bounds);
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        if self.base.state() & SF_VISIBLE == 0 {
            return;
        }
        self.window.draw(buf, area);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        // Clone the event kind to avoid borrow issues
        let kind = event.kind.clone();

        match &kind {
            EventKind::Key(key) => {
                // Handle Escape and Enter specially
                match key.code {
                    KeyCode::Esc => {
                        self.end_modal(CM_CANCEL);
                        event.clear();
                    }
                    KeyCode::Enter => {
                        // Default to CM_OK
                        // TODO: Scan children for default button
                        self.end_modal(CM_OK);
                        event.clear();
                    }
                    _ => {
                        // Pass other keys to window
                        self.window.handle_event(event);
                    }
                }
            }

            EventKind::Command(cmd) => {
                let cmd = *cmd;
                // Commands < INTERNAL_COMMAND_BASE close the dialog (except 0)
                if cmd < INTERNAL_COMMAND_BASE && cmd != 0 {
                    self.end_modal(cmd);
                    event.clear();
                } else {
                    // Internal commands pass through
                    self.window.handle_event(event);
                }
            }

            _ => {
                // Mouse, broadcast, resize — pass to window
                self.window.handle_event(event);
            }
        }
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
        self.window.set_state(state);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// DialogBuilder
// ============================================================================

/// Builder for creating a [`Dialog`] with custom options.
///
/// # Example
///
/// ```ignore
/// let dialog = DialogBuilder::new()
///     .bounds(Rect::new(10, 5, 40, 15))
///     .title("Confirm Delete")
///     .resizable(true)
///     .build();
/// ```
#[must_use]
pub struct DialogBuilder {
    bounds: Rect,
    title: String,
    resizable: bool,
}

impl DialogBuilder {
    /// Create a new builder with defaults.
    ///
    /// Default bounds: `Rect::new(0, 0, 40, 15)`
    /// Default title: `""` (empty)
    /// Default resizable: `false`
    pub fn new() -> Self {
        Self {
            bounds: Rect::new(0, 0, 40, 15),
            title: String::new(),
            resizable: false,
        }
    }

    /// Set the dialog bounds.
    pub fn bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the dialog title.
    pub fn title(mut self, title: &str) -> Self {
        title.clone_into(&mut self.title);
        self
    }

    /// Set whether the dialog is resizable.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Build the dialog.
    pub fn build(self) -> Dialog {
        let mut dialog = Dialog::new(self.bounds, &self.title);
        if self.resizable {
            dialog.set_resizable(true);
        }
        dialog
    }
}

impl Default for DialogBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_NO, CM_YES};
    use crate::view::{View, SF_FOCUSED};
    use crossterm::event::KeyModifiers;

    #[test]
    fn test_dialog_new() {
        let dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test Dialog");

        // SF_MODAL should be set
        assert_ne!(dialog.state() & SF_MODAL, 0, "SF_MODAL should be set");

        // SF_VISIBLE should be set (from ViewBase::new)
        assert_ne!(dialog.state() & SF_VISIBLE, 0, "SF_VISIBLE should be set");

        // Result should be 0 (running)
        assert_eq!(dialog.result(), 0);
        assert!(dialog.is_running());

        // Title should match
        assert_eq!(dialog.title(), "Test Dialog");
    }

    #[test]
    fn test_dialog_is_running() {
        let dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        assert!(dialog.is_running());
        assert_eq!(dialog.result(), 0);
    }

    #[test]
    fn test_dialog_end_modal() {
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");

        dialog.end_modal(CM_OK);
        assert_eq!(dialog.result(), CM_OK);
        assert!(!dialog.is_running());

        // Reset
        dialog.end_modal(CM_CANCEL);
        assert_eq!(dialog.result(), CM_CANCEL);
        assert!(!dialog.is_running());
    }

    #[test]
    fn test_dialog_escape_closes() {
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "Confirm");

        // Press Escape
        let mut event = Event::key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::empty(),
        ));
        dialog.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_CANCEL);
        assert!(!dialog.is_running());
    }

    #[test]
    fn test_dialog_enter_closes() {
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "Confirm");

        // Press Enter
        let mut event = Event::key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::empty(),
        ));
        dialog.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_OK);
        assert!(!dialog.is_running());
    }

    #[test]
    fn test_dialog_command_closes() {
        // Test CM_YES
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        let mut event = Event::command(CM_YES);
        dialog.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_YES);

        // Test CM_NO
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        let mut event = Event::command(CM_NO);
        dialog.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_NO);

        // Test CM_OK
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        let mut event = Event::command(CM_OK);
        dialog.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_OK);

        // Test CM_CANCEL
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        let mut event = Event::command(CM_CANCEL);
        dialog.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(dialog.result(), CM_CANCEL);
    }

    #[test]
    fn test_dialog_internal_command_passes() {
        // INTERNAL_COMMAND_BASE = 1000
        // Commands >= 1000 should NOT close the dialog
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");

        let mut event = Event::command(INTERNAL_COMMAND_BASE);
        dialog.handle_event(&mut event);

        // Event should NOT be cleared (passes through to window)
        // Result should still be 0
        assert_eq!(dialog.result(), 0);
        assert!(dialog.is_running());

        // Test a larger internal command
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");
        let mut event = Event::command(2000);
        dialog.handle_event(&mut event);
        assert_eq!(dialog.result(), 0);
        assert!(dialog.is_running());
    }

    #[test]
    fn test_dialog_add_child() {
        use crate::group::Group;
        use ratatui::layout::Rect;

        // Create a simple test view
        struct TestChild {
            base: ViewBase,
        }

        impl TestChild {
            fn new() -> Self {
                Self {
                    base: ViewBase::new(Rect::new(0, 0, 10, 5)),
                }
            }
        }

        impl View for TestChild {
            fn id(&self) -> ViewId {
                self.base.id()
            }
            fn bounds(&self) -> Rect {
                self.base.bounds()
            }
            fn set_bounds(&mut self, bounds: Rect) {
                self.base.set_bounds(bounds);
            }
            fn draw(&self, _buf: &mut Buffer, _area: Rect) {}
            fn handle_event(&mut self, _event: &mut Event) {}
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, state: u16) {
                self.base.set_state(state);
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "Test");

        assert_eq!(dialog.child_count(), 0);

        let child1 = Box::new(TestChild::new());
        let _id1 = dialog.add(child1);
        assert_eq!(dialog.child_count(), 1);

        let child2 = Box::new(TestChild::new());
        let _id2 = dialog.add(child2);
        assert_eq!(dialog.child_count(), 2);

        // Verify we can access interior
        let interior: &Group = dialog.interior();
        assert_eq!(interior.child_count(), 2);
    }

    #[test]
    fn test_dialog_builder() {
        let dialog = DialogBuilder::new()
            .bounds(Rect::new(5, 3, 50, 20))
            .title("Builder Test")
            .resizable(true)
            .build();

        assert_eq!(dialog.bounds(), Rect::new(5, 3, 50, 20));
        assert_eq!(dialog.title(), "Builder Test");
    }

    #[test]
    fn test_dialog_builder_defaults() {
        let dialog = DialogBuilder::new().build();

        assert_eq!(dialog.bounds(), Rect::new(0, 0, 40, 15));
        assert_eq!(dialog.title(), "");
    }

    #[test]
    fn test_dialog_bounds() {
        let mut dialog = Dialog::new(Rect::new(10, 5, 30, 10), "Test");

        assert_eq!(dialog.bounds(), Rect::new(10, 5, 30, 10));

        dialog.set_bounds(Rect::new(20, 10, 50, 25));
        assert_eq!(dialog.bounds(), Rect::new(20, 10, 50, 25));
    }

    #[test]
    fn test_dialog_state_flags() {
        let mut dialog = Dialog::new(Rect::new(0, 0, 40, 15), "");

        // Should start with SF_MODAL | SF_VISIBLE
        let initial_state = dialog.state();
        assert_ne!(initial_state & SF_MODAL, 0);
        assert_ne!(initial_state & SF_VISIBLE, 0);

        // Modify state
        let new_state = initial_state | crate::view::SF_FOCUSED;
        dialog.set_state(new_state);
        assert_ne!(dialog.state() & SF_FOCUSED, 0);
        assert_ne!(dialog.state() & SF_MODAL, 0);
    }
}
