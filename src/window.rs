//! Window — Overlapping window with drag, resize, and interior group.
//!
//! A `Window` combines a [`Frame`] (border, title, close/resize handles) with
//! an interior [`Group`] that holds child views. It implements drag and resize
//! via a state machine driven by mouse events.

use crate::command::CM_CLOSE;
use crate::frame::{Frame, FrameType};
use crate::group::Group;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, SF_DRAGGING, SF_RESIZING, SF_VISIBLE};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

/// Overlapping window with drag, resize, and interior group.
///
/// A `Window` consists of:
/// - **Frame**: Border, title, close button, resize handle
/// - **Interior Group**: Container for child views
///
/// The window supports:
/// - **Drag**: Click and drag title bar to move
/// - **Resize**: Drag the resize handle (bottom-right) to resize
/// - **Zoom**: Toggle between maximized and previous size
/// - **Close**: Close button generates [`CM_CLOSE`] command
///
/// # Example
///
/// ```ignore
/// let mut window = Window::new(Rect::new(10, 5, 40, 20), "Document");
/// window.set_resizable(true);
///
/// // Add a child view to the interior
/// window.add(Box::new(TextView::new("Hello, World!")));
///
/// // Check interior bounds
/// let interior = window.interior_rect();
/// ```
#[allow(clippy::module_name_repetitions)]
pub struct Window {
    /// Common view state (id, bounds, state flags).
    base: ViewBase,
    /// Frame providing border, title, close/resize handles.
    frame: Frame,
    /// Interior group holding child views.
    interior: Group,
    /// Offset from mouse to window origin during drag (`mouse_x` - `window_x`, `mouse_y` - `window_y`).
    drag_offset: Option<(i16, i16)>,
    /// Original size at the start of a resize operation.
    resize_start: Option<(u16, u16)>,
    /// Minimum window size (width, height).
    min_size: (u16, u16),
    /// Previous bounds before zoom (for zoom toggle restore).
    prev_bounds: Option<Rect>,
    /// Optional limits for drag/resize operations.
    drag_limits: Option<Rect>,
}

impl Window {
    /// Create a new window with [`FrameType::Window`].
    ///
    /// The window starts with:
    /// - `closeable = true`
    /// - `resizable = false`
    /// - `min_size = (16, 6)`
    #[must_use]
    pub fn new(bounds: Rect, title: &str) -> Self {
        let frame = Frame::new(bounds, title);
        let interior_rect = frame.interior();
        let interior = Group::new(interior_rect);
        Self {
            base: ViewBase::new(bounds),
            frame,
            interior,
            drag_offset: None,
            resize_start: None,
            min_size: (16, 6),
            prev_bounds: None,
            drag_limits: None,
        }
    }

    /// Create a new window with an explicit [`FrameType`].
    #[must_use]
    pub fn with_frame_type(bounds: Rect, title: &str, frame_type: FrameType) -> Self {
        let frame = Frame::with_type(bounds, title, frame_type);
        let interior_rect = frame.interior();
        let interior = Group::new(interior_rect);
        Self {
            base: ViewBase::new(bounds),
            frame,
            interior,
            drag_offset: None,
            resize_start: None,
            min_size: (16, 6),
            prev_bounds: None,
            drag_limits: None,
        }
    }

    // -----------------------------------------------------------------------
    // Configuration
    // -----------------------------------------------------------------------

    /// Enable or disable the resize handle.
    pub fn set_resizable(&mut self, resizable: bool) {
        self.frame.set_resizable(resizable);
    }

    /// Enable or disable the close button.
    pub fn set_closeable(&mut self, closeable: bool) {
        self.frame.set_closeable(closeable);
    }

    /// Set the minimum window size (width, height).
    pub fn set_min_size(&mut self, width: u16, height: u16) {
        self.min_size = (width, height);
    }

    /// Set the limits for drag/resize operations.
    ///
    /// The window will be constrained to stay within these bounds.
    pub fn set_drag_limits(&mut self, limits: Rect) {
        self.drag_limits = Some(limits);
    }

