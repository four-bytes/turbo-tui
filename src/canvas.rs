//! Canvas abstraction for view rendering.
//!
//! A [`Canvas`] is the drawing surface a view renders onto. The design keeps
//! the rendering API independent from the concrete backing store: the
//! current implementation ([`CellCanvas`]) draws into a [`Buffer`], while
//! future canvases (e.g. a sub-cell graphics canvas) can expose a different
//! [`Canvas::metric`] without changing any view code.

use crate::core::{Buffer, Position, Rect, Style};

/// A drawing surface for views.
///
/// Coordinates are expressed in canvas units. For [`CellCanvas`] one unit is
/// exactly one terminal cell, so all coordinates are plain terminal columns
/// and rows.
pub trait Canvas {
    /// Width and height of one character in canvas units.
    ///
    /// [`CellCanvas`] returns `(1, 1)`.
    fn metric(&self) -> (u16, u16) {
        (1, 1)
    }

    /// The rectangle covered by this canvas.
    fn area(&self) -> Rect;

    /// Draws `text` starting at `(x, y)`, clipped to `clip`.
    fn draw_text(&mut self, x: u16, y: u16, text: &str, style: Style, clip: Rect);

    /// Draws a single character at `(x, y)` if it lies inside `clip`.
    fn put_char(&mut self, x: u16, y: u16, ch: char, style: Style, clip: Rect);

    /// Writes a multi-character symbol (e.g. grapheme cluster) at `(x, y)` if
    /// it lies inside `clip`.
    fn put_symbol(&mut self, x: u16, y: u16, symbol: &str, style: Style, clip: Rect);

    /// Fills `rect` with `ch`, clamping to the canvas area.
    fn fill(&mut self, rect: Rect, ch: char, style: Style);

    /// Applies `style` to every cell inside `rect`.
    fn set_style(&mut self, rect: Rect, style: Style);

    /// Places the terminal cursor, or hides it when `None`.
    fn set_cursor(&mut self, pos: Option<Position>);
}

/// A [`Canvas`] backed by a mutable [`Buffer`].
///
/// This is the concrete canvas used by the terminal renderer: every draw call
/// resolves to cell writes on the borrowed buffer.
pub struct CellCanvas<'a> {
    buffer: &'a mut Buffer,
    cursor: Option<Position>,
}

impl<'a> CellCanvas<'a> {
    /// Wraps an existing buffer as a canvas.
    #[must_use]
    pub fn new(buffer: &'a mut Buffer) -> Self {
        Self {
            buffer,
            cursor: None,
        }
    }

    /// Returns the cursor position previously set via [`Canvas::set_cursor`].
    #[must_use]
    pub fn cursor(&self) -> Option<Position> {
        self.cursor
    }
}

