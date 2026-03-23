//! Window — overlapping window with frame border, interior container, drag and resize support.
//!
//! Window combines a [`Frame`] for the border and a [`Container`] for child views.
//! It implements drag-to-move, resize-from-corner, and zoom toggle.
//!
//! # State Machine
//!
//! - **Idle:** Normal operation, mouse events hit-tested against frame and interior.
//! - **Dragging:** Title bar grabbed, mouse movement moves the window.
//! - **Resizing:** Resize handle grabbed, mouse movement resizes the window.
//!
//! State transitions:
//! - `MouseDown` on title bar → Dragging (sets `SF_DRAGGING`)
//! - `MouseDown` on resize handle → Resizing (sets `SF_RESIZING`)
//! - `MouseDrag` while Dragging → update position (mouse pos - `drag_offset`)
//! - `MouseDrag` while Resizing → update size (clamped to `min_size`)
//! - `MouseUp` → back to Idle (clears `SF_DRAGGING`/`SF_RESIZING`)
//!
//! # Zoom Toggle
//!
//! Double-click on title bar toggles between maximized and previous bounds.

use crate::command::{CM_CLOSE, CM_MINIMIZE, CM_SCROLL_CHANGED, CM_ZOOM};
use crate::container::Container;
use crate::frame::{Frame, FrameConfig, FrameType};
use crate::theme;
use crate::view::{
    Event, EventKind, View, ViewBase, ViewId, SF_DRAGGING, SF_FOCUSED, SF_MINIMIZED, SF_RESIZING,
};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Overlapping window with frame border, interior container, drag and resize support.
///
/// Window combines a [`Frame`] for the border and a [`Container`] for child views.
/// It implements drag-to-move, resize-from-corner, and zoom toggle.
///
/// # State Machine
///
/// - **Idle:** Normal operation, mouse events hit-tested against frame and interior
/// - **Dragging:** Title bar grabbed, mouse movement moves the window
/// - **Resizing:** Resize handle grabbed, mouse movement resizes the window
///
/// State transitions:
/// - `MouseDown` on title bar → Dragging (sets `SF_DRAGGING`)
/// - `MouseDown` on resize handle → Resizing (sets `SF_RESIZING`)
/// - `MouseDrag` while Dragging → update position (mouse pos - `drag_offset`)
/// - `MouseDrag` while Resizing → update size (clamped to `min_size`)
/// - `MouseUp` → back to Idle (clears `SF_DRAGGING`/`SF_RESIZING`)
///
/// # Zoom Toggle
///
/// Double-click on title bar toggles between maximized and previous bounds.
pub struct Window {
    base: ViewBase,
    frame: Frame,
    interior: Container,
    /// Offset from mouse position to window top-left when drag started.
    drag_offset: Option<(i16, i16)>,
    /// Starting bounds when resize started: `(start_x, start_y, start_w, start_h)`.
    resize_start: Option<(u16, u16, u16, u16)>,
    /// Mouse position when resize started.
    resize_mouse_start: Option<(u16, u16)>,
    /// Minimum window size.
    min_size: (u16, u16),
    /// Saved bounds before zoom (for toggle-back).
    prev_bounds: Option<Rect>,
    /// Drag movement limits (if set, window can't be dragged outside).
    drag_limits: Option<Rect>,
    /// Saved bounds before minimize (for restore). When `Some`, window is minimized.
    minimized_bounds: Option<Rect>,
    /// Maximum width for minimized title bar (characters). Default: 30.
    minimized_max_width: u16,
    /// Current scroll offset (columns scrolled right, rows scrolled down).
    /// Children are drawn translated by `(-scroll_offset.0, -scroll_offset.1)`.
    scroll_offset: (i32, i32),
    /// Logical content size in cells `(width, height)`.
    /// If `None`, content size is auto-calculated from child bounds.
    /// Used to set scrollbar max values.
    content_size: Option<(u16, u16)>,
}

impl Window {
    /// Create a new window with the given bounds and title.
    ///
    /// Creates a [`FrameType::Window`] frame (closeable, resizable).
    /// Interior container is automatically sized to fit inside the frame.
    #[must_use]
    pub fn new(bounds: Rect, title: &str) -> Self {
        let frame = Frame::new(bounds, title, FrameType::Window);
        let interior_rect = frame.interior_area();
        let interior = Container::new(interior_rect);
        Self {
            base: ViewBase::new(bounds),
            frame,
            interior,
            drag_offset: None,
            resize_start: None,
            resize_mouse_start: None,
            min_size: (10, 4),
            prev_bounds: None,
            drag_limits: None,
            minimized_bounds: None,
            minimized_max_width: 30,
            scroll_offset: (0, 0),
            content_size: None,
        }
    }

    /// Create a new dialog window.
    ///
    /// Uses [`FrameType::Dialog`] — not closeable or resizable by default.
    #[must_use]
    pub fn dialog(bounds: Rect, title: &str) -> Self {
        let frame = Frame::new(bounds, title, FrameType::Dialog);
        let interior_rect = frame.interior_area();
        let interior = Container::new(interior_rect);
        Self {
            base: ViewBase::new(bounds),
            frame,
            interior,
            drag_offset: None,
            resize_start: None,
            resize_mouse_start: None,
            min_size: (10, 4),
            prev_bounds: None,
            drag_limits: None,
            minimized_bounds: None,
            minimized_max_width: 30,
            scroll_offset: (0, 0),
            content_size: None,
        }
    }

    // ── BuilderLite ─────────────────────────────────────────────────────────

    /// Create a window from a [`FrameConfig`].
    ///
    /// This allows full control over frame features at construction time.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use turbo_tui::prelude::*;
    /// use turbo_tui::frame::FrameConfig;
    ///
    /// let win = Window::with_config(
    ///     Rect::new(5, 5, 40, 15),
    ///     "Editor",
    ///     FrameConfig::window().with_v_scrollbar(true),
    /// );
    /// ```
    #[must_use]
    pub fn with_config(bounds: Rect, title: &str, config: impl Into<FrameConfig>) -> Self {
        let config = config.into();
        let frame = Frame::from_config(bounds, title, &config);
        let interior_rect = frame.interior_area();
        let interior = Container::new(interior_rect);
        Self {
            base: ViewBase::new(bounds),
            frame,
            interior,
            drag_offset: None,
            resize_start: None,
            resize_mouse_start: None,
            min_size: (10, 4),
            prev_bounds: None,
            drag_limits: None,
            minimized_bounds: None,
            minimized_max_width: 30,
            scroll_offset: (0, 0),
            content_size: None,
        }
    }

    /// Set the minimum window size (Builder Lite).
    ///
    /// Existing `set_min_size()` stays for runtime mutation.
    #[must_use]
    pub fn with_min_size(mut self, w: u16, h: u16) -> Self {
        self.min_size = (w, h);
        self
    }

    /// Set the drag limits (Builder Lite).
    ///
    /// The window cannot be moved or resized outside this rect.
    /// Existing `set_drag_limits()` stays for runtime mutation.
    #[must_use]
    pub fn with_drag_limits(mut self, limits: Rect) -> Self {
        self.drag_limits = Some(limits);
        self
    }

