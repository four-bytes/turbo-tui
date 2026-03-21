//! Frame — window border with title, close button and optional resize handle.
//!
//! Renders a bordered frame using Ratatui's [`Buffer`] API. Supports three
//! visual styles ([`FrameType`]) and handles mouse interaction for dragging,
//! resizing and closing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use std::any::Any;

use crate::command::CM_CLOSE;
use crate::theme;
use crate::view::{
    Event, EventKind, View, ViewBase, ViewId, SF_ACTIVE, SF_DRAGGING, SF_FOCUSED, SF_RESIZING,
};

// ============================================================================
// FrameType
// ============================================================================

/// Visual style of the window border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameType {
    /// Double-line border `╔═╗║╚╝` with cyan/dim palette (default).
    #[default]
    Window,
    /// Double-line border `╔═╗║╚╝` with white/bright palette for dialogs.
    Dialog,
    /// Single-line border `┌─┐│└┘` with gray palette.
    Single,
}

// ============================================================================
// Frame
// ============================================================================

/// Window border with title, close button and optional resize handle.
///
/// `Frame` implements [`View`] and renders directly to a Ratatui [`Buffer`].
/// It handles mouse events for drag-initiation, resize-initiation and
/// close-button clicks (converting the last to a [`CM_CLOSE`] command).
///
/// # Example
///
/// ```ignore
/// let mut frame = Frame::new(Rect::new(0, 0, 40, 20), "My Window");
/// frame.set_resizable(true);
/// frame.draw(&mut buf, area);
/// ```
pub struct Frame {
    /// Common view state (id, bounds, state/option flags, …).
    base: ViewBase,
    /// Window title displayed on the top border.
    title: String,
    /// Border character set and palette.
    #[allow(clippy::struct_field_names)]
    frame_type: FrameType,
    /// Show a resize handle `⋱` in the bottom-right corner.
    resizable: bool,
    /// Show a close button `[■]` near the top-left corner.
    closeable: bool,
}

impl Frame {
    /// Create a new [`Frame`] with [`FrameType::Window`], closeable, not resizable.
    pub fn new(bounds: Rect, title: &str) -> Self {
        Self {
            base: ViewBase::new(bounds),
            title: title.to_owned(),
            frame_type: FrameType::default(),
            resizable: false,
            closeable: true,
        }
    }

    /// Create a new [`Frame`] with an explicit [`FrameType`].
    pub fn with_type(bounds: Rect, title: &str, frame_type: FrameType) -> Self {
        Self {
            base: ViewBase::new(bounds),
            title: title.to_owned(),
            frame_type,
            resizable: false,
            closeable: true,
        }
    }

    /// Enable or disable the resize handle in the bottom-right corner.
    pub fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    /// Enable or disable the close button `[■]` near the top-left.
    pub fn set_closeable(&mut self, closeable: bool) {
        self.closeable = closeable;
    }

    /// Get the window title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Replace the window title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Get the frame style variant.
    pub fn frame_type(&self) -> FrameType {
        self.frame_type
    }

    // -----------------------------------------------------------------------
    // Geometry helpers
    // -----------------------------------------------------------------------

    /// Return the interior area — bounds shrunk by one cell on every side.
    ///
    /// Returns a zero-size [`Rect`] when the frame is too small (width or
    /// height < 3).
    pub fn interior(&self) -> Rect {
        let b = self.base.bounds();
        if b.width < 3 || b.height < 3 {
            return Rect::new(b.x, b.y, 0, 0);
        }
        Rect::new(b.x + 1, b.y + 1, b.width - 2, b.height - 2)
    }

    /// Return `true` if `(x, y)` is anywhere on the title bar (top row).
    pub fn is_title_bar(&self, x: u16, y: u16) -> bool {
        let b = self.base.bounds();
        y == b.y && x >= b.x && x < b.x + b.width
    }

    /// Return `true` if `(x, y)` is over the close button `[■]`.
    ///
    /// The close button occupies columns `x+1 ..= x+3` of the top row.
    pub fn is_close_button(&self, x: u16, y: u16) -> bool {
        let b = self.base.bounds();
        self.closeable && y == b.y && x > b.x && x <= b.x + 3
    }

    /// Return `true` if `(x, y)` is over the resize handle (bottom-right).
    pub fn is_resize_handle(&self, x: u16, y: u16) -> bool {
        let b = self.base.bounds();
        self.resizable && y == b.y + b.height - 1 && x >= b.x + b.width.saturating_sub(2)
    }

