//! Terminal buffer types: [`Cell`] and [`Buffer`].
//!
//! Provides the buffer API used by turbo-tui, including grapheme-aware
//! width handling in [`Buffer::set_string`] via `unicode-segmentation` and
//! `unicode-width`.

use std::ops::{Index, IndexMut};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::geometry::{Position, Rect};
use crate::core::style::{Color, Modifier, Style};

/// A single terminal cell.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cell {
    /// The symbol drawn in the cell.
    symbol: String,
    /// The foreground color.
    pub fg: Color,
    /// The background color.
    pub bg: Color,
    /// The underline color.
    pub underline_color: Color,
    /// The active modifiers.
    pub modifier: Modifier,
    /// Whether the cell should be skipped when diffing.
    pub skip: bool,
}

impl Cell {
    /// Creates a cell with the given symbol and default colors.
    #[must_use]
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_owned(),
            ..Self::default()
        }
    }

    /// Returns the cell symbol.
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Sets the cell symbol.
    pub fn set_symbol(&mut self, symbol: &str) -> &mut Self {
        symbol.clone_into(&mut self.symbol);
        self
    }

    /// Sets the cell symbol to a single character.
    pub fn set_char(&mut self, ch: char) -> &mut Self {
        let mut buf = [0; 4];
        ch.encode_utf8(&mut buf).clone_into(&mut self.symbol);
        self
    }

    /// Applies a style to the cell.
    pub fn set_style<S: Into<Style>>(&mut self, style: S) -> &mut Self {
        let style = style.into();
        if let Some(color) = style.fg {
            self.fg = color;
        }
        if let Some(color) = style.bg {
            self.bg = color;
        }
        if let Some(color) = style.underline_color {
            self.underline_color = color;
        }
        self.modifier.insert(style.add_modifier);
        self.modifier.remove(style.sub_modifier);
        self
    }

    /// Returns the style of the cell.
    #[must_use]
    pub const fn style(&self) -> Style {
        Style {
            fg: Some(self.fg),
            bg: Some(self.bg),
            underline_color: Some(self.underline_color),
            add_modifier: self.modifier,
            sub_modifier: Modifier::empty(),
        }
    }

    /// Resets the cell to the empty state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            symbol: " ".to_owned(),
            fg: Color::Reset,
            bg: Color::Reset,
            underline_color: Color::Reset,
            modifier: Modifier::empty(),
            skip: false,
        }
    }
}

impl From<char> for Cell {
    fn from(ch: char) -> Self {
        let mut cell = Self::default();
        cell.set_char(ch);
        cell
    }
}

/// A grid of [`Cell`]s covering a [`Rect`].
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Buffer {
    /// The area covered by this buffer.
    pub area: Rect,
    /// The cells of the buffer, in row-major order.
    pub content: Vec<Cell>,
}

impl Buffer {
    /// Creates a buffer filled with empty cells.
    #[must_use]
    pub fn empty(area: Rect) -> Self {
        let size = area.area() as usize;
        Self {
            area,
            content: vec![Cell::default(); size],
        }
    }

    /// Returns the cells of the buffer as a slice.
    #[must_use]
    pub fn content(&self) -> &[Cell] {
        &self.content
    }

    /// Returns the area covered by this buffer.
    #[must_use]
    pub const fn area(&self) -> &Rect {
        &self.area
    }

    /// Returns a reference to the cell at `position`.
    #[must_use]
    pub fn cell<P: Into<Position>>(&self, position: P) -> Option<&Cell> {
        let index = self.index_of_opt(position.into())?;
        self.content.get(index)
    }

    /// Returns a mutable reference to the cell at `position`.
    #[must_use]
    pub fn cell_mut<P: Into<Position>>(&mut self, position: P) -> Option<&mut Cell> {
        let index = self.index_of_opt(position.into())?;
        self.content.get_mut(index)
    }

    /// Returns the index of a position in the content vector.
    ///
    /// # Panics
    ///
    /// Panics if `(x, y)` is outside the buffer area.
    #[must_use]
    pub fn index_of(&self, x: u16, y: u16) -> usize {
        self.index_of_opt(Position { x, y }).unwrap_or_else(|| {
            panic!(
                "index outside of buffer: the area is {area:?} but index is ({x}, {y})",
                area = self.area,
            )
        })
    }