    /// Enable or disable scrollbars (Builder Lite).
    ///
    /// Creates/removes scrollbars on the frame.
    #[must_use]
    pub fn with_scrollbars(mut self, vertical: bool, horizontal: bool) -> Self {
        if vertical {
            if self.frame.v_scrollbar().is_none() {
                let bounds = self.frame.bounds();
                let sb = crate::scrollbar::ScrollBar::vertical(Rect::new(
                    0,
                    0,
                    1,
                    bounds.height.saturating_sub(2),
                ));
                self.frame.set_v_scrollbar(sb);
            }
        } else {
            self.frame.remove_v_scrollbar();
        }
        if horizontal {
            if self.frame.h_scrollbar().is_none() {
                let bounds = self.frame.bounds();
                let sb = crate::scrollbar::ScrollBar::horizontal(Rect::new(
                    0,
                    0,
                    bounds.width.saturating_sub(2),
                    1,
                ));
                self.frame.set_h_scrollbar(sb);
            }
        } else {
            self.frame.remove_h_scrollbar();
        }
        // Recalculate interior since scrollbars affect available space
        let interior_rect = self.frame.interior_area();
        self.interior.set_bounds(interior_rect);
        self
    }

    /// Set whether the window is closeable (Builder Lite).
    #[must_use]
    pub fn with_closeable(mut self, yes: bool) -> Self {
        self.frame.set_closeable(yes);
        self
    }

    /// Set whether the window is resizable (Builder Lite).
    #[must_use]
    pub fn with_resizable(mut self, yes: bool) -> Self {
        self.frame.set_resizable(yes);
        self
    }

    /// Set the maximized width for the minimized title bar (Builder Lite).
    #[must_use]
    pub fn with_minimized_max_width(mut self, width: u16) -> Self {
        self.minimized_max_width = width;
        self
    }

    /// Set the logical content size for scrolling (Builder Lite).
    ///
    /// When content is larger than the interior, scrollbars automatically
    /// adjust their range. Pass `None` to auto-detect from child bounds.
    #[must_use]
    pub fn with_content_size(mut self, width: u16, height: u16) -> Self {
        self.content_size = Some((width, height));
        self.update_scrollbar_params();
        self
    }

    // ── Presets ──────────────────────────────────────────────────────────────

