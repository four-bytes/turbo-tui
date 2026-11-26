//! Terminal backend with persistent front and back buffers.
//!
//! [`Terminal`] owns an output writer and renders only the cells that changed
//! since the previous frame. [`TestTerminal`] provides the same persistent
//! buffer diffing in memory so widgets and integration tests can run without a
//! real terminal.

use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::queue;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Print, SetAttribute, SetBackgroundColor,
    SetForegroundColor, SetUnderlineColor,
};
use crossterm::terminal::{Clear, ClearType};
use unicode_width::UnicodeWidthStr;

use crate::core::{Buffer, Cell, Color, Modifier, Position, Rect};

/// Maps a turbo-tui [`Color`] to the equivalent crossterm color.
///
/// The named colors follow the same mapping Ratatui uses: turbo-tui's
/// `Red` is the normal ANSI red, which crossterm calls `DarkRed`.
fn map_color(color: Color) -> CrosstermColor {
    match color {
        Color::Reset => CrosstermColor::Reset,
        Color::Black => CrosstermColor::Black,
        Color::Red => CrosstermColor::DarkRed,
        Color::Green => CrosstermColor::DarkGreen,
        Color::Yellow => CrosstermColor::DarkYellow,
        Color::Blue => CrosstermColor::DarkBlue,
        Color::Magenta => CrosstermColor::DarkMagenta,
        Color::Cyan => CrosstermColor::DarkCyan,
        Color::Gray => CrosstermColor::Grey,
        Color::DarkGray => CrosstermColor::DarkGrey,
        Color::LightRed => CrosstermColor::Red,
        Color::LightGreen => CrosstermColor::Green,
        Color::LightYellow => CrosstermColor::Yellow,
        Color::LightBlue => CrosstermColor::Blue,
        Color::LightMagenta => CrosstermColor::Magenta,
        Color::LightCyan => CrosstermColor::Cyan,
        Color::White => CrosstermColor::White,
        Color::Rgb(r, g, b) => CrosstermColor::Rgb { r, g, b },
        Color::Indexed(index) => CrosstermColor::AnsiValue(index),
    }
}

/// Returns the row-major `(x, y)` positions of the cells that changed between
/// `front` and `back`.
///
/// Mirrors `ratatui_core`'s `Buffer::diff`: cells hidden behind a preceding
/// multi-width grapheme are skipped, cells invalidated by a multi-width symbol
/// change are re-emitted, and VS16 emoji presentation sequences get explicit
/// trailing-cell clears.
fn diff_updates(front: &Buffer, back: &Buffer) -> Vec<(u16, u16)> {
    let mut updates = Vec::new();
    let mut invalidated: usize = 0;
    let mut to_skip: usize = 0;

    let area = back.area;
    for (i, (current, previous)) in back.content.iter().zip(front.content.iter()).enumerate() {
        if !current.skip && (current != previous || invalidated > 0) && to_skip == 0 {
            updates.push(position_of(area, i));

            // Some terminals do not reliably clear the trailing cell(s) of a wide
            // grapheme. Emit explicit updates for the trailing cells of emoji
            // presentation sequences (VS16 / U+FE0F); standard wide characters
            // such as CJK are cleared by the terminal itself.
            let symbol = current.symbol();
            let cell_width = symbol.width();
            let contains_vs16 = symbol.chars().any(|c| c == '\u{FE0F}');
            if cell_width > 1 && contains_vs16 {
                for k in 1..cell_width {
                    let j = i + k;
                    if j >= back.content.len() || j >= front.content.len() {
                        break;
                    }
                    let previous_trailing = &front.content[j];
                    let next_trailing = &back.content[j];
                    if !next_trailing.skip && previous_trailing != next_trailing {
                        updates.push(position_of(area, j));
                    }
                }
            }
        }

        to_skip = current.symbol().width().saturating_sub(1);
        let affected_width = current.symbol().width().max(previous.symbol().width());
        invalidated = affected_width.max(invalidated).saturating_sub(1);
    }

    updates
}

