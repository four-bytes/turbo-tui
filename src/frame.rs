//! Frame — Window border decoration with optional scrollbars.
//!
//! Frame draws a rectangular border using characters from the current theme.
//! It supports:
//! - Three frame types: Window, Dialog, Single
//! - Centered title on the top border
//! - Close button `[■]` on the top-left (for closeable frames)
//! - Resize handle `⋱` on the bottom-right (for resizable frames)
//! - Optional vertical scrollbar on the right border
//! - Optional horizontal scrollbar on the bottom border
//!
//! Frame does NOT manage children — it only draws the border decoration.
//! Use [`Window`] for a complete window with Frame + interior Container.

use crate::scrollbar::ScrollBar;
use crate::theme;
use crate::view::{Event, View, ViewBase, ViewId, SF_FOCUSED};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

// ============================================================================
// FrameType
// ============================================================================

/// Frame type determines the visual style of the border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Window frame — used for regular overlapping windows.
    Window,
    /// Dialog frame — used for modal dialogs.
    Dialog,
    /// Single-line frame — used for group boxes and panels.
    Single,
}

// ============================================================================
// Frame
// ============================================================================

/// Window border decoration with title, close button, resize handle, and optional scrollbars.
///
/// Frame draws a rectangular border using characters from the current theme.
/// It supports:
/// - Centered title on the top border
/// - Close button `[■]` on the top-left (configurable)
/// - Resize handle `⋱` on the bottom-right (configurable)
/// - Optional vertical scrollbar on the right border
/// - Optional horizontal scrollbar on the bottom border
///
/// Frame does NOT manage children — it only draws the border decoration.
/// Use [`Window`] for a complete window with Frame + interior Container.
///
/// # Example
///
/// ```ignore
/// use turbo_tui::frame::{Frame, FrameType};
/// use ratatui::layout::Rect;
///
/// let frame = Frame::new(Rect::new(10, 5, 40, 20), "My Window", FrameType::Window);
/// assert!(frame.closeable());
/// assert!(frame.resizable());
/// ```
#[allow(clippy::struct_field_names)]
pub struct Frame {
    /// Base view functionality.
    base: ViewBase,
    /// Window/dialog title displayed on the top border.
    title: String,
    /// Frame type determines border style.
    frame_type: FrameType,
    /// Whether the frame has a close button.
    closeable: bool,
    /// Whether the frame has a resize handle.
    resizable: bool,
    /// Optional vertical scrollbar (occupies right border).
    v_scrollbar: Option<ScrollBar>,
    /// Optional horizontal scrollbar (occupies bottom border).
    h_scrollbar: Option<ScrollBar>,
}

impl Frame {
    /// Create a new frame with the given bounds, title, and frame type.
    ///
    /// By default, the frame is closeable and resizable for Window type,
    /// and neither closeable nor resizable for Dialog/Single types.
    ///
    /// # Arguments
    ///
    /// * `bounds` - The bounding rectangle for the frame.
    /// * `title` - The title text displayed on the top border.
    /// * `frame_type` - The frame type (Window, Dialog, or Single).
    #[must_use]
    pub fn new(bounds: Rect, title: &str, frame_type: FrameType) -> Self {
        let (closeable, resizable) = match frame_type {
            FrameType::Window => (true, true),
            FrameType::Dialog | FrameType::Single => (false, false),
        };
        Self {
            base: ViewBase::new(bounds),
            title: title.to_owned(),
            frame_type,
            closeable,
            resizable,
            v_scrollbar: None,
            h_scrollbar: None,
        }
    }

    /// Get the frame title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the frame title.
    pub fn set_title(&mut self, title: &str) {
        if self.title != title {
            title.clone_into(&mut self.title);
            self.base.mark_dirty();
        }
    }

    /// Get the frame type.
    #[must_use]
    pub fn frame_type(&self) -> FrameType {
        self.frame_type
    }

    /// Check if the frame is closeable (has close button).
    #[must_use]
    pub fn closeable(&self) -> bool {
        self.closeable
    }