    /// Editor window preset — vertical scrollbar, generous min size.
    ///
    /// Creates a standard closeable, resizable window with a vertical scrollbar
    /// and a minimum size of 20×8. Suitable for text editors, code views, etc.
    #[must_use]
    pub fn editor(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title)
            .with_scrollbars(true, false)
            .with_min_size(20, 8)
    }

    /// Palette window preset — small, not resizable, not closeable.
    ///
    /// Creates a fixed-size tool palette window. Suitable for color pickers,
    /// tool panels, floating inspectors, etc.
    #[must_use]
    pub fn palette(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title)
            .with_resizable(false)
            .with_closeable(false)
    }

    /// Tool window preset — small, resizable with compact min size.
    ///
    /// Creates a resizable tool window with a minimum size of 10×5.
    /// Suitable for floating tool windows, property inspectors, etc.
    #[must_use]
    pub fn tool(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title).with_min_size(10, 5)
    }

    // ── Accessors ────────────────────────────────────────────────────────────

    /// Get the window title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.frame.title()
    }

    /// Set the window title.
    pub fn set_title(&mut self, title: &str) {
        self.frame.set_title(title);
    }

    /// Get an immutable reference to the frame.
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Get a mutable reference to the frame.
    pub fn frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    /// Get an immutable reference to the interior container.
    #[must_use]
    pub fn interior(&self) -> &Container {
        &self.interior
    }

    /// Get a mutable reference to the interior container.
    pub fn interior_mut(&mut self) -> &mut Container {
        &mut self.interior
    }

    /// Get the minimum window size `(width, height)`.
    #[must_use]
    pub fn min_size(&self) -> (u16, u16) {
        self.min_size
    }

    /// Set the minimum window size.
    pub fn set_min_size(&mut self, min_w: u16, min_h: u16) {
        self.min_size = (min_w, min_h);
    }

    /// Get the drag limits if set.
    #[must_use]
    pub fn drag_limits(&self) -> Option<Rect> {
        self.drag_limits
    }

    /// Set the drag limits — the window cannot be moved or resized outside this rect.
    pub fn set_drag_limits(&mut self, limits: Rect) {
        self.drag_limits = Some(limits);
    }

    /// Clear the drag limits.
    pub fn clear_drag_limits(&mut self) {
        self.drag_limits = None;
    }

    /// Add a child view to the window's interior container.
    ///
    /// Convenience method that delegates to `self.interior.add()`.
    /// The child's bounds must be **relative** to the interior container's top-left.
    pub fn add(&mut self, child: Box<dyn View>) -> ViewId {
        self.interior.add(child)
    }

    /// Check if the window is currently being dragged.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.base.state() & SF_DRAGGING != 0
    }

    /// Check if the window is currently being resized.
    #[must_use]
    pub fn is_resizing(&self) -> bool {
        self.base.state() & SF_RESIZING != 0
    }

    /// Check if the window is zoomed (maximized).
    ///
    /// A window is considered zoomed when `prev_bounds` is `Some`, meaning
    /// the previous (non-maximized) bounds have been saved.
    #[must_use]
    pub fn is_zoomed(&self) -> bool {
        self.prev_bounds.is_some()
    }

    /// Toggle zoom: maximize to `drag_limits` (or full area) / restore to `prev_bounds`.
    ///
    /// If the window is not zoomed, saves current bounds and maximizes.
    /// If already zoomed, restores the saved bounds.
    /// If the window is minimized, restores it first before zooming.
    pub fn toggle_zoom(&mut self, screen_size: Rect) {
        // If minimized, restore first before zooming
        if self.is_minimized() {
            self.restore();
        }
        if let Some(prev) = self.prev_bounds.take() {
            // Restore
            self.update_bounds(prev);
        } else {
            // Maximize
            let saved = self.base.bounds();
            self.prev_bounds = Some(saved);
            let max_bounds = self.drag_limits.unwrap_or(screen_size);
            self.update_bounds(max_bounds);
        }
    }

    /// Minimize the window: collapse to a single title bar row.
    ///
    /// The window saves its current bounds and shrinks to just the frame's
    /// top border row. Width is clamped to `minimized_max_width`.
    /// The interior is hidden. Double-click or clicking the restore button restores.
    ///
    /// Sets `SF_MINIMIZED` on the window state.
    ///
    /// Note: The horizontal position (x, y) will be overridden by Desktop's shelf layout.
    /// This method only shrinks the window in-place.
    pub fn minimize(&mut self) {
        if self.is_minimized() {
            return;
        }
        // Save current bounds for later restore
        self.minimized_bounds = Some(self.base.bounds());
        // Calculate minimized width: title + close button + padding, clamped to max
        let title_len = u16::try_from(self.frame.title().chars().count()).unwrap_or(0);
        let min_width = (title_len + 6).min(self.minimized_max_width).max(10);
        // Shrink to 1-row title bar. Position (x, y) will be overridden by Desktop's shelf.
        let b = self.base.bounds();
        self.update_bounds(Rect::new(b.x, b.y, min_width, 1));
        let st = self.base.state();
        self.base.set_state(st | SF_MINIMIZED);
    }

    /// Get the width this window will have when minimized (title + padding, clamped to max).
    ///
    /// Useful for Desktop to calculate shelf layout before the window is actually minimized.
    #[must_use]
    pub fn minimized_width(&self) -> u16 {
        let title_len = u16::try_from(self.frame.title().chars().count()).unwrap_or(0);
        (title_len + 6).min(self.minimized_max_width).max(10)
    }

    /// Restore a minimized window to its previous bounds.
    ///
    /// Clears `SF_MINIMIZED` from the window state.
    pub fn restore(&mut self) {
        if let Some(prev) = self.minimized_bounds.take() {
            self.update_bounds(prev);
            let st = self.base.state();
            self.base.set_state(st & !SF_MINIMIZED);
        }
    }

    /// Check if the window is minimized.
    #[must_use]
    pub fn is_minimized(&self) -> bool {
        self.base.state() & SF_MINIMIZED != 0
    }

    /// Get the maximum width for minimized title bar.
    #[must_use]
    pub fn minimized_max_width(&self) -> u16 {
        self.minimized_max_width
    }

    /// Set the maximum width for minimized title bar.
    pub fn set_minimized_max_width(&mut self, width: u16) {
        self.minimized_max_width = width;
    }

    // ── Scroll offset and content size ───────────────────────────────────────────

    /// Get the current scroll offset `(x, y)` in cells.
    #[must_use]
    pub fn scroll_offset(&self) -> (i32, i32) {
        self.scroll_offset
    }

    /// Set the scroll offset directly.
    ///
    /// Clamps to valid range based on content size vs interior size.
    /// Also syncs scrollbar values.
    pub fn set_scroll_offset(&mut self, x: i32, y: i32) {
        let (max_x, max_y) = self.max_scroll_offset();
        self.scroll_offset = (x.clamp(0, max_x), y.clamp(0, max_y));
        self.sync_scrollbars_from_offset();
    }

    /// Get the logical content size if explicitly set.
    #[must_use]
    pub fn content_size(&self) -> Option<(u16, u16)> {
        self.content_size
    }

    /// Set the logical content size explicitly.
    ///
    /// This determines the scrollbar range: `max_scroll = content_size - interior_size`.
    /// Pass `None` to auto-calculate from child bounds.
    /// After setting, scrollbar params are updated and scroll offset is clamped.
    pub fn set_content_size(&mut self, size: Option<(u16, u16)>) {
        self.content_size = size;
        self.update_scrollbar_params();
    }

    /// Close this window by posting a `CM_CLOSE` command as a deferred event.
    ///
    /// Sets the event as handled.
    pub fn close(&self, event: &mut Event) {
        event.post(Event::command(CM_CLOSE));
        event.clear();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Update bounds for window, frame, and interior simultaneously.
    ///
    /// This is critical for keeping frame, interior, and base in sync.
    fn update_bounds(&mut self, new_bounds: Rect) {
        self.base.set_bounds(new_bounds);
        self.frame.set_bounds(new_bounds);
        let interior_rect = self.frame.interior_area();
        self.interior.set_bounds(interior_rect);
        self.update_scrollbar_params();
    }

    // ── Scroll offset helpers ───────────────────────────────────────────────────

    /// Calculate the effective content size.
    ///
    /// If `content_size` is explicitly set, returns that.
    /// Otherwise, calculates the bounding box of all children relative to the interior.
    pub(crate) fn effective_content_size(&self) -> (u16, u16) {
        if let Some(size) = self.content_size {
            return size;
        }

        // Auto-calculate from children bounds
        let interior = self.frame.interior_area();
        let mut max_right: u16 = 0;
        let mut max_bottom: u16 = 0;

        for child in self.interior.children() {
            let cb = child.bounds();
            // Child bounds are absolute — convert to relative to interior
            let rel_right = (cb.x + cb.width).saturating_sub(interior.x);
            let rel_bottom = (cb.y + cb.height).saturating_sub(interior.y);
            max_right = max_right.max(rel_right);
            max_bottom = max_bottom.max(rel_bottom);
        }

        // Content size is at least the interior size
        (
            max_right.max(interior.width),
            max_bottom.max(interior.height),
        )
    }

    /// Calculate the maximum scroll offset based on content vs interior size.
    fn max_scroll_offset(&self) -> (i32, i32) {
        let interior = self.frame.interior_area();
        let (cw, ch) = self.effective_content_size();
        let max_x = i32::from(cw)
            .saturating_sub(i32::from(interior.width))
            .max(0);
        let max_y = i32::from(ch)
            .saturating_sub(i32::from(interior.height))
            .max(0);
        (max_x, max_y)
    }

    /// Update scrollbar parameters based on content size and interior size.
    ///
    /// Called after resize, `content_size` change, or child addition.
    fn update_scrollbar_params(&mut self) {
        let interior = self.frame.interior_area();
        let (_cw, _ch) = self.effective_content_size();
        let (max_x, max_y) = self.max_scroll_offset();

        // Update vertical scrollbar
        if let Some(sb) = self.frame.v_scrollbar_mut() {
            let page = i32::from(interior.height).max(1);
            sb.set_params(self.scroll_offset.1, 0, max_y, page, 1);
        }

        // Update horizontal scrollbar
        if let Some(sb) = self.frame.h_scrollbar_mut() {
            let page = i32::from(interior.width).max(1);
            sb.set_params(self.scroll_offset.0, 0, max_x, page, 1);
        }

        // Clamp scroll offset if content shrank
        self.scroll_offset.0 = self.scroll_offset.0.clamp(0, max_x);
        self.scroll_offset.1 = self.scroll_offset.1.clamp(0, max_y);
    }

    /// Sync scrollbar values FROM the current `scroll_offset`.
    fn sync_scrollbars_from_offset(&mut self) {
        if let Some(sb) = self.frame.v_scrollbar_mut() {
            sb.set_value(self.scroll_offset.1);
        }
        if let Some(sb) = self.frame.h_scrollbar_mut() {
            sb.set_value(self.scroll_offset.0);
        }
    }

    /// Sync `scroll_offset` FROM scrollbar values (after user interacts with scrollbar).
    fn sync_offset_from_scrollbars(&mut self) {
        if let Some(sb) = self.frame.v_scrollbar() {
            self.scroll_offset.1 = sb.value();
        }
        if let Some(sb) = self.frame.h_scrollbar() {
            self.scroll_offset.0 = sb.value();
        }
    }

    /// Start a drag operation from the given mouse position.
    #[allow(clippy::cast_possible_wrap)]
    fn start_drag(&mut self, mouse_col: u16, mouse_row: u16) {
        let b = self.base.bounds();
        self.drag_offset = Some((mouse_col as i16 - b.x as i16, mouse_row as i16 - b.y as i16));
        let st = self.base.state();
        self.base.set_state(st | SF_DRAGGING);
        self.frame.set_state(self.frame.state() | SF_DRAGGING);
    }

    /// Continue drag: update window position based on current mouse position.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn continue_drag(&mut self, mouse_col: u16, mouse_row: u16) {
        if let Some((dx, dy)) = self.drag_offset {
            let mut new_x = (mouse_col as i16 - dx).max(0) as u16;
            let mut new_y = (mouse_row as i16 - dy).max(0) as u16;

            // Clamp to drag limits if set
            if let Some(limits) = self.drag_limits {
                let b = self.base.bounds();
                new_x = new_x.clamp(limits.x, limits.x + limits.width.saturating_sub(b.width));
                new_y = new_y.clamp(limits.y, limits.y + limits.height.saturating_sub(b.height));
            }

            let b = self.base.bounds();
            self.update_bounds(Rect::new(new_x, new_y, b.width, b.height));
        }
    }

    /// End drag or resize: clear state flags and stored start positions.
    fn end_drag_resize(&mut self) {
        self.drag_offset = None;
        self.resize_start = None;
        self.resize_mouse_start = None;
        let st = self.base.state();
        self.base.set_state(st & !(SF_DRAGGING | SF_RESIZING));
        self.frame
            .set_state(self.frame.state() & !(SF_DRAGGING | SF_RESIZING));
    }

    /// Start a resize operation from the given mouse position.
    fn start_resize(&mut self, mouse_col: u16, mouse_row: u16) {
        let b = self.base.bounds();
        self.resize_start = Some((b.x, b.y, b.width, b.height));
        self.resize_mouse_start = Some((mouse_col, mouse_row));
        let st = self.base.state();
        self.base.set_state(st | SF_RESIZING);
        self.frame.set_state(self.frame.state() | SF_RESIZING);
    }

    /// Continue resize: update window size based on mouse delta.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn continue_resize(&mut self, mouse_col: u16, mouse_row: u16) {
        if let (Some((sx, sy, sw, sh)), Some((mx, my))) =
            (self.resize_start, self.resize_mouse_start)
        {
            let delta_w = i32::from(mouse_col) - i32::from(mx);
            let delta_h = i32::from(mouse_row) - i32::from(my);

            let new_w = (i32::from(sw) + delta_w).max(i32::from(self.min_size.0)) as u16;
            let new_h = (i32::from(sh) + delta_h).max(i32::from(self.min_size.1)) as u16;

            // Clamp to drag limits if set
            let (clamped_w, clamped_h) = if let Some(limits) = self.drag_limits {
                let max_w = (limits.x + limits.width).saturating_sub(sx);
                let max_h = (limits.y + limits.height).saturating_sub(sy);
                (new_w.min(max_w), new_h.min(max_h))
            } else {
                (new_w, new_h)
            };

            self.update_bounds(Rect::new(sx, sy, clamped_w, clamped_h));
        }
    }

    /// Draw children with scroll offset applied.
    ///
    /// Temporarily shifts each child's bounds by `-scroll_offset` for drawing,
    /// then restores the original bounds. Children outside the visible area
    /// after the shift are culled (not drawn).
    fn draw_scrolled_children(&self, buf: &mut Buffer, clip: Rect) {
        use crate::view::SF_VISIBLE;

        let (sx, sy) = self.scroll_offset;
        let _interior = self.frame.interior_area();

        for child in self.interior.children() {
            if child.state() & SF_VISIBLE == 0 {
                continue;
            }
            let original = child.bounds();

            // Calculate shifted bounds (relative to interior origin)
            let shifted_x = i32::from(original.x) - sx;
            let shifted_y = i32::from(original.y) - sy;

            // Cull: if shifted bounds are entirely outside clip, skip
            if shifted_x >= i32::from(clip.x + clip.width)
                || shifted_y >= i32::from(clip.y + clip.height)
            {
                continue;
            }
            let shifted_right = shifted_x + i32::from(original.width);
            let shifted_bottom = shifted_y + i32::from(original.height);
            if shifted_right <= i32::from(clip.x) || shifted_bottom <= i32::from(clip.y) {
                continue;
            }

            // Create a mini buffer for this child's original area
            if original.width == 0 || original.height == 0 {
                continue;
            }
            let mut temp_buf = ratatui::buffer::Buffer::empty(original);
            // Pre-fill with window interior style so unpainted cells don't
            // overwrite the background with default (no-color) style
            let bg_style = theme::with_current(|t| t.window_interior);
            for row_idx in original.y..original.y + original.height {
                for col_idx in original.x..original.x + original.width {
                    if let Some(cell) =
                        temp_buf.cell_mut(ratatui::layout::Position::new(col_idx, row_idx))
                    {
                        cell.set_char(' ').set_style(bg_style);
                    }
                }
            }
            child.draw(&mut temp_buf, original);

            // Copy visible cells from temp to main buf, applying scroll offset
            for row in 0..original.height {
                for col in 0..original.width {
                    let dest_x = i32::from(original.x + col) - sx;
                    let dest_y = i32::from(original.y + row) - sy;

                    // Check if destination is within clip
                    if dest_x < i32::from(clip.x)
                        || dest_x >= i32::from(clip.x + clip.width)
                        || dest_y < i32::from(clip.y)
                        || dest_y >= i32::from(clip.y + clip.height)
                    {
                        continue;
                    }

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let (dx, dy) = (dest_x as u16, dest_y as u16);
                    let src_pos = Position::new(original.x + col, original.y + row);
                    let dst_pos = Position::new(dx, dy);

                    if let Some(src_cell) = temp_buf.cell(src_pos) {
                        if let Some(dst_cell) = buf.cell_mut(dst_pos) {
                            dst_cell.set_symbol(src_cell.symbol());
                            dst_cell.set_style(src_cell.style());
                        }
                    }
                }
            }
        }
    }

    /// Fill the interior area with the window background style.
    ///
    /// This prevents background bleed-through when children don't cover the full interior.
    fn fill_interior(&self, buf: &mut Buffer, clip: Rect) {
        let interior = self.frame.interior_area();
        let fill_area = interior.intersection(clip);
        if fill_area.width == 0 || fill_area.height == 0 {
            return;
        }
        let style = theme::with_current(|t| t.window_interior);
        for row in fill_area.y..fill_area.y + fill_area.height {
            for col in fill_area.x..fill_area.x + fill_area.width {
                if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                    cell.set_char(' ').set_style(style);
                }
            }
        }
    }
}