    /// Get the window title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.frame.title()
    }

    /// Replace the window title.
    pub fn set_title(&mut self, title: String) {
        self.frame.set_title(title);
    }

    // -----------------------------------------------------------------------
    // Child management (delegates to interior)
    // -----------------------------------------------------------------------

    /// Add a child view to the interior group.
    ///
    /// Returns the [`ViewId`] of the added child.
    pub fn add(&mut self, child: Box<dyn View>) -> ViewId {
        self.interior.add(child)
    }

    /// Get the number of child views in the interior.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.interior.child_count()
    }

    /// Get a reference to the interior group.
    #[must_use]
    pub fn interior(&self) -> &Group {
        &self.interior
    }

    /// Get a mutable reference to the interior group.
    pub fn interior_mut(&mut self) -> &mut Group {
        &mut self.interior
    }

    // -----------------------------------------------------------------------
    // Geometry
    // -----------------------------------------------------------------------

    /// Get the interior area (bounds minus frame border).
    #[must_use]
    pub fn interior_rect(&self) -> Rect {
        self.frame.interior()
    }

    // -----------------------------------------------------------------------
    // Zoom
    // -----------------------------------------------------------------------

    /// Toggle between maximized and previous size.
    ///
    /// If the window is not zoomed, it saves the current bounds and
    /// resizes to `max_bounds`. If already zoomed, it restores the
    /// previous bounds.
    pub fn zoom(&mut self, max_bounds: Rect) {
        if let Some(prev) = self.prev_bounds {
            // Restore previous bounds
            self.resize_to(prev.x, prev.y, prev.width, prev.height);
            self.prev_bounds = None;
        } else {
            // Save current bounds and maximize
            self.prev_bounds = Some(self.base.bounds());
            self.resize_to(
                max_bounds.x,
                max_bounds.y,
                max_bounds.width,
                max_bounds.height,
            );
        }
    }

    /// Check if the window is currently zoomed.
    #[must_use]
    pub fn is_zoomed(&self) -> bool {
        self.prev_bounds.is_some()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Move the window to a new position.
    fn move_to(&mut self, x: u16, y: u16) {
        let bounds = self.base.bounds();
        let new_bounds = Rect::new(x, y, bounds.width, bounds.height);
        self.base.set_bounds(new_bounds);
        self.frame.set_bounds(new_bounds);
        self.update_interior_bounds();
    }

    /// Resize the window to new dimensions.
    fn resize_to(&mut self, x: u16, y: u16, w: u16, h: u16) {
        let new_bounds = Rect::new(x, y, w, h);
        self.base.set_bounds(new_bounds);
        self.frame.set_bounds(new_bounds);
        self.update_interior_bounds();
    }

    /// Update the interior group bounds to match the frame's interior.
    fn update_interior_bounds(&mut self) {
        let interior_rect = self.frame.interior();
        self.interior.set_bounds(interior_rect);
    }

    /// Constrain a position to drag limits.
    fn constrain_position(&self, x: u16, y: u16) -> (u16, u16) {
        if let Some(limits) = self.drag_limits {
            let bounds = self.base.bounds();
            let cx = x
                .max(limits.x)
                .min(limits.x + limits.width.saturating_sub(bounds.width));
            let cy = y
                .max(limits.y)
                .min(limits.y + limits.height.saturating_sub(bounds.height));
            (cx, cy)
        } else {
            (x, y)
        }
    }

    /// Constrain a size to minimum and drag limits.
    fn constrain_size(&self, w: u16, h: u16) -> (u16, u16) {
        let cw = w.max(self.min_size.0);
        let ch = h.max(self.min_size.1);
        if let Some(limits) = self.drag_limits {
            let bounds = self.base.bounds();
            let max_w = limits.x + limits.width - bounds.x;
            let max_h = limits.y + limits.height - bounds.y;
            (cw.min(max_w), ch.min(max_h))
        } else {
            (cw, ch)
        }
    }
}

// ============================================================================
// View implementation
// ============================================================================