/// Maps a linear buffer index to its `(x, y)` position within `area`.
fn position_of(area: Rect, index: usize) -> (u16, u16) {
    let width = usize::from(area.width);
    if width == 0 {
        return (area.x, area.y);
    }
    let column = u16::try_from(index % width).unwrap_or_default();
    let row = u16::try_from(index / width).unwrap_or_default();
    (area.x.saturating_add(column), area.y.saturating_add(row))
}

/// Queues the escape sequences that draw a single cell.
///
/// The attribute reset comes first because crossterm's reset clears the colors
/// as well; colors and modifiers are applied afterwards.
fn queue_cell<W: Write>(writer: &mut W, cell: &Cell) -> io::Result<()> {
    queue!(
        writer,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(map_color(cell.fg)),
        SetBackgroundColor(map_color(cell.bg)),
        SetUnderlineColor(map_color(cell.underline_color)),
    )?;
    if cell.modifier.contains(Modifier::BOLD) {
        queue!(writer, SetAttribute(Attribute::Bold))?;
    }
    if cell.modifier.contains(Modifier::DIM) {
        queue!(writer, SetAttribute(Attribute::Dim))?;
    }
    if cell.modifier.contains(Modifier::ITALIC) {
        queue!(writer, SetAttribute(Attribute::Italic))?;
    }
    if cell.modifier.contains(Modifier::UNDERLINED) {
        queue!(writer, SetAttribute(Attribute::Underlined))?;
    }
    if cell.modifier.contains(Modifier::SLOW_BLINK) {
        queue!(writer, SetAttribute(Attribute::SlowBlink))?;
    }
    if cell.modifier.contains(Modifier::RAPID_BLINK) {
        queue!(writer, SetAttribute(Attribute::RapidBlink))?;
    }
    if cell.modifier.contains(Modifier::REVERSED) {
        queue!(writer, SetAttribute(Attribute::Reverse))?;
    }
    if cell.modifier.contains(Modifier::HIDDEN) {
        queue!(writer, SetAttribute(Attribute::Hidden))?;
    }
    if cell.modifier.contains(Modifier::CROSSED_OUT) {
        queue!(writer, SetAttribute(Attribute::CrossedOut))?;
    }
    let symbol = if cell.symbol().is_empty() {
        " "
    } else {
        cell.symbol()
    };
    queue!(writer, Print(symbol))
}

/// A terminal that renders into persistent front and back buffers.
///
/// The back buffer is handed to the draw callback and keeps its contents
/// between frames, so callers that only update a small region do not have to
/// repaint the whole screen.
pub struct Terminal<W: Write> {
    writer: W,
    width: u16,
    height: u16,
    front: Buffer,
    back: Buffer,
    cursor: Option<Position>,
    cursor_visible: bool,
}

impl<W: Write> Terminal<W> {
    /// Creates a terminal with empty front and back buffers.
    #[must_use]
    pub fn new(writer: W, width: u16, height: u16) -> Self {
        let area = Rect::new(0, 0, width, height);
        Self {
            writer,
            width,
            height,
            front: Buffer::empty(area),
            back: Buffer::empty(area),
            cursor: None,
            cursor_visible: true,
        }
    }

    /// Returns the terminal size as `(width, height)`.
    #[must_use]
    pub const fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Returns a reference to the underlying writer.
    #[must_use]
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Renders one frame and flushes the changed cells to the writer.
    ///
    /// The callback receives the persistent back buffer, which is **not**
    /// cleared between frames. The returned position becomes the cursor
    /// position for this frame.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Buffer) -> Option<Position>,
    {
        let cursor = render(&mut self.back);
        self.set_cursor(cursor);
        self.flush()
    }

