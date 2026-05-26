//! `Memo` — multi-line text editor widget.
//!
//! A multi-line text editing widget supporting text insertion, deletion,
//! cursor movement, scrolling, and terminal cursor placement. Designed
//! to be embedded in a `Window` with scrollbar support via
//! [`content_size_hint()`], [`scroll_to()`], and [`scroll_position()`].
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{Memo, Rect};
//!
//! let mut memo = Memo::new(Rect::new(1, 1, 38, 10))
//!     .with_text("Hello\nWorld");
//! ```

use crate::clip;
use crate::theme;
use crate::view::{
    Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE, SF_FOCUSED,
};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Multi-line text editor widget.
///
/// Provides full text editing capabilities:
/// - Text insertion at cursor (all printable characters + Enter for newlines)
/// - Cursor movement (Up/Down/Left/Right, Home/End, PageUp/PageDown)
/// - Deletion (Backspace, Delete)
/// - Mouse click to place cursor
/// - Auto-scrolling to keep cursor visible
/// - Content size reporting for parent window scrollbars
/// - Scroll position management for external scrollbar sync
/// - Terminal cursor placement when focused
///
/// # Visual Style
///
/// ```text
/// ┌───────────────────────────────────────────┐
/// │ Line 1 text ▎                              │
/// │ Line 2 text                                │
/// │ Line 3 text                                │
/// │ ...                                        │
/// └───────────────────────────────────────────┘
/// ```
pub struct Memo {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// Lines of text.
    text: Vec<String>,
    /// Cursor position as (line, column byte offset).
    cursor: (usize, usize),
    /// Scroll offset as (vertical lines, horizontal columns).
    scroll: (usize, usize),
}