    /// Return `true` while [`SF_DRAGGING`] is set on this frame.
    pub fn is_dragging(&self) -> bool {
        self.base.state() & SF_DRAGGING != 0
    }

    /// Return `true` while [`SF_RESIZING`] is set on this frame.
    pub fn is_resizing(&self) -> bool {
        self.base.state() & SF_RESIZING != 0
    }

    /// Clear both the drag and resize state flags.
    pub fn clear_drag_resize(&mut self) {
        let state = self.base.state();
        self.base.set_state(state & !SF_DRAGGING & !SF_RESIZING);
    }

    // -----------------------------------------------------------------------
    // Style helpers
    // -----------------------------------------------------------------------

    /// Border style — bright when active/focused, dim otherwise.
    fn border_style(&self) -> Style {
        let is_active = self.base.state() & SF_ACTIVE != 0;
        let is_focused = self.base.state() & SF_FOCUSED != 0;
        theme::with_current(|t| match self.frame_type {
            FrameType::Window => {
                if is_active || is_focused {
                    t.window_frame_active
                } else {
                    t.window_frame_inactive
                }
            }
            FrameType::Dialog => t.dialog_frame,
            FrameType::Single => t.single_frame,
        })
    }

    /// Title text style — bright when active/focused.
    fn title_style(&self) -> Style {
        let is_active = self.base.state() & SF_ACTIVE != 0;
        let is_focused = self.base.state() & SF_FOCUSED != 0;
        theme::with_current(|t| match self.frame_type {
            FrameType::Window => {
                if is_active || is_focused {
                    t.window_title_active
                } else {
                    t.window_title_inactive
                }
            }
            FrameType::Dialog => t.dialog_title,
            FrameType::Single => t.single_frame,
        })
    }

    /// Close button style from theme.
    #[allow(clippy::unused_self)]
    fn close_button_style(&self) -> Style {
        theme::with_current(|t| t.window_close_button)
    }

    /// Resize handle style from theme.
    #[allow(clippy::unused_self)]
    fn resize_handle_style(&self) -> Style {
        theme::with_current(|t| t.window_resize_handle)
    }
}

