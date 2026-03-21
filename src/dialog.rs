//! Dialog — Modal dialog window.
//!
//! A dialog is a [`Window`] with `FrameType::Dialog` that captures all input
//! until closed. It's used for message boxes, input forms, and confirmations.
//!
//! # Key behaviors
//!
//! - **Escape** → posts `CM_CANCEL` command (closes dialog)
//! - **Enter** → posts `CM_OK` command (closes dialog if default button exists)
//! - Any command with ID < `INTERNAL_COMMAND_BASE` (1000) closes the dialog
//! - Dialog has a "result" (the `CommandId` that closed it)
//!
//! # Usage
//!
//! ```ignore
//! use turbo_tui::dialog::Dialog;
//! use ratatui::layout::Rect;
//!
//! let mut dialog = Dialog::new(Rect::new(20, 5, 40, 15), "Confirm");
//! // Add buttons, labels etc. to dialog.interior_mut()
//!
//! // In event loop:
//! if !dialog.is_open() {
//!     let result = dialog.result(); // Some(CM_OK) or Some(CM_CANCEL)
//! }
//! ```

use crate::command::{CommandId, CM_CANCEL, CM_OK, INTERNAL_COMMAND_BASE};
use crate::container::Container;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, SF_MODAL, SF_VISIBLE};
use crate::window::Window;
use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

/// Modal dialog window.
///
/// A dialog is a [`Window`] with `FrameType::Dialog` that handles modal interaction.
/// Pressing Escape posts `CM_CANCEL`, pressing Enter posts `CM_OK`.
/// Any command with ID < `INTERNAL_COMMAND_BASE` (1000) closes the dialog.
///
/// # Example
///
/// ```ignore
/// use turbo_tui::dialog::Dialog;
/// use ratatui::layout::Rect;
///
/// let mut dialog = Dialog::new(Rect::new(20, 5, 40, 15), "Confirm");
/// // Add buttons, labels etc. to dialog.interior_mut()
/// ```
pub struct Dialog {
    base: ViewBase,
    window: Window,
    /// The command that closed this dialog, if closed.
    result: Option<CommandId>,
    /// Whether the dialog is still open/active.
    open: bool,
}

impl Dialog {
    /// Create a new modal dialog with the given bounds and title.
    ///
    /// The dialog starts with `SF_MODAL` set and is visible by default.
    #[must_use]
    pub fn new(bounds: Rect, title: &str) -> Self {
        let mut window = Window::dialog(bounds, title);
        // Mark as modal
        let st = window.state();
        window.set_state(st | SF_MODAL);

        let mut base = ViewBase::new(bounds);
        base.set_state(base.state() | SF_MODAL | SF_VISIBLE);

        Self {
            base,
            window,
            result: None,
            open: true,
        }
    }

    /// Get the dialog result (the command that closed it).
    #[must_use]
    pub fn result(&self) -> Option<CommandId> {
        self.result
    }

    /// Check if the dialog is still open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Close the dialog with the given command result.
    ///
    /// Sets `self.result = Some(cmd)` and `self.open = false`.
    pub fn end_modal(&mut self, cmd: CommandId) {
        self.result = Some(cmd);
        self.open = false;
    }

    /// Get the window's title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.window.title()
    }

    /// Get immutable access to the underlying window.
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Get mutable access to the underlying window.
    pub fn window_mut(&mut self) -> &mut Window {
        &mut self.window
    }

    /// Get immutable access to the interior container.
    #[must_use]
    pub fn interior(&self) -> &Container {
        self.window.interior()
    }

    /// Get mutable access to the interior container.
    pub fn interior_mut(&mut self) -> &mut Container {
        self.window.interior_mut()
    }

    /// Add a child view to the dialog's interior.
    ///
    /// Convenience method that delegates to `self.interior.add()`.
    /// The child's bounds must be **relative** to the interior container's top-left.
    pub fn add(&mut self, child: Box<dyn View>) -> ViewId {
        self.window.add(child)
    }
}

