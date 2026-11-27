//! Cell-based fallback rendering for the semantic [`Canvas`] primitives.
//!
//! This module owns every theme lookup and every character glyph used by the
//! primitive defaults. It is deliberately crate-internal: the public primitive
//! types in [`crate::canvas`] stay free of theme and cell-encoding details so
//! they can later move into a shared crate without an API break.

use crate::canvas::{
    Canvas, Direction, FrameKind, FrameSpec, Gadget, GadgetState, Orientation, ScrollHover,
    ScrollSpec,
};
use crate::core::{Color, Rect, Style};
use crate::frame::FrameHover;
use crate::theme::{self, ButtonSide, Theme};

// ============================================================================
// Scrollbar glyphs (cell fallback)
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

// ============================================================================
// ButtonTray — single source of truth for title-bar button positions
// ============================================================================

/// Computed button layout for the title bar.
///
/// Built once from theme config and the presence of gadgets, then used for
/// hit-testing, drawing, and title clamping. All positions are absolute screen
/// coordinates.
pub(crate) struct ButtonTray {
    /// Close button position: `(start_col, char_count)`. `None` if not present.
    pub(crate) close: Option<(u16, u16)>,
    /// Minimize button position. `None` if not present or no text.
    pub(crate) minimize: Option<(u16, u16)>,
    /// Maximize button position. `None` if not present or no text.
    pub(crate) maximize: Option<(u16, u16)>,
    /// First column available for the title (after all left-side buttons).
    pub(crate) title_start: u16,
    /// First column occupied by right-side buttons (title must stop before this).
    pub(crate) title_end: u16,
}

impl ButtonTray {
    /// Build the button tray from gadget presence and theme.
    ///
    /// Layout rules:
    /// - Each side (left/right) has buttons stacked from the edge inward
    /// - Left side: buttons go left-to-right starting at `x + margin_left`
    /// - Right side: buttons go right-to-left starting at `x + width - margin_right`
    /// - Close button is placed closest to its configured corner
    /// - Controls (minimize, maximize) stack after close on their side
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn build(
        b: Rect,
        closeable: bool,
        minimizable: bool,
        maximizable: bool,
        t: &Theme,
    ) -> Self {
        let close_len: u16 = if closeable {
            t.close_button_text.chars().count() as u16
        } else {
            0
        };
        let min_len: u16 = if minimizable && !t.minimize_button_text.is_empty() {
            t.minimize_button_text.chars().count() as u16
        } else {
            0
        };
        let max_len: u16 = if maximizable && !t.maximize_button_text.is_empty() {
            t.maximize_button_text.chars().count() as u16
        } else {
            0
        };

        // Left cursor starts after left corner + margin
        let mut left_cursor = b.x + t.button_margin_left;
        // Right cursor starts before right corner - margin (first usable col from right)
        let mut right_cursor = b.x + b.width.saturating_sub(t.button_margin_right);

        let mut close_pos: Option<(u16, u16)> = None;
        let mut min_pos: Option<(u16, u16)> = None;
        let mut max_pos: Option<(u16, u16)> = None;

        // Place close button (it goes on close_button_side, closest to corner)
        if closeable && close_len > 0 {
            match t.close_button_side {
                ButtonSide::Left => {
                    close_pos = Some((left_cursor, close_len));
                    left_cursor += close_len;
                }
                ButtonSide::Right => {
                    right_cursor = right_cursor.saturating_sub(close_len);
                    close_pos = Some((right_cursor, close_len));
                }
            }
        }

        // Place controls (minimize, then maximize) on controls_side.
        // Right side order (rightmost = closest to corner):
        //   ... [minimize][maximize] [close] margin |
        //   cursor moves leftward: close first, then max, then min
        //
        // Left side order (leftmost = closest to corner):
        //   | margin [close] [minimize][maximize] ...
        //   cursor moves rightward: close first, then min, then max
        match t.controls_side {
            ButtonSide::Left => {
                if min_len > 0 {
                    min_pos = Some((left_cursor, min_len));
                    left_cursor += min_len;
                }
                if max_len > 0 {
                    max_pos = Some((left_cursor, max_len));
                    left_cursor += max_len;
                }
            }
            ButtonSide::Right => {
                // Place maximize first (closer to corner), then minimize
                if max_len > 0 {
                    right_cursor = right_cursor.saturating_sub(max_len);
                    max_pos = Some((right_cursor, max_len));
                }
                if min_len > 0 {
                    right_cursor = right_cursor.saturating_sub(min_len);
                    min_pos = Some((right_cursor, min_len));
                }
            }
        }

        Self {
            close: close_pos,
            minimize: min_pos,
            maximize: max_pos,
            title_start: left_cursor,
            title_end: right_cursor,
        }
    }