// ============================================================================
// View implementation
// ============================================================================

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

    /// Draw the frame border, title, close button and optional resize handle.
    fn draw(&self, buf: &mut Buffer, area: Rect) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        let style = self.border_style();
        let title_style = self.title_style();

        // Border characters for the selected frame type.
        let (tl, tr, bl, br, h, v) = match self.frame_type {
            FrameType::Window | FrameType::Dialog => ('╔', '╗', '╚', '╝', '═', '║'),
            FrameType::Single => ('┌', '┐', '└', '┘', '─', '│'),
        };

        // --- Top border ---
        buf.set_string(area.x, area.y, tl.to_string(), style);
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, area.y, h.to_string(), style);
        }
        buf.set_string(area.x + area.width - 1, area.y, tr.to_string(), style);

        // Title — centered on the top border.
        if !self.title.is_empty() {
            let title_display = format!(" {} ", self.title);
            #[allow(clippy::cast_possible_truncation)]
            let title_len = title_display.len() as u16;
            let title_x = area.x + (area.width.saturating_sub(title_len)) / 2;
            buf.set_string(title_x, area.y, &title_display, title_style);
        }

        // Close button — overwrites part of the top border (left side).
        if self.closeable {
            buf.set_string(area.x + 1, area.y, "[■]", self.close_button_style());
        }

        // --- Left and right borders ---
        for y in (area.y + 1)..(area.y + area.height - 1) {
            buf.set_string(area.x, y, v.to_string(), style);
            buf.set_string(area.x + area.width - 1, y, v.to_string(), style);
        }

        // --- Bottom border ---
        buf.set_string(area.x, area.y + area.height - 1, bl.to_string(), style);
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, area.y + area.height - 1, h.to_string(), style);
        }
        buf.set_string(
            area.x + area.width - 1,
            area.y + area.height - 1,
            br.to_string(),
            style,
        );

        // Resize handle — overwrites the bottom-right corner character.
        if self.resizable {
            buf.set_string(
                area.x + area.width - 2,
                area.y + area.height - 1,
                "⋱",
                self.resize_handle_style(),
            );
        }
    }

    /// Handle mouse events: close button → [`CM_CLOSE`] command; title bar →
    /// [`SF_DRAGGING`]; resize handle → [`SF_RESIZING`].
    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        if let EventKind::Mouse(mouse) = &event.kind.clone() {
            use crossterm::event::{MouseButton, MouseEventKind};

            let area = self.base.bounds();

            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                let col = mouse.column;
                let row = mouse.row;

                // Close button: columns x+1 ..= x+3 on the top row.
                if self.closeable && row == area.y && col > area.x && col <= area.x + 3 {
                    event.kind = EventKind::Command(CM_CLOSE);
                    return;
                }

                // Title bar: any column on the top row (not close button).
                if row == area.y && col >= area.x && col < area.x + area.width {
                    let state = self.base.state();
                    self.base.set_state(state | SF_DRAGGING);
                    event.clear();
                    return;
                }

                // Resize handle: bottom-right corner.
                if self.resizable
                    && row == area.y + area.height - 1
                    && col >= area.x + area.width.saturating_sub(2)
                {
                    let state = self.base.state();
                    self.base.set_state(state | SF_RESIZING);
                    event.clear();
                }
            }
        }
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn make_mouse_down(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    // --- Construction -------------------------------------------------------

    #[test]
    fn test_frame_new() {
        let f = Frame::new(Rect::new(0, 0, 20, 10), "Hello");
        assert_eq!(f.frame_type(), FrameType::Window);
        assert!(f.closeable);
        assert!(!f.resizable);
        assert_eq!(f.title(), "Hello");
    }

    // --- Interior -----------------------------------------------------------

    #[test]
    fn test_frame_interior() {
        let f = Frame::new(Rect::new(5, 3, 20, 10), "T");
        let i = f.interior();
        assert_eq!(i, Rect::new(6, 4, 18, 8));
    }

    #[test]
    fn test_frame_interior_too_small_width() {
        let f = Frame::new(Rect::new(0, 0, 2, 5), "T");
        let i = f.interior();
        assert_eq!(i.width, 0);
        assert_eq!(i.height, 0);
    }

    #[test]
    fn test_frame_interior_too_small_height() {
        let f = Frame::new(Rect::new(0, 0, 5, 2), "T");
        let i = f.interior();
        assert_eq!(i.width, 0);
        assert_eq!(i.height, 0);
    }

    // --- Hit-test helpers ---------------------------------------------------

    #[test]
    fn test_frame_title_bar_hit() {
        let f = Frame::new(Rect::new(2, 3, 20, 10), "T");
        assert!(f.is_title_bar(2, 3));
        assert!(f.is_title_bar(10, 3));
        assert!(f.is_title_bar(21, 3));
        assert!(!f.is_title_bar(5, 4));
    }

    #[test]
    fn test_frame_close_button_hit() {
        let f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        assert!(f.is_close_button(1, 0));
        assert!(f.is_close_button(2, 0));
        assert!(f.is_close_button(3, 0));
        // Column 0 and 4 are NOT the close button
        assert!(!f.is_close_button(0, 0));
        assert!(!f.is_close_button(4, 0));
        // Wrong row
        assert!(!f.is_close_button(2, 1));
    }

    #[test]
    fn test_frame_close_button_disabled() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        f.set_closeable(false);
        assert!(!f.is_close_button(2, 0));
    }

    #[test]
    fn test_frame_resize_handle_hit() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        f.set_resizable(true);
        // Bottom row = y + height - 1 = 9; handle cols >= width - 2 = 18
        assert!(f.is_resize_handle(18, 9));
        assert!(f.is_resize_handle(19, 9));
        // One column to the left — not the handle
        assert!(!f.is_resize_handle(17, 9));
        // Wrong row
        assert!(!f.is_resize_handle(18, 8));
    }

    #[test]
    fn test_frame_resize_handle_disabled() {
        let f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        // resizable = false by default
        assert!(!f.is_resize_handle(18, 9));
    }

    // --- Drawing ------------------------------------------------------------

    #[test]
    fn test_frame_draw_borders() {
        let area = Rect::new(0, 0, 10, 6);
        let f = Frame::new(area, "");
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area);

        // Corners (double-line Window type)
        assert_eq!(buf[(0, 0)].symbol(), "╔");
        assert_eq!(buf[(9, 0)].symbol(), "╗");
        assert_eq!(buf[(0, 5)].symbol(), "╚");
        assert_eq!(buf[(9, 5)].symbol(), "╝");

        // Top horizontal fill
        assert_eq!(buf[(5, 0)].symbol(), "═");
        // Side verticals
        assert_eq!(buf[(0, 3)].symbol(), "║");
        assert_eq!(buf[(9, 3)].symbol(), "║");
        // Bottom horizontal fill
        assert_eq!(buf[(5, 5)].symbol(), "═");
    }

    #[test]
    fn test_frame_draw_single_borders() {
        let area = Rect::new(0, 0, 10, 6);
        let f = Frame::with_type(area, "", FrameType::Single);
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area);

        assert_eq!(buf[(0, 0)].symbol(), "┌");
        assert_eq!(buf[(9, 0)].symbol(), "┐");
        assert_eq!(buf[(0, 5)].symbol(), "└");
        assert_eq!(buf[(9, 5)].symbol(), "┘");
        assert_eq!(buf[(5, 0)].symbol(), "─");
        assert_eq!(buf[(0, 3)].symbol(), "│");
    }

    #[test]
    fn test_frame_draw_title() {
        let area = Rect::new(0, 0, 20, 5);
        // Disable close button so it does not overwrite the title position
        let mut f = Frame::new(area, "Test");
        f.set_closeable(false);
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area);

        // Title " Test " (6 chars) centered in 20 cols → starts at (20-6)/2 = 7
        assert_eq!(buf[(7, 0)].symbol(), " ");
        assert_eq!(buf[(8, 0)].symbol(), "T");
        assert_eq!(buf[(9, 0)].symbol(), "e");
        assert_eq!(buf[(10, 0)].symbol(), "s");
        assert_eq!(buf[(11, 0)].symbol(), "t");
        assert_eq!(buf[(12, 0)].symbol(), " ");
    }

    #[test]
    fn test_frame_draw_close_button() {
        let area = Rect::new(0, 0, 20, 5);
        let f = Frame::new(area, "");
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area);

        // Close button "[■]" starts at column 1
        assert_eq!(buf[(1, 0)].symbol(), "[");
        assert_eq!(buf[(2, 0)].symbol(), "■");
        assert_eq!(buf[(3, 0)].symbol(), "]");
    }

    #[test]
    fn test_frame_draw_resize_handle() {
        let area = Rect::new(0, 0, 20, 5);
        let mut f = Frame::new(area, "");
        f.set_resizable(true);
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area);

        // Resize handle "⋱" at (width-2, height-1) = (18, 4)
        assert_eq!(buf[(18, 4)].symbol(), "⋱");
    }

    #[test]
    fn test_frame_draw_too_small() {
        // width < 4 — draw() should be a no-op (no panic)
        let area = Rect::new(0, 0, 3, 5);
        let f = Frame::new(area, "T");
        let mut buf = Buffer::empty(area);
        f.draw(&mut buf, area); // must not panic
    }

    // --- Event handling -----------------------------------------------------

    #[test]
    fn test_frame_close_click_generates_command() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        let mut event = make_mouse_down(2, 0); // column 2 = close button
        f.handle_event(&mut event);

        // Event must be converted to CM_CLOSE, NOT cleared
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_CLOSE));
        assert!(!event.is_cleared());
    }

    #[test]
    fn test_frame_title_click_sets_dragging() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        // Column 10 on the title row — past close button area
        let mut event = make_mouse_down(10, 0);
        f.handle_event(&mut event);

        assert!(f.is_dragging());
        assert!(event.is_cleared());
    }

    #[test]
    fn test_frame_resize_click_sets_resizing() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        f.set_resizable(true);
        // Bottom-right corner: col 18 or 19, row 9
        let mut event = make_mouse_down(18, 9);
        f.handle_event(&mut event);

        assert!(f.is_resizing());
        assert!(event.is_cleared());
    }

    #[test]
    fn test_frame_clear_drag_resize() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        f.set_resizable(true);

        let state = f.base.state();
        f.base.set_state(state | SF_DRAGGING | SF_RESIZING);
        assert!(f.is_dragging());
        assert!(f.is_resizing());

        f.clear_drag_resize();
        assert!(!f.is_dragging());
        assert!(!f.is_resizing());
    }

    #[test]
    fn test_frame_cleared_event_ignored() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 10), "T");
        let mut event = Event::default(); // already cleared
        f.handle_event(&mut event);
        // Frame state must remain unchanged
        assert!(!f.is_dragging());
        assert!(!f.is_resizing());
    }
}
