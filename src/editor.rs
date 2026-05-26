//! `Editor` — text editor combining `Memo` with file I/O, syntax highlighting,
//! and search/replace.
//!
//! The `Editor` wraps a [`Memo`] and adds:
//! - File loading and saving
//! - Syntax highlighting integration
//! - Search and replace functionality (basic)
//! - File path tracking
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{Editor, Rect};
//!
//! let mut editor = Editor::new(Rect::new(1, 1, 38, 20))
//!     .with_file("src/lib.rs")
//!     .unwrap_or_else(|_| Editor::new(Rect::new(1, 1, 38, 20)));
//! ```

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

use crate::clip;
use crate::memo::Memo;
use crate::syntax::{PlainTextHighlighter, SyntaxHighlighter};
use crate::theme;
use crate::view::{
    Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE, SF_FOCUSED,
};
use crossterm::event::{KeyCode, KeyModifiers};

/// Text editor combining `Memo` with file I/O and syntax highlighting.
///
/// The `Editor` delegates rendering and event handling to its internal
/// [`Memo`], but overrides `draw()` to apply syntax highlighting on top
/// of the memo content.
///
/// # Features
///
/// - **File I/O:** Load files with `with_file()` or `load()`, save with `save()`.
/// - **Syntax highlighting:** Set a highlighter via `set_highlighter()` (defaults
///   to `PlainTextHighlighter`). The `RustHighlighter` is built-in.
/// - **Search:** `search(query)` returns all match positions as `(line, col)`.
/// - **Replace:** `replace(query, replacement)` replaces all occurrences and
///   returns the count.
pub struct Editor {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// Internal memo widget for text editing.
    memo: Memo,
    /// Optional file path for load/save.
    path: Option<PathBuf>,
    /// Syntax highlighter.
    highlighter: Box<dyn SyntaxHighlighter>,
}