    /// Set whether the frame is closeable.
    pub fn set_closeable(&mut self, closeable: bool) {
        if self.closeable != closeable {
            self.closeable = closeable;
            self.base.mark_dirty();
        }
    }

    /// Check if the frame is resizable (has resize handle).
    #[must_use]
    pub fn resizable(&self) -> bool {
        self.resizable
    }

    /// Set whether the frame is resizable.
    pub fn set_resizable(&mut self, resizable: bool) {
        if self.resizable != resizable {
            self.resizable = resizable;
            self.base.mark_dirty();
        }
    }

    /// Set the vertical scrollbar.
    ///
    /// The scrollbar occupies the entire right column of the frame.
    pub fn set_v_scrollbar(&mut self, scrollbar: ScrollBar) {
        self.v_scrollbar = Some(scrollbar);
        self.base.mark_dirty();
    }

    /// Get the vertical scrollbar.
    #[must_use]
    pub fn v_scrollbar(&self) -> Option<&ScrollBar> {
        self.v_scrollbar.as_ref()
    }

    /// Get mutable access to the vertical scrollbar.
    pub fn v_scrollbar_mut(&mut self) -> Option<&mut ScrollBar> {
        self.v_scrollbar.as_mut()
    }

    /// Remove and return the vertical scrollbar.
    pub fn remove_v_scrollbar(&mut self) -> Option<ScrollBar> {
        let scrollbar = self.v_scrollbar.take();
        if scrollbar.is_some() {
            self.base.mark_dirty();
        }
        scrollbar
    }

    /// Set the horizontal scrollbar.
    ///
    /// The scrollbar occupies the entire bottom row of the frame.
    pub fn set_h_scrollbar(&mut self, scrollbar: ScrollBar) {
        self.h_scrollbar = Some(scrollbar);
        self.base.mark_dirty();
    }

    /// Get the horizontal scrollbar.
    #[must_use]
    pub fn h_scrollbar(&self) -> Option<&ScrollBar> {
        self.h_scrollbar.as_ref()
    }

    /// Get mutable access to the horizontal scrollbar.
    pub fn h_scrollbar_mut(&mut self) -> Option<&mut ScrollBar> {
        self.h_scrollbar.as_mut()
    }

    /// Remove and return the horizontal scrollbar.
    pub fn remove_h_scrollbar(&mut self) -> Option<ScrollBar> {
        let scrollbar = self.h_scrollbar.take();
        if scrollbar.is_some() {
            self.base.mark_dirty();
        }
        scrollbar
    }

    /// Calculate the interior area (inside the borders, minus scrollbars).
    ///
    /// Returns the `Rect` that child content can use, accounting for:
    /// - 1-cell borders on all sides
    /// - Right border offset if vertical scrollbar present
    /// - Bottom border offset if horizontal scrollbar present
    ///
    /// Returns an empty `Rect` if the frame is too small for an interior.
    #[must_use]
    pub fn interior_area(&self) -> Rect {
        let b = self.base.bounds();
        if b.width < 3 || b.height < 3 {
            return Rect::default();
        }

        // Right inset: 1 for border + 1 for scrollbar if present
        let right_inset: u16 = if self.v_scrollbar.is_some() { 2 } else { 1 };
        // Bottom inset: 1 for border + 1 for scrollbar if present
        let bottom_inset: u16 = if self.h_scrollbar.is_some() { 2 } else { 1 };

        let w = b.width.saturating_sub(1 + right_inset);
        let h = b.height.saturating_sub(1 + bottom_inset);

        if w == 0 || h == 0 {
            return Rect::default();
        }

        Rect::new(b.x + 1, b.y + 1, w, h)
    }

    /// Check if the given position is on the close button `[■]`.
    ///
    /// Returns `true` if closeable and point is at top-left corner area.
    /// The close button occupies positions (x+1, y) through (x+3, y).
    #[must_use]
    pub fn is_close_button(&self, col: u16, row: u16) -> bool {
        if !self.closeable {
            return false;
        }

        let b = self.base.bounds();
        col > b.x && col <= b.x + 3 && row == b.y
    }

