//! `InputLine` — single-line text input widget.
//!
//! A single-line text input field supporting text insertion, deletion,
//! cursor movement, and mouse click-to-place cursor.
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{InputLine, Rect};
//!
//! let input = InputLine::new(Rect::new(10, 5, 30, 1))
//!     .with_max_length(100)
//!     .with_text("Hello");
//! ```

use crate::clip;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE, SF_FOCUSED};
use crossterm::event::{KeyCode, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Single-line text input widget.
///
/// Provides text editing capabilities including:
/// - Text insertion at cursor position (all printable characters)
/// - Cursor movement (Left/Right, Home/End)
/// - Deletion (Backspace / Delete)
/// - Mouse click to place cursor
/// - Length-limited input (configurable max length)
/// - Terminal cursor placement when focused
///
/// # Visual Style
///
/// ```text
/// ┌─ InputLine (focused) ──────────────────────┐
/// │ Hello▎                                      │
/// └─────────────────────────────────────────────┘
/// ```
///
/// The cursor is rendered as a terminal block cursor at the current
/// position when the widget has focus.
pub struct InputLine {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// Current text content.
    text: String,
    /// Cursor position as byte index into `text`.
    cursor: usize,
    /// Maximum length in bytes. Default: 256.
    max_len: usize,
}

impl InputLine {
    /// Create a new input line with the given bounds.
    ///
    /// The input line starts empty with a default max length of 256 bytes
    /// and is selectable (can receive focus).
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the input field.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            text: String::new(),
            cursor: 0,
            max_len: 256,
        }
    }

    /// Set the maximum length (in bytes) for this input line.
    ///
    /// If the current text exceeds the new limit, it is truncated.
    /// The cursor is clamped to the new text length.
    #[must_use]
    pub fn with_max_length(mut self, max_len: usize) -> Self {
        self.max_len = max_len;
        if self.text.len() > max_len {
            self.text.truncate(max_len);
            // Ensure we're at a char boundary after truncation
            while !self.text.is_char_boundary(self.text.len()) {
                self.text.pop();
            }
            self.cursor = self.cursor.min(self.text.len());
        }
        self
    }

    /// Set the initial text content.
    ///
    /// The text is truncated to `max_len` bytes. The cursor is placed
    /// at the end of the text.
    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        let len = text.len().min(self.max_len);
        // Truncate at a char boundary
        let mut truncated = text[..len].to_owned();
        while !truncated.is_char_boundary(truncated.len()) {
            truncated.pop();
        }
        self.text = truncated;
        self.cursor = self.text.len();
        self
    }

    /// Get the current text content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Set the text content (replaces current text).
    ///
    /// The text is truncated to `max_len` bytes. The cursor is placed
    /// at the end of the new text.
    pub fn set_text(&mut self, text: &str) {
        let len = text.len().min(self.max_len);
        let mut truncated = text[..len].to_owned();
        while !truncated.is_char_boundary(truncated.len()) {
            truncated.pop();
        }
        self.text = truncated;
        self.cursor = self.text.len();
    }

    /// Get the current cursor position (byte index).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Set the cursor position (byte index, clamped to text length).
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.text.len());
        // Ensure cursor is at a char boundary
        while self.cursor < self.text.len() && !self.text.is_char_boundary(self.cursor) {
            self.cursor += 1;
        }
    }

    /// Get the maximum length in bytes.
    #[must_use]
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    /// Get the previous character boundary before `pos`.
    fn prev_char_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos.saturating_sub(1);
        while p > 0 && !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// Get the next character boundary after `pos`.
    fn next_char_boundary(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }
        let mut p = pos + 1;
        while p < self.text.len() && !self.text.is_char_boundary(p) {
            p += 1;
        }
        p
    }

    /// Insert a character at the cursor position.
    fn insert_char(&mut self, c: char) {
        if self.text.len() + c.len_utf8() <= self.max_len {
            self.text.insert(self.cursor, c);
            self.cursor += c.len_utf8();
        }
    }

    /// Delete the character before the cursor (Backspace).
    fn delete_before_cursor(&mut self) {
        if self.cursor > 0 {
            let prev = self.prev_char_boundary(self.cursor);
            self.text.remove(prev);
            self.cursor = prev;
        }
    }

    /// Delete the character at the cursor position (Delete).
    fn delete_at_cursor(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Check if a mouse position is inside the input line bounds.
    fn is_inside(&self, col: u16, row: u16) -> bool {
        let b = self.base.bounds();
        col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height
    }

    /// Get the display text (clipped to visible width, with ellipsis if truncated).
    fn display_text(&self, width: usize) -> String {
        if self.text.len() <= width {
            return self.text.clone();
        }
        // Truncate with ellipsis at the beginning or end based on cursor position
        let mut result = self.text.clone();
        result.truncate(width.saturating_sub(1));
        result.push('…');
        result
    }
}