impl Memo {
    /// Create a new empty memo with the given bounds.
    ///
    /// The memo starts with a single empty line, cursor at (0,0),
    /// no scroll offset, and is selectable (can receive focus).
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the edit area.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            text: vec![String::new()],
            cursor: (0, 0),
            scroll: (0, 0),
        }
    }

    /// Set the initial text content, splitting on newlines.
    ///
    /// At least one line is always present (even for empty text).
    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        if text.is_empty() {
            self.text = vec![String::new()];
        } else {
            self.text = text.lines().map(ToString::to_string).collect();
            if self.text.is_empty() {
                self.text = vec![String::new()];
            }
        }
        self.cursor = (0, 0);
        self.scroll = (0, 0);
        self.clamp_cursor();
        self
    }

    /// Get the current text as a single string with newline separators.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.join("\n")
    }

    /// Get a reference to the lines vector.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.text
    }

    /// Get the current cursor position `(line, column)`.
    #[must_use]
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Get the current scroll offset `(vertical, horizontal)`.
    #[must_use]
    pub fn scroll(&self) -> (usize, usize) {
        self.scroll
    }

    /// Get the total number of lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.text.len()
    }

    /// Get the maximum line width (in bytes) across all lines.
    #[must_use]
    pub fn max_line_width(&self) -> usize {
        self.text.iter().map(String::len).max().unwrap_or(0)
    }

    /// Get the previous character boundary before `pos` in a string.
    fn prev_char_boundary(s: &str, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos.saturating_sub(1);
        while p > 0 && !s.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// Get the next character boundary after `pos` in a string.
    fn next_char_boundary(s: &str, pos: usize) -> usize {
        if pos >= s.len() {
            return s.len();
        }
        let mut p = pos + 1;
        while p < s.len() && !s.is_char_boundary(p) {
            p += 1;
        }
        p
    }

    /// Insert a character at the cursor position.
    fn insert_char(&mut self, c: char) {
        let (line, col) = self.cursor;
        if line < self.text.len() {
            if c == '\n' {
                // Split line at cursor
                let rest = self.text[line][col..].to_string();
                self.text[line].truncate(col);
                self.text.insert(line + 1, rest);
                self.cursor = (line + 1, 0);
            } else {
                self.text[line].insert(col, c);
                self.cursor.1 += c.len_utf8();
            }
        }
    }

    /// Delete the character before the cursor (Backspace).
    fn delete_before_cursor(&mut self) {
        let (line, col) = self.cursor;
        if col > 0 && line < self.text.len() {
            // Delete within the same line
            let prev = Self::prev_char_boundary(&self.text[line], col);
            self.text[line].remove(prev);
            self.cursor.1 = prev;
        } else if col == 0 && line > 0 {
            // Join with previous line
            let prev_len = self.text[line - 1].len();
            let rest = self.text.remove(line);
            self.text[line - 1].push_str(&rest);
            self.cursor = (line - 1, prev_len);
        }
    }

    /// Delete the character at the cursor position (Delete).
    fn delete_at_cursor(&mut self) {
        let (line, col) = self.cursor;
        if line >= self.text.len() {
            return;
        }
        if col < self.text[line].len() {
            // Delete within line
            self.text[line].remove(col);
        } else if line + 1 < self.text.len() {
            // Join with next line
            let rest = self.text.remove(line + 1);
            self.text[line].push_str(&rest);
        }
    }

    /// Move cursor up one line.
    fn cursor_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.clamp_cursor_col();
        }
    }

    /// Move cursor down one line.
    fn cursor_down(&mut self) {
        if self.cursor.0 + 1 < self.text.len() {
            self.cursor.0 += 1;
            self.clamp_cursor_col();
        }
    }

    /// Move cursor left one character.
    fn cursor_left(&mut self) {
        if self.cursor.1 > 0 {
            let prev = Self::prev_char_boundary(&self.text[self.cursor.0], self.cursor.1);
            self.cursor.1 = prev;
        } else if self.cursor.0 > 0 {
            // Move to end of previous line
            self.cursor.0 -= 1;
            self.cursor.1 = self.text[self.cursor.0].len();
        }
    }

    /// Move cursor right one character.
    fn cursor_right(&mut self) {
        let (line, col) = self.cursor;
        if line < self.text.len() && col < self.text[line].len() {
            let next = Self::next_char_boundary(&self.text[line], col);
            self.cursor.1 = next;
        } else if line + 1 < self.text.len() {
            // Move to start of next line
            self.cursor.0 += 1;
            self.cursor.1 = 0;
        }
    }

    /// Move cursor to the beginning of the current line.
    fn cursor_home(&mut self) {
        self.cursor.1 = 0;
    }

    /// Move cursor to the end of the current line.
    fn cursor_end(&mut self) {
        if self.cursor.0 < self.text.len() {
            self.cursor.1 = self.text[self.cursor.0].len();
        }
    }

    /// Scroll up by one page (minus one line to preserve context).
    fn page_up(&mut self) {
        let visible = self.visible_lines();
        let rows = visible.saturating_sub(1);
        for _ in 0..rows {
            if self.cursor.0 == 0 {
                break;
            }
            self.cursor.0 = self.cursor.0.saturating_sub(1);
        }
        self.clamp_cursor_col();
        self.scroll_to_cursor();
    }

    /// Scroll down by one page (minus one line to preserve context).
    fn page_down(&mut self) {
        let visible = self.visible_lines();
        let rows = visible.saturating_sub(1);
        let max_line = self.text.len().saturating_sub(1);
        for _ in 0..rows {
            if self.cursor.0 >= max_line {
                break;
            }
            self.cursor.0 += 1;
        }
        self.clamp_cursor_col();
        self.scroll_to_cursor();
    }

    /// Number of visible text lines in the bounds.
    fn visible_lines(&self) -> usize {
        usize::from(self.base.bounds().height).max(1)
    }

    /// Visible columns in the bounds.
    fn visible_cols(&self) -> usize {
        usize::from(self.base.bounds().width).max(1)
    }

    /// Clamp cursor column to the current line length.
    fn clamp_cursor_col(&mut self) {
        if self.cursor.0 < self.text.len() {
            let line_len = self.text[self.cursor.0].len();
            if self.cursor.1 > line_len {
                self.cursor.1 = line_len;
            }
        } else {
            self.cursor.1 = 0;
        }
    }

    /// Clamp cursor line to valid range.
    fn clamp_cursor(&mut self) {
        let max_line = self.text.len().saturating_sub(1);
        if self.cursor.0 > max_line {
            self.cursor.0 = max_line;
        }
        self.clamp_cursor_col();
    }

    /// Scroll to keep cursor visible.
    fn scroll_to_cursor(&mut self) {
        let (line, col) = self.cursor;
        let visible = self.visible_lines();
        let cols = self.visible_cols();

        // Vertical scroll
        if line < self.scroll.0 {
            self.scroll.0 = line;
        } else if line >= self.scroll.0 + visible {
            self.scroll.0 = line.saturating_sub(visible).saturating_add(1);
        }

        // Horizontal scroll
        if col < self.scroll.1 {
            self.scroll.1 = col;
        } else if col >= self.scroll.1 + cols {
            self.scroll.1 = col.saturating_sub(cols).saturating_add(1);
        }
    }

    /// Check if a mouse position is inside the memo bounds.
    fn is_inside(&self, col: u16, row: u16) -> bool {
        let b = self.base.bounds();
        col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height
    }
}