    /// Check if the given position is on the resize handle `⋱`.
    ///
    /// Returns `true` if resizable and point is at bottom-right corner.
    /// The resize handle is at position (x + width - 1, y + height - 1).
    #[must_use]
    pub fn is_resize_handle(&self, col: u16, row: u16) -> bool {
        if !self.resizable {
            return false;
        }

        let b = self.base.bounds();
        col == b.x + b.width - 1 && row == b.y + b.height - 1
    }

    /// Check if the given position is on the title bar.
    ///
    /// The title bar is the top border row, excluding the close button area.
    #[must_use]
    pub fn is_title_bar(&self, col: u16, row: u16) -> bool {
        let b = self.base.bounds();
        if row != b.y {
            return false;
        }

        // Not on title bar if outside horizontal bounds
        if col < b.x || col >= b.x + b.width {
            return false;
        }

        // If closeable, exclude close button area (columns 1-3)
        if self.closeable && col > b.x && col <= b.x + 3 {
            return false;
        }

        true
    }

    /// Draw a single character to the buffer at the given position.
    ///
    /// Returns `true` if the character was drawn (within clip bounds).
    fn draw_char(
        buf: &mut Buffer,
        col: u16,
        row: u16,
        ch: char,
        style: ratatui::style::Style,
        clip: Rect,
    ) -> bool {
        if col < clip.x || col >= clip.x + clip.width || row < clip.y || row >= clip.y + clip.height
        {
            return false;
        }

        if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
            cell.set_char(ch).set_style(style);
        }
        true
    }
}

impl View for Frame {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    #[allow(clippy::too_many_lines)]
    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        let b = self.base.bounds();
        if b.width < 2 || b.height < 2 {
            return;
        }

        // Intersect with clip
        let draw_area = b.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        // Get theme styles and border characters
        let styles = theme::with_current(|t| {
            let is_active = (self.base.state() & SF_FOCUSED) != 0;
            let (frame_style, title_style) = match self.frame_type {
                FrameType::Window => {
                    if is_active {
                        (t.window_frame_active, t.window_title_active)
                    } else {
                        (t.window_frame_inactive, t.window_title_inactive)
                    }
                }
                FrameType::Dialog => (t.dialog_frame, t.dialog_title),
                FrameType::Single => (t.single_frame, t.single_frame),
            };
            FrameStyles {
                frame: frame_style,
                title: title_style,
                close: t.window_close_button,
                resize: t.window_resize_handle,
                tl: t.border_tl,
                tr: t.border_tr,
                bl: t.border_bl,
                br: t.border_br,
                h: t.border_h,
                v: t.border_v,
            }
        });