// ============================================================================
// View trait implementation
// ============================================================================

impl View for Window {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.update_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        let b = self.base.bounds();
        let draw_area = b.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        // 1. Draw frame (border, title, close button, resize handle)
        self.frame.draw(buf, clip);

        // 2. If minimized (height == 1), skip interior
        if self.is_minimized() {
            return;
        }

        // 3. Fill interior with background (prevents bleed-through)
        self.fill_interior(buf, clip);

        // 4. Draw children with scroll offset applied
        let interior_clip = self.frame.interior_area().intersection(clip);
        if self.scroll_offset == (0, 0) {
            // Fast path: no scrolling, just draw normally
            self.interior.draw(buf, interior_clip);
        } else {
            // Scroll offset: draw children with offset translation
            self.draw_scrolled_children(buf, interior_clip);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind.clone() {
            EventKind::Mouse(mouse) => {
                let col = mouse.column;
                let row = mouse.row;

                match mouse.kind {
                    // Mouse down — check what was clicked
                    MouseEventKind::Down(MouseButton::Left) => {
                        // If minimized, any click restores (except close button)
                        if self.is_minimized() {
                            if self.frame.is_close_button(col, row) {
                                self.close(event);
                                return;
                            }
                            self.restore();
                            event.clear();
                            return;
                        }

                        // Close button?
                        if self.frame.is_close_button(col, row) {
                            self.close(event);
                            return;
                        }

                        // Minimize button?
                        if self.frame.is_minimize_button(col, row) {
                            self.minimize();
                            event.post(Event::command(CM_MINIMIZE));
                            event.clear();
                            return;
                        }

                        // Maximize button?
                        if self.frame.is_maximize_button(col, row) {
                            let screen = self.drag_limits.unwrap_or(Rect::new(0, 0, 80, 24));
                            self.toggle_zoom(screen);
                            event.post(Event::command(CM_ZOOM));
                            event.clear();
                            return;
                        }

                        // Resize handle?
                        if self.frame.is_resize_handle(col, row) {
                            self.start_resize(col, row);
                            event.clear();
                            return;
                        }

                        // Scrollbar click? Forward to frame scrollbar.
                        if self.frame.handle_scrollbar_click(col, row, event) {
                            self.sync_offset_from_scrollbars();
                            return;
                        }

                        // Title bar? Start drag.
                        if self.frame.is_title_bar(col, row) {
                            self.start_drag(col, row);
                            event.clear();
                            return;
                        }

                        // Interior? Delegate to container.
                        let interior = self.frame.interior_area();
                        if col >= interior.x
                            && col < interior.x + interior.width
                            && row >= interior.y
                            && row < interior.y + interior.height
                        {
                            self.interior.handle_event(event);
                        }
                    }

                    // Drag events — continue drag or resize
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if self.is_dragging() {
                            self.continue_drag(col, row);
                            event.clear();
                        } else if self.is_resizing() {
                            self.continue_resize(col, row);
                            event.clear();
                        } else {
                            // Check if a scrollbar thumb is being dragged
                            if self.frame.handle_scrollbar_click(col, row, event) {
                                self.sync_offset_from_scrollbars();
                            } else {
                                // Forward drag to interior (e.g., other scrollbar-like widgets)
                                self.interior.handle_event(event);
                            }
                        }
                    }

                    // Mouse up — end drag/resize
                    MouseEventKind::Up(MouseButton::Left) => {
                        if self.is_dragging() || self.is_resizing() {
                            self.end_drag_resize();
                            event.clear();
                        }
                    }

                    // Mouse moved (no button) — update hover states
                    MouseEventKind::Moved => {
                        let b = self.base.bounds();
                        if col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height {
                            // Inside window — update frame hover (buttons, resize handle)
                            self.frame.update_hover(col, row);
                            // Update scrollbar hover state
                            self.frame.update_scrollbar_hover(col, row);
                            // Also forward to interior for child hover tracking
                            self.interior.handle_event(event);
                        } else {
                            // Outside window — clear all hover states
                            self.frame.clear_hover();
                            self.frame.clear_scrollbar_hover();
                        }
                    }

                    // Scroll wheel → adjust scrollbar value
                    MouseEventKind::ScrollUp => {
                        if let Some(sb) = self.frame.v_scrollbar_mut() {
                            let step = sb.arrow_step();
                            let new_val = sb.value().saturating_sub(step * 3);
                            sb.set_value(new_val);
                            self.sync_offset_from_scrollbars();
                            event.clear();
                        } else if !self.is_minimized() {
                            self.interior.handle_event(event);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if let Some(sb) = self.frame.v_scrollbar_mut() {
                            let step = sb.arrow_step();
                            let new_val = sb.value().saturating_add(step * 3);
                            sb.set_value(new_val);
                            self.sync_offset_from_scrollbars();
                            event.clear();
                        } else if !self.is_minimized() {
                            self.interior.handle_event(event);
                        }
                    }

                    // Other mouse events (right-click, etc.) → interior
                    _ => {
                        if !self.is_minimized() {
                            self.interior.handle_event(event);
                        }
                    }
                }
            }

            // Key, command, and resize → delegate to interior
            EventKind::Key(_) | EventKind::Command(_) | EventKind::Resize(_, _) => {
                if !self.is_minimized() {
                    self.interior.handle_event(event);
                }
            }

            // Broadcast commands
            EventKind::Broadcast(cmd) => {
                if *cmd == CM_SCROLL_CHANGED && !self.is_minimized() {
                    // A scrollbar value changed — sync offset
                    self.sync_offset_from_scrollbars();
                }
                if !self.is_minimized() {
                    self.interior.handle_event(event);
                }
            }

            EventKind::None => {}
        }
    }