impl View for Memo {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.clamp_cursor();
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        let bounds = self.base.bounds();
        let draw_area = bounds.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let focused = self.state() & SF_FOCUSED != 0;
        let style = theme::with_current(|t| {
            if focused {
                t.memo_focused
            } else {
                t.memo_normal
            }
        });

        let visible_lines = usize::from(bounds.height);
        let visible_cols = usize::from(bounds.width);

        // Fill background for all visible rows
        for row in draw_area.y..draw_area.y + draw_area.height {
            for col in draw_area.x..draw_area.x + draw_area.width {
                if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                    cell.set_char(' ').set_style(style);
                }
            }
        }

        // Draw visible lines
        let max_visible = self.text.len().saturating_sub(self.scroll.0);
        for i in 0..visible_lines.min(max_visible) {
            let line_idx = self.scroll.0 + i;
            let row = bounds.y + u16::try_from(i).unwrap_or(0);

            if row < clip.y || row >= clip.y + clip.height {
                continue;
            }

            if let Some(line) = self.text.get(line_idx) {
                // Get the visible portion of the line
                let visible_text = if self.scroll.1 < line.len() {
                    &line[self.scroll.1..]
                } else {
                    ""
                };
                clip::set_string_clipped(buf, bounds.x, row, visible_text, style, clip);

                // Fill remainder of line width with background
                let text_width = visible_text.len().min(visible_cols);
                if text_width < visible_cols {
                    let fill_start = bounds.x + u16::try_from(text_width).unwrap_or(0);
                    for col in fill_start..bounds.x + u16::try_from(visible_cols).unwrap_or(0) {
                        if col >= clip.x && col < clip.x + clip.width {
                            if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                                cell.set_char(' ').set_style(style);
                            }
                        }
                    }
                }
            }
        }

        // Fill remaining empty rows
        let drawn = visible_lines.min(max_visible);
        for i in drawn..visible_lines {
            let row = bounds.y + u16::try_from(i).unwrap_or(0);
            if row < clip.y || row >= clip.y + clip.height {
                continue;
            }
            for col in bounds.x..bounds.x + u16::try_from(visible_cols).unwrap_or(0) {
                if col >= clip.x && col < clip.x + clip.width {
                    if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                        cell.set_char(' ').set_style(style);
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Key(key) => match key.code {
                KeyCode::Char(c) => {
                    self.insert_char(c);
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Enter => {
                    self.insert_char('\n');
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Backspace => {
                    self.delete_before_cursor();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Delete => {
                    self.delete_at_cursor();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Up => {
                    self.cursor_up();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Down => {
                    self.cursor_down();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Left => {
                    self.cursor_left();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Right => {
                    self.cursor_right();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::Home => {
                    self.cursor_home();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::End => {
                    self.cursor_end();
                    self.scroll_to_cursor();
                    event.clear();
                }
                KeyCode::PageUp => {
                    self.page_up();
                    event.clear();
                }
                KeyCode::PageDown => {
                    self.page_down();
                    event.clear();
                }
                _ => {}
            },
            EventKind::Mouse(mouse) => {
                if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                    if self.is_inside(mouse.column, mouse.row) {
                        let bounds = self.base.bounds();
                        let rel_row = (mouse.row - bounds.y) as usize;
                        let rel_col = (mouse.column - bounds.x) as usize;
                        let line = self.scroll.0 + rel_row;
                        if line < self.text.len() {
                            self.cursor.0 = line;
                            let col = self.scroll.1 + rel_col;
                            self.cursor.1 = col.min(self.text[line].len());
                            self.clamp_cursor_col();
                        }
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
        let bounds = self.base.bounds();
        let rel_line = self.cursor.0.saturating_sub(self.scroll.0);
        let rel_col = self.cursor.1.saturating_sub(self.scroll.1);
        #[allow(clippy::cast_possible_truncation)]
        let x = bounds.x.saturating_add(rel_col as u16);
        #[allow(clippy::cast_possible_truncation)]
        let y = bounds.y.saturating_add(rel_line as u16);
        // Clamp to visible area
        let x = x.min(bounds.x + bounds.width.saturating_sub(1));
        let y = y.min(bounds.y + bounds.height.saturating_sub(1));
        Some(Position::new(x, y))
    }

    fn content_size_hint(&self) -> Option<(u16, u16)> {
        #[allow(clippy::cast_possible_truncation)]
        Some((
            self.max_line_width().max(1) as u16,
            self.text.len().max(1) as u16,
        ))
    }

    #[allow(clippy::cast_sign_loss)]
    fn scroll_to(&mut self, x: i32, y: i32) -> bool {
        // Window-scrollbars call this; we store the offset.
        // x = horizontal scroll, y = vertical scroll.
        self.scroll.1 = x.max(0) as usize;
        self.scroll.0 = y.max(0) as usize;
        self.base.mark_dirty();
        true
    }

    fn scroll_position(&self) -> (i32, i32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        (self.scroll.1 as i32, self.scroll.0 as i32)
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::SF_VISIBLE;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};

    #[test]
    fn test_memo_new() {
        let memo = Memo::new(Rect::new(10, 5, 30, 10));

        assert_eq!(memo.bounds(), Rect::new(10, 5, 30, 10));
        assert_eq!(memo.text(), "");
        assert_eq!(memo.cursor(), (0, 0));
        assert_eq!(memo.scroll(), (0, 0));
        assert!(memo.can_focus());
        assert_ne!(memo.options() & OF_SELECTABLE, 0);
    }

    #[test]
    fn test_memo_with_text() {
        let memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");

        assert_eq!(memo.text(), "Hello\nWorld");
        assert_eq!(memo.line_count(), 2);
        assert_eq!(memo.lines()[0], "Hello");
        assert_eq!(memo.lines()[1], "World");
    }

    #[test]
    fn test_memo_with_text_empty() {
        let memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("");
        assert_eq!(memo.text(), "");
        assert_eq!(memo.line_count(), 1); // Always at least one line
    }

    #[test]
    fn test_memo_insert_char() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Heo");
        memo.cursor = (0, 2);

        memo.insert_char('l');
        assert_eq!(memo.text(), "Helo");
        assert_eq!(memo.cursor, (0, 3));
    }

    #[test]
    fn test_memo_insert_newline() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("HelloWorld");
        memo.cursor = (0, 5);

        memo.insert_char('\n');
        assert_eq!(memo.text(), "Hello\nWorld");
        assert_eq!(memo.cursor, (1, 0));
        assert_eq!(memo.line_count(), 2);
    }

    #[test]
    fn test_memo_backspace() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello");
        memo.cursor = (0, 5);

        memo.delete_before_cursor();
        assert_eq!(memo.text(), "Hell");
        assert_eq!(memo.cursor, (0, 4));
    }

    #[test]
    fn test_memo_backspace_joins_lines() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");
        memo.cursor = (1, 0);

        memo.delete_before_cursor();
        assert_eq!(memo.text(), "HelloWorld");
        assert_eq!(memo.cursor, (0, 5));
        assert_eq!(memo.line_count(), 1);
    }

    #[test]
    fn test_memo_delete() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello");
        memo.cursor = (0, 0);

        memo.delete_at_cursor();
        assert_eq!(memo.text(), "ello");
        assert_eq!(memo.cursor, (0, 0));
    }

    #[test]
    fn test_memo_delete_joins_lines() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");
        memo.cursor = (0, 5);

        memo.delete_at_cursor();
        assert_eq!(memo.text(), "HelloWorld");
        assert_eq!(memo.cursor, (0, 5));
        assert_eq!(memo.line_count(), 1);
    }

    #[test]
    fn test_memo_cursor_up_down() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Line 1\nLine 2\nLine 3");
        memo.cursor = (1, 3);

        memo.cursor_up();
        assert_eq!(memo.cursor, (0, 3));

        memo.cursor_down();
        assert_eq!(memo.cursor, (1, 3));

        memo.cursor_down();
        assert_eq!(memo.cursor, (2, 3));

        memo.cursor_down(); // At end, stays
        assert_eq!(memo.cursor, (2, 3));
    }

    #[test]
    fn test_memo_cursor_left_right() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello");
        memo.cursor = (0, 2);

        memo.cursor_left();
        assert_eq!(memo.cursor, (0, 1));

        memo.cursor_right();
        assert_eq!(memo.cursor, (0, 2));
    }

    #[test]
    fn test_memo_cursor_left_wraps_to_prev_line() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");
        memo.cursor = (1, 0);

        memo.cursor_left();
        assert_eq!(memo.cursor, (0, 5)); // End of "Hello"
    }

    #[test]
    fn test_memo_cursor_right_wraps_to_next_line() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");
        memo.cursor = (0, 5);

        memo.cursor_right();
        assert_eq!(memo.cursor, (1, 0)); // Start of "World"
    }

    #[test]
    fn test_memo_home_end() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello World");
        memo.cursor = (0, 5);

        memo.cursor_home();
        assert_eq!(memo.cursor, (0, 0));

        memo.cursor_end();
        assert_eq!(memo.cursor, (0, 11));
    }

    #[test]
    fn test_memo_page_up_down() {
        // Use a small visible area to test page keys
        let mut memo = Memo::new(Rect::new(0, 0, 30, 3))
            .with_text("Line1\nLine2\nLine3\nLine4\nLine5");
        memo.cursor = (4, 0);
        memo.scroll_to_cursor();

        memo.page_up();
        // Should move up by visible_lines - 1 = 2
        assert_eq!(memo.cursor.0, 2);

        memo.page_down();
        assert_eq!(memo.cursor.0, 4);
    }

    #[test]
    fn test_memo_cursor_clamped_to_line_length() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hi\nLonger Line");
        memo.cursor = (0, 5); // Beyond line length

        memo.clamp_cursor_col();
        assert_eq!(memo.cursor.1, 2); // "Hi".len() = 2
    }

    #[test]
    fn test_memo_scroll_to_cursor() {
        let mut memo = Memo::new(Rect::new(0, 0, 10, 3))
            .with_text("Line1\nLine2\nLine3\nLine4\nLine5");

        // Move cursor past visible area
        memo.cursor = (4, 0);
        memo.scroll_to_cursor();

        // Scroll should adjust so cursor is visible
        assert!(memo.cursor.0 >= memo.scroll.0);
        assert!(memo.cursor.0 < memo.scroll.0 + 3);
    }

    #[test]
    fn test_memo_content_size_hint() {
        let memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld\nFoo");

        let (w, h) = memo.content_size_hint().unwrap();
        assert_eq!(h, 3); // 3 lines
        assert_eq!(w, 5); // max line width is 5 ("Hello" / "World")
    }

    #[test]
    fn test_memo_scroll_position() {
        let mut memo = Memo::new(Rect::new(0, 0, 10, 3))
            .with_text("Line1\nLine2\nLine3\nLine4\nLine5");
        memo.scroll = (2, 3);

        let (x, y) = memo.scroll_position();
        assert_eq!(x, 3);
        assert_eq!(y, 2);
    }

    #[test]
    fn test_memo_scroll_to() {
        let mut memo = Memo::new(Rect::new(0, 0, 10, 3))
            .with_text("Line1\nLine2\nLine3\nLine4\nLine5");

        let result = memo.scroll_to(2, 1);
        assert!(result);
        assert_eq!(memo.scroll.1, 2);
        assert_eq!(memo.scroll.0, 1);
    }

    #[test]
    fn test_memo_key_event_insert() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10));

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(memo.text(), "a");
    }

    #[test]
    fn test_memo_key_event_enter() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello");

        memo.cursor = (0, 5);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(memo.text(), "Hello\n");
        assert_eq!(memo.cursor, (1, 0));
    }

    #[test]
    fn test_memo_key_event_backspace() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hi");
        memo.cursor = (0, 2);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Backspace,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);

        assert_eq!(memo.text(), "H");
        assert_eq!(memo.cursor, (0, 1));
    }

    #[test]
    fn test_memo_key_event_delete() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hi");
        memo.cursor = (0, 0);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Delete,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);

        assert_eq!(memo.text(), "i");
        assert_eq!(memo.cursor, (0, 0));
    }

    #[test]
    fn test_memo_key_event_navigation() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello\nWorld");
        memo.cursor = (1, 3);

        // Up
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(memo.cursor, (0, 3));

        // Down
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);
        assert_eq!(memo.cursor, (1, 3));

        // Left
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);
        assert_eq!(memo.cursor, (1, 2));

        // Right
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);
        assert_eq!(memo.cursor, (1, 3));
    }

    #[test]
    fn test_memo_mouse_click_places_cursor() {
        let mut memo = Memo::new(Rect::new(10, 5, 30, 10))
            .with_text("Hello\nWorld");

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 13, // 10 + 3
            row: 6,     // 5 + 1 (second line)
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        memo.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(memo.cursor, (1, 3)); // Line 1, col 3
    }

    #[test]
    fn test_memo_cleared_event_not_processed() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10));

        let mut event = Event::default();
        event.clear();

        memo.handle_event(&mut event);
        // Should not panic
    }

    #[test]
    fn test_memo_state() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10));

        assert_ne!(memo.state() & SF_VISIBLE, 0);
        assert_eq!(memo.state() & SF_FOCUSED, 0);

        memo.set_state(memo.state() | SF_FOCUSED);
        assert_ne!(memo.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_memo_cursor_position_none_when_not_focused() {
        let memo = Memo::new(Rect::new(10, 5, 30, 10))
            .with_text("Hello");
        assert_eq!(memo.cursor_position(), None);
    }

    #[test]
    fn test_memo_cursor_position_when_focused() {
        let mut memo = Memo::new(Rect::new(10, 5, 30, 10))
            .with_text("Hello");
        memo.set_state(memo.state() | SF_FOCUSED);
        memo.cursor = (0, 3);

        let pos = memo.cursor_position();
        assert_eq!(pos, Some(Position::new(13, 5))); // 10 + 3
    }

    #[test]
    fn test_memo_draw() {
        let memo = Memo::new(Rect::new(0, 0, 20, 5))
            .with_text("Test Content");
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        memo.draw(&mut buf, Rect::new(0, 0, 30, 10));

        let content = buf.content();
        let has_test = content.iter().any(|cell| {
            cell.symbol().contains('T') || cell.symbol().contains('C')
        });
        assert!(has_test, "Memo should draw its content");
    }

    #[test]
    fn test_memo_draw_empty() {
        let memo = Memo::new(Rect::new(0, 0, 20, 5));
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        memo.draw(&mut buf, Rect::new(0, 0, 20, 5));
        // Should not panic
    }

    #[test]
    fn test_memo_bounds() {
        let mut memo = Memo::new(Rect::new(5, 3, 20, 8));
        assert_eq!(memo.bounds(), Rect::new(5, 3, 20, 8));

        memo.set_bounds(Rect::new(10, 10, 30, 12));
        assert_eq!(memo.bounds(), Rect::new(10, 10, 30, 12));
    }

    #[test]
    fn test_memo_ignores_non_text_input() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10));

        let key = crossterm::event::KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        memo.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(memo.text(), "");
    }

    #[test]
    fn test_memo_multi_line_insert() {
        let mut memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("abcdef");
        memo.cursor = (0, 3);

        memo.insert_char('\n');
        assert_eq!(memo.text(), "abc\ndef");
        assert_eq!(memo.line_count(), 2);
        assert_eq!(memo.cursor, (1, 0));

        memo.insert_char('X');
        assert_eq!(memo.text(), "abc\nXdef");
        assert_eq!(memo.cursor, (1, 1));
    }

    #[test]
    fn test_memo_max_line_width() {
        let memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("a\nbb\nccc");
        assert_eq!(memo.max_line_width(), 3);
    }
}