    /// Emits the cells that differ between the back and the front buffer.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        for (x, y) in diff_updates(&self.front, &self.back) {
            let cell = self.back[(x, y)].clone();
            queue!(self.writer, MoveTo(x, y))?;
            queue_cell(&mut self.writer, &cell)?;
        }
        self.front = self.back.clone();

        match self.cursor {
            Some(position) => {
                queue!(self.writer, MoveTo(position.x, position.y))?;
                if !self.cursor_visible {
                    queue!(self.writer, Show)?;
                    self.cursor_visible = true;
                }
            }
            None => {
                if self.cursor_visible {
                    queue!(self.writer, Hide)?;
                    self.cursor_visible = false;
                }
            }
        }

        self.writer.flush()
    }

    /// Clears the screen and resets both buffers.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn clear(&mut self) -> io::Result<()> {
        queue!(self.writer, Clear(ClearType::All))?;
        let area = Rect::new(0, 0, self.width, self.height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        self.writer.flush()
    }

    /// Resizes both buffers and forces a full repaint on the next frame.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.width = width;
        self.height = height;
        let area = Rect::new(0, 0, width, height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        queue!(self.writer, Clear(ClearType::All))?;
        self.writer.flush()
    }

    /// Shows the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Show)?;
        self.cursor_visible = true;
        self.writer.flush()
    }

    /// Hides the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Hide)?;
        self.cursor_visible = false;
        self.writer.flush()
    }

    /// Stores the cursor position requested by the render callback.
    const fn set_cursor(&mut self, cursor: Option<Position>) {
        self.cursor = cursor;
    }
}

/// An in-memory terminal used by tests instead of a real backend.
///
/// It keeps persistent front and back buffers and performs the same cell-level
/// diff as [`Terminal`], but records the frame in memory instead of writing
/// escape sequences.
pub struct TestTerminal {
    width: u16,
    height: u16,
    front: Buffer,
    back: Buffer,
    cursor: Option<Position>,
    changed: usize,
}