    /// Window can always receive focus.
    fn can_focus(&self) -> bool {
        true
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
        let is_focused = state & SF_FOCUSED != 0;
        // Propagate focus state to frame (for active/inactive border rendering)
        if is_focused {
            self.frame.set_state(self.frame.state() | SF_FOCUSED);
        } else {
            self.frame.set_state(self.frame.state() & !SF_FOCUSED);
        }
        // Propagate active state to frame scrollbars
        if let Some(sb) = self.frame.v_scrollbar_mut() {
            sb.set_active(is_focused);
        }
        if let Some(sb) = self.frame.h_scrollbar_mut() {
            sb.set_active(is_focused);
        }
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_CLOSE, CM_MINIMIZE, CM_ZOOM};
    use crate::frame::{FrameConfig, FrameType};
    use crate::theme::Theme;
    use crate::view::{EventKind, SF_DRAGGING, SF_FOCUSED, SF_MINIMIZED, SF_RESIZING};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn setup_theme() {
        crate::theme::set(Theme::turbo_vision());
    }

    fn mouse_down(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn mouse_drag(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn mouse_up(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn test_window_new_defaults() {
        setup_theme();
        let win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        assert_eq!(win.min_size(), (10, 4));
        assert_eq!(win.frame().frame_type(), FrameType::Window);
        assert!(!win.is_dragging());
        assert!(!win.is_resizing());
        assert!(!win.is_zoomed());
        assert!(win.drag_limits().is_none());
    }

    #[test]
    fn test_window_dialog_defaults() {
        setup_theme();
        let win = Window::dialog(Rect::new(5, 5, 40, 20), "Dialog");

        assert_eq!(win.frame().frame_type(), FrameType::Dialog);
        assert!(!win.frame().closeable());
        assert!(!win.frame().resizable());
        assert!(!win.is_dragging());
        assert!(!win.is_resizing());
    }

    #[test]
    fn test_window_set_title() {
        setup_theme();
        let mut win = Window::new(Rect::new(0, 0, 20, 10), "Old");

        assert_eq!(win.title(), "Old");
        win.set_title("New Title");
        assert_eq!(win.title(), "New Title");
        assert_eq!(win.frame().title(), "New Title");
    }

    #[test]
    fn test_window_interior_matches_frame() {
        setup_theme();
        let win = Window::new(Rect::new(10, 5, 40, 20), "Test");

        let expected_interior = win.frame().interior_area();
        assert_eq!(win.interior().bounds(), expected_interior);
    }

    #[test]
    fn test_window_add_child() {
        setup_theme();
        use crate::view::ViewBase;

        struct DummyView {
            base: ViewBase,
        }

        impl View for DummyView {
            fn id(&self) -> ViewId {
                self.base.id()
            }
            fn bounds(&self) -> Rect {
                self.base.bounds()
            }
            fn set_bounds(&mut self, b: Rect) {
                self.base.set_bounds(b);
            }
            fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}
            fn handle_event(&mut self, event: &mut Event) {
                event.handled = true;
            }
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, s: u16) {
                self.base.set_state(s);
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let mut win = Window::new(Rect::new(0, 0, 40, 20), "Test");
        assert_eq!(win.interior().child_count(), 0);

        win.add(Box::new(DummyView {
            base: ViewBase::new(Rect::new(1, 1, 5, 2)),
        }));
        assert_eq!(win.interior().child_count(), 1);
    }

    #[test]
    fn test_window_set_bounds_updates_all() {
        setup_theme();
        let mut win = Window::new(Rect::new(5, 5, 30, 15), "Test");

        let new_bounds = Rect::new(10, 10, 40, 20);
        win.set_bounds(new_bounds);

        assert_eq!(win.bounds(), new_bounds);
        assert_eq!(win.frame().bounds(), new_bounds);

        // Interior should match frame.interior_area() for the new bounds
        let expected_interior = win.frame().interior_area();
        assert_eq!(win.interior().bounds(), expected_interior);
    }

    #[test]
    fn test_window_start_drag() {
        setup_theme();
        // Window at (10, 5, 30, 15): title bar is row 5, cols 10..39
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Click on title bar (col=20, row=5) — past close button (cols 11-13)
        let mut ev = mouse_down(20, 5);
        win.handle_event(&mut ev);

        assert!(
            win.is_dragging(),
            "should be dragging after title bar click"
        );
        assert!(!win.is_resizing());
    }

    #[test]
    fn test_window_continue_drag() {
        setup_theme();
        // Window at (10, 5, 30, 15)
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // MouseDown on title bar at (15, 5) — offset = (5, 0)
        let mut down = mouse_down(15, 5);
        win.handle_event(&mut down);
        assert!(win.is_dragging());

        // Drag to (20, 10) → new position: (20 - 5, 10 - 0) = (15, 10)
        let mut drag = mouse_drag(20, 10);
        win.handle_event(&mut drag);

        assert_eq!(win.bounds().x, 15, "window x should move to 15");
        assert_eq!(win.bounds().y, 10, "window y should move to 10");
        assert_eq!(win.bounds().width, 30, "width unchanged");
        assert_eq!(win.bounds().height, 15, "height unchanged");
    }

    #[test]
    fn test_window_end_drag() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Start drag
        let mut down = mouse_down(15, 5);
        win.handle_event(&mut down);
        assert!(win.is_dragging());

        // MouseUp ends drag
        let mut up = mouse_up(20, 10);
        win.handle_event(&mut up);

        assert!(!win.is_dragging(), "drag should be cleared after MouseUp");
        assert!(!win.is_resizing());
    }

    #[test]
    fn test_window_start_resize() {
        setup_theme();
        // Window at (10, 5, 30, 15): resize handle at bottom-right (39, 19)
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Click resize handle
        let mut ev = mouse_down(39, 19);
        win.handle_event(&mut ev);

        assert!(
            win.is_resizing(),
            "should be resizing after resize handle click"
        );
        assert!(!win.is_dragging());
    }

    #[test]
    fn test_window_continue_resize() {
        setup_theme();
        // Window at (10, 5, 30, 15): resize handle at (39, 19)
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Start resize from bottom-right corner
        let mut down = mouse_down(39, 19);
        win.handle_event(&mut down);
        assert!(win.is_resizing());

        // Drag to (44, 24): delta = (+5, +5) → new size (35, 20)
        let mut drag = mouse_drag(44, 24);
        win.handle_event(&mut drag);

        assert_eq!(win.bounds().x, 10, "x unchanged during resize");
        assert_eq!(win.bounds().y, 5, "y unchanged during resize");
        assert_eq!(win.bounds().width, 35, "width grew by 5");
        assert_eq!(win.bounds().height, 20, "height grew by 5");
    }

    #[test]
    fn test_window_resize_clamps_to_min_size() {
        setup_theme();
        // Window at (10, 5, 30, 15)
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Start resize from bottom-right (39, 19)
        let mut down = mouse_down(39, 19);
        win.handle_event(&mut down);

        // Try to resize to almost nothing: drag far up-left
        // drag to (15, 8): delta = (15-39, 8-19) = (-24, -11) → new size clamped to min (10, 4)
        let mut drag = mouse_drag(15, 8);
        win.handle_event(&mut drag);

        assert_eq!(win.bounds().width, win.min_size().0, "width clamped to min");
        assert_eq!(
            win.bounds().height,
            win.min_size().1,
            "height clamped to min"
        );
    }

    #[test]
    fn test_window_close_button_posts_cm_close() {
        setup_theme();
        // Window at (10, 5, 30, 15): close button at cols 11-13, row 5
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        let mut ev = mouse_down(12, 5); // click close button [■]
        win.handle_event(&mut ev);

        assert!(
            ev.is_cleared(),
            "event should be cleared after close button"
        );
        assert_eq!(
            ev.deferred.len(),
            1,
            "one deferred event (CM_CLOSE) should be posted"
        );
        match &ev.deferred[0].kind {
            EventKind::Command(id) => assert_eq!(*id, CM_CLOSE, "deferred should be CM_CLOSE"),
            _ => panic!("expected Command event in deferred"),
        }
    }

    #[test]
    fn test_window_toggle_zoom() {
        setup_theme();
        let original_bounds = Rect::new(10, 5, 30, 15);
        let mut win = Window::new(original_bounds, "Test");
        let screen = Rect::new(0, 0, 80, 24);

        assert!(!win.is_zoomed());

        // Zoom in — should maximize to screen_size
        win.toggle_zoom(screen);
        assert!(win.is_zoomed());
        assert_eq!(win.bounds(), screen, "zoomed window fills screen");

        // Zoom out — should restore original bounds
        win.toggle_zoom(screen);
        assert!(!win.is_zoomed());
        assert_eq!(win.bounds(), original_bounds, "restored to original bounds");
    }

    #[test]
    fn test_window_set_state_propagates_focus_to_frame() {
        setup_theme();
        let mut win = Window::new(Rect::new(0, 0, 30, 15), "Test");

        // Initially not focused
        assert_eq!(win.frame().state() & SF_FOCUSED, 0);

        // Set focused
        win.set_state(win.state() | SF_FOCUSED);
        assert_ne!(
            win.frame().state() & SF_FOCUSED,
            0,
            "SF_FOCUSED should propagate to frame"
        );

        // Clear focused
        win.set_state(win.state() & !SF_FOCUSED);
        assert_eq!(
            win.frame().state() & SF_FOCUSED,
            0,
            "SF_FOCUSED removal should propagate to frame"
        );
    }

    #[test]
    fn test_window_drag_clamped_to_limits() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 20, 10), "Test");
        win.set_drag_limits(Rect::new(0, 0, 80, 24));

