//! Canvas abstraction for view rendering.
//!
//! A [`Canvas`] is the drawing surface a view renders onto. The design keeps
//! the rendering API independent from the concrete backing store: the
//! current implementation ([`CellCanvas`]) draws into a [`Buffer`], while
//! future canvases (e.g. a sub-cell graphics canvas) can expose a different
//! [`Canvas::metric`] without changing any view code.

use crate::core::{Buffer, Position, Rect, Style};
use crate::scrollbar::Orientation;

// ============================================================================
// Semantic drawing types
// ============================================================================

/// Visual elevation of a bevel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bevel {
    /// The surface appears to rise out of the background.
    Raised,
    /// The surface appears to sink into the background.
    Recessed,
}

/// A direction used by arrow gadgets and scrollbars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Upwards.
    Up,
    /// Downwards.
    Down,
    /// Towards the left.
    Left,
    /// Towards the right.
    Right,
}

/// A small frame control rendered on the title bar or border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gadget {
    /// Close button.
    Close,
    /// Maximize/zoom button.
    Zoom,
    /// Minimize/depth button.
    Depth,
    /// Resize grip in the bottom-right corner.
    Size,
    /// Directional arrow.
    Arrow(Direction),
}

/// Interaction state of a [`Gadget`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GadgetState {
    /// Idle, not hovered or pressed.
    Normal,
    /// Currently pressed.
    Pressed,
    /// Currently hovered by the pointer.
    Hover,
}

/// The six glyphs that make up a rectangular border.
#[derive(Clone, Copy, Debug)]
pub struct BorderChars {
    /// Top-left corner.
    pub tl: char,
    /// Top-right corner.
    pub tr: char,
    /// Bottom-left corner.
    pub bl: char,
    /// Bottom-right corner.
    pub br: char,
    /// Horizontal edge.
    pub h: char,
    /// Vertical edge.
    pub v: char,
}

/// Rendering description for one [`Gadget`].
///
/// The semantic fields ([`GadgetSpec::kind`], [`GadgetSpec::state`],
/// [`GadgetSpec::rect`]) describe *what* to draw. The fields
/// [`GadgetSpec::chars`], [`GadgetSpec::len`] and [`GadgetSpec::style`] are the
/// resolved cell fallback used by the default [`Canvas::draw_gadget`]; a pixel
/// canvas ignores them and renders the semantic gadget instead.
#[derive(Clone, Copy, Debug)]
pub struct GadgetSpec {
    /// Which gadget this is.
    pub kind: Gadget,
    /// Current interaction state.
    pub state: GadgetState,
    /// Target rectangle in canvas units.
    pub rect: Rect,
    /// Cell fallback glyphs (up to eight).
    pub chars: [char; 8],
    /// Number of valid entries in [`GadgetSpec::chars`].
    pub len: u8,
    /// Cell fallback style.
    pub style: Style,
}

/// Which scrollbar element is hovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollHover {
    /// Nothing is hovered.
    None,
    /// The empty track is hovered.
    Track,
    /// The thumb is hovered.
    Thumb,
    /// An arrow button is hovered.
    Arrow,
}

/// Rendering description for a scrollbar.
///
/// The numeric fields describe the scroll state semantically; the
/// `*_style` fields are the resolved cell fallback and are ignored by pixel
/// canvases.
#[derive(Clone, Copy, Debug)]
pub struct ScrollSpec {
    /// Current scroll value.
    pub value: i32,
    /// Minimum value.
    pub min: i32,
    /// Maximum value.
    pub max: i32,
    /// Page step size.
    pub page: i32,
    /// Whether the owning window is focused.
    pub active: bool,
    /// Which element the pointer currently hovers.
    pub hover: ScrollHover,
    /// Whether the thumb is being dragged.
    pub dragging: bool,
    /// Resolved cell-fallback style for the track.
    pub track_style: Style,
    /// Resolved cell-fallback style for the thumb.
    pub thumb_style: Style,
    /// Resolved cell-fallback style for the arrows.
    pub arrows_style: Style,
}

/// Rendering description for a frame border and its title-bar gadgets.
///
/// Geometry, theme resolution and hit-testing stay in the view; this struct is
/// the resolved input to [`Canvas::draw_frame`]. The style fields are the cell
/// fallback and are ignored by pixel canvases.
#[derive(Debug)]
pub struct FrameSpec<'a> {
    /// Frame title (without the surrounding spaces).
    pub title: &'a str,
    /// Whether the owning window is focused.
    pub active: bool,
    /// Whether the owning window is being dragged or resized.
    pub dragging: bool,
    /// Resolved cell-fallback style for the border.
    pub frame_style: Style,
    /// Resolved cell-fallback style for the title.
    pub title_style: Style,
    /// Optional fill style for the entire title-bar row.
    pub title_bar_bg: Option<Style>,
    /// Border glyphs.
    pub border: BorderChars,
    /// Close button, if present.
    pub close: Option<GadgetSpec>,
    /// Minimize button, if present.
    pub minimize: Option<GadgetSpec>,
    /// Maximize button, if present.
    pub maximize: Option<GadgetSpec>,
    /// Resize grip, if present (replaces the bottom-right corner).
    pub resize_grip: Option<GadgetSpec>,
    /// First column available for the title.
    pub title_start: u16,
    /// First column occupied by right-side buttons.
    pub title_end: u16,
}