    /// Hit-test: which button (if any) is at the given column?
    ///
    /// Only checks column — caller must verify row == title bar row.
    pub(crate) fn hit_test(&self, col: u16) -> FrameHover {
        if let Some((start, len)) = self.close {
            if col >= start && col < start + len {
                return FrameHover::CloseButton;
            }
        }
        if let Some((start, len)) = self.minimize {
            if col >= start && col < start + len {
                return FrameHover::MinimizeButton;
            }
        }
        if let Some((start, len)) = self.maximize {
            if col >= start && col < start + len {
                return FrameHover::MaximizeButton;
            }
        }
        FrameHover::None
    }
}

// ============================================================================
// Shared helpers
// ============================================================================

/// Computes the thumb offset in track cells for a scrollbar.
///
/// `position` is `value - min`, `total` is `max - min`, and `track_size` is the
/// number of cells between the two arrow buttons. The result is clamped to
/// `0..=track_size-1`; a non-positive track size returns `0`.
#[must_use]
pub(crate) fn scroll_thumb_position(position: i32, total: i32, track_size: i32) -> i32 {
    if track_size <= 0 {
        return 0;
    }
    let range = total.max(1);
    let pos = position * track_size / range;
    pos.clamp(0, track_size - 1)
}

/// The style used for everything on a window while it is being dragged/resized.
fn dragging_style(t: &Theme) -> Style {
    Style::default()
        .fg(t.window_frame_dragging.fg.unwrap_or(Color::White))
        .bg(t.window_frame_dragging.bg.unwrap_or(Color::Black))
}

/// Resolves the border and title styles for a frame.
fn frame_styles(t: &Theme, kind: FrameKind, active: bool, dragging: bool) -> (Style, Style) {
    match kind {
        FrameKind::Window => {
            if dragging {
                // Title inherits the dragging frame's background.
                let title_during_drag = Style::default()
                    .fg(t.window_title_active.fg.unwrap_or(Color::White))
                    .bg(t.window_frame_dragging.bg.unwrap_or(Color::Black))
                    .add_modifier(t.window_title_active.add_modifier);
                (t.window_frame_dragging, title_during_drag)
            } else if active {
                (t.window_frame_active, t.window_title_active)
            } else {
                (t.window_frame_inactive, t.window_title_inactive)
            }
        }
        FrameKind::Dialog => (t.dialog_frame, t.dialog_title),
        FrameKind::Single => (t.single_frame, t.single_frame),
    }
}

/// Maps a gadget state to the matching theme style.
///
/// Cell mode has no pressed relief, so [`GadgetState::Pressed`] uses the hover
/// style and [`GadgetState::Dragging`] uses the window-dragging style.
fn gadget_style(
    state: GadgetState,
    inactive: Style,
    normal: Style,
    hover: Style,
    t: &Theme,
) -> Style {
    match state {
        GadgetState::Dragging => dragging_style(t),
        GadgetState::Hover | GadgetState::Pressed => hover,
        GadgetState::Normal => normal,
        GadgetState::Inactive => inactive,
    }
}

/// Resolves the glyphs and style for one gadget.
fn gadget_cell(t: &Theme, gadget: Gadget, state: GadgetState) -> ([char; 8], usize, Style) {
    let mut chars = ['\0'; 8];
    let mut len = 0usize;
    let mut push = |ch: char| {
        if len < 8 {
            chars[len] = ch;
            len += 1;
        }
    };

    let style = match gadget {
        Gadget::Close => {
            for ch in t.close_button_text.chars().take(8) {
                push(ch);
            }
            gadget_style(
                state,
                t.window_close_button_inactive,
                t.window_close_button,
                t.window_close_button_hover,
                t,
            )
        }
        Gadget::Zoom => {
            for ch in t.maximize_button_text.chars().take(8) {
                push(ch);
            }
            gadget_style(
                state,
                t.window_maximize_button_inactive,
                t.window_maximize_button,
                t.window_maximize_button_hover,
                t,
            )
        }
        Gadget::Depth => {
            for ch in t.minimize_button_text.chars().take(8) {
                push(ch);
            }
            gadget_style(
                state,
                t.window_minimize_button_inactive,
                t.window_minimize_button,
                t.window_minimize_button_hover,
                t,
            )
        }
        Gadget::Size => {
            push(t.resize_grip_char);
            gadget_style(
                state,
                t.window_resize_handle_inactive,
                t.window_resize_handle,
                t.window_resize_handle_hover,
                t,
            )
        }
        Gadget::Arrow(direction) => {
            push(match direction {
                Direction::Up => V_UP,
                Direction::Down => V_DOWN,
                Direction::Left => H_LEFT,
                Direction::Right => H_RIGHT,
            });
            gadget_style(
                state,
                t.scrollbar_arrows_inactive,
                t.scrollbar_arrows,
                t.scrollbar_arrows_hover,
                t,
            )
        }
    };

    (chars, len, style)
}

