//! Canvas abstraction for view rendering.
//!
//! A [`Canvas`] is the drawing surface a view renders onto. The design keeps
//! the rendering API independent from the concrete backing store: the
//! current implementation ([`CellCanvas`]) draws into a [`Buffer`], while
//! future canvases (e.g. a sub-cell graphics canvas) can expose a different
//! [`Canvas::metric`] without changing any view code.

use crate::core::{Buffer, Link, Position, Rect, Style};

// ============================================================================
// Semantic drawing types
// ============================================================================
//
// These types are deliberately neutral: they contain no theme, style or
// cell-encoding data, so they can move to a shared crate without an API break.
// All character/theme behaviour lives in the crate-internal `cell_skin` module.

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

/// Semantic gadget state.
///
/// `Inactive` and `Dragging` are neutral UI states needed to reproduce the
/// inactive-window and drag-time cell styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GadgetState {
    /// The owning window is not focused.
    Inactive,
    /// Idle and focused.
    Normal,
    /// Currently hovered by the pointer.
    Hover,
    /// Currently pressed.
    Pressed,
    /// The owning window is being dragged or resized.
    Dragging,
}

/// Orientation of a scrollbar (or another one-dimensional element).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Vertical orientation.
    Vertical,
    /// Horizontal orientation.
    Horizontal,
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

/// The kind of frame to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// Regular overlapping window.
    Window,
    /// Modal dialog.
    Dialog,
    /// Single-line group/panel frame.
    Single,
}

/// Presence and state of the interactive frame gadgets.
///
/// `None` means the gadget is not shown at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameGadgets {
    /// Close button.
    pub close: Option<GadgetState>,
    /// Minimize button.
    pub minimize: Option<GadgetState>,
    /// Maximize/zoom button.
    pub maximize: Option<GadgetState>,
    /// Resize grip in the bottom-right corner.
    pub resize_grip: Option<GadgetState>,
}

/// Semantic description of a frame border, title and gadgets.
///
/// Geometry, hit-testing and theme resolution stay in the view; this is the
/// neutral input to [`Canvas::draw_frame`].
#[derive(Debug)]
pub struct FrameSpec<'a> {
    /// Frame title (without the surrounding spaces).
    pub title: &'a str,
    /// Frame kind.
    pub kind: FrameKind,
    /// Whether the owning window is focused.
    pub active: bool,
    /// Whether the owning window is being dragged or resized.
    pub dragging: bool,
    /// Presence and state of the frame gadgets.
    pub gadgets: FrameGadgets,
}