// ============================================================================
// Scrollbar glyphs (shared cell fallback)
// ============================================================================

/// Vertical scrollbar: up arrow.
pub(crate) const V_UP: char = '▲';
/// Vertical scrollbar: down arrow.
pub(crate) const V_DOWN: char = '▼';
/// Vertical scrollbar: track (empty area).
pub(crate) const V_TRACK: char = '░';
/// Vertical scrollbar: thumb.
pub(crate) const V_THUMB: char = '█';
/// Horizontal scrollbar: left arrow.
pub(crate) const H_LEFT: char = '◄';
/// Horizontal scrollbar: right arrow.
pub(crate) const H_RIGHT: char = '►';
/// Horizontal scrollbar: track (empty area).
pub(crate) const H_TRACK: char = '░';
/// Horizontal scrollbar: thumb.
pub(crate) const H_THUMB: char = '█';

/// Computes the thumb offset in track cells for a scrollbar.
///
/// `track_size` is the number of cells between the two arrow buttons. The
/// result is clamped to `0..=track_size-1`; a non-positive track size returns
/// `0`.
#[must_use]
pub(crate) fn scroll_thumb_position(value: i32, min: i32, max: i32, track_size: i32) -> i32 {
    if track_size <= 0 {
        return 0;
    }
    let range = (max - min).max(1);
    let pos = (value - min) * track_size / range;
    pos.clamp(0, track_size - 1)
}

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

    /// Draws a frame border with title and gadgets.
    ///
    /// The default implementation is a pure cell renderer driven by the
    /// resolved [`FrameSpec`]. Every write is clipped to `clip` exactly as the
    /// caller passes it (never an intersected draw area). Pixel canvases
    /// override this to render the semantic frame.
    fn draw_frame(&mut self, rect: Rect, spec: &FrameSpec<'_>, clip: Rect) {
        if rect.width < 2 || rect.height < 1 {
            return;
        }
        let draw_area = rect.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        // Optional title-bar background fills the whole top row.
        if let Some(bg) = spec.title_bar_bg {
            for col in (rect.x + 1)..(rect.x + rect.width - 1) {
                self.put_char(col, rect.y, ' ', bg, clip);
            }
        }

        // Top corners.
        self.put_char(rect.x, rect.y, spec.border.tl, spec.frame_style, clip);
        self.put_char(
            rect.x + rect.width - 1,
            rect.y,
            spec.border.tr,
            spec.frame_style,
            clip,
        );

        // Top horizontal edge.
        for col in (rect.x + 1)..(rect.x + rect.width - 1) {
            self.put_char(col, rect.y, spec.border.h, spec.frame_style, clip);
        }

        // Title-bar gadgets.
        if let Some(gadget) = &spec.close {
            self.draw_gadget(gadget, clip);
        }
        if let Some(gadget) = &spec.minimize {
            self.draw_gadget(gadget, clip);
        }
        if let Some(gadget) = &spec.maximize {
            self.draw_gadget(gadget, clip);
        }

        // Title, centered within the full frame width and clipped to the tray.
        if !spec.title.is_empty() {
            let title_full = format!(" {} ", spec.title);
            let title_chars: Vec<char> = title_full.chars().collect();
            #[allow(clippy::cast_possible_truncation)]
            let title_len = title_chars.len() as u16;
            let available = spec.title_end.saturating_sub(spec.title_start);

            if available >= 2 {
                let ideal_start = rect.x + (rect.width.saturating_sub(title_len)) / 2;
                let clamped_start = ideal_start.max(spec.title_start);

                let vis_start = clamped_start;
                let vis_end = (clamped_start + title_len).min(spec.title_end);

                if vis_end > vis_start {
                    #[allow(clippy::cast_possible_truncation)]
                    let vis_count = (vis_end - vis_start) as usize;

                    let truncated_right = (clamped_start + title_len) > spec.title_end;

                    for (i, &ch) in title_chars.iter().take(vis_count).enumerate() {
                        #[allow(clippy::cast_possible_truncation)]
                        let col = vis_start + i as u16;
                        let draw_ch = if truncated_right && i == vis_count - 1 {
                            '…'
                        } else {
                            ch
                        };
                        self.put_char(col, rect.y, draw_ch, spec.title_style, clip);
                    }
                }
            }
        }

        if rect.height >= 2 {
            // Side borders (both columns). A visible scrollbar is drawn later
            // by the view and overwrites the right column cell-for-cell.
            for row in (rect.y + 1)..(rect.y + rect.height - 1) {
                self.put_char(rect.x, row, spec.border.v, spec.frame_style, clip);
                self.put_char(
                    rect.x + rect.width - 1,
                    row,
                    spec.border.v,
                    spec.frame_style,
                    clip,
                );
            }

            // Bottom corners and horizontal edge.
            let bottom = rect.y + rect.height - 1;
            self.put_char(rect.x, bottom, spec.border.bl, spec.frame_style, clip);
            for col in (rect.x + 1)..(rect.x + rect.width - 1) {
                self.put_char(col, bottom, spec.border.h, spec.frame_style, clip);
            }

            // Bottom-right corner, replaced by the resize grip when present.
            if let Some(gadget) = &spec.resize_grip {
                self.draw_gadget(gadget, clip);
            } else {
                self.put_char(
                    rect.x + rect.width - 1,
                    bottom,
                    spec.border.br,
                    spec.frame_style,
                    clip,
                );
            }
        }
    }

    /// Draws a bevel.
    ///
    /// Cell-based canvases have no three-dimensional elevation and encode it in
    /// styles and text instead, so the default implementation is a no-op that
    /// keeps existing cell output byte-identical. Pixel canvases override this
    /// to draw actual shading.
    fn draw_bevel(&mut self, _rect: Rect, _bevel: Bevel, _clip: Rect) {}

    /// Draws a single [`Gadget`] from its resolved cell fallback.
    ///
    /// The default implementation writes `spec.chars[0..spec.len]` starting at
    /// `spec.rect.x`, all on `spec.rect.y`, with `spec.style`, clipped to
    /// `clip`.
    fn draw_gadget(&mut self, spec: &GadgetSpec, clip: Rect) {
        for (i, &ch) in spec.chars.iter().take(spec.len as usize).enumerate() {
            let col = spec.rect.x + u16::try_from(i).unwrap_or(0);
            self.put_char(col, spec.rect.y, ch, spec.style, clip);
        }
    }

    /// Draws a scrollbar.
    ///
    /// The default implementation is a pure cell renderer driven by the
    /// resolved [`ScrollSpec`]: the first and last cells become arrow buttons,
    /// the cell at the thumb offset is the thumb and everything else is track.
    /// Empty rectangles are ignored.
    fn draw_scrollbar(
        &mut self,
        rect: Rect,
        orientation: Orientation,
        spec: &ScrollSpec,
        clip: Rect,
    ) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let draw_area = rect.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let track_size = match orientation {
            Orientation::Vertical => i32::from(rect.height.saturating_sub(2)),
            Orientation::Horizontal => i32::from(rect.width.saturating_sub(2)),
        };
        let thumb_pos = usize::try_from(scroll_thumb_position(
            spec.value, spec.min, spec.max, track_size,
        ))
        .unwrap_or(0);

        match orientation {
            Orientation::Vertical => {
                let height = rect.height as usize;
                for row in 0..height {
                    let buf_row = rect.y + u16::try_from(row).unwrap_or(0);
                    let (ch, style) = if row == 0 {
                        (V_UP, spec.arrows_style)
                    } else if row >= height.saturating_sub(1) {
                        (V_DOWN, spec.arrows_style)
                    } else if row - 1 == thumb_pos {
                        (V_THUMB, spec.thumb_style)
                    } else {
                        (V_TRACK, spec.track_style)
                    };
                    self.put_char(rect.x, buf_row, ch, style, clip);
                }
            }
            Orientation::Horizontal => {
                let width = rect.width as usize;
                for col in 0..width {
                    let buf_col = rect.x + u16::try_from(col).unwrap_or(0);
                    let (ch, style) = if col == 0 {
                        (H_LEFT, spec.arrows_style)
                    } else if col >= width.saturating_sub(1) {
                        (H_RIGHT, spec.arrows_style)
                    } else if col - 1 == thumb_pos {
                        (H_THUMB, spec.thumb_style)
                    } else {
                        (H_TRACK, spec.track_style)
                    };
                    self.put_char(buf_col, rect.y, ch, style, clip);
                }
            }
        }
    }
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

    fn sample_scroll_spec(value: i32) -> ScrollSpec {
        ScrollSpec {
            value,
            min: 0,
            max: 100,
            page: 1,
            active: true,
            hover: ScrollHover::None,
            dragging: false,
            track_style: Style::default(),
            thumb_style: Style::default(),
            arrows_style: Style::default(),
        }
    }

    #[test]
    fn test_draw_gadget_writes_glyphs_and_respects_clip() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
        let mut chars = ['\0'; 8];
        chars[0] = 'A';
        chars[1] = 'B';
        chars[2] = 'C';
        let spec = GadgetSpec {
            kind: Gadget::Close,
            state: GadgetState::Normal,
            rect: Rect::new(2, 1, 3, 1),
            chars,
            len: 3,
            style: Style::default(),
        };
        {
            let mut canvas = CellCanvas::new(&mut buf);
            // Clip covers only the first two gadget cells.
            canvas.draw_gadget(&spec, Rect::new(2, 1, 2, 1));
        }

        assert_eq!(buf.cell(Position::new(2, 1)).unwrap().symbol(), "A");
        assert_eq!(buf.cell(Position::new(3, 1)).unwrap().symbol(), "B");
        assert_eq!(buf.cell(Position::new(4, 1)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_draw_scrollbar_vertical_arrows_and_thumb() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 6));
        let spec = sample_scroll_spec(50);
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_scrollbar(
                Rect::new(0, 0, 1, 6),
                Orientation::Vertical,
                &spec,
                Rect::new(0, 0, 1, 6),
            );
        }

        // track_size = 4, value 50 → 50 * 4 / 100 = 2 → thumb at row 1 + 2.
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "▲");
        assert_eq!(buf.cell(Position::new(0, 5)).unwrap().symbol(), "▼");
        assert_eq!(buf.cell(Position::new(0, 1)).unwrap().symbol(), "░");
        assert_eq!(buf.cell(Position::new(0, 3)).unwrap().symbol(), "█");
    }

    #[test]
    fn test_draw_scrollbar_horizontal_arrows_and_thumb() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 1));
        let spec = sample_scroll_spec(50);
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_scrollbar(
                Rect::new(0, 0, 6, 1),
                Orientation::Horizontal,
                &spec,
                Rect::new(0, 0, 6, 1),
            );
        }

        // track_size = 4, value 50 → thumb at column 1 + 2.
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "◄");
        assert_eq!(buf.cell(Position::new(5, 0)).unwrap().symbol(), "►");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "█");
    }

    #[test]
    fn test_draw_primitives_empty_and_tiny_rect_do_not_panic() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        let spec = sample_scroll_spec(0);
        let border = BorderChars {
            tl: '┌',
            tr: '┐',
            bl: '└',
            br: '┘',
            h: '─',
            v: '│',
        };
        let frame_spec = FrameSpec {
            title: "",
            active: false,
            dragging: false,
            frame_style: Style::default(),
            title_style: Style::default(),
            title_bar_bg: None,
            border,
            close: None,
            minimize: None,
            maximize: None,
            resize_grip: None,
            title_start: 0,
            title_end: 0,
        };
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_scrollbar(
                Rect::new(0, 0, 0, 0),
                Orientation::Vertical,
                &spec,
                Rect::new(0, 0, 4, 2),
            );
            canvas.draw_scrollbar(
                Rect::new(0, 0, 1, 1),
                Orientation::Vertical,
                &spec,
                Rect::new(0, 0, 4, 2),
            );
            // Width < 2 returns early.
            canvas.draw_frame(Rect::new(0, 0, 1, 1), &frame_spec, Rect::new(0, 0, 4, 2));
            canvas.draw_bevel(Rect::new(0, 0, 4, 2), Bevel::Raised, Rect::new(0, 0, 4, 2));
        }

        // A one-cell scrollbar renders only its start arrow.
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "▲");
        // The bevel is a no-op and the too-narrow frame draws nothing here.
        assert_eq!(buf.cell(Position::new(0, 1)).unwrap().symbol(), " ");
    }

    #[test]
    fn test_draw_frame_height_one_draws_only_top_row() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 3));
        let border = BorderChars {
            tl: '┌',
            tr: '┐',
            bl: '└',
            br: '┘',
            h: '─',
            v: '│',
        };
        let spec = FrameSpec {
            title: "",
            active: true,
            dragging: false,
            frame_style: Style::default(),
            title_style: Style::default(),
            title_bar_bg: None,
            border,
            close: None,
            minimize: None,
            maximize: None,
            resize_grip: None,
            title_start: 0,
            title_end: 0,
        };
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_frame(Rect::new(0, 0, 4, 1), &spec, Rect::new(0, 0, 6, 3));
        }

        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "┌");
        assert_eq!(buf.cell(Position::new(1, 0)).unwrap().symbol(), "─");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "┐");
        // Rows below the single-row frame stay untouched.
        assert_eq!(buf.cell(Position::new(0, 1)).unwrap().symbol(), " ");
    }
}