// ============================================================================
// Semantic primitive defaults
// ============================================================================

/// Owned paint data resolved during one theme access.
struct FramePaint {
    frame_style: Style,
    title_style: Style,
    title_bar_bg: Option<Style>,
    tl: char,
    tr: char,
    bl: char,
    br: char,
    h: char,
    v: char,
    tray: ButtonTray,
}

/// Cell fallback for [`Canvas::draw_frame`].
#[allow(clippy::too_many_lines)]
pub(crate) fn draw_frame<C: Canvas + ?Sized>(
    canvas: &mut C,
    rect: Rect,
    spec: &FrameSpec<'_>,
    clip: Rect,
) {
    if rect.width < 2 || rect.height < 1 {
        return;
    }
    let draw_area = rect.intersection(clip);
    if draw_area.width == 0 || draw_area.height == 0 {
        return;
    }

    // Resolve styles, border glyphs and the button tray in one theme access.
    let paint = theme::with_current(|t| {
        let (frame_style, title_style) = frame_styles(t, spec.kind, spec.active, spec.dragging);
        let tray = ButtonTray::build(
            rect,
            spec.gadgets.close.is_some(),
            spec.gadgets.minimize.is_some(),
            spec.gadgets.maximize.is_some(),
            t,
        );
        FramePaint {
            frame_style,
            title_style,
            title_bar_bg: t.title_bar_bg,
            tl: t.border_tl,
            tr: t.border_tr,
            bl: t.border_bl,
            br: t.border_br,
            h: t.border_h,
            v: t.border_v,
            tray,
        }
    });

    // Optional title-bar background fills the whole top row.
    if let Some(bg) = paint.title_bar_bg {
        for col in (rect.x + 1)..(rect.x + rect.width - 1) {
            canvas.put_char(col, rect.y, ' ', bg, clip);
        }
    }

    // Top corners.
    canvas.put_char(rect.x, rect.y, paint.tl, paint.frame_style, clip);
    canvas.put_char(
        rect.x + rect.width - 1,
        rect.y,
        paint.tr,
        paint.frame_style,
        clip,
    );

    // Top horizontal edge.
    for col in (rect.x + 1)..(rect.x + rect.width - 1) {
        canvas.put_char(col, rect.y, paint.h, paint.frame_style, clip);
    }

    // Title-bar gadgets, positioned by the tray.
    if let (Some(state), Some((start, len))) = (spec.gadgets.close, paint.tray.close) {
        canvas.draw_gadget(Rect::new(start, rect.y, len, 1), Gadget::Close, state, clip);
    }
    if let (Some(state), Some((start, len))) = (spec.gadgets.minimize, paint.tray.minimize) {
        canvas.draw_gadget(Rect::new(start, rect.y, len, 1), Gadget::Depth, state, clip);
    }
    if let (Some(state), Some((start, len))) = (spec.gadgets.maximize, paint.tray.maximize) {
        canvas.draw_gadget(Rect::new(start, rect.y, len, 1), Gadget::Zoom, state, clip);
    }

    // Title, centered within the full frame width and clipped to the tray.
    if !spec.title.is_empty() {
        let title_full = format!(" {} ", spec.title);
        let title_chars: Vec<char> = title_full.chars().collect();
        #[allow(clippy::cast_possible_truncation)]
        let title_len = title_chars.len() as u16;
        let available = paint.tray.title_end.saturating_sub(paint.tray.title_start);

        if available >= 2 {
            let ideal_start = rect.x + (rect.width.saturating_sub(title_len)) / 2;
            let clamped_start = ideal_start.max(paint.tray.title_start);

            let vis_start = clamped_start;
            let vis_end = (clamped_start + title_len).min(paint.tray.title_end);

            if vis_end > vis_start {
                #[allow(clippy::cast_possible_truncation)]
                let vis_count = (vis_end - vis_start) as usize;

                let truncated_right = (clamped_start + title_len) > paint.tray.title_end;

                for (i, &ch) in title_chars.iter().take(vis_count).enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    let col = vis_start + i as u16;
                    let draw_ch = if truncated_right && i == vis_count - 1 {
                        '…'
                    } else {
                        ch
                    };
                    canvas.put_char(col, rect.y, draw_ch, paint.title_style, clip);
                }
            }
        }
    }

    if rect.height >= 2 {
        // Side borders (both columns). A visible scrollbar is drawn later by
        // the view and overwrites the right column cell-for-cell.
        for row in (rect.y + 1)..(rect.y + rect.height - 1) {
            canvas.put_char(rect.x, row, paint.v, paint.frame_style, clip);
            canvas.put_char(
                rect.x + rect.width - 1,
                row,
                paint.v,
                paint.frame_style,
                clip,
            );
        }

        // Bottom corners and horizontal edge.
        let bottom = rect.y + rect.height - 1;
        canvas.put_char(rect.x, bottom, paint.bl, paint.frame_style, clip);
        for col in (rect.x + 1)..(rect.x + rect.width - 1) {
            canvas.put_char(col, bottom, paint.h, paint.frame_style, clip);
        }

        // Bottom-right corner, replaced by the resize grip when present.
        if let Some(state) = spec.gadgets.resize_grip {
            canvas.draw_gadget(
                Rect::new(rect.x + rect.width - 1, bottom, 1, 1),
                Gadget::Size,
                state,
                clip,
            );
        } else {
            canvas.put_char(
                rect.x + rect.width - 1,
                bottom,
                paint.br,
                paint.frame_style,
                clip,
            );
        }
    }
}

