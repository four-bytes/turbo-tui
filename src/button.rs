//! Button — clickable button with hotkey support.
//!
//! A `Button` is a selectable view that emits a [`CommandId`] when activated
//! by mouse click or keyboard (Space/Enter). It supports hotkey markers
//! in the label string using `~X~` syntax.
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{Button, CommandId, Rect};
//! use four_turbo_tui::command::CM_OK;
//!
//! let button = Button::new(
//!     Rect::new(10, 5, 10, 1),
//!     "O~k~",
//!     CM_OK,
//!     true  // is_default (responds to Enter)
//! );
//!
//! // In event loop:
//! button.handle_event(&mut event);
//! if let EventKind::Command(cmd) = event.kind {
//!     // Button was clicked
//! }
//! ```

use crate::command::CommandId;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE, SF_FOCUSED};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

/// Clickable button that emits a command when activated.
///
/// Buttons can be:
/// - Clicked with the mouse
/// - Activated with Space or Enter when focused
/// - Marked as "default" to respond to Enter in dialogs
/// - Labeled with hotkey markers using `~X~` syntax
///
/// # Visual Style
///
/// ```text
/// [ OK ]     or    [  Cancel  ]
/// ```
///
/// - Default button: bold/bright style
/// - Focused button: inverse colors
/// - Hovered button: highlighted background
/// - Normal button: standard style
pub struct Button {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// Button label (may contain `~X~` hotkey markers).
    label: String,
    /// Command to emit when activated.
    command: CommandId,
    /// Whether this is the default button (responds to Enter in dialogs).
    is_default: bool,
    /// Whether the mouse is currently hovering over this button.
    hovered: bool,
}

impl Button {
    /// Create a new button with the given bounds, label, and command.
    ///
    /// The label may contain `~X~` markers to indicate a hotkey character.
    /// For example, `"O~k~"` will display as "Ok" with "k" underlined.
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the button.
    /// * `label` — Button text (may contain hotkey markers).
    /// * `command` — Command to emit when activated.
    /// * `is_default` — Whether this is the default button (bold style, responds to Enter).
    #[must_use]
    pub fn new(bounds: Rect, label: &str, command: CommandId, is_default: bool) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            label: label.to_owned(),
            command,
            is_default,
            hovered: false,
        }
    }

    /// Get the button label (with hotkey markers).
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Get the command that this button emits.
    #[must_use]
    pub fn command(&self) -> CommandId {
        self.command
    }

    /// Check if this is the default button.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.is_default
    }

    /// Get the display label (hotkey markers stripped).
    #[must_use]
    pub fn display_label(&self) -> String {
        self.label.replace('~', "")
    }

    /// Draw the button content to the buffer.
    fn draw_button(&self, buf: &mut Buffer, clip: Rect) {
        let bounds = self.base.bounds();

        let display = self.display_label();
        let focused = self.base.state() & SF_FOCUSED != 0;

        let button_text = format!("[ {display} ]");
        #[allow(clippy::cast_possible_truncation)]
        let text_len = button_text.len() as u16;

        // Center within own bounds
        let x = bounds.x + bounds.width.saturating_sub(text_len) / 2;
        let y = bounds.y;

        let style = theme::with_current(|t| {
            if focused {
                t.button_focused
            } else if self.hovered {
                t.button_hover
            } else if self.is_default {
                t.button_default
            } else {
                t.button_normal
            }
        });

        crate::clip::set_string_clipped(buf, x, y, &button_text, style, clip);
    }

    /// Check if a mouse event is inside the button bounds.
    fn is_inside(&self, col: u16, row: u16) -> bool {
        let b = self.base.bounds();
        col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height
    }
}