        // Start drag on title bar (col=15, row=5)
        let mut down = mouse_down(15, 5);
        win.handle_event(&mut down);

        // Try to drag past the right limit: drag to (75, 5)
        // offset is (5, 0), so new_x = 75 - 5 = 70, but max is 80 - 20 = 60
        let mut drag = mouse_drag(75, 5);
        win.handle_event(&mut drag);

        assert!(
            win.bounds().x <= 60,
            "window x should not exceed drag limit"
        );
    }

    #[test]
    fn test_window_can_focus() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 30, 15), "Test");
        assert!(win.can_focus());
    }

    #[test]
    fn test_window_view_id_unique() {
        setup_theme();
        let w1 = Window::new(Rect::new(0, 0, 30, 15), "W1");
        let w2 = Window::new(Rect::new(10, 5, 30, 15), "W2");
        assert_ne!(w1.id(), w2.id());
    }

    #[test]
    fn test_window_drag_flag_cleared_by_end() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        // Start drag
        let mut down = mouse_down(20, 5);
        win.handle_event(&mut down);
        assert!(win.is_dragging());

        // SF_DRAGGING is in base state
        assert_ne!(win.state() & SF_DRAGGING, 0);

        // End drag
        let mut up = mouse_up(25, 8);
        win.handle_event(&mut up);
        assert_eq!(win.state() & SF_DRAGGING, 0);
        assert_eq!(win.state() & SF_RESIZING, 0);
    }

    #[test]
    fn test_window_resize_flag_cleared_by_up() {
        setup_theme();
        // Window at (10, 5, 30, 15): resize handle at (39, 19)
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        let mut down = mouse_down(39, 19);
        win.handle_event(&mut down);
        assert!(win.is_resizing());
        assert_ne!(win.state() & SF_RESIZING, 0);

        let mut up = mouse_up(45, 22);
        win.handle_event(&mut up);
        assert_eq!(win.state() & SF_RESIZING, 0);
        assert!(!win.is_resizing());
    }

    #[test]
    fn test_window_minimize_sets_sf_minimized() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        assert!(!win.is_minimized());
        assert_eq!(win.state() & SF_MINIMIZED, 0);

        win.minimize();

        assert!(win.is_minimized());
        assert_ne!(win.state() & SF_MINIMIZED, 0);
        assert_eq!(win.bounds().height, 1, "minimized window has height 1");
        assert!(win.bounds().width >= 10, "minimized width at least 10");
        assert!(
            win.bounds().width <= 30,
            "minimized width does not exceed max"
        );
    }

    #[test]
    fn test_window_minimize_saves_bounds_for_restore() {
        setup_theme();
        let original = Rect::new(10, 5, 30, 15);
        let mut win = Window::new(original, "Test");

        win.minimize();
        assert!(win.is_minimized());
        assert_ne!(win.bounds(), original, "bounds changed after minimize");

        win.restore();
        assert!(!win.is_minimized());
        assert_eq!(win.bounds(), original, "bounds restored after restore()");
        assert_eq!(win.state() & SF_MINIMIZED, 0);
    }

    #[test]
    fn test_window_minimize_idempotent() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        win.minimize();
        let bounds_after_first = win.bounds();

        // Second minimize should be a no-op
        win.minimize();
        assert_eq!(win.bounds(), bounds_after_first, "second minimize is no-op");
    }

    #[test]
    fn test_window_restore_when_not_minimized_is_noop() {
        setup_theme();
        let original = Rect::new(10, 5, 30, 15);
        let mut win = Window::new(original, "Test");

        // restore() without prior minimize should be a no-op
        win.restore();
        assert!(!win.is_minimized());
        assert_eq!(win.bounds(), original);
    }

    #[test]
    fn test_window_minimized_max_width_clamps() {
        setup_theme();
        let mut win = Window::new(Rect::new(0, 0, 60, 20), "A Very Long Window Title Here");
        win.set_minimized_max_width(20);

        win.minimize();
        assert!(
            win.bounds().width <= 20,
            "minimized width clamped to max_width"
        );
    }

    #[test]
    fn test_window_minimize_button_click_posts_cm_minimize() {
        setup_theme();
        // Frame::new with FrameType::Window creates minimize/maximize buttons;
        // need to check that the frame has them enabled.
        let mut win = Window::new(Rect::new(10, 5, 40, 15), "Test");

        // Only test if the frame has a minimize button
        if !win.frame().minimizable() {
            return;
        }

        // Find the minimize button column (right side of frame, before maximize)
        // Use the frame's is_minimize_button to probe
        let b = win.bounds();
        let row = b.y;
        let mut min_col = None;
        for col in b.x..b.x + b.width {
            if win.frame().is_minimize_button(col, row) {
                min_col = Some(col);
                break;
            }
        }

        if let Some(col) = min_col {
            let mut ev = mouse_down(col, row);
            win.handle_event(&mut ev);

            assert!(
                win.is_minimized(),
                "window should be minimized after button click"
            );
            assert!(ev.is_cleared(), "event cleared");
            assert_eq!(ev.deferred.len(), 1, "one deferred event posted");
            match &ev.deferred[0].kind {
                EventKind::Command(id) => {
                    assert_eq!(*id, CM_MINIMIZE, "deferred should be CM_MINIMIZE")
                }
                _ => panic!("expected Command event"),
            }
        }
    }

    #[test]
    fn test_window_maximize_button_click_posts_cm_zoom() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 40, 15), "Test");
        win.set_drag_limits(Rect::new(0, 0, 80, 24));

        // Only test if the frame has a maximize button
        if !win.frame().maximizable() {
            return;
        }

        let b = win.bounds();
        let row = b.y;
        let mut max_col = None;
        for col in b.x..b.x + b.width {
            if win.frame().is_maximize_button(col, row) {
                max_col = Some(col);
                break;
            }
        }

        if let Some(col) = max_col {
            let mut ev = mouse_down(col, row);
            win.handle_event(&mut ev);

            assert!(
                win.is_zoomed(),
                "window should be zoomed after maximize button"
            );
            assert!(ev.is_cleared(), "event cleared");
            assert_eq!(ev.deferred.len(), 1, "one deferred event posted");
            match &ev.deferred[0].kind {
                EventKind::Command(id) => {
                    assert_eq!(*id, CM_ZOOM, "deferred should be CM_ZOOM")
                }
                _ => panic!("expected Command event"),
            }
        }
    }

    #[test]
    fn test_window_minimized_click_restores() {
        setup_theme();
        let original = Rect::new(10, 5, 30, 15);
        let mut win = Window::new(original, "Test");

        win.minimize();
        assert!(win.is_minimized());

        // Click on the title bar area while minimized → should restore
        // Use a column that is NOT the close button (e.g., col = b.x + b.width / 2)
        let b = win.bounds();
        let col = b.x + b.width / 2;
        let row = b.y;

        // Ensure it's not the close button
        if !win.frame().is_close_button(col, row) {
            let mut ev = mouse_down(col, row);
            win.handle_event(&mut ev);

            assert!(!win.is_minimized(), "window should be restored after click");
            assert_eq!(win.bounds(), original, "bounds restored");
            assert!(ev.is_cleared());
        }
    }

    #[test]
    fn test_window_minimized_close_button_still_closes() {
        setup_theme();
        let mut win = Window::new(Rect::new(10, 5, 30, 15), "Test");

        win.minimize();
        assert!(win.is_minimized());

        // Click close button while minimized
        let b = win.bounds();
        let row = b.y;
        let mut close_col = None;
        for col in b.x..b.x + b.width {
            if win.frame().is_close_button(col, row) {
                close_col = Some(col);
                break;
            }
        }

        if let Some(col) = close_col {
            let mut ev = mouse_down(col, row);
            win.handle_event(&mut ev);

            assert!(ev.is_cleared(), "event cleared after close");
            assert!(!ev.deferred.is_empty(), "CM_CLOSE posted");
            match &ev.deferred[0].kind {
                EventKind::Command(id) => assert_eq!(*id, CM_CLOSE),
                _ => panic!("expected CM_CLOSE"),
            }
        }
    }

    #[test]
    fn test_window_toggle_zoom_restores_from_minimized() {
        setup_theme();
        let original = Rect::new(10, 5, 30, 15);
        let mut win = Window::new(original, "Test");
        let screen = Rect::new(0, 0, 80, 24);

        win.minimize();
        assert!(win.is_minimized());

        // toggle_zoom should restore from minimized first, then zoom
        win.toggle_zoom(screen);
        assert!(!win.is_minimized(), "no longer minimized after toggle_zoom");
        assert!(win.is_zoomed(), "window is zoomed");
        assert_eq!(win.bounds(), screen, "zoomed to full screen");
    }

    #[test]
    fn test_window_minimized_skips_key_events_to_interior() {
        setup_theme();
        use crate::view::ViewBase;

        struct EventTracker {
            base: ViewBase,
            received: bool,
        }
        impl View for EventTracker {
            fn id(&self) -> ViewId {
                self.base.id()
            }
            fn bounds(&self) -> Rect {
                self.base.bounds()
            }
            fn set_bounds(&mut self, b: Rect) {
                self.base.set_bounds(b);
            }
            fn draw(&self, _: &mut Buffer, _: Rect) {}
            fn handle_event(&mut self, event: &mut Event) {
                self.received = true;
                event.handled = true;
            }
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, s: u16) {
                self.base.set_state(s);
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let mut win = Window::new(Rect::new(0, 0, 40, 20), "Test");
        win.add(Box::new(EventTracker {
            base: ViewBase::new(Rect::new(1, 1, 5, 2)),
            received: false,
        }));

        win.minimize();
        assert!(win.is_minimized());

        // Send a key event — interior should NOT receive it
        use crossterm::event::{KeyCode, KeyEvent};
        let mut key_ev = Event::key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        win.handle_event(&mut key_ev);

        // Access the child to check if it received the event
        let child = win.interior().child_at(0).unwrap();
        let tracker = child.as_any().downcast_ref::<EventTracker>().unwrap();
        assert!(
            !tracker.received,
            "interior should not receive key events when minimized"
        );
    }

    #[test]
    fn test_window_minimized_max_width_accessor() {
        setup_theme();
        let mut win = Window::new(Rect::new(0, 0, 40, 20), "Test");

        assert_eq!(win.minimized_max_width(), 30, "default max width is 30");
        win.set_minimized_max_width(25);
        assert_eq!(win.minimized_max_width(), 25);
    }

    #[test]
    fn test_window_with_config() {
        setup_theme();
        let bounds = Rect::new(5, 5, 40, 15);
        let config = FrameConfig::window().with_v_scrollbar(true);
        let win = Window::with_config(bounds, "Config Test", config);
        assert_eq!(win.title(), "Config Test");
        assert!(win.frame().closeable());
        assert!(win.frame().resizable());
        assert!(win.frame().v_scrollbar().is_some());
    }

    #[test]
    fn test_window_builder_lite_min_size() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 40, 15), "Test").with_min_size(20, 8);
        assert_eq!(win.min_size(), (20, 8));
    }

    #[test]
    fn test_window_builder_lite_drag_limits() {
        setup_theme();
        let limits = Rect::new(0, 0, 80, 25);
        let win = Window::new(Rect::new(5, 5, 30, 10), "Test").with_drag_limits(limits);
        assert_eq!(win.drag_limits(), Some(limits));
    }

    #[test]
    fn test_window_builder_lite_scrollbars() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 40, 15), "Scroll").with_scrollbars(true, true);
        assert!(win.frame().v_scrollbar().is_some());
        assert!(win.frame().h_scrollbar().is_some());
    }

    #[test]
    fn test_window_builder_lite_closeable() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 40, 15), "Test").with_closeable(false);
        assert!(!win.frame().closeable());
    }

    #[test]
    fn test_window_builder_lite_chain() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 40, 15), "Chained")
            .with_scrollbars(true, false)
            .with_min_size(20, 8)
            .with_drag_limits(Rect::new(0, 0, 80, 25))
            .with_resizable(true)
            .with_closeable(true);
        assert!(win.frame().v_scrollbar().is_some());
        assert!(win.frame().h_scrollbar().is_none());
        assert_eq!(win.min_size(), (20, 8));
        assert!(win.frame().closeable());
        assert!(win.frame().resizable());
    }

    #[test]
    fn test_window_with_config_dialog() {
        setup_theme();
        let config = FrameConfig::dialog();
        let win = Window::with_config(Rect::new(10, 10, 30, 10), "Dialog", config);
        assert!(!win.frame().closeable());
        assert!(!win.frame().resizable());
    }

    #[test]
    fn test_window_preset_editor() {
        setup_theme();
        let win = Window::editor(Rect::new(0, 0, 40, 15), "Editor");
        assert!(win.frame().v_scrollbar().is_some());
        assert!(win.frame().h_scrollbar().is_none());
        assert_eq!(win.min_size(), (20, 8));
        assert!(win.frame().closeable());
        assert!(win.frame().resizable());
    }

    #[test]
    fn test_window_preset_palette() {
        setup_theme();
        let win = Window::palette(Rect::new(0, 0, 20, 10), "Colors");
        assert!(!win.frame().resizable());
        assert!(!win.frame().closeable());
    }

    #[test]
    fn test_window_preset_tool() {
        setup_theme();
        let win = Window::tool(Rect::new(0, 0, 15, 8), "Props");
        assert_eq!(win.min_size(), (10, 5));
        assert!(win.frame().closeable());
        assert!(win.frame().resizable());
    }

    // ── Viewport scrolling tests ─────────────────────────────────────────────────

    #[test]
    fn test_window_scroll_offset_defaults_to_zero() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 30, 15), "Test");
        assert_eq!(win.scroll_offset(), (0, 0));
        assert!(win.content_size().is_none());
    }

    #[test]
    fn test_window_set_content_size_updates_scrollbar_params() {
        setup_theme();
        let mut win = Window::editor(Rect::new(0, 0, 30, 15), "Test");
        // Editor preset has vertical scrollbar

        // Set content larger than interior
        win.set_content_size(Some((100, 50)));

        // Vertical scrollbar max should be content_height - interior_height
        let interior = win.frame().interior_area();
        let expected_max_y = 50i32 - i32::from(interior.height);
        if let Some(sb) = win.frame().v_scrollbar() {
            assert_eq!(sb.max_val(), expected_max_y.max(0));
            assert_eq!(sb.value(), 0);
        }
    }

    #[test]
    fn test_window_set_scroll_offset_clamps() {
        setup_theme();
        let mut win = Window::editor(Rect::new(0, 0, 30, 15), "Test");
        win.set_content_size(Some((100, 50)));

        // Set valid offset
        win.set_scroll_offset(5, 10);
        assert_eq!(win.scroll_offset(), (5, 10));

        // Try to set negative — should clamp to 0
        win.set_scroll_offset(-5, -10);
        assert_eq!(win.scroll_offset(), (0, 0));

        // Try to set beyond max — should clamp
        win.set_scroll_offset(1000, 1000);
        let (_max_x, max_y) = {
            let interior = win.frame().interior_area();
            (
                100i32 - i32::from(interior.width),
                50i32 - i32::from(interior.height),
            )
        };
        // Since no h_scrollbar, x stays at 0 (or max if h_scrollbar exists)
        assert_eq!(win.scroll_offset().1, max_y.max(0));
    }

    #[test]
    fn test_window_resize_updates_scrollbar_params() {
        setup_theme();
        let mut win = Window::editor(Rect::new(0, 0, 30, 15), "Test");
        win.set_content_size(Some((100, 50)));

        // Get initial max
        let initial_max = win.frame().v_scrollbar().map(|sb| sb.max_val());

        // Resize window (larger) — max should decrease
        win.set_bounds(Rect::new(0, 0, 30, 25));
        let new_max = win.frame().v_scrollbar().map(|sb| sb.max_val());

        assert!(
            new_max < initial_max,
            "larger window should have smaller scroll max"
        );
    }

    #[test]
    fn test_window_with_content_size_builder() {
        setup_theme();
        let win = Window::new(Rect::new(0, 0, 30, 15), "Test")
            .with_scrollbars(true, true)
            .with_content_size(100, 50);

        assert_eq!(win.content_size(), Some((100, 50)));

        // Scrollbars should have been updated
        if let Some(sb) = win.frame().v_scrollbar() {
            assert!(sb.max_val() > 0, "v_scrollbar should have positive max");
        }
        if let Some(sb) = win.frame().h_scrollbar() {
            assert!(sb.max_val() > 0, "h_scrollbar should have positive max");
        }
    }

    #[test]
    fn test_window_auto_content_size_from_children() {
        setup_theme();
        use crate::static_text::StaticText;

        let mut win = Window::editor(Rect::new(0, 0, 20, 10), "Test");
        // Add a child far from origin
        let text = StaticText::new(
            Rect::new(1, 1, 50, 1),
            "A very long text that exceeds the window",
        );
        win.add(Box::new(text));

        // Auto content size should be at least as large as the child's right edge
        let (cw, _ch) = win.effective_content_size();
        assert!(
            cw >= 50,
            "auto content width should cover the child: got {cw}"
        );
    }
}