/// Cell fallback for [`Canvas::draw_gadget`].
pub(crate) fn draw_gadget<C: Canvas + ?Sized>(
    canvas: &mut C,
    rect: Rect,
    gadget: Gadget,
    state: GadgetState,
    clip: Rect,
) {
    let (chars, len, style) = theme::with_current(|t| gadget_cell(t, gadget, state));

    for (i, &ch) in chars.iter().take(len).enumerate() {
        let col = rect.x + u16::try_from(i).unwrap_or(0);
        canvas.put_char(col, rect.y, ch, style, clip);
    }
}

/// Cell fallback for [`Canvas::draw_scrollbar`].
pub(crate) fn draw_scrollbar<C: Canvas + ?Sized>(
    canvas: &mut C,
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

    // Resolve theme styles from the semantic scroll state.
    let (track_style, thumb_style, arrows_style) = theme::with_current(|t| {
        if spec.dragging {
            let style = dragging_style(t);
            return (style, style, style);
        }
        if !spec.active {
            return (
                t.scrollbar_track_inactive,
                t.scrollbar_thumb_inactive,
                t.scrollbar_arrows_inactive,
            );
        }
        let track = if spec.hover == ScrollHover::Track {
            t.scrollbar_track_hover
        } else {
            t.scrollbar_track
        };
        let thumb = if spec.hover == ScrollHover::Thumb {
            t.scrollbar_thumb_hover
        } else {
            t.scrollbar_thumb
        };
        let arrows = if spec.hover == ScrollHover::Arrow {
            t.scrollbar_arrows_hover
        } else {
            t.scrollbar_arrows
        };
        (track, thumb, arrows)
    });

    let track_size = match orientation {
        Orientation::Vertical => i32::from(rect.height.saturating_sub(2)),
        Orientation::Horizontal => i32::from(rect.width.saturating_sub(2)),
    };
    let thumb_pos =
        usize::try_from(scroll_thumb_position(spec.position, spec.total, track_size)).unwrap_or(0);

    match orientation {
        Orientation::Vertical => {
            let height = rect.height as usize;
            for row in 0..height {
                let buf_row = rect.y + u16::try_from(row).unwrap_or(0);
                let (ch, style) = if row == 0 {
                    (V_UP, arrows_style)
                } else if row >= height.saturating_sub(1) {
                    (V_DOWN, arrows_style)
                } else if row - 1 == thumb_pos {
                    (V_THUMB, thumb_style)
                } else {
                    (V_TRACK, track_style)
                };
                canvas.put_char(rect.x, buf_row, ch, style, clip);
            }
        }
        Orientation::Horizontal => {
            let width = rect.width as usize;
            for col in 0..width {
                let buf_col = rect.x + u16::try_from(col).unwrap_or(0);
                let (ch, style) = if col == 0 {
                    (H_LEFT, arrows_style)
                } else if col >= width.saturating_sub(1) {
                    (H_RIGHT, arrows_style)
                } else if col - 1 == thumb_pos {
                    (H_THUMB, thumb_style)
                } else {
                    (H_TRACK, track_style)
                };
                canvas.put_char(buf_col, rect.y, ch, style, clip);
            }
        }
    }
}