impl View for Dialog {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.window.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        if !self.open {
            return;
        }
        self.window.draw(buf, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() || !self.open {
            return;
        }

        match &event.kind.clone() {
            EventKind::Key(key) => {
                match key.code {
                    // Escape → cancel
                    KeyCode::Esc => {
                        self.end_modal(CM_CANCEL);
                        event.clear();
                    }
                    // Enter → OK
                    KeyCode::Enter => {
                        self.end_modal(CM_OK);
                        event.clear();
                    }
                    _ => {
                        // Forward other keys to the window interior
                        self.window.handle_event(event);
                    }
                }
            }

            EventKind::Command(cmd) => {
                // Commands < INTERNAL_COMMAND_BASE close the dialog
                if *cmd < INTERNAL_COMMAND_BASE {
                    self.end_modal(*cmd);
                    event.clear();
                } else {
                    // Internal commands are forwarded
                    self.window.handle_event(event);
                }
            }

            // Mouse and other events → forward to window
            _ => {
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

    fn options(&self) -> u16 {
        self.base.options()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_CANCEL, CM_OK};
    use crate::theme::Theme;
    use crate::view::SF_MODAL;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn setup_theme() {
        crate::theme::set(Theme::dark());
    }

    fn key_event(code: KeyCode) -> Event {
        Event::key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn test_dialog_new() {
        setup_theme();
        let dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test Dialog");

        assert!(dialog.is_open());
        assert!(dialog.result().is_none());
        assert_ne!(dialog.state() & SF_MODAL, 0, "SF_MODAL should be set");
        assert_ne!(dialog.state() & SF_VISIBLE, 0, "SF_VISIBLE should be set");
    }

    #[test]
    fn test_dialog_escape_cancels() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        let mut ev = key_event(KeyCode::Esc);
        dialog.handle_event(&mut ev);

        assert!(!dialog.is_open(), "dialog should be closed after Escape");
        assert_eq!(dialog.result(), Some(CM_CANCEL));
        assert!(ev.is_cleared());
    }

    #[test]
    fn test_dialog_enter_accepts() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        let mut ev = key_event(KeyCode::Enter);
        dialog.handle_event(&mut ev);

        assert!(!dialog.is_open(), "dialog should be closed after Enter");
        assert_eq!(dialog.result(), Some(CM_OK));
        assert!(ev.is_cleared());
    }

    #[test]
    fn test_dialog_command_closes() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        // CM_OK is 10, which is < INTERNAL_COMMAND_BASE (1000)
        let mut ev = Event::command(CM_OK);
        dialog.handle_event(&mut ev);

        assert!(!dialog.is_open());
        assert_eq!(dialog.result(), Some(CM_OK));
        assert!(ev.is_cleared());
    }

    #[test]
    fn test_dialog_command_close_with_cm_cancel() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        let mut ev = Event::command(CM_CANCEL);
        dialog.handle_event(&mut ev);

        assert!(!dialog.is_open());
        assert_eq!(dialog.result(), Some(CM_CANCEL));
        assert!(ev.is_cleared());
    }

    #[test]
    fn test_dialog_internal_command_forwarded() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        // Command >= INTERNAL_COMMAND_BASE should be forwarded, not close the dialog
        let mut ev = Event::command(INTERNAL_COMMAND_BASE + 100); // 1100
        dialog.handle_event(&mut ev);

        assert!(
            dialog.is_open(),
            "dialog should still be open for internal command"
        );
        assert!(dialog.result().is_none());
        // The event is forwarded to the window, which forwards to interior
        // Interior has no children, so event remains unhandled but not cleared
    }

    #[test]
    fn test_dialog_end_modal() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        dialog.end_modal(CM_OK);

        assert!(!dialog.is_open());
        assert_eq!(dialog.result(), Some(CM_OK));
    }

    #[test]
    fn test_dialog_closed_ignores_events() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        // Close the dialog
        dialog.end_modal(CM_CANCEL);

        // Try to send more key events
        let mut ev = key_event(KeyCode::Enter);
        dialog.handle_event(&mut ev);

        // Result should still be CM_CANCEL, not CM_OK
        assert_eq!(dialog.result(), Some(CM_CANCEL));
        // Event should NOT be cleared (dialog is closed)
        assert!(!ev.is_cleared());
    }

    #[test]
    fn test_dialog_add_child() {
        setup_theme();
        use crate::view::ViewBase;

        struct DummyView {
            base: ViewBase,
        }

        impl View for DummyView {
            fn id(&self) -> ViewId {
                self.base.id()
            }
            fn bounds(&self) -> Rect {
                self.base.bounds()
            }
            fn set_bounds(&mut self, b: Rect) {
                self.base.set_bounds(b);
            }
            fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}
            fn handle_event(&mut self, event: &mut Event) {
                event.handled = true;
            }
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, s: u16) {
                self.base.set_state(s);
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");
        assert_eq!(dialog.interior().child_count(), 0);

        let child = Box::new(DummyView {
            base: ViewBase::new(Rect::new(1, 1, 5, 2)),
        });
        dialog.add(child);
        assert_eq!(dialog.interior().child_count(), 1);
    }

    #[test]
    fn test_dialog_set_bounds_propagates() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        let new_bounds = Rect::new(20, 10, 50, 20);
        dialog.set_bounds(new_bounds);

        assert_eq!(dialog.bounds(), new_bounds);
        assert_eq!(dialog.window().bounds(), new_bounds);
    }

    #[test]
    fn test_dialog_title() {
        setup_theme();
        let dialog = Dialog::new(Rect::new(10, 5, 40, 15), "My Dialog");
        assert_eq!(dialog.title(), "My Dialog");
    }

    #[test]
    fn test_dialog_draw_when_closed_is_noop() {
        setup_theme();
        let mut dialog = Dialog::new(Rect::new(10, 5, 40, 15), "Test");

        // Close the dialog
        dialog.end_modal(CM_CANCEL);

        // Create a buffer
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));

        // Draw should be a no-op - no panic, no changes to buffer
        dialog.draw(&mut buf, Rect::new(0, 0, 80, 24));

        // Buffer should remain empty (all cells default)
        for cell in buf.content() {
            assert_eq!(cell.symbol(), " ");
        }
    }
}
