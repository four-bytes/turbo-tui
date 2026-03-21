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

use crate::command::CM_CLOSE;
use crate::container::Container;
use crate::frame::{Frame, FrameType};
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, SF_DRAGGING, SF_FOCUSED, SF_RESIZING};
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
        }
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
    pub fn toggle_zoom(&mut self, screen_size: Rect) {
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
    }

    /// Start a drag operation from the given mouse position.
    #[allow(clippy::cast_possible_wrap)]
    fn start_drag(&mut self, mouse_col: u16, mouse_row: u16) {
        let b = self.base.bounds();
        self.drag_offset = Some((mouse_col as i16 - b.x as i16, mouse_row as i16 - b.y as i16));
        let st = self.base.state();
        self.base.set_state(st | SF_DRAGGING);
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
    }

    /// Start a resize operation from the given mouse position.
    fn start_resize(&mut self, mouse_col: u16, mouse_row: u16) {
        let b = self.base.bounds();
        self.resize_start = Some((b.x, b.y, b.width, b.height));
        self.resize_mouse_start = Some((mouse_col, mouse_row));
        let st = self.base.state();
        self.base.set_state(st | SF_RESIZING);
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

        // 2. Fill interior with background (prevents bleed-through)
        self.fill_interior(buf, clip);

        // 3. Draw children
        self.interior.draw(buf, clip);
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
                        // Close button?
                        if self.frame.is_close_button(col, row) {
                            self.close(event);
                            return;
                        }

                        // Resize handle?
                        if self.frame.is_resize_handle(col, row) {
                            self.start_resize(col, row);
                            event.clear();
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
                            // Forward drag to interior (e.g., scrollbar thumb)
                            self.interior.handle_event(event);
                        }
                    }

                    // Mouse up — end drag/resize
                    MouseEventKind::Up(MouseButton::Left) => {
                        if self.is_dragging() || self.is_resizing() {
                            self.end_drag_resize();
                            event.clear();
                        }
                    }

                    // Other mouse events (scroll, right-click) → interior
                    _ => {
                        self.interior.handle_event(event);
                    }
                }
            }

            // Key, command, broadcast and resize → delegate to interior
            EventKind::Key(_)
            | EventKind::Command(_)
            | EventKind::Broadcast(_)
            | EventKind::Resize(_, _) => {
                self.interior.handle_event(event);
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
        // Propagate focus state to frame (for active/inactive border rendering)
        if state & SF_FOCUSED != 0 {
            self.frame.set_state(self.frame.state() | SF_FOCUSED);
        } else {
            self.frame.set_state(self.frame.state() & !SF_FOCUSED);
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
    use crate::command::CM_CLOSE;
    use crate::frame::FrameType;
    use crate::theme::Theme;
    use crate::view::{EventKind, SF_DRAGGING, SF_FOCUSED, SF_RESIZING};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn setup_theme() {
        crate::theme::set(Theme::dark());
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
}