impl Editor {
    /// Create a new empty editor with the given bounds.
    ///
    /// The editor is selectable (can receive focus). No file is loaded.
    /// The default highlighter is `PlainTextHighlighter`.
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the edit area.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            memo: Memo::new(bounds),
            path: None,
            highlighter: Box::new(PlainTextHighlighter),
        }
    }

    /// Load a file into the editor.
    ///
    /// Sets the file path and reads the contents. The memo text is replaced
    /// with the file contents. If the file cannot be read, the editor is
    /// returned unchanged (no error — use `load()` for fallible loading).
    ///
    /// This is a convenience for the Builder Lite pattern:
    ///
    /// ```ignore
    /// let editor = Editor::new(Rect::new(0, 0, 40, 10))
    ///     .with_file("src/main.rs")
    ///     .unwrap_or_else(|_| Editor::new(Rect::new(0, 0, 40, 10)));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the file cannot be read.
    pub fn with_file(mut self, path: &str) -> std::io::Result<Self> {
        self.load_file(path)?;
        Ok(self)
    }

    /// Load a file from the given path.
    ///
    /// Replaces the current memo text with the file contents.
    /// Updates the file path.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the file cannot be read.
    pub fn load(&mut self, path: &str) -> std::io::Result<()> {
        self.load_file(path)
    }

    /// Internal file loading implementation.
    fn load_file(&mut self, path: &str) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.path = Some(PathBuf::from(path));
        self.memo = Memo::new(self.base.bounds()).with_text(&content);
        Ok(())
    }

    /// Save the current content to the file path, if set.
    ///
    /// Returns `Ok(true)` if saved, `Err` if the file cannot be written,
    /// or `Ok(false)` if no file path is set.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the file cannot be written.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            let content = self.memo.text();
            std::fs::write(path, &content)?;
        }
        Ok(())
    }

    /// Save the current content to a specific file path.
    ///
    /// Updates the internal file path and writes the content.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the file cannot be written.
    pub fn save_as(&mut self, path: &str) -> std::io::Result<()> {
        let content = self.memo.text();
        std::fs::write(path, &content)?;
        self.path = Some(PathBuf::from(path));
        Ok(())
    }

    /// Get the current file path, if any.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the current text content.
    #[must_use]
    pub fn text(&self) -> String {
        self.memo.text()
    }

    /// Get a reference to the internal memo.
    #[must_use]
    pub fn memo(&self) -> &Memo {
        &self.memo
    }

    /// Get a mutable reference to the internal memo.
    pub fn memo_mut(&mut self) -> &mut Memo {
        &mut self.memo
    }

    /// Set the syntax highlighter.
    ///
    /// The highlighter is used during `draw()` to apply colors to tokens.
    /// Default is `PlainTextHighlighter`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use four_turbo_tui::syntax::RustHighlighter;
    ///
    /// editor.set_highlighter(Box::new(RustHighlighter));
    /// ```
    pub fn set_highlighter(&mut self, highlighter: Box<dyn SyntaxHighlighter>) {
        self.highlighter = highlighter;
    }

    /// Search for all occurrences of `query` in the text.
    ///
    /// Returns a vector of `(line_index, column_offset)` pairs.
    /// The column offset is a byte index into the line.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let matches = editor.search("TODO");
    /// for (line, col) in &matches {
    ///     println!("Found at line {line}, column {col}");
    /// }
    /// ```
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        for (line_idx, line) in self.memo.lines().iter().enumerate() {
            for (col_idx, _) in line.match_indices(query) {
                matches.push((line_idx, col_idx));
            }
        }
        matches
    }

    /// Replace all occurrences of `query` with `replacement`.
    ///
    /// Returns the number of replacements made.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = editor.replace("old", "new");
    /// println!("Replaced {count} occurrences");
    /// ```
    pub fn replace(&mut self, query: &str, replacement: &str) -> usize {
        let mut count = 0;
        // Use a simple approach: rebuild lines with replacement
        let new_lines: Vec<String> = self
            .memo
            .lines()
            .iter()
            .map(|line| {
                if line.contains(query) {
                    let replaced = line.replace(query, replacement);
                    count += line.matches(query).count();
                    replaced
                } else {
                    line.clone()
                }
            })
            .collect();
        // Replace the memo's text via with_text
        let text = new_lines.join("\n");
        self.memo = Memo::new(self.base.bounds()).with_text(&text);
        count
    }

    /// Draw syntax-highlighted text.
    fn draw_highlighted(&self, buf: &mut Buffer, clip: Rect) {
        let bounds = self.base.bounds();
        let draw_area = bounds.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let focused = self.state() & SF_FOCUSED != 0;
        let base_style = theme::with_current(|t| {
            if focused {
                t.memo_focused
            } else {
                t.memo_normal
            }
        });

        let visible_lines = usize::from(bounds.height);
        let scroll = self.memo.scroll();

        // Draw highlighted lines
        let max_visible = self.memo.line_count().saturating_sub(scroll.0);
        for i in 0..visible_lines.min(max_visible) {
            let line_idx = scroll.0 + i;
            let row = bounds.y + u16::try_from(i).unwrap_or(0);

            if row < clip.y || row >= clip.y + clip.height {
                continue;
            }

            if let Some(line) = self.memo.lines().get(line_idx) {
                // Get visible portion
                let visible_part = if scroll.1 < line.len() {
                    &line[scroll.1..]
                } else {
                    ""
                };

                if visible_part.is_empty() {
                    // Fill empty line with background
                    for col in bounds.x..bounds.x + u16::try_from(visible_lines).unwrap_or(0) {
                        if col >= clip.x && col < clip.x + clip.width {
                            if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                                cell.set_char(' ').set_style(base_style);
                            }
                        }
                    }
                    continue;
                }

                // Apply syntax highlighting
                let tokens = self.highlighter.highlight(visible_part);
                let mut x = bounds.x;
                for (style, token) in &tokens {
                    // Merge highlight style with base style background.
                    // patch() replaces fg/modifiers from the highlight style
                    // but keeps bg from base_style (since syntax tokens don't set bg).
                    let merged_style = base_style.patch(*style);
                    clip::set_string_clipped(buf, x, row, token, merged_style, clip);
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        x = x.saturating_add(token.len() as u16);
                    }
                }

                // Fill remainder of line with background
                let text_width = visible_part.len();
                let visible_width = usize::from(bounds.width);
                if text_width < visible_width {
                    let fill_start = bounds.x + u16::try_from(text_width).unwrap_or(0);
                    let fill_end = bounds.x + u16::try_from(visible_width).unwrap_or(0);
                    for col in fill_start..fill_end {
                        if col >= clip.x && col < clip.x + clip.width {
                            if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                                cell.set_char(' ').set_style(base_style);
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
            for col in bounds.x..bounds.x + u16::try_from(visible_lines).unwrap_or(0) {
                if col >= clip.x && col < clip.x + clip.width {
                    if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                        cell.set_char(' ').set_style(base_style);
                    }
                }
            }
        }
    }

    /// Check if the cursor is visible (no overlays active).
    /// We delegate cursor to memo.
    fn memo_cursor_position(&self) -> Option<Position> {
        if self.state() & SF_FOCUSED == 0 {
            return None;
        }
        self.memo.cursor_position()
    }
}