/// Semantic description of a scrollbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollSpec {
    /// Offset from the minimum (`value - min`).
    pub position: i32,
    /// Scroll range (`max - min`), at least zero.
    pub total: i32,
    /// Page/visible size; semantic only, not used by the cell fallback.
    pub visible: i32,
    /// Whether the owning window is focused.
    pub active: bool,
    /// Which element the pointer currently hovers.
    pub hover: ScrollHover,
    /// Whether the owning window is being dragged.
    pub dragging: bool,
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

    /// Draws `text` as a link, clipped to `clip`.
    ///
    /// The default implementation draws plain styled text and ignores `link`;
    /// canvases that can carry link ids override it.
    fn draw_text_link(
        &mut self,
        x: u16,
        y: u16,
        text: &str,
        style: Style,
        link: &Link,
        clip: Rect,
    ) {
        let _ = link;
        self.draw_text(x, y, text, style, clip);
    }

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

    /// Draws a frame border, title and gadgets.
    ///
    /// The default implementation delegates to the crate-internal cell skin,
    /// which reproduces the historical character output byte-for-byte. Pixel
    /// canvases override this to render semantically.
    fn draw_frame(&mut self, rect: Rect, spec: &FrameSpec<'_>, clip: Rect) {
        crate::cell_skin::draw_frame(self, rect, spec, clip);
    }

    /// Draws a bevel.
    ///
    /// Cell-based canvases have no three-dimensional elevation and encode it in
    /// styles and text instead, so the default implementation is a no-op that
    /// keeps existing cell output byte-identical. Pixel canvases override this
    /// to draw actual shading.
    fn draw_bevel(&mut self, _rect: Rect, _bevel: Bevel, _clip: Rect) {}

    /// Draws a single [`Gadget`] at `rect` in `state`.
    ///
    /// The default implementation delegates to the crate-internal cell skin,
    /// which resolves the glyphs and style from the current theme and clips
    /// every write to `clip`.
    fn draw_gadget(&mut self, rect: Rect, gadget: Gadget, state: GadgetState, clip: Rect) {
        crate::cell_skin::draw_gadget(self, rect, gadget, state, clip);
    }

    /// Draws a scrollbar.
    ///
    /// The default implementation delegates to the crate-internal cell skin:
    /// the first and last cells become arrow buttons, the cell at the thumb
    /// offset is the thumb and everything else is track. Empty rectangles are
    /// ignored.
    fn draw_scrollbar(
        &mut self,
        rect: Rect,
        orientation: Orientation,
        spec: &ScrollSpec,
        clip: Rect,
    ) {
        crate::cell_skin::draw_scrollbar(self, rect, orientation, spec, clip);
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

    fn draw_text_link(
        &mut self,
        x: u16,
        y: u16,
        text: &str,
        style: Style,
        link: &Link,
        clip: Rect,
    ) {
        let id = self.buffer.links.insert(link);
        crate::clip::set_string_linked(&mut *self.buffer, x, y, text, style, id, clip);
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

    fn sample_scroll_spec(position: i32) -> ScrollSpec {
        ScrollSpec {
            position,
            total: 100,
            visible: 1,
            active: true,
            hover: ScrollHover::None,
            dragging: false,
        }
    }

    fn setup_default_theme() {
        crate::theme::set(crate::theme::Theme::turbo_vision());
    }

    fn no_gadgets() -> FrameGadgets {
        FrameGadgets {
            close: None,
            minimize: None,
            maximize: None,
            resize_grip: None,
        }
    }

    #[test]
    fn test_draw_gadget_writes_glyphs_and_respects_clip() {
        setup_default_theme();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
        {
            let mut canvas = CellCanvas::new(&mut buf);
            // Default close text is "[■]"; clip covers only the first two cells.
            canvas.draw_gadget(
                Rect::new(2, 1, 3, 1),
                Gadget::Close,
                GadgetState::Normal,
                Rect::new(2, 1, 2, 1),
            );
        }

        assert_eq!(buf.cell(Position::new(2, 1)).unwrap().symbol(), "[");
        assert_eq!(buf.cell(Position::new(3, 1)).unwrap().symbol(), "■");
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

        // track_size = 4, position 50 → 50 * 4 / 100 = 2 → thumb at row 1 + 2.
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

        // track_size = 4, position 50 → thumb at column 1 + 2.
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "◄");
        assert_eq!(buf.cell(Position::new(5, 0)).unwrap().symbol(), "►");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "█");
    }

    #[test]
    fn test_draw_primitives_empty_and_tiny_rect_do_not_panic() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        let spec = sample_scroll_spec(0);
        let frame_spec = FrameSpec {
            title: "",
            kind: FrameKind::Dialog,
            active: false,
            dragging: false,
            gadgets: no_gadgets(),
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
        setup_default_theme();
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 3));
        let spec = FrameSpec {
            title: "",
            kind: FrameKind::Single,
            active: true,
            dragging: false,
            gadgets: no_gadgets(),
        };
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_frame(Rect::new(0, 0, 4, 1), &spec, Rect::new(0, 0, 6, 3));
        }

        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().symbol(), "╔");
        assert_eq!(buf.cell(Position::new(1, 0)).unwrap().symbol(), "═");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "╗");
        // Rows below the single-row frame stay untouched.
        assert_eq!(buf.cell(Position::new(0, 1)).unwrap().symbol(), " ");
    }

    fn url_link(url: &str) -> Link {
        Link::new(crate::core::LinkTarget::Url(url.to_owned()))
    }

    #[test]
    fn test_draw_text_link_tags_cells_inside_clip() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        let link = url_link("https://example.com");
        {
            let mut canvas = CellCanvas::new(&mut buf);
            let clip = Rect::new(2, 1, 3, 1);
            canvas.draw_text_link(1, 1, "Hello", Style::default(), &link, clip);
        }

        // Cells outside the clip stay unlinked.
        assert_eq!(buf.cell(Position::new(1, 1)).unwrap().link(), None);
        assert_eq!(buf.cell(Position::new(5, 1)).unwrap().link(), None);
        // Cells inside the clip carry the id and resolve to the link.
        let id = buf.cell(Position::new(2, 1)).unwrap().link().unwrap();
        assert_eq!(buf.cell(Position::new(3, 1)).unwrap().link(), Some(id));
        assert_eq!(buf.cell(Position::new(4, 1)).unwrap().link(), Some(id));
        assert_eq!(buf.links.get(id), Some(&link));
    }

    #[test]
    fn test_draw_text_clears_link_after_linked_write() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let link = url_link("https://example.com");
        {
            let mut canvas = CellCanvas::new(&mut buf);
            let clip = Rect::new(0, 0, 10, 1);
            canvas.draw_text_link(0, 0, "abc", Style::default(), &link, clip);
        }
        assert!(buf.cell(Position::new(0, 0)).unwrap().link().is_some());
        {
            let mut canvas = CellCanvas::new(&mut buf);
            canvas.draw_text(0, 0, "abc", Style::default(), Rect::new(0, 0, 10, 1));
        }
        assert_eq!(buf.cell(Position::new(0, 0)).unwrap().link(), None);
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().link(), None);
    }

    /// Minimal canvas that records `draw_text` calls to check the default
    /// `draw_text_link` delegation.
    struct RecordingCanvas {
        drawn: Vec<(u16, u16, String)>,
    }

    impl Canvas for RecordingCanvas {
        fn area(&self) -> Rect {
            Rect::new(0, 0, 20, 5)
        }

        fn draw_text(&mut self, x: u16, y: u16, text: &str, _style: Style, _clip: Rect) {
            self.drawn.push((x, y, text.to_owned()));
        }

        fn put_char(&mut self, _x: u16, _y: u16, _ch: char, _style: Style, _clip: Rect) {}

        fn put_symbol(&mut self, _x: u16, _y: u16, _symbol: &str, _style: Style, _clip: Rect) {}

        fn fill(&mut self, _rect: Rect, _ch: char, _style: Style) {}

        fn set_style(&mut self, _rect: Rect, _style: Style) {}

        fn set_cursor(&mut self, _pos: Option<Position>) {}
    }

    #[test]
    fn test_default_draw_text_link_delegates_to_draw_text() {
        let mut canvas = RecordingCanvas { drawn: Vec::new() };
        let link = url_link("https://example.com");
        canvas.draw_text_link(
            1,
            2,
            "hello",
            Style::default(),
            &link,
            Rect::new(0, 0, 20, 5),
        );
        assert_eq!(canvas.drawn, vec![(1, 2, "hello".to_owned())]);
    }
}