impl View for Window {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.frame.set_bounds(bounds);
        self.update_interior_bounds();
    }

    fn draw(&self, buf: &mut Buffer, _area: Rect) {
        // Don't draw if not visible
        if self.base.state() & SF_VISIBLE == 0 {
            return;
        }

        // 1. Draw frame
        self.frame.draw(buf, self.base.bounds());

        // 2. Fill interior with background color from theme (clipped to buffer)
        let interior_area = self.frame.interior();
        let bg_style = theme::with_current(|t| t.window_interior);
        let (buf_x, buf_y, buf_w, buf_h) = {
            let a = *buf.area();
            (a.x, a.y, a.width, a.height)
        };
        for y in interior_area.y..interior_area.y.saturating_add(interior_area.height) {
            if y < buf_y || y >= buf_y + buf_h {
                continue;
            }
            for x in interior_area.x..interior_area.x.saturating_add(interior_area.width) {
                if x >= buf_x && x < buf_x + buf_w {
                    buf.set_string(x, y, " ", bg_style);
                }
            }
        }

        // 3. Draw interior children
        self.interior.draw(buf, interior_area);
    }

    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        // Clone the event kind to avoid borrow issues
        let kind = event.kind.clone();

        match &kind {
            EventKind::Mouse(mouse) => {
                let bounds = self.base.bounds();
                let mouse_kind = mouse.kind;
                let mouse_col = mouse.column;
                let mouse_row = mouse.row;

                match mouse_kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        // First, let frame handle it (close button, drag/resize initiation)
                        self.frame.handle_event(event);

                        // Check if frame initiated drag
                        if self.frame.is_dragging() {
                            // Casts are safe: mouse/column values are screen coordinates (typically < 1000),
                            // which fit in i16 range. Subtraction yields small offsets.
                            self.drag_offset = Some((
                                mouse_col as i16 - bounds.x as i16,
                                mouse_row as i16 - bounds.y as i16,
                            ));
                            self.frame.clear_drag_resize();
                            let state = self.base.state();
                            self.base.set_state(state | SF_DRAGGING);
                            event.clear();
                            return;
                        }

                        // Check if frame initiated resize
                        if self.frame.is_resizing() {
                            self.resize_start = Some((bounds.width, bounds.height));
                            self.frame.clear_drag_resize();
                            let state = self.base.state();
                            self.base.set_state(state | SF_RESIZING);
                            event.clear();
                            return;
                        }

                        // If not consumed by frame, pass to interior
                        if !event.is_cleared() {
                            self.interior.handle_event(event);
                        }
                    }

                    MouseEventKind::Drag(MouseButton::Left) => {
                        if self.base.state() & SF_DRAGGING != 0 {
                            if let Some((ox, oy)) = self.drag_offset {
                                // Casts are safe: result is clamped to >= 0 before casting back to u16
                                let new_x = (mouse_col as i16 - ox).max(0) as u16;
                                let new_y = (mouse_row as i16 - oy).max(0) as u16;

                                // Apply drag limits
                                let (new_x, new_y) = self.constrain_position(new_x, new_y);

                                self.move_to(new_x, new_y);
                            }
                            event.clear();
                            return;
                        }

                        if self.base.state() & SF_RESIZING != 0 {
                            // Casts are safe: sizes are clamped to min_size before assignment
                            let new_w = (mouse_col as i16 - bounds.x as i16 + 1)
                                .max(self.min_size.0 as i16)
                                as u16;
                            let new_h = (mouse_row as i16 - bounds.y as i16 + 1)
                                .max(self.min_size.1 as i16)
                                as u16;

                            // Apply limits
                            let (new_w, new_h) = self.constrain_size(new_w, new_h);

                            self.resize_to(bounds.x, bounds.y, new_w, new_h);
                            event.clear();
                            return;
                        }

                        // Not dragging/resizing — pass to interior
                        self.interior.handle_event(event);
                    }

                    MouseEventKind::Up(MouseButton::Left) => {
                        if self.base.state() & (SF_DRAGGING | SF_RESIZING) != 0 {
                            let state = self.base.state();
                            self.base.set_state(state & !SF_DRAGGING & !SF_RESIZING);
                            self.drag_offset = None;
                            self.resize_start = None;
                            event.clear();
                            return;
                        }
                        self.interior.handle_event(event);
                    }

                    _ => {
                        // Other mouse events go to interior
                        self.interior.handle_event(event);
                    }
                }
            }

            EventKind::Command(cmd) => {
                match *cmd {
                    CM_CLOSE => {
                        // Mark window for closing
                        event.clear();
                    }
                    crate::command::CM_ZOOM => {
                        // Get reasonable max bounds (will be passed in real usage)
                        self.zoom(Rect::new(0, 0, 120, 40));
                        event.clear();
                    }
                    _ => {
                        self.interior.handle_event(event);
                    }
                }
            }

            _ => {
                // Key, Broadcast, Resize — delegate to interior
                self.interior.handle_event(event);
            }
        }
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
        self.frame.set_state(state);
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
    use crate::view::View;
    use crossterm::event::{KeyModifiers, MouseEvent};

    /// Helper test view that tracks bounds.
    struct TestChild {
        base: ViewBase,
    }

    impl TestChild {
        fn new(bounds: Rect) -> Self {
            Self {
                base: ViewBase::new(bounds),
            }
        }
    }

    impl View for TestChild {
        fn id(&self) -> ViewId {
            self.base.id()
        }
        fn bounds(&self) -> Rect {
            self.base.bounds()
        }
        fn set_bounds(&mut self, bounds: Rect) {
            self.base.set_bounds(bounds);
        }
        fn draw(&self, _buf: &mut Buffer, _area: Rect) {}
        fn handle_event(&mut self, _event: &mut Event) {}
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

    fn make_mouse_down(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn make_mouse_drag(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn make_mouse_up(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn test_window_new() {
        let w = Window::new(Rect::new(10, 5, 40, 20), "Test Window");
        assert_eq!(w.bounds(), Rect::new(10, 5, 40, 20));
        assert_eq!(w.title(), "Test Window");
        assert_eq!(w.min_size, (16, 6));
        assert!(w.drag_offset.is_none());
        assert!(w.resize_start.is_none());
        assert!(w.prev_bounds.is_none());
        assert!(w.drag_limits.is_none());
    }

    #[test]
    fn test_window_interior_rect() {
        let w = Window::new(Rect::new(10, 5, 40, 20), "Test");
        let interior = w.interior_rect();
        // Interior is bounds minus 1 cell on each side
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_window_add_child() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        assert_eq!(w.child_count(), 0);

        let child1 = Box::new(TestChild::new(Rect::new(0, 0, 10, 5)));
        let _id1 = w.add(child1);
        assert_eq!(w.child_count(), 1);

        let child2 = Box::new(TestChild::new(Rect::new(0, 0, 10, 5)));
        let _id2 = w.add(child2);
        assert_eq!(w.child_count(), 2);
    }

    #[test]
    fn test_window_move_to() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        w.move_to(5, 3);

        assert_eq!(w.bounds(), Rect::new(5, 3, 20, 10));
        assert_eq!(w.frame.bounds(), Rect::new(5, 3, 20, 10));
        assert_eq!(w.interior.bounds(), w.interior_rect());
    }

    #[test]
    fn test_window_resize_to() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        w.resize_to(0, 0, 30, 15);

        assert_eq!(w.bounds(), Rect::new(0, 0, 30, 15));
        assert_eq!(w.frame.bounds(), Rect::new(0, 0, 30, 15));

        // Interior updated
        let interior = w.interior_rect();
        assert_eq!(interior, Rect::new(1, 1, 28, 13));
    }

    #[test]
    fn test_window_zoom_toggle() {
        let mut w = Window::new(Rect::new(10, 5, 20, 10), "Test");
        assert!(!w.is_zoomed());

        // Zoom to max
        w.zoom(Rect::new(0, 0, 80, 24));
        assert!(w.is_zoomed());
        assert_eq!(w.bounds(), Rect::new(0, 0, 80, 24));
        assert_eq!(w.prev_bounds, Some(Rect::new(10, 5, 20, 10)));

        // Zoom again to restore
        w.zoom(Rect::new(0, 0, 80, 24));
        assert!(!w.is_zoomed());
        assert_eq!(w.bounds(), Rect::new(10, 5, 20, 10));
        assert!(w.prev_bounds.is_none());
    }

    #[test]
    fn test_window_drag_limits() {
        let mut w = Window::new(Rect::new(10, 10, 20, 10), "Test");
        w.set_drag_limits(Rect::new(0, 0, 100, 50));

        // Move beyond limits
        let (x, y) = w.constrain_position(90, 45);
        // x = 90, but window width is 20, so max x = 100 - 20 = 80
        assert_eq!(x, 80);
        // y = 45, but window height is 10, so max y = 50 - 10 = 40
        assert_eq!(y, 40);

        // Move within limits
        let (x, y) = w.constrain_position(20, 15);
        assert_eq!(x, 20);
        assert_eq!(y, 15);
    }

    #[test]
    fn test_window_drag_state_machine() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        w.set_resizable(true);

        // Mouse down on title bar (row 0, column 10 — past close button)
        let mut event = make_mouse_down(10, 0);
        w.handle_event(&mut event);

        // Window should be in drag state
        assert!(w.base.state() & SF_DRAGGING != 0);
        assert!(event.is_cleared());
        assert!(w.drag_offset.is_some());

        // Drag
        let mut event = make_mouse_drag(15, 5);
        w.handle_event(&mut event);

        // Window should have moved
        assert_eq!(w.bounds().x, 5);
        assert_eq!(w.bounds().y, 5);
        assert!(event.is_cleared());

        // Mouse up
        let mut event = make_mouse_up(15, 5);
        w.handle_event(&mut event);

        // Drag state cleared
        assert!(w.base.state() & SF_DRAGGING == 0);
        assert!(w.drag_offset.is_none());
    }

    #[test]
    fn test_window_resize_state_machine() {
        let mut w = Window::new(Rect::new(10, 5, 20, 10), "Test");
        w.set_resizable(true);

        // Mouse down on resize handle (bottom-right)
        // Window bounds: x=10, y=5, width=20, height=10
        // Resize handle: columns 28-29, row 14
        let mut event = make_mouse_down(28, 14);
        w.handle_event(&mut event);

        // Window should be in resize state
        assert!(w.base.state() & SF_RESIZING != 0);
        assert!(event.is_cleared());
        assert!(w.resize_start.is_some());

        // Drag to resize
        let mut event = make_mouse_drag(35, 20);
        w.handle_event(&mut event);

        // Window should have resized
        // new_w = 35 - 10 + 1 = 26
        // new_h = 20 - 5 + 1 = 16
        assert_eq!(w.bounds().width, 26);
        assert_eq!(w.bounds().height, 16);
        assert!(event.is_cleared());

        // Mouse up
        let mut event = make_mouse_up(35, 20);
        w.handle_event(&mut event);

        // Resize state cleared
        assert!(w.base.state() & SF_RESIZING == 0);
        assert!(w.resize_start.is_none());
    }

    #[test]
    fn test_window_close_command() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");

        let mut event = Event::command(CM_CLOSE);
        w.handle_event(&mut event);

        assert!(event.is_cleared());
    }

    #[test]
    fn test_window_min_size_enforced() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        w.set_min_size(10, 5);

        // Try to resize below min
        let (new_w, new_h) = w.constrain_size(5, 3);
        assert_eq!(new_w, 10);
        assert_eq!(new_h, 5);
    }

    #[test]
    fn test_window_set_bounds_updates_all() {
        let mut w = Window::new(Rect::new(0, 0, 20, 10), "Test");
        w.add(Box::new(TestChild::new(Rect::new(0, 0, 10, 5))));

        w.set_bounds(Rect::new(5, 5, 30, 20));

        assert_eq!(w.base.bounds(), Rect::new(5, 5, 30, 20));
        assert_eq!(w.frame.bounds(), Rect::new(5, 5, 30, 20));
        // Interior bounds should be frame.interior()
        let expected_interior = Rect::new(6, 6, 28, 18);
        assert_eq!(w.interior.bounds(), expected_interior);
    }

    #[test]
    fn test_window_with_frame_type() {
        let w = Window::with_frame_type(Rect::new(0, 0, 20, 10), "Dialog", FrameType::Dialog);
        assert_eq!(w.frame.frame_type(), FrameType::Dialog);
    }
}