    fn index_of_opt(&self, position: Position) -> Option<usize> {
        if !self.area.contains(position) {
            return None;
        }
        let y = usize::from(position.y - self.area.y);
        let x = usize::from(position.x - self.area.x);
        Some(y * usize::from(self.area.width) + x)
    }

    /// Writes a string starting at `(x, y)`.
    pub fn set_string<T, S>(&mut self, x: u16, y: u16, string: T, style: S)
    where
        T: AsRef<str>,
        S: Into<Style>,
    {
        self.set_stringn(x, y, string, usize::MAX, style);
    }

    /// Writes at most `max_width` cells of a string starting at `(x, y)`.
    ///
    /// The input is split into grapheme clusters (not scalar values), so a
    /// multi-codepoint sequence such as an emoji ZWJ family is written as one
    /// cell symbol. A cluster that would cross the right edge of the buffer is
    /// not drawn at all; zero-width clusters and control characters are skipped.
    pub fn set_stringn<T, S>(
        &mut self,
        mut x: u16,
        y: u16,
        string: T,
        max_width: usize,
        style: S,
    ) -> (u16, u16)
    where
        T: AsRef<str>,
        S: Into<Style>,
    {
        let max_width = u16::try_from(max_width).unwrap_or(u16::MAX);
        let mut remaining_width = self.area.right().saturating_sub(x).min(max_width);
        let graphemes = UnicodeSegmentation::graphemes(string.as_ref(), true)
            .filter(|symbol| !symbol.contains(char::is_control))
            .map(|symbol| (symbol, u16::try_from(symbol.width()).unwrap_or(u16::MAX)))
            .filter(|(_symbol, width)| *width > 0)
            .map_while(|(symbol, width)| {
                remaining_width = remaining_width.checked_sub(width)?;
                Some((symbol, width))
            });
        let style = style.into();
        for (symbol, width) in graphemes {
            self[(x, y)].set_symbol(symbol).set_style(style);
            let next_symbol = x + width;
            x += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while x < next_symbol {
                self[(x, y)].reset();
                x += 1;
            }
        }
        (x, y)
    }

    /// Applies a style to every cell inside `area`.
    pub fn set_style<S: Into<Style>>(&mut self, area: Rect, style: S) {
        let style = style.into();
        let area = self.area.intersection(area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                self[(x, y)].set_style(style);
            }
        }
    }

    /// Resizes the buffer to `area`, truncating or extending the content.
    pub fn resize(&mut self, area: Rect) {
        let length = area.area() as usize;
        if self.content.len() > length {
            self.content.truncate(length);
        } else {
            self.content.resize(length, Cell::default());
        }
        self.area = area;
    }

    /// Resets every cell to the empty state.
    pub fn reset(&mut self) {
        for cell in &mut self.content {
            cell.reset();
        }
    }
}

impl<P: Into<Position>> Index<P> for Buffer {
    type Output = Cell;

    fn index(&self, position: P) -> &Self::Output {
        let position = position.into();
        let index = self.index_of(position.x, position.y);
        &self.content[index]
    }
}