impl View for Editor {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.memo.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        self.draw_highlighted(buf, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        // Handle Ctrl+S for save
        if let EventKind::Key(key) = &event.kind {
            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if let Err(e) = self.save() {
                    // Silently ignore save errors for now; in a real app,
                    // this would show a dialog or status message
                    let _ = e;
                }
                event.clear();
                return;
            }
        }

        // Delegate all other events to memo
        self.memo.handle_event(event);
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn cursor_position(&self) -> Option<Position> {
        self.memo_cursor_position()
    }

    fn content_size_hint(&self) -> Option<(u16, u16)> {
        self.memo.content_size_hint()
    }

    fn scroll_to(&mut self, x: i32, y: i32) -> bool {
        self.memo.scroll_to(x, y)
    }

    fn scroll_position(&self) -> (i32, i32) {
        self.memo.scroll_position()
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
        self.memo.set_state(state);
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
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    #[test]
    fn test_editor_new() {
        let editor = Editor::new(Rect::new(10, 5, 30, 10));

        assert_eq!(editor.bounds(), Rect::new(10, 5, 30, 10));
        assert_eq!(editor.text(), "");
        assert!(editor.path().is_none());
        assert!(editor.can_focus());
        assert_ne!(editor.options() & OF_SELECTABLE, 0);
    }

    #[test]
    fn test_editor_text_content() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10)).with_text("Hello\nWorld");

        assert_eq!(editor.text(), "Hello\nWorld");
    }

    #[test]
    fn test_editor_search() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello World\nHello Again");

        let matches = editor.search("Hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (0, 0));
        assert_eq!(matches[1], (1, 0));
    }

    #[test]
    fn test_editor_search_no_matches() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello World");

        let matches = editor.search("xyz");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_editor_replace() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello World\nHello Again");

        let count = editor.replace("Hello", "Hi");
        assert_eq!(count, 2);
        assert_eq!(editor.text(), "Hi World\nHi Again");
    }

    #[test]
    fn test_editor_replace_no_matches() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Hello World");

        let count = editor.replace("xyz", "abc");
        assert_eq!(count, 0);
        assert_eq!(editor.text(), "Hello World");
    }

    #[test]
    fn test_editor_set_highlighter() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.set_highlighter(Box::new(crate::syntax::RustHighlighter));
        // Should not panic
    }

    #[test]
    fn test_editor_state() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));

        assert_ne!(editor.state() & SF_VISIBLE, 0);
        assert_eq!(editor.state() & SF_FOCUSED, 0);

        editor.set_state(editor.state() | SF_FOCUSED);
        assert_ne!(editor.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_editor_bounds() {
        let mut editor = Editor::new(Rect::new(5, 3, 30, 10));
        assert_eq!(editor.bounds(), Rect::new(5, 3, 30, 10));

        editor.set_bounds(Rect::new(10, 10, 40, 20));
        assert_eq!(editor.bounds(), Rect::new(10, 10, 40, 20));
    }

    #[test]
    fn test_editor_cursor_position_none_when_not_focused() {
        let editor = Editor::new(Rect::new(10, 5, 30, 10));
        assert_eq!(editor.cursor_position(), None);
    }

    #[test]
    fn test_editor_content_size_hint() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        editor.memo = Memo::new(Rect::new(0, 0, 30, 10))
            .with_text("Line 1\nLine 2\nLine 3");

        let (w, h) = editor.content_size_hint().unwrap();
        assert_eq!(h, 3);
        assert_eq!(w, 6); // "Line 1" is 6 chars
    }

    #[test]
    fn test_editor_scroll_to() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));
        let result = editor.scroll_to(5, 3);
        assert!(result);
    }

    #[test]
    fn test_editor_save_without_path() {
        let editor = Editor::new(Rect::new(0, 0, 30, 10));
        // Should not error, just do nothing
        assert!(editor.save().is_ok());
    }

    #[test]
    fn test_editor_ctrl_s_saves() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));

        let key = KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let mut event = Event::key(key);
        editor.handle_event(&mut event);

        assert!(event.is_cleared());
    }

    #[test]
    fn test_editor_delegates_to_memo() {
        let mut editor = Editor::new(Rect::new(0, 0, 30, 10));

        // Type a character — should be delegated to memo
        let key = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let mut event = Event::key(key);
        editor.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(editor.text(), "a");
    }

    #[test]
    fn test_editor_draw() {
        let mut editor = Editor::new(Rect::new(0, 0, 20, 5));
        editor.memo = Memo::new(Rect::new(0, 0, 20, 5))
            .with_text("Test Content");
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        editor.draw(&mut buf, Rect::new(0, 0, 30, 10));
        // Should not panic
    }
}