impl View for InputLine {
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
        let b = self.base.bounds();
        let draw_area = b.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let focused = self.state() & SF_FOCUSED != 0;
        let style = theme::with_current(|t| {
            if focused {
                t.input_line_focused
            } else {
                t.input_line_normal
            }
        });

        // Fill background with style
        for row in draw_area.y..draw_area.y + draw_area.height {
            for col in draw_area.x..draw_area.x + draw_area.width {
                if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                    cell.set_char(' ').set_style(style);
                }
            }
        }

        // Draw text clipped to bounds
        #[allow(clippy::cast_possible_truncation)]
        let available = b.width as usize;
        let display = self.display_text(available);
        clip::set_string_clipped(buf, b.x, b.y, &display, style, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Key(key) => match key.code {
                KeyCode::Char(c) => {
                    self.insert_char(c);
                    event.clear();
                }
                KeyCode::Backspace => {
                    self.delete_before_cursor();
                    event.clear();
                }
                KeyCode::Delete => {
                    self.delete_at_cursor();
                    event.clear();
                }
                KeyCode::Left => {
                    self.cursor = self.prev_char_boundary(self.cursor);
                    event.clear();
                }
                KeyCode::Right => {
                    self.cursor = self.next_char_boundary(self.cursor);
                    event.clear();
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    event.clear();
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    event.clear();
                }
                _ => {}
            },
            EventKind::Mouse(mouse) => {
                if let MouseEventKind::Down(_) = mouse.kind {
                    if self.is_inside(mouse.column, mouse.row) {
                        let b = self.base.bounds();
                        let click_pos = (mouse.column - b.x) as usize;
                        self.cursor = click_pos.min(self.text.len());
                        event.clear();
                    }
                }
            }
            _ => {}
        }
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn cursor_position(&self) -> Option<Position> {
        if self.state() & SF_FOCUSED == 0 {
            return None;
        }
        let b = self.base.bounds();
        #[allow(clippy::cast_possible_truncation)]
        let x = b.x.saturating_add(self.cursor.min(self.text.len()) as u16);
        Some(Position::new(x, b.y))
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
    use crate::view::SF_VISIBLE;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};

    #[test]
    fn test_input_line_new() {
        let input = InputLine::new(Rect::new(10, 5, 30, 1));

        assert_eq!(input.bounds(), Rect::new(10, 5, 30, 1));
        assert_eq!(input.text(), "");
        assert_eq!(input.cursor(), 0);
        assert_eq!(input.max_len(), 256);
        assert!(input.can_focus());
        assert_ne!(input.options() & OF_SELECTABLE, 0);
    }

    #[test]
    fn test_input_line_with_text() {
        let input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_with_max_length() {
        let input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_max_length(5)
            .with_text("Hello World");

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_insert_char() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Heo");
        input.cursor = 2; // After "He", before "o"

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(input.text(), "Helo");
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn test_input_line_backspace() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hell");
        input.cursor = 4;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Backspace,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "Hel");
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn test_input_line_backspace_at_start() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Backspace,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_line_delete() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Delete,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "ello");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_line_delete_at_end() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 5;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Delete,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_input_line_cursor_left() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 5;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 4);
    }

    #[test]
    fn test_input_line_cursor_left_at_start() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_input_line_cursor_right() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn test_input_line_cursor_right_at_end() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 5;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_home() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 3;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_input_line_end() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");
        input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::End,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_mouse_click() {
        let mut input = InputLine::new(Rect::new(10, 5, 30, 1))
            .with_text("Hello");

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 13, // 10 + 3
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        input.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn test_input_line_mouse_click_outside() {
        let mut input = InputLine::new(Rect::new(10, 5, 30, 1));

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5, // Outside bounds
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        input.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_input_line_mouse_click_beyond_text() {
        let mut input = InputLine::new(Rect::new(10, 5, 30, 1))
            .with_text("Hi");

        // Click at column 35 — inside bounds (10..40), but beyond text length (2)
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 35,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        input.handle_event(&mut event);

        assert!(event.is_cleared());
        // Should clamp to text length (2 chars)
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn test_input_line_mouse_click_outside_bounds_ignored() {
        let mut input = InputLine::new(Rect::new(10, 5, 30, 1))
            .with_text("Hi");

        // Cursor starts at end of text (2)
        assert_eq!(input.cursor(), 2);

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50, // Outside bounds (10 + 30 = 40)
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        input.handle_event(&mut event);

        assert!(!event.is_cleared());
        // Cursor unchanged since click was outside bounds
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn test_input_line_set_text() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1));
        input.set_text("Hello World");

        assert_eq!(input.text(), "Hello World");
        assert_eq!(input.cursor(), 11);
    }

    #[test]
    fn test_input_line_set_text_truncated() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_max_length(5);
        input.set_text("Hello World");

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_set_cursor() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");

        input.set_cursor(10);
        assert_eq!(input.cursor(), 5); // Clamped to text length

        input.set_cursor(2);
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn test_input_line_max_length_prevents_insertion() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_max_length(5)
            .with_text("Hello");

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Char('X'),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "Hello");
    }

    #[test]
    fn test_input_line_insert_in_middle() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hlo");
        input.cursor = 1;

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert_eq!(input.text(), "Helo");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_line_state() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1));

        // Initial state should have SF_VISIBLE
        assert_ne!(input.state() & SF_VISIBLE, 0);
        assert_eq!(input.state() & SF_FOCUSED, 0);

        // Set focused
        input.set_state(input.state() | SF_FOCUSED);
        assert_ne!(input.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_input_line_cursor_position_none_when_not_focused() {
        let input = InputLine::new(Rect::new(10, 5, 30, 1))
            .with_text("Hello");

        assert_eq!(input.cursor_position(), None);
    }

    #[test]
    fn test_input_line_cursor_position_when_focused() {
        let mut input = InputLine::new(Rect::new(10, 5, 30, 1))
            .with_text("Hello");
        input.set_state(input.state() | SF_FOCUSED);

        let pos = input.cursor_position();
        assert_eq!(pos, Some(Position::new(15, 5))); // 10 + 5
    }

    #[test]
    fn test_input_line_draw() {
        let input = InputLine::new(Rect::new(0, 0, 20, 1))
            .with_text("Test Input");
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 5));
        input.draw(&mut buf, Rect::new(0, 0, 30, 5));

        // Verify text was drawn
        let content = buf.content();
        let has_test = content.iter().any(|cell| {
            cell.symbol().contains('T')
                || cell.symbol().contains('I')
                || cell.symbol().contains('n')
        });
        assert!(has_test, "InputLine should draw its content");
    }

    #[test]
    fn test_input_line_ignores_non_text_input() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1));

        let key = crossterm::event::KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        input.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(input.text(), "");
    }

    #[test]
    fn test_input_line_cleared_event_not_processed() {
        let mut input = InputLine::new(Rect::new(0, 0, 30, 1));

        let mut event = Event::default();
        event.clear();

        input.handle_event(&mut event);
        // Should not panic
    }

    #[test]
    fn test_input_line_empty_draw() {
        let input = InputLine::new(Rect::new(0, 0, 20, 1));
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        input.draw(&mut buf, Rect::new(0, 0, 20, 1));
        // Should not panic
    }

    #[test]
    fn test_input_line_bounds() {
        let mut input = InputLine::new(Rect::new(5, 3, 20, 1));
        assert_eq!(input.bounds(), Rect::new(5, 3, 20, 1));

        input.set_bounds(Rect::new(10, 10, 30, 1));
        assert_eq!(input.bounds(), Rect::new(10, 10, 30, 1));
    }

    #[test]
    fn test_input_line_with_max_length_after_text() {
        let input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello World")
            .with_max_length(5);

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_input_line_text_getter() {
        let input = InputLine::new(Rect::new(0, 0, 30, 1))
            .with_text("Hello");

        assert_eq!(input.text(), "Hello");
        assert_eq!(input.max_len(), 256);
    }
}
