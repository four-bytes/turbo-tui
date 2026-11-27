//! Clip-aware rendering utilities.
//!
//! These functions write text to a `Buffer` while respecting a clip rectangle.
//! Characters outside the clip region are silently skipped.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::Buffer;
use crate::core::Style;
use crate::core::{LinkId, Position, Rect};

/// Write a string to the buffer, clipping to the given rectangle.
///
/// Text is split into grapheme clusters. Clusters outside `clip` are skipped;
/// a wide cluster that would cross the clip edge is skipped entirely so it
/// cannot corrupt the neighbouring cell. The written cell keeps the cluster
/// symbol and its trailing cell(s) are reset.
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
    set_string_impl(buf, x, y, text, style, clip, None);
}

/// Write a string to the buffer as a link, clipping to the given rectangle.
///
/// Behaves like [`set_string_clipped`], but every written cell is tagged with
/// `link` via [`crate::core::Cell::set_link`]. Cells outside `clip` are not
/// written and therefore keep their previous link state.
pub fn set_string_linked(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    style: Style,
    link: LinkId,
    clip: Rect,
) {
    set_string_impl(buf, x, y, text, style, clip, Some(link));
}

fn set_string_impl(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    style: Style,
    clip: Rect,
    link: Option<LinkId>,
) {
    // Row entirely outside clip — skip everything
    if y < clip.y || y >= clip.y + clip.height {
        return;
    }

    let mut col = x;
    for grapheme in text.graphemes(true) {
        // Past right edge of clip — stop early
        if col >= clip.x + clip.width {
            break;
        }

        // Control characters are not drawn; zero-width clusters carry no cell.
        if grapheme.contains(char::is_control) {
            continue;
        }
        let width = grapheme.width();
        if width == 0 {
            continue;
        }
        let width = u16::try_from(width).unwrap_or(u16::MAX);

        // Write only clusters that fit completely inside the clip; a wide
        // grapheme that would cross the clip edge is skipped, because writing
        // half of it would corrupt the neighbouring cell.
        if col >= clip.x && col + width <= clip.x + clip.width {
            if let Some(cell) = buf.cell_mut(Position::new(col, y)) {
                cell.set_symbol(grapheme).set_style(style).set_link(link);
            }
            for offset in 1..width {
                if let Some(cell) = buf.cell_mut(Position::new(col + offset, y)) {
                    cell.reset();
                }
            }
        }

        col = col.saturating_add(width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Color;

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

    #[test]
    fn test_set_string_clipped_wide_grapheme_advances_by_display_width() {
        let clip = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        set_string_clipped(&mut buf, 0, 0, "世A", Style::default(), clip);

        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "世");
        assert_eq!(
            buf.cell(Position::new(1, 0)).unwrap().symbol(),
            " ",
            "the trailing cell of a wide grapheme must be reset"
        );
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().symbol(), "A");
    }

    #[test]
    fn test_set_string_linked_wide_grapheme_links_only_the_leading_cell() {
        let clip = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let mut links = crate::core::LinkTable::new();
        let id = links.insert(&crate::core::Link::new(crate::core::LinkTarget::Url(
            "https://example.com".to_owned(),
        )));
        buf.links = links;
        set_string_linked(&mut buf, 0, 0, "世A", Style::default(), id, clip);

        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().link(), Some(id));
        assert_eq!(
            buf.cell(Position::new(1, 0)).unwrap().link(),
            None,
            "the trailing cell must not carry the link"
        );
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().link(), Some(id));
    }

    #[test]
    fn test_set_string_clipped_wide_grapheme_crossing_the_clip_is_skipped() {
        let clip = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        set_string_clipped(&mut buf, 0, 0, "世", Style::default(), clip);

        assert_eq!(
            buf.cell(Position::new(0, 0)).unwrap().symbol(),
            " ",
            "a wide grapheme that does not fully fit the clip is not written"
        );
    }
}