impl View for Button {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        self.draw_button(buf, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.is_inside(mouse.column, mouse.row) {
                        event.kind = EventKind::Command(self.command);
                    }
                }
                MouseEventKind::Moved => {
                    let inside = self.is_inside(mouse.column, mouse.row);
                    if inside != self.hovered {
                        self.hovered = inside;
                        self.base.mark_dirty();
                    }
                }
                _ => {}
            },
            EventKind::Key(key) => {
                // Only respond to Space/Enter when focused
                if self.base.state() & SF_FOCUSED != 0 {
                    match key.code {
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            event.kind = EventKind::Command(self.command);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CM_OK;
    use crate::view::SF_VISIBLE;
    use crossterm::event::{KeyModifiers, MouseEvent};

    #[test]
    fn test_button_new() {
        let button = Button::new(Rect::new(10, 5, 10, 1), "OK", CM_OK, true);

        assert_eq!(button.bounds(), Rect::new(10, 5, 10, 1));
        assert_eq!(button.label(), "OK");
        assert_eq!(button.command(), CM_OK);
        assert!(button.is_default());
        assert!(button.can_focus());
        assert_ne!(button.options() & OF_SELECTABLE, 0);
    }

    #[test]
    fn test_button_display_label() {
        let button = Button::new(Rect::new(0, 0, 10, 1), "O~k~", CM_OK, false);
        assert_eq!(button.display_label(), "Ok");
        assert_eq!(button.label(), "O~k~");
    }

    #[test]
    fn test_button_draw() {
        let button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        button.draw(&mut buf, Rect::new(0, 0, 20, 5));

        // Button text "[ OK ]" should be drawn
        // Verify content exists in buffer
        let content = buf.content();
        let has_ok = content
            .iter()
            .any(|cell| cell.symbol().contains('O') || cell.symbol().contains('K'));
        assert!(has_ok, "Button should draw its label");
    }

    #[test]
    fn test_button_click() {
        let mut button = Button::new(Rect::new(5, 3, 10, 1), "Cancel", CM_OK, false);

        // Click inside button
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7, // Inside bounds (5-14)
            row: 3,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        button.handle_event(&mut event);

        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_OK));
    }

    #[test]
    fn test_button_click_outside() {
        let mut button = Button::new(Rect::new(5, 3, 10, 1), "Cancel", CM_OK, false);

        // Click outside button
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 20, // Outside bounds
            row: 3,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        button.handle_event(&mut event);

        // Event should not be a command
        assert!(!event.is_command());
    }

    #[test]
    fn test_button_key_enter_when_focused() {
        let mut button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);

        // Set focus
        button.set_state(button.state() | SF_FOCUSED);

        // Press Enter
        let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let mut event = Event::key(key);
        button.handle_event(&mut event);

        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_OK));
    }

    #[test]
    fn test_button_key_space_when_focused() {
        let mut button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);

        // Set focus
        button.set_state(button.state() | SF_FOCUSED);

        // Press Space
        let key = crossterm::event::KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
        let mut event = Event::key(key);
        button.handle_event(&mut event);

        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_OK));
    }

    #[test]
    fn test_button_key_not_focused() {
        let mut button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);

        // Not focused - should not respond to Enter
        let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let mut event = Event::key(key);
        button.handle_event(&mut event);

        // Event should not be a command
        assert!(!event.is_command());
    }

    #[test]
    fn test_button_is_default() {
        let default_button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, true);
        let normal_button = Button::new(Rect::new(0, 0, 10, 1), "Cancel", CM_OK, false);

        assert!(default_button.is_default());
        assert!(!normal_button.is_default());
    }

    #[test]
    fn test_button_can_focus() {
        let button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);
        assert!(button.can_focus());
    }

    #[test]
    fn test_button_state() {
        let mut button = Button::new(Rect::new(0, 0, 10, 1), "OK", CM_OK, false);

        // Initial state should have SF_VISIBLE
        assert_ne!(button.state() & SF_VISIBLE, 0);
        assert_eq!(button.state() & SF_FOCUSED, 0);

        // Set focused
        button.set_state(button.state() | SF_FOCUSED);
        assert_ne!(button.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_button_hover_state() {
        let mut button = Button::new(Rect::new(5, 3, 10, 1), "OK", CM_OK, false);

        // Initially not hovered
        assert!(!button.hovered);

        // Mouse moves inside
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 7,
            row: 3,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        button.handle_event(&mut event);
        assert!(
            button.hovered,
            "Button should be hovered when mouse is inside"
        );

        // Mouse moves outside
        let mouse_out = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 20,
            row: 3,
            modifiers: KeyModifiers::empty(),
        };
        let mut event_out = Event::mouse(mouse_out);
        button.handle_event(&mut event_out);
        assert!(
            !button.hovered,
            "Button should not be hovered when mouse is outside"
        );
    }

    #[test]
    fn test_button_hover_ignored_when_focused() {
        let mut button = Button::new(Rect::new(5, 3, 10, 1), "OK", CM_OK, false);

        // Set focused
        button.set_state(button.state() | SF_FOCUSED);

        // Mouse moves inside — should set hovered flag
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 7,
            row: 3,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        button.handle_event(&mut event);

        // When drawing, focused style takes priority over hover style
        // This test just verifies the hovered flag is set
        assert!(button.hovered);
    }
}
