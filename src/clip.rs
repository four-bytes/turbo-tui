//! Clip-aware rendering utilities.
//!
//! These functions write text to a `Buffer` while respecting a clip rectangle.
//! Characters outside the clip region are silently skipped.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

/// Write a string to the buffer, clipping to the given rectangle.
///
/// Characters at positions outside `clip` are silently skipped.
/// This replaces direct `buf.set_string()` calls in widgets that must
/// respect the parent's clip region.
///
/// # Arguments
///
/// * `buf` — Target buffer.
/// * `x` — Starting column (absolute).
/// * `y` — Row (absolute).
/// * `text` — Text to write.
/// * `style` — Style for each character.
/// * `clip` — Clip rectangle — only cells within this rect are written.
pub fn set_string_clipped(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, clip: Rect) {
    // Row entirely outside clip — skip everything
    if y < clip.y || y >= clip.y + clip.height {
        return;
    }

    let mut col = x;
    for ch in text.chars() {
        // Past right edge of clip — stop early
        if col >= clip.x + clip.width {
            break;
        }
        // Only write if within clip's horizontal range
        if col >= clip.x {
            if let Some(cell) = buf.cell_mut(Position::new(col, y)) {
                cell.set_char(ch).set_style(style);
            }
        }
        col = col.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_set_string_clipped_full_visible() {
        let clip = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        set_string_clipped(&mut buf, 2, 1, "Hello", Style::default(), clip);
        // Verify each character
        assert_eq!(buf.cell(Position::new(2, 1)).unwrap().symbol(), "H");
        assert_eq!(buf.cell(Position::new(3, 1)).unwrap().symbol(), "e");
        assert_eq!(buf.cell(Position::new(4, 1)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(5, 1)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(6, 1)).unwrap().symbol(), "o");
    }

    #[test]
    fn test_set_string_clipped_right_edge() {
        // Clip rect only covers columns 0..5, text starts at col 3
        let clip = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        set_string_clipped(&mut buf, 3, 0, "Hello", Style::default(), clip);
        // Only "He" should be written (cols 3,4), "llo" is clipped
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "H");
        assert_eq!(buf.cell(Position::new(4, 0)).unwrap().symbol(), "e");
        // Col 5 should still be empty (space)
        assert_eq!(buf.cell(Position::new(5, 0)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_set_string_clipped_left_edge() {
        // Clip rect starts at column 3, text starts at col 1
        let clip = Rect::new(3, 0, 10, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 15, 1));
        set_string_clipped(&mut buf, 1, 0, "Hello", Style::default(), clip);
        // "He" is at cols 1,2 — outside clip, skipped
        // "llo" is at cols 3,4,5 — inside clip, written
        assert_eq!(buf.cell(Position::new(1, 0)).unwrap().symbol(), " "); // clipped
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().symbol(), " "); // clipped
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(4, 0)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(5, 0)).unwrap().symbol(), "o");
    }

    #[test]
    fn test_set_string_clipped_row_outside() {
        let clip = Rect::new(0, 2, 20, 3); // rows 2..5
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        set_string_clipped(&mut buf, 0, 0, "Hidden", Style::default(), clip);
        // Row 0 is outside clip — nothing written
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_set_string_clipped_style_applied() {
        let clip = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let style = Style::default().fg(Color::Red);
        set_string_clipped(&mut buf, 0, 0, "A", style, clip);
        let cell = buf.cell(Position::new(0, 0)).unwrap();
        assert_eq!(cell.symbol(), "A");
        assert_eq!(cell.fg, Color::Red);
    }

    #[test]
    fn test_set_string_clipped_empty_string() {
        let clip = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        set_string_clipped(&mut buf, 0, 0, "", Style::default(), clip);
        // Should not panic, nothing written
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_set_string_clipped_both_edges() {
        // Clip rect is cols 2..5 (width 3), text "Hello" starts at col 1
        let clip = Rect::new(2, 0, 3, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        set_string_clipped(&mut buf, 1, 0, "Hello", Style::default(), clip);
        // col 1: 'H' — outside clip (left)
        // col 2: 'e' — inside clip
        // col 3: 'l' — inside clip
        // col 4: 'l' — inside clip
        // col 5: 'o' — outside clip (right, clip.x+clip.width = 5)
        assert_eq!(buf.cell(Position::new(1, 0)).unwrap().symbol(), " ");
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().symbol(), "e");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(4, 0)).unwrap().symbol(), "l");
        assert_eq!(buf.cell(Position::new(5, 0)).unwrap().symbol(), " ");
    }
}