impl Canvas for CellCanvas<'_> {
    fn area(&self) -> Rect {
        self.buffer.area
    }

    fn draw_text(&mut self, x: u16, y: u16, text: &str, style: Style, clip: Rect) {
        crate::clip::set_string_clipped(&mut *self.buffer, x, y, text, style, clip);
    }

    fn put_char(&mut self, x: u16, y: u16, ch: char, style: Style, clip: Rect) {
        let pos = Position::new(x, y);
        if clip.contains(pos) {
            if let Some(cell) = self.buffer.cell_mut(pos) {
                cell.set_char(ch).set_style(style);
            }
        }
    }

    fn put_symbol(&mut self, x: u16, y: u16, symbol: &str, style: Style, clip: Rect) {
        let pos = Position::new(x, y);
        if clip.contains(pos) {
            if let Some(cell) = self.buffer.cell_mut(pos) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    }

    fn fill(&mut self, rect: Rect, ch: char, style: Style) {
        let area = rect.intersection(self.buffer.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = self.buffer.cell_mut(Position::new(x, y)) {
                    cell.set_char(ch).set_style(style);
                }
            }
        }
    }

    fn set_style(&mut self, rect: Rect, style: Style) {
        self.buffer.set_style(rect, style);
    }

    fn set_cursor(&mut self, pos: Option<Position>) {
        self.cursor = pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Color;

    #[test]
    fn test_cellcanvas_metric_is_one_by_one() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let canvas = CellCanvas::new(&mut buf);
        assert_eq!(canvas.metric(), (1, 1));
    }

    #[test]
    fn test_cellcanvas_area() {
        let area = Rect::new(2, 3, 10, 5);
        let mut buf = Buffer::empty(area);
        let canvas = CellCanvas::new(&mut buf);
        assert_eq!(canvas.area(), area);
    }

    #[test]
    fn test_draw_text_respects_clip() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        {
            let mut canvas = CellCanvas::new(&mut buf);
            // Clip covers columns 5..10 on row 1 only.
            let clip = Rect::new(5, 1, 5, 1);
            canvas.draw_text(3, 1, "HelloWorld", Style::default(), clip);
        }

        // Columns 3 and 4 are left of the clip — untouched.
        assert_eq!(buf.cell(Position::new(3, 1)).unwrap().symbol(), " ");
        assert_eq!(buf.cell(Position::new(4, 1)).unwrap().symbol(), " ");
        // Columns 5..10 are inside the clip and receive the corresponding chars.
        assert_eq!(buf.cell(Position::new(5, 1)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(6, 1)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(7, 1)).unwrap().symbol(), "o");
        assert_eq!(buf.cell(Position::new(8, 1)).unwrap().symbol(), "W");
        assert_eq!(buf.cell(Position::new(9, 1)).unwrap().symbol(), "o");
        // Column 10 is past the clip — untouched.
        assert_eq!(buf.cell(Position::new(10, 1)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_put_char_outside_clip_is_ignored() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
        let clip = Rect::new(5, 5, 2, 2);
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.put_char(0, 0, 'X', Style::default(), clip);
            canvas.put_char(5, 5, 'Y', Style::default(), clip);
        }

        // Outside the clip the character is ignored.
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), " ");
        // Inside the clip the character is written.
        assert_eq!(buf.cell(Position::new(5, 5)).unwrap().symbol(), "Y");
    }

    #[test]
    fn test_put_symbol_writes_multi_char_symbol() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let clip = Rect::new(2, 1, 3, 1);
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.put_symbol(2, 1, "ab", Style::default(), clip);
            // Outside the clip nothing is written.
            canvas.put_symbol(0, 0, "cd", Style::default(), clip);
        }

        assert_eq!(buf.cell(Position::new(2, 1)).unwrap().symbol(), "ab");
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_fill_clamps_to_buffer_area() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
        {
            let mut canvas = CellCanvas::new(&mut buf);
            // Extends past the right and bottom edges of the 10x3 buffer.
            canvas.fill(Rect::new(8, 0, 10, 5), '#', Style::default());
        }

        assert_eq!(buf.cell(Position::new(7, 0)).unwrap().symbol(), " ");
        assert_eq!(buf.cell(Position::new(8, 0)).unwrap().symbol(), "#");
        assert_eq!(buf.cell(Position::new(9, 2)).unwrap().symbol(), "#");
    }

    #[test]
    fn test_set_style_applies_to_rect() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        {
            let mut canvas = CellCanvas::new(&mut buf);
            let style = Style::default().fg(Color::Red);
            canvas.set_style(Rect::new(1, 1, 3, 2), style);
        }

        assert_eq!(buf.cell(Position::new(1, 1)).unwrap().fg, Color::Red);
        assert_eq!(buf.cell(Position::new(3, 2)).unwrap().fg, Color::Red);
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().fg, Color::Reset);
    }

    #[test]
    fn test_set_cursor_is_stored() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        let mut canvas = CellCanvas::new(&mut buf);

        canvas.set_cursor(Some(Position::new(3, 2)));
        assert_eq!(canvas.cursor(), Some(Position::new(3, 2)));

        canvas.set_cursor(None);
        assert_eq!(canvas.cursor(), None);
    }
}