impl TestTerminal {
    /// Creates a test terminal with empty front and back buffers.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let area = Rect::new(0, 0, width, height);
        Self {
            width,
            height,
            front: Buffer::empty(area),
            back: Buffer::empty(area),
            cursor: None,
            changed: 0,
        }
    }

    /// Renders one frame into the back buffer and copies the changed cells.
    pub fn draw<F>(&mut self, render: F)
    where
        F: FnOnce(&mut Buffer) -> Option<Position>,
    {
        let cursor = render(&mut self.back);
        let updates = diff_updates(&self.front, &self.back);
        self.changed = updates.len();
        self.front = self.back.clone();
        self.cursor = cursor;
    }

    /// Returns the rendered screen (the front buffer).
    #[must_use]
    pub const fn buffer(&self) -> &Buffer {
        &self.front
    }

    /// Returns the cursor position stored by the last [`TestTerminal::draw`].
    #[must_use]
    pub const fn cursor(&self) -> Option<Position> {
        self.cursor
    }

    /// Returns the number of cells that changed in the last frame.
    #[must_use]
    pub const fn last_changed_cells(&self) -> usize {
        self.changed
    }

    /// Recreates both buffers at the new size, discarding their contents.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let area = Rect::new(0, 0, width, height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        self.changed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_first_draw_writes_all_nonblank_cells() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "AB", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains('A'));
        assert!(output.contains('B'));
    }

    #[test]
    fn test_terminal_identical_redraw_writes_nothing() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        let render = |buffer: &mut Buffer| {
            buffer.set_string(0, 0, "AB", crate::core::Style::default());
            None
        };
        terminal.draw(render).expect("first draw succeeds");
        let after_first = terminal.writer().len();

        terminal.draw(render).expect("second draw succeeds");
        assert_eq!(terminal.writer().len(), after_first);
    }

    #[test]
    fn test_terminal_changed_cell_writes_only_that_cell() {
        let mut terminal = Terminal::new(Vec::new(), 6, 2);
        terminal
            .draw(|buffer| {
                for y in 0..2 {
                    buffer.set_string(0, y, "AAAAAA", crate::core::Style::default());
                }
                None
            })
            .expect("first draw succeeds");
        let full_frame_len = terminal.writer().len();

        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "Z", crate::core::Style::default());
                None
            })
            .expect("second draw succeeds");

        let delta = &terminal.writer()[full_frame_len..];
        let delta = String::from_utf8_lossy(delta);
        assert!(delta.contains('Z'));
        assert!(!delta.contains('A'), "unchanged cells were re-emitted");
        assert!(
            delta.len() < full_frame_len,
            "dirty frame should be smaller than a full frame"
        );
    }

    #[test]
    fn test_test_terminal_dirty_redraw_leaves_rest_intact() {
        let mut terminal = TestTerminal::new(12, 3);
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "HELLO", crate::core::Style::default());
            buffer.set_string(5, 1, "X", crate::core::Style::default());
            None
        });

        terminal.draw(|buffer| {
            buffer.set_string(1, 0, "A", crate::core::Style::default());
            None
        });

        let buffer = terminal.buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "H");
        assert_eq!(buffer[(1, 0)].symbol(), "A");
        assert_eq!(buffer[(2, 0)].symbol(), "L");
        assert_eq!(buffer[(5, 1)].symbol(), "X", "untouched cell was lost");
        assert_eq!(buffer[(11, 2)].symbol(), " ");
    }

    #[test]
    fn test_test_terminal_changed_cell_count() {
        let mut terminal = TestTerminal::new(4, 1);
        let render = |buffer: &mut Buffer| {
            buffer.set_string(0, 0, "AB", crate::core::Style::default());
            None
        };
        terminal.draw(render);
        assert_eq!(terminal.last_changed_cells(), 2);

        terminal.draw(render);
        assert_eq!(terminal.last_changed_cells(), 0);

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "X", crate::core::Style::default());
            None
        });
        assert_eq!(terminal.last_changed_cells(), 1);
    }

    #[test]
    fn test_terminal_resize_clears() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "AB", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");
        let before = terminal.writer().len();

        terminal.resize(6, 2).expect("resize succeeds");

        assert_eq!(terminal.size(), (6, 2));
        assert_eq!(terminal.front.content.len(), 12);
        assert_eq!(terminal.back.content.len(), 12);
        assert!(terminal
            .front
            .content
            .iter()
            .all(|cell| *cell == Cell::default()));
        assert!(terminal
            .back
            .content
            .iter()
            .all(|cell| *cell == Cell::default()));
        let written = &terminal.writer()[before..];
        assert!(
            written.windows(4).any(|window| window == b"\x1b[2J"),
            "resize should queue a full clear"
        );
    }

    #[test]
    fn test_cursor_position_is_tracked() {
        let mut terminal = TestTerminal::new(10, 10);
        terminal.draw(|_buffer| Some(Position::new(3, 4)));
        assert_eq!(terminal.cursor(), Some(Position::new(3, 4)));

        terminal.draw(|_buffer| None);
        assert_eq!(terminal.cursor(), None);
    }

    #[test]
    fn test_wide_char_diff() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "世", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(
            output.matches('世').count(),
            1,
            "wide grapheme must be emitted exactly once"
        );
    }

    #[test]
    fn test_wide_char_does_not_print_continuation() {
        let mut terminal = TestTerminal::new(4, 1);
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "ab", crate::core::Style::default());
            None
        });
        assert_eq!(terminal.last_changed_cells(), 2);

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "世", crate::core::Style::default());
            None
        });
        assert_eq!(
            terminal.last_changed_cells(),
            1,
            "the continuation cell must not be counted or printed"
        );

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "ab", crate::core::Style::default());
            None
        });
        assert_eq!(
            terminal.last_changed_cells(),
            2,
            "the trailing cell of the previous wide char must be re-emitted"
        );
    }

    #[test]
    fn test_wide_char_over_previous_content_terminal() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "ab", crate::core::Style::default());
                None
            })
            .expect("first draw succeeds");
        let after_first = terminal.writer().len();

        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "世", crate::core::Style::default());
                None
            })
            .expect("second draw succeeds");

        let delta = &terminal.writer()[after_first..];
        let delta = String::from_utf8_lossy(delta);
        assert!(delta.contains('世'), "wide grapheme was not printed");
        assert_eq!(
            delta.matches(' ').count(),
            0,
            "the continuation column must not be printed as a space: {delta:?}"
        );
    }
}