impl<P: Into<Position>> IndexMut<P> for Buffer {
    fn index_mut(&mut self, position: P) -> &mut Self::Output {
        let position = position.into();
        let index = self.index_of(position.x, position.y);
        &mut self.content[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_index_and_index_mut() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        buf[(1, 1)].set_symbol("X");
        assert_eq!(buf[(1, 1)].symbol(), "X");
        assert_eq!(buf[Position::new(1, 1)].symbol(), "X");
    }

    #[test]
    fn test_buffer_index_out_of_bounds_panics() {
        let buf = Buffer::empty(Rect::new(0, 0, 2, 2));
        let result = std::panic::catch_unwind(|| buf[(2, 0)].symbol().to_owned());
        assert!(result.is_err());
    }

    #[test]
    fn test_buffer_cell_out_of_bounds_is_none() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 2));
        assert!(buf.cell(Position::new(2, 0)).is_none());
        assert!(buf.cell_mut(Position::new(0, 2)).is_none());
    }

    #[test]
    fn test_set_string_clips_at_right_edge() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
        buf.set_string(3, 0, "Hello", Style::default());
        assert_eq!(buf[(3, 0)].symbol(), "H");
        assert_eq!(buf[(4, 0)].symbol(), "e");
        assert_eq!(buf[(0, 0)].symbol(), " ");
    }

    #[test]
    fn test_set_string_applies_style() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        buf.set_string(0, 0, "AB", Style::default().fg(Color::Red));
        assert_eq!(buf[(0, 0)].fg, Color::Red);
        assert_eq!(buf[(1, 0)].fg, Color::Red);
    }

    #[test]
    fn test_set_string_wide_char_uses_two_cells() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        buf.set_string(0, 0, "あい", Style::default().fg(Color::Green));
        assert_eq!(buf[(0, 0)].symbol(), "あ");
        // Continuation cells are reset to the empty cell, not styled.
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(2, 0)].symbol(), "い");
        assert_eq!(buf[(3, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].fg, Color::Reset);
    }

    #[test]
    fn test_set_string_wide_char_resets_continuation_cells() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));
        // Pre-fill so we can observe that continuation cells are reset, not
        // merely blanked.
        buf.set_string(0, 0, "abc", Style::default().fg(Color::Red).bg(Color::Blue));
        buf.set_string(0, 0, "あ", Style::default().fg(Color::Green));
        assert_eq!(buf[(0, 0)].symbol(), "あ");
        assert_eq!(buf[(0, 0)].fg, Color::Green);
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].fg, Color::Reset);
        assert_eq!(buf[(1, 0)].bg, Color::Reset);
    }

    #[test]
    fn test_set_string_zwj_emoji_is_one_grapheme() {
        const FAMILY: &str = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        let end = buf.set_stringn(
            0,
            0,
            FAMILY,
            usize::MAX,
            Style::default().fg(Color::Magenta),
        );
        // The whole ZWJ sequence is a single grapheme with width 2.
        assert_eq!(end, (2, 0));
        assert_eq!(buf[(0, 0)].symbol(), FAMILY);
        assert_eq!(buf[(0, 0)].fg, Color::Magenta);
        // Its continuation cell is reset.
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].fg, Color::Reset);
        // Cells after the grapheme are untouched.
        assert_eq!(buf[(2, 0)].symbol(), " ");
    }

    #[test]
    fn test_set_string_wide_char_not_split_at_edge() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 1));
        buf.set_string(1, 0, "あ", Style::default());
        // Only one column remains: the wide char must not be drawn.
        assert_eq!(buf[(1, 0)].symbol(), " ");
    }

    #[test]
    fn test_set_style_covers_intersection_only() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        buf.set_style(Rect::new(1, 1, 10, 10), Style::default().bg(Color::Blue));
        assert_eq!(buf[(1, 1)].bg, Color::Blue);
        assert_eq!(buf[(3, 1)].bg, Color::Blue);
        assert_eq!(buf[(0, 1)].bg, Color::Reset);
    }

    #[test]
    fn test_buffer_resize_truncates_and_extends() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        buf[(0, 0)].set_symbol("A");
        buf.resize(Rect::new(0, 0, 2, 1));
        assert_eq!(buf.content.len(), 2);
        assert_eq!(buf[(0, 0)].symbol(), "A");

        buf.resize(Rect::new(0, 0, 3, 2));
        assert_eq!(buf.content.len(), 6);
        assert_eq!(buf[(2, 1)].symbol(), " ");
    }

    #[test]
    fn test_buffer_reset() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 1));
        buf[(0, 0)].set_symbol("A");
        buf[(0, 0)].fg = Color::Red;
        buf.reset();
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(0, 0)].fg, Color::Reset);
    }

    #[test]
    fn test_cell_style_roundtrip() {
        let mut cell = Cell::new("A");
        cell.set_style(
            Style::default()
                .fg(Color::Blue)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let style = cell.style();
        assert_eq!(style.fg, Some(Color::Blue));
        assert_eq!(style.bg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }
}
