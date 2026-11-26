//! Temporary conversions between turbo-tui's own core types and Ratatui.
//!
//! These bridge implementations exist only while `Application::draw` still
//! renders into a `ratatui::Frame`. They are the only place under
//! `src/core/` that may mention `ratatui`.

use crate::core::buffer::{Buffer, Cell};
use crate::core::geometry::{Position, Rect};
use crate::core::style::{Color, Modifier, Style};

impl From<Rect> for ratatui::layout::Rect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<ratatui::layout::Rect> for Rect {
    fn from(rect: ratatui::layout::Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<Position> for ratatui::layout::Position {
    fn from(position: Position) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

impl From<ratatui::layout::Position> for Position {
    fn from(position: ratatui::layout::Position) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

impl From<Color> for ratatui::style::Color {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => Self::Reset,
            Color::Black => Self::Black,
            Color::Red => Self::Red,
            Color::Green => Self::Green,
            Color::Yellow => Self::Yellow,
            Color::Blue => Self::Blue,
            Color::Magenta => Self::Magenta,
            Color::Cyan => Self::Cyan,
            Color::Gray => Self::Gray,
            Color::DarkGray => Self::DarkGray,
            Color::LightRed => Self::LightRed,
            Color::LightGreen => Self::LightGreen,
            Color::LightYellow => Self::LightYellow,
            Color::LightBlue => Self::LightBlue,
            Color::LightMagenta => Self::LightMagenta,
            Color::LightCyan => Self::LightCyan,
            Color::White => Self::White,
            Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
            Color::Indexed(index) => Self::Indexed(index),
        }
    }
}

impl From<ratatui::style::Color> for Color {
    fn from(color: ratatui::style::Color) -> Self {
        match color {
            ratatui::style::Color::Reset => Self::Reset,
            ratatui::style::Color::Black => Self::Black,
            ratatui::style::Color::Red => Self::Red,
            ratatui::style::Color::Green => Self::Green,
            ratatui::style::Color::Yellow => Self::Yellow,
            ratatui::style::Color::Blue => Self::Blue,
            ratatui::style::Color::Magenta => Self::Magenta,
            ratatui::style::Color::Cyan => Self::Cyan,
            ratatui::style::Color::Gray => Self::Gray,
            ratatui::style::Color::DarkGray => Self::DarkGray,
            ratatui::style::Color::LightRed => Self::LightRed,
            ratatui::style::Color::LightGreen => Self::LightGreen,
            ratatui::style::Color::LightYellow => Self::LightYellow,
            ratatui::style::Color::LightBlue => Self::LightBlue,
            ratatui::style::Color::LightMagenta => Self::LightMagenta,
            ratatui::style::Color::LightCyan => Self::LightCyan,
            ratatui::style::Color::White => Self::White,
            ratatui::style::Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
            ratatui::style::Color::Indexed(index) => Self::Indexed(index),
        }
    }
}

impl From<Modifier> for ratatui::style::Modifier {
    fn from(modifier: Modifier) -> Self {
        Self::from_bits_truncate(modifier.bits())
    }
}

impl From<ratatui::style::Modifier> for Modifier {
    fn from(modifier: ratatui::style::Modifier) -> Self {
        Self::from_bits_truncate(modifier.bits())
    }
}

impl From<Style> for ratatui::style::Style {
    fn from(style: Style) -> Self {
        let mut out = Self::new();
        if let Some(color) = style.fg {
            out = out.fg(color.into());
        }
        if let Some(color) = style.bg {
            out = out.bg(color.into());
        }
        if let Some(color) = style.underline_color {
            out = out.underline_color(color.into());
        }
        out = out.add_modifier(style.add_modifier.into());
        out.remove_modifier(style.sub_modifier.into())
    }
}

impl From<ratatui::style::Style> for Style {
    fn from(style: ratatui::style::Style) -> Self {
        Self {
            fg: style.fg.map(Into::into),
            bg: style.bg.map(Into::into),
            underline_color: style.underline_color.map(Into::into),
            add_modifier: style.add_modifier.into(),
            sub_modifier: style.sub_modifier.into(),
        }
    }
}

impl From<Cell> for ratatui::buffer::Cell {
    fn from(cell: Cell) -> Self {
        let mut out = Self::default();
        out.set_symbol(cell.symbol());
        out.set_style(cell.style());
        out.skip = cell.skip;
        out
    }
}

impl From<Buffer> for ratatui::buffer::Buffer {
    fn from(buffer: Buffer) -> Self {
        Self {
            area: buffer.area.into(),
            content: buffer.content.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_roundtrip() {
        let rect = Rect::new(1, 2, 3, 4);
        let ratatui_rect: ratatui::layout::Rect = rect.into();
        assert_eq!(ratatui_rect, ratatui::layout::Rect::new(1, 2, 3, 4));
        assert_eq!(Rect::from(ratatui_rect), rect);
    }

    #[test]
    fn test_position_roundtrip() {
        let position = Position::new(5, 6);
        let ratatui_position: ratatui::layout::Position = position.into();
        assert_eq!(ratatui_position, ratatui::layout::Position::new(5, 6));
        assert_eq!(Position::from(ratatui_position), position);
    }

    #[test]
    fn test_color_roundtrip() {
        for color in [
            Color::Reset,
            Color::Red,
            Color::LightCyan,
            Color::Rgb(1, 2, 3),
            Color::Indexed(200),
        ] {
            let ratatui_color: ratatui::style::Color = color.into();
            assert_eq!(Color::from(ratatui_color), color);
        }
    }

    #[test]
    fn test_modifier_roundtrip() {
        let modifier = Modifier::BOLD | Modifier::UNDERLINED;
        let ratatui_modifier: ratatui::style::Modifier = modifier.into();
        assert_eq!(Modifier::from(ratatui_modifier), modifier);
    }

    #[test]
    fn test_buffer_conversion_preserves_cells() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        buffer[(0, 0)].set_symbol("A");
        buffer[(0, 0)].fg = Color::Red;
        let ratatui_buffer: ratatui::buffer::Buffer = buffer.into();
        assert_eq!(ratatui_buffer.area, ratatui::layout::Rect::new(0, 0, 2, 1));
        assert_eq!(ratatui_buffer[(0, 0)].symbol(), "A");
        assert_eq!(ratatui_buffer[(0, 0)].fg, ratatui::style::Color::Red);
    }
}