        self.draw_top_border(buf, clip, &styles);
        self.draw_side_borders(buf, clip, &styles);
        self.draw_scrollbars(buf, clip);
        self.draw_bottom_border(buf, clip, &styles);
    }

    fn handle_event(&mut self, _event: &mut Event) {
        // Frame does NOT handle events — Window handles events and delegates to Frame's hit-test methods
    }

    fn can_focus(&self) -> bool {
        false
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
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

/// Styles and characters for drawing a frame.
struct FrameStyles {
    frame: ratatui::style::Style,
    title: ratatui::style::Style,
    close: ratatui::style::Style,
    resize: ratatui::style::Style,
    tl: char,
    tr: char,
    bl: char,
    br: char,
    h: char,
    v: char,
}

impl Frame {
    /// Draw the top border including corners, horizontal line, close button, and title.
    fn draw_top_border(&self, buf: &mut Buffer, clip: Rect, styles: &FrameStyles) {
        let b = self.base.bounds();

        // Corner characters
        Self::draw_char(buf, b.x, b.y, styles.tl, styles.frame, clip);
        Self::draw_char(buf, b.x + b.width - 1, b.y, styles.tr, styles.frame, clip);

        // Horizontal line
        for col in (b.x + 1)..(b.x + b.width - 1) {
            Self::draw_char(buf, col, b.y, styles.h, styles.frame, clip);
        }

        // Close button
        if self.closeable {
            Self::draw_char(buf, b.x + 1, b.y, '[', styles.close, clip);
            Self::draw_char(buf, b.x + 2, b.y, '■', styles.close, clip);
            Self::draw_char(buf, b.x + 3, b.y, ']', styles.close, clip);
        }

        // Title
        if !self.title.is_empty() && b.width > 6 {
            let title_full = format!(" {} ", self.title);
            let title_len = u16::try_from(title_full.chars().count()).unwrap_or(0);
            let available_width = b.width.saturating_sub(2);
            let start_col = if title_len < available_width {
                b.x + 1 + (available_width.saturating_sub(title_len)) / 2
            } else {
                b.x + 1
            };

            for (i, ch) in title_full.chars().enumerate() {
                let col = start_col + u16::try_from(i).unwrap_or(0);
                // Skip close button area
                if self.closeable && col > b.x && col <= b.x + 3 {
                    continue;
                }
                if col >= b.x + b.width - 1 {
                    break;
                }
                Self::draw_char(buf, col, b.y, ch, styles.title, clip);
            }
        }
    }

    /// Draw the left and right vertical borders.
    fn draw_side_borders(&self, buf: &mut Buffer, clip: Rect, styles: &FrameStyles) {
        let b = self.base.bounds();

        for row in (b.y + 1)..(b.y + b.height - 1) {
            // Left border
            Self::draw_char(buf, b.x, row, styles.v, styles.frame, clip);

            // Right border
            // Note: If v_scrollbar present, scrollbar is drawn separately
            if self.v_scrollbar.is_none() {
                Self::draw_char(buf, b.x + b.width - 1, row, styles.v, styles.frame, clip);
            }
        }
    }

    /// Draw the vertical and horizontal scrollbars if present.
    fn draw_scrollbars(&self, buf: &mut Buffer, clip: Rect) {
        let b = self.base.bounds();

        // Vertical scrollbar
        if let Some(ref sb) = self.v_scrollbar {
            let sb_x = b.x + b.width.saturating_sub(2);
            let sb_y = b.y + 1;
            let sb_height = b.height.saturating_sub(2);
            let sb_bounds = Rect::new(sb_x, sb_y, 1, sb_height);

            let mut sb_owned = sb.clone();
            sb_owned.set_bounds(sb_bounds);
            sb_owned.draw(buf, clip);
        }

        // Horizontal scrollbar
        if let Some(ref sb) = self.h_scrollbar {
            let h_sb_bounds = Rect::new(
                b.x + 1,
                b.y + b.height.saturating_sub(2),
                b.width.saturating_sub(2),
                1,
            );
            let mut sb_owned = sb.clone();
            sb_owned.set_bounds(h_sb_bounds);
            sb_owned.draw(buf, clip);
        }
    }

    /// Draw the bottom border including corners, horizontal line, and resize handle.
    fn draw_bottom_border(&self, buf: &mut Buffer, clip: Rect, styles: &FrameStyles) {
        let b = self.base.bounds();

        // Corner characters
        Self::draw_char(buf, b.x, b.y + b.height - 1, styles.bl, styles.frame, clip);

        // Horizontal line (or scrollbar area handled in draw_scrollbars)
        if self.h_scrollbar.is_none() {
            for col in (b.x + 1)..(b.x + b.width - 1) {
                Self::draw_char(buf, col, b.y + b.height - 1, styles.h, styles.frame, clip);
            }
        }

        // Bottom-right corner (or resize handle)
        if self.resizable && self.frame_type == FrameType::Window {
            Self::draw_char(
                buf,
                b.x + b.width - 1,
                b.y + b.height - 1,
                '⋱',
                styles.resize,
                clip,
            );
        } else {
            Self::draw_char(
                buf,
                b.x + b.width - 1,
                b.y + b.height - 1,
                styles.br,
                styles.frame,
                clip,
            );
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn setup_default_theme() {
        theme::set(Theme::dark());
    }

    #[test]
    fn test_frame_new_window_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        assert!(frame.closeable());
        assert!(frame.resizable());
        assert_eq!(frame.title(), "Test");
        assert_eq!(frame.frame_type(), FrameType::Window);
    }

    #[test]
    fn test_frame_new_dialog_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 30, 15), "Dialog", FrameType::Dialog);
        assert!(!frame.closeable());
        assert!(!frame.resizable());
        assert_eq!(frame.frame_type(), FrameType::Dialog);
    }

    #[test]
    fn test_frame_new_single_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 20, 10), "Group", FrameType::Single);
        assert!(!frame.closeable());
        assert!(!frame.resizable());
        assert_eq!(frame.frame_type(), FrameType::Single);
    }

    #[test]
    fn test_frame_set_title() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Old", FrameType::Window);
        frame.set_title("New Title");
        assert_eq!(frame.title(), "New Title");
    }

    #[test]
    fn test_frame_set_closeable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);
        assert!(frame.closeable());
        frame.set_closeable(false);
        assert!(!frame.closeable());
        frame.set_closeable(true);
        assert!(frame.closeable());
    }

    #[test]
    fn test_frame_set_resizable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);
        assert!(frame.resizable());
        frame.set_resizable(false);
        assert!(!frame.resizable());
        frame.set_resizable(true);
        assert!(frame.resizable());
    }

    #[test]
    fn test_frame_interior_area_no_scrollbars() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        // Bounds: (10, 5, 40, 20)
        // Interior: (11, 6, 38, 18) — 1 border on each side
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_frame_interior_area_with_v_scrollbar() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));

        // With v_scrollbar: right inset is 2 instead of 1
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 37, 18));
    }

    #[test]
    fn test_frame_interior_area_with_h_scrollbar() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));

        // With h_scrollbar: bottom inset is 2 instead of 1
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 17));
    }

    #[test]
    fn test_frame_interior_area_with_both_scrollbars() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));

        // Both scrollbars: right inset 2, bottom inset 2
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 37, 17));
    }

    #[test]
    fn test_frame_interior_area_too_small() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 2, 2), "Test", FrameType::Window);
        // Width and height < 3, so interior should be empty
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::default());

        let frame2 = Frame::new(Rect::new(0, 0, 3, 3), "Test", FrameType::Window);
        // Minimum size for interior: 3x3 gives interior 1x1
        let interior2 = frame2.interior_area();
        assert_eq!(interior2, Rect::new(1, 1, 1, 1));
    }

    #[test]
    fn test_frame_is_close_button() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Close button at (11, 5), (12, 5), (13, 5)
        assert!(frame.is_close_button(11, 5));
        assert!(frame.is_close_button(12, 5));
        assert!(frame.is_close_button(13, 5));

        // Not on close button
        assert!(!frame.is_close_button(10, 5)); // Top-left corner
        assert!(!frame.is_close_button(14, 5)); // Past close button
        assert!(!frame.is_close_button(11, 6)); // Different row
    }

    #[test]
    fn test_frame_is_close_button_not_closeable() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Dialog);

        // Not closeable, so is_close_button always returns false
        assert!(!frame.is_close_button(11, 5));
        assert!(!frame.is_close_button(12, 5));
    }

    #[test]
    fn test_frame_is_resize_handle() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Resize handle at (49, 24) — bottom-right corner
        assert!(frame.is_resize_handle(49, 24));

        // Not on resize handle
        assert!(!frame.is_resize_handle(48, 24));
        assert!(!frame.is_resize_handle(49, 23));
    }

    #[test]
    fn test_frame_is_resize_handle_not_resizable() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Dialog);

        // Not resizable, so is_resize_handle always returns false
        assert!(!frame.is_resize_handle(49, 24));
    }

    #[test]
    fn test_frame_is_title_bar() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Title bar is row 5 (top border)
        assert!(frame.is_title_bar(10, 5)); // Top-left corner
        assert!(frame.is_title_bar(14, 5)); // Past close button
        assert!(frame.is_title_bar(49, 5)); // Top-right corner

        // Close button area is NOT part of title bar for Window type
        assert!(!frame.is_title_bar(11, 5)); // Close button
        assert!(!frame.is_title_bar(12, 5)); // Close button
        assert!(!frame.is_title_bar(13, 5)); // Close button

        // Wrong row
        assert!(!frame.is_title_bar(10, 6));
        assert!(!frame.is_title_bar(10, 4));
    }

    #[test]
    fn test_frame_draw_renders_corners() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Test", FrameType::Window);
        frame.draw(&mut buf, bounds);

        // Check corner characters
        let tl = buf.cell(Position::new(0, 0)).unwrap();
        let tr = buf.cell(Position::new(19, 0)).unwrap();
        let bl = buf.cell(Position::new(0, 9)).unwrap();
        let br = buf.cell(Position::new(19, 9)).unwrap();

        // Use theme characters (dark theme uses thick borders)
        assert_eq!(tl.symbol(), "┏");
        assert_eq!(tr.symbol(), "┓");
        assert_eq!(bl.symbol(), "┗");
        // Resize handle in corner for Window type
        assert_eq!(br.symbol(), "⋱");
    }

    #[test]
    fn test_frame_draw_renders_title() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "MyTitle", FrameType::Window);
        frame.draw(&mut buf, bounds);

        // Title should be centered on the top border
        // Width 20, title " MyTitle " = 9 chars
        // Start position: (20 - 9) / 2 = 5.5 -> 5
        // But need to account for close button [■] at positions 1-3
        // So title starts after position 4

        // Verify we can find the title text somewhere on the top row
        let top_row: String = (0..20)
            .map(|col| buf.cell(Position::new(col, 0)).unwrap().symbol())
            .collect::<String>();

        // Title should appear somewhere
        assert!(top_row.contains('T') || top_row.contains('M'));
    }

    #[test]
    fn test_frame_draw_renders_close_button() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Test", FrameType::Window);
        frame.draw(&mut buf, bounds);

        // Close button at positions 1, 2, 3
        let b1 = buf.cell(Position::new(1, 0)).unwrap();
        let b2 = buf.cell(Position::new(2, 0)).unwrap();
        let b3 = buf.cell(Position::new(3, 0)).unwrap();

        assert_eq!(b1.symbol(), "[");
        assert_eq!(b2.symbol(), "■");
        assert_eq!(b3.symbol(), "]");
    }

    #[test]
    fn test_frame_draw_dialog_no_close_button() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Dialog", FrameType::Dialog);
        frame.draw(&mut buf, bounds);

        // Dialog frame should not have close button
        // Position 1, 2 should have border characters, not [■
        let c1 = buf.cell(Position::new(1, 0)).unwrap();
        assert_ne!(c1.symbol(), "[");
    }

    #[test]
    fn test_frame_draw_single_frame() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "GroupBox", FrameType::Single);
        frame.draw(&mut buf, bounds);

        // Single frame should not have close button or resize handle
        let c1 = buf.cell(Position::new(1, 0)).unwrap();
        assert_ne!(c1.symbol(), "[");

        let br = buf.cell(Position::new(19, 9)).unwrap();
        // Bottom-right should be normal corner, not resize handle
        assert_eq!(br.symbol(), "┛");
    }

    #[test]
    fn test_frame_scrollbar_management() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);

        // Add vertical scrollbar
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));
        assert!(frame.v_scrollbar().is_some());

        // Add horizontal scrollbar
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));
        assert!(frame.h_scrollbar().is_some());

        // Remove vertical scrollbar
        let v_sb = frame.remove_v_scrollbar();
        assert!(v_sb.is_some());
        assert!(frame.v_scrollbar().is_none());

        // Remove horizontal scrollbar
        let h_sb = frame.remove_h_scrollbar();
        assert!(h_sb.is_some());
        assert!(frame.h_scrollbar().is_none());
    }

    #[test]
    fn test_frame_view_trait() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);

        // Test View trait methods
        let _id1 = frame.id();
        assert_eq!(frame.bounds(), Rect::new(0, 0, 20, 10));

        frame.set_bounds(Rect::new(5, 5, 30, 15));
        assert_eq!(frame.bounds(), Rect::new(5, 5, 30, 15));

        // State management
        assert_eq!(
            frame.state() & crate::view::SF_VISIBLE,
            crate::view::SF_VISIBLE
        );
        frame.set_state(0);
        assert_eq!(frame.state(), 0);

        // Cannot focus
        assert!(!frame.can_focus());
    }
}
