//! Desktop — Window manager with background and overlapping windows.
//!
//! Desktop manages a collection of [`Window`] views with:
//! - Themed background (fills entire area)
//! - Z-order window stacking (click-to-front)
//! - Tile and cascade layout algorithms
//! - Focus cycling between windows
//!
//! Desktop draws the background first, then delegates to its internal
//! [`Container`] for window rendering and event dispatch.

use crate::container::Container;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId};
use crate::window::Window;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Desktop window manager — background with overlapping windows.
///
/// Manages a collection of [`Window`] views with:
/// - Themed background (fills entire area)
/// - Z-order window stacking (click-to-front)
/// - Tile and cascade layout algorithms
/// - Focus cycling between windows
///
/// Desktop draws the background first, then delegates to its internal
/// [`Container`] for window rendering and event dispatch.
pub struct Desktop {
    base: ViewBase,
    windows: Container,
    /// Height of the task shelf in rows (0 = no minimized windows).
    task_shelf_height: u16,
}

impl Desktop {
    /// Create a new desktop with the given bounds.
    ///
    /// Background style and character are read from the current theme at draw-time,
    /// so theme changes via [`theme::set`] are always reflected without rebuilding.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            windows: Container::new(bounds),
            task_shelf_height: 0,
        }
    }

    /// Add a window to the desktop.
    ///
    /// The window is added to the front (top of Z-order) and receives focus.
    /// The window's drag limits are set to the desktop bounds.
    pub fn add_window(&mut self, mut window: Window) -> ViewId {
        window.set_drag_limits(self.base.bounds());
        let id = self.windows.add(Box::new(window));
        let count = self.windows.child_count();
        self.windows.bring_to_front(count - 1);
        self.windows.set_focus_to(count - 1);
        self.base.mark_dirty();
        self.recalculate_shelf();
        id
    }

    /// Close (remove) a window by its `ViewId`.
    pub fn close_window(&mut self, id: ViewId) -> Option<Box<dyn View>> {
        let removed = self.windows.remove_by_id(id);
        if removed.is_some() {
            self.base.mark_dirty();
            // Focus the new front window if any
            let count = self.windows.child_count();
            if count > 0 {
                self.windows.set_focus_to(count - 1);
            }
            // Recalculate shelf after window removal
            self.recalculate_shelf();
        }
        removed
    }

    /// Close (remove) all windows from the desktop.
    ///
    /// Clears the task shelf since no minimized windows remain.
    pub fn close_all_windows(&mut self) {
        while self.windows.child_count() > 0 {
            self.windows.remove(0);
        }
        self.task_shelf_height = 0;
        self.base.mark_dirty();
    }

    /// Get the current task shelf height in rows.
    #[must_use]
    pub fn task_shelf_height(&self) -> u16 {
        self.task_shelf_height
    }

    /// Recalculate the task shelf: position minimized windows at the bottom of the desktop.
    ///
    /// Minimized windows tile left-to-right in the shelf area. If they overflow
    /// one row, the shelf grows to 2 rows (and so on). Non-minimized windows are
    /// not affected.
    pub fn recalculate_shelf(&mut self) {
        let desktop_bounds = self.base.bounds();

        // Collect indices of minimized windows
        let count = self.windows.child_count();
        let mut minimized_indices: Vec<usize> = Vec::new();
        for i in 0..count {
            if let Some(child) = self.windows.child_at(i) {
                if let Some(win) = child.as_any().downcast_ref::<Window>() {
                    if win.is_minimized() {
                        minimized_indices.push(i);
                    }
                }
            }
        }

        if minimized_indices.is_empty() {
            self.task_shelf_height = 0;
            return;
        }

        // Calculate shelf layout: tile left-to-right
        let mut shelf_x = desktop_bounds.x;
        let mut shelf_row: u16 = 0; // 0 = first row from bottom

        for &idx in &minimized_indices {
            if let Some(child) = self.windows.child_at(idx) {
                let min_w = if let Some(win) = child.as_any().downcast_ref::<Window>() {
                    win.minimized_width()
                } else {
                    20 // fallback
                };

                // Wrap to next row if this window would exceed desktop width
                if shelf_x + min_w > desktop_bounds.x + desktop_bounds.width
                    && shelf_x > desktop_bounds.x
                {
                    shelf_row += 1;
                    shelf_x = desktop_bounds.x;
                }

                let shelf_y =
                    desktop_bounds.y + desktop_bounds.height.saturating_sub(1 + shelf_row);

                if let Some(child_mut) = self.windows.child_at_mut(idx) {
                    child_mut.set_bounds(Rect::new(shelf_x, shelf_y, min_w, 1));
                }

                shelf_x += min_w;
            }
        }

        self.task_shelf_height = shelf_row + 1;
        self.base.mark_dirty();
    }

    /// Get the effective window area (desktop bounds minus task shelf).
    ///
    /// Non-minimized windows should be constrained to this area during tile/cascade.
    #[must_use]
    pub fn effective_area(&self) -> Rect {
        let b = self.base.bounds();
        Rect::new(
            b.x,
            b.y,
            b.width,
            b.height.saturating_sub(self.task_shelf_height),
        )
    }

    /// Return the number of windows.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.child_count()
    }

    /// Get immutable access to the internal container.
    #[must_use]
    pub fn windows(&self) -> &Container {
        &self.windows
    }

    /// Get mutable access to the internal container.
    pub fn windows_mut(&mut self) -> &mut Container {
        &mut self.windows
    }

    /// Bring the window at the given index to the front and give it focus.
    pub fn click_to_front(&mut self, index: usize) {
        self.windows.bring_to_front(index);
        let count = self.windows.child_count();
        if count > 0 {
            // Clear focus on all, then focus the front window
            self.windows.set_focus_to(count - 1);
        }
        self.base.mark_dirty();
    }

    /// Cycle focus to the next window.
    ///
    /// Also brings the newly focused window to front.
    pub fn next_window(&mut self) {
        if self.windows.child_count() < 2 {
            return;
        }
        // Send current front to back, then focus new front
        let count = self.windows.child_count();
        self.windows.send_to_back(count - 1);
        let new_count = self.windows.child_count();
        self.windows.set_focus_to(new_count - 1);
        self.base.mark_dirty();
    }

    /// Cycle focus to the previous window.
    pub fn prev_window(&mut self) {
        if self.windows.child_count() < 2 {
            return;
        }
        // Bring back to front
        self.windows.bring_to_front(0);
        let count = self.windows.child_count();
        self.windows.set_focus_to(count - 1);
        self.base.mark_dirty();
    }

    /// Tile all windows in a grid layout.
    ///
    /// Arranges windows in a grid that fits the desktop area.
    /// Windows are resized equally to fill the available space.
    /// Minimized windows are skipped and remain in the task shelf.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn tile(&mut self) {
        let area = self.effective_area();

        // Collect non-minimized window indices
        let total = self.windows.child_count();
        let mut active_indices: Vec<usize> = Vec::new();
        for i in 0..total {
            if let Some(child) = self.windows.child_at(i) {
                if let Some(win) = child.as_any().downcast_ref::<Window>() {
                    if !win.is_minimized() {
                        active_indices.push(i);
                    }
                } else {
                    active_indices.push(i);
                }
            }
        }

        let count = active_indices.len();
        if count == 0 {
            return;
        }

        let cols = (count as f64).sqrt().ceil() as u16;
        let rows = ((count as f64) / f64::from(cols)).ceil() as u16;

        let w = area.width / cols;
        let h = area.height / rows;

        for (seq, &idx) in active_indices.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let col = (seq as u16) % cols;
            #[allow(clippy::cast_possible_truncation)]
            let row = (seq as u16) / cols;

            let x = area.x + col * w;
            let y = area.y + row * h;

            let actual_w = if col == cols - 1 {
                area.x + area.width - x
            } else {
                w
            };
            let actual_h = if row == rows - 1 {
                area.y + area.height - y
            } else {
                h
            };

            if let Some(child) = self.windows.child_at_mut(idx) {
                child.set_bounds(Rect::new(x, y, actual_w, actual_h));
            }
        }
        self.base.mark_dirty();
    }

    /// Cascade windows from top-left with offset.
    ///
    /// Each window gets a 2-column, 1-row offset from the previous one.
    /// Windows are resized to approximately 60% of the desktop area.
    /// Minimized windows are skipped and remain in the task shelf.
    pub fn cascade(&mut self) {
        let area = self.effective_area();

        // Collect non-minimized window indices
        let total = self.windows.child_count();
        let mut active_indices: Vec<usize> = Vec::new();
        for i in 0..total {
            if let Some(child) = self.windows.child_at(i) {
                if let Some(win) = child.as_any().downcast_ref::<Window>() {
                    if !win.is_minimized() {
                        active_indices.push(i);
                    }
                } else {
                    active_indices.push(i);
                }
            }
        }

        let count = active_indices.len();
        if count == 0 {
            return;
        }

        let w = (area.width * 3) / 5;
        let h = (area.height * 3) / 5;

        for (seq, &idx) in active_indices.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let offset_x = (seq as u16) * 2;
            #[allow(clippy::cast_possible_truncation)]
            let offset_y = seq as u16;

            let x = area.x + offset_x;
            let y = area.y + offset_y;

            let actual_w = w.min(area.x + area.width - x);
            let actual_h = h.min(area.y + area.height - y);

            if let Some(child) = self.windows.child_at_mut(idx) {
                child.set_bounds(Rect::new(x, y, actual_w, actual_h));
            }
        }
        self.base.mark_dirty();
    }

    /// Return the cursor position from the currently focused window, if any.
    ///
    /// Delegates to the focused window's `View::cursor_position()` via the
    /// internal [`Container`].
    #[must_use]
    pub fn cursor_position(&self) -> Option<Position> {
        self.windows.cursor_position()
    }

    /// Draw the desktop background.
    ///
    /// Reads the background style and character from the current theme at draw-time
    /// so that runtime theme changes via [`theme::set`] are always reflected.
    fn draw_background(&self, buf: &mut Buffer, clip: Rect) {
        let b = self.base.bounds();
        let fill_area = b.intersection(clip);
        if fill_area.width == 0 || fill_area.height == 0 {
            return;
        }
        let (bg_style, bg_char) = theme::with_current(|t| (t.desktop_bg, t.desktop_char));
        for row in fill_area.y..fill_area.y + fill_area.height {
            for col in fill_area.x..fill_area.x + fill_area.width {
                if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                    cell.set_char(bg_char).set_style(bg_style);
                }
            }
        }
    }
}

impl View for Desktop {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.windows.set_bounds(bounds);

        // Update drag limits on all existing windows
        for i in 0..self.windows.child_count() {
            if let Some(child) = self.windows.child_at_mut(i) {
                if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                    win.set_drag_limits(bounds);
                }
            }
        }
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        // 1. Background
        self.draw_background(buf, clip);
        // 2. Windows (Container handles Z-order)
        self.windows.draw(buf, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        if let EventKind::Mouse(mouse) = &event.kind.clone() {
            let col = mouse.column;
            let row = mouse.row;

            // Click-to-front: on MouseDown, find hit window and bring to front
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                if let Some(hit_index) = self.windows.child_at_point(col, row) {
                    let focused = self.windows.focused_index();
                    let count = self.windows.child_count();
                    // Bring clicked window to front if:
                    // - It's not already the front window, OR
                    // - The focused window is not the front window
                    if hit_index != count - 1 || focused != Some(count - 1) {
                        // Bring clicked window to front if not already
                        if hit_index != count - 1 || focused != Some(hit_index) {
                            self.click_to_front(hit_index);
                        }
                    }
                }
            }

            // Delegate all mouse events to container (which handles capture + hit-test)
            self.windows.handle_event(event);
            // Recalculate shelf after any mouse event that might have triggered minimize/restore
            self.recalculate_shelf();
        } else {
            // Key/Command events → delegate to container (three-phase dispatch)
            self.windows.handle_event(event);
            self.recalculate_shelf();
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
    use crate::theme::Theme;
    use crate::view::{SF_FOCUSED, SF_VISIBLE};

    fn setup_theme() {
        crate::theme::set(Theme::turbo_vision());
    }

    #[test]
    fn test_desktop_new() {
        setup_theme();
        let bounds = Rect::new(0, 0, 80, 24);
        let desktop = Desktop::new(bounds);

        assert_eq!(desktop.bounds(), bounds);
        assert_eq!(desktop.window_count(), 0);
    }

    #[test]
    fn test_desktop_add_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let win1 = Window::new(Rect::new(5, 5, 30, 10), "Window 1");
        let id1 = desktop.add_window(win1);
        assert_eq!(desktop.window_count(), 1);

        let win2 = Window::new(Rect::new(10, 10, 30, 10), "Window 2");
        let id2 = desktop.add_window(win2);
        assert_eq!(desktop.window_count(), 2);

        // Windows should have different IDs
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_desktop_add_window_sets_drag_limits() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(10, 5, 60, 20));

        let win = Window::new(Rect::new(20, 10, 30, 10), "Test");
        desktop.add_window(win);

        // Check that the window has drag limits set
        let front = desktop.windows().child_at(0);
        assert!(front.is_some());
        let front_view = front.unwrap();
        let front_window = front_view.as_any().downcast_ref::<Window>().unwrap();
        assert!(front_window.drag_limits().is_some());
        assert_eq!(front_window.drag_limits(), Some(Rect::new(10, 5, 60, 20)));
    }

    #[test]
    fn test_desktop_close_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let win1 = Window::new(Rect::new(5, 5, 30, 10), "Window 1");
        let id1 = desktop.add_window(win1);
        let win2 = Window::new(Rect::new(10, 10, 30, 10), "Window 2");
        let id2 = desktop.add_window(win2);

        assert_eq!(desktop.window_count(), 2);

        // Close window 2
        let removed = desktop.close_window(id2);
        assert!(removed.is_some());
        assert_eq!(desktop.window_count(), 1);

        // Try to close non-existent ID
        let nope = desktop.close_window(ViewId::new());
        assert!(nope.is_none());
        assert_eq!(desktop.window_count(), 1);

        // Close window 1
        desktop.close_window(id1);
        assert_eq!(desktop.window_count(), 0);
    }

    #[test]
    fn test_desktop_click_to_front() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let win1 = Window::new(Rect::new(5, 5, 30, 10), "Window 1");
        let id1 = desktop.add_window(win1);
        let win2 = Window::new(Rect::new(10, 10, 30, 10), "Window 2");
        let _id2 = desktop.add_window(win2);

        // Window 2 should be in front (last added)
        assert_eq!(desktop.windows().focused_index(), Some(1));

        // Click on window 1 (index 0 in Z-order)
        desktop.click_to_front(0);

        // Now window 1 should be in front and focused
        assert_eq!(desktop.windows().focused_index(), Some(1)); // focused is at back (now front after bring_to_front)
        assert_eq!(desktop.windows().child_at(0).unwrap().id(), _id2); // win2 at back
        assert_eq!(desktop.windows().child_at(1).unwrap().id(), id1); // win1 at front
    }

    #[test]
    fn test_desktop_next_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // With 0 windows
        desktop.next_window(); // No-op, should not panic

        // With 1 window
        let _id1 = desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W1"));
        desktop.next_window(); // No-op with 1 window

        // With 2 windows
        let _id2 = desktop.add_window(Window::new(Rect::new(5, 5, 20, 10), "W2"));
        assert_eq!(desktop.windows().focused_index(), Some(1)); // W2 focused

        desktop.next_window(); // Send W2 to back, W1 comes to front
        assert_eq!(desktop.windows().focused_index(), Some(1)); // Still index 1 (now W1)
    }

    #[test]
    fn test_desktop_prev_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // With 0 windows
        desktop.prev_window(); // No-op

        // With 1 window
        let _id1 = desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W1"));
        desktop.prev_window(); // No-op with 1 window

        // With 2 windows
        let id2 = desktop.add_window(Window::new(Rect::new(5, 5, 20, 10), "W2"));
        assert_eq!(desktop.windows().focused_index(), Some(1)); // W2 focused (at front)

        desktop.prev_window(); // Bring W1 (at back) to front
        assert_eq!(desktop.windows().focused_index(), Some(1)); // Still index 1 (now W1)
        assert_eq!(desktop.windows().child_at(0).unwrap().id(), id2); // W2 at back now
    }

    #[test]
    fn test_desktop_tile_single_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let win = Window::new(Rect::new(10, 5, 20, 10), "Single");
        desktop.add_window(win);

        desktop.tile();

        // Single window should fill entire area
        let child = desktop.windows().child_at(0).unwrap();
        assert_eq!(child.bounds(), Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn test_desktop_tile_four_windows() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W3"));
        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W4"));

        desktop.tile();

        // sqrt(4) = 2, so 2x2 grid
        // cols = 2, rows = 2
        // w = 80 / 2 = 40, h = 24 / 2 = 12

        let c0 = desktop.windows().child_at(0).unwrap();
        let c1 = desktop.windows().child_at(1).unwrap();
        let c2 = desktop.windows().child_at(2).unwrap();
        let c3 = desktop.windows().child_at(3).unwrap();

        // Row 0: (0,0,40,12) and (40,0,40,12)
        // Row 1: (0,12,40,12) and (40,12,40,12)
        assert_eq!(c0.bounds(), Rect::new(0, 0, 40, 12));
        assert_eq!(c1.bounds(), Rect::new(40, 0, 40, 12)); // last col gets remaining
        assert_eq!(c2.bounds(), Rect::new(0, 12, 40, 12));
        assert_eq!(c3.bounds(), Rect::new(40, 12, 40, 12)); // last col gets remaining
    }

    #[test]
    fn test_desktop_cascade() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(0, 0, 20, 10), "W3"));

        desktop.cascade();

        // Each window is offset by (2, 1) from previous
        // w = 80 * 3 / 5 = 48, h = 24 * 3 / 5 = 14

        let c0 = desktop.windows().child_at(0).unwrap();
        let c1 = desktop.windows().child_at(1).unwrap();
        let c2 = desktop.windows().child_at(2).unwrap();

        assert_eq!(c0.bounds().x, 0);
        assert_eq!(c0.bounds().y, 0);
        assert_eq!(c1.bounds().x, 2);
        assert_eq!(c1.bounds().y, 1);
        assert_eq!(c2.bounds().x, 4);
        assert_eq!(c2.bounds().y, 2);
    }

    #[test]
    fn test_desktop_draw_background() {
        setup_theme();
        let bounds = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(bounds);
        let desktop = Desktop::new(bounds);

        desktop.draw(&mut buf, bounds);

        // Verify the buffer has the current theme's background character
        let bg_char = theme::with_current(|t| t.desktop_char);
        let cell = buf.cell(Position::new(0, 0)).unwrap();
        assert_eq!(cell.symbol(), bg_char.to_string().as_str());
    }

    #[test]
    fn test_desktop_set_bounds_propagates() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));

        let new_bounds = Rect::new(10, 5, 60, 20);
        desktop.set_bounds(new_bounds);

        assert_eq!(desktop.bounds(), new_bounds);
        assert_eq!(desktop.windows().bounds(), new_bounds);
    }

    #[test]
    fn test_desktop_close_refocuses() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let win1 = Window::new(Rect::new(0, 0, 20, 10), "W1");
        let id1 = desktop.add_window(win1);
        let win2 = Window::new(Rect::new(5, 5, 20, 10), "W2");
        let id2 = desktop.add_window(win2);
        let win3 = Window::new(Rect::new(10, 10, 20, 10), "W3");
        let id3 = desktop.add_window(win3);

        // W3 is front/focused
        assert_eq!(desktop.windows().focused_index(), Some(2));

        // Close front window (W3)
        desktop.close_window(id3);

        // Focus should move to new front (W2)
        assert_eq!(desktop.windows().focused_index(), Some(1));

        // Close W2
        desktop.close_window(id2);

        // Focus on W1
        assert_eq!(desktop.windows().focused_index(), Some(0));

        // Close last window
        desktop.close_window(id1);
        assert_eq!(desktop.window_count(), 0);
        assert_eq!(desktop.windows().focused_index(), None);
    }

    #[test]
    fn test_desktop_background_uses_theme() {
        setup_theme();
        let bounds = Rect::new(0, 0, 10, 3);
        let mut buf = Buffer::empty(bounds);
        let desktop = Desktop::new(bounds);

        desktop.draw(&mut buf, bounds);

        // The buffer cells should reflect the theme's desktop background.
        // Compare fg/bg only — Ratatui cells carry an extra underline_color(Reset)
        // that is not part of the theme style.
        let (theme_bg, theme_char) = theme::with_current(|t| (t.desktop_bg, t.desktop_char));
        let cell = buf.cell(Position::new(0, 0)).unwrap();
        assert_eq!(cell.symbol(), theme_char.to_string().as_str());
        assert_eq!(cell.fg, theme_bg.fg.unwrap_or(ratatui::style::Color::Reset));
        assert_eq!(cell.bg, theme_bg.bg.unwrap_or(ratatui::style::Color::Reset));
    }

    #[test]
    fn test_desktop_can_focus() {
        setup_theme();
        let desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        assert!(desktop.can_focus());
    }

    #[test]
    fn test_desktop_state_management() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // Initial state is SF_VISIBLE
        assert_ne!(desktop.state() & SF_VISIBLE, 0);

        // Set focused
        desktop.set_state(SF_VISIBLE | SF_FOCUSED);
        assert_ne!(desktop.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_desktop_shelf_empty_when_no_minimized() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));

        assert_eq!(
            desktop.task_shelf_height(),
            0,
            "no minimized windows = no shelf"
        );
        assert_eq!(desktop.effective_area(), Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn test_desktop_shelf_one_minimized_window() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "Window 1"));

        // Minimize the window
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();

        assert_eq!(
            desktop.task_shelf_height(),
            1,
            "one minimized window = 1 row shelf"
        );

        // Check the minimized window is positioned at the bottom of desktop
        let db = desktop.bounds();
        if let Some(child) = desktop.windows().child_at(0) {
            let b = child.bounds();
            assert_eq!(b.y, db.y + db.height - 1, "minimized window at bottom");
            assert_eq!(b.height, 1, "minimized window is 1 row tall");
        }
    }

    #[test]
    fn test_desktop_shelf_multiple_minimized_tile_left_to_right() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(15, 15, 30, 10), "W3"));

        // Minimize all
        for i in 0..3 {
            if let Some(child) = desktop.windows_mut().child_at_mut(i) {
                if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                    win.minimize();
                }
            }
        }
        desktop.recalculate_shelf();

        // All should be on the bottom row, left-to-right
        let db = desktop.bounds();
        let b0 = desktop.windows().child_at(0).unwrap().bounds();
        let b1 = desktop.windows().child_at(1).unwrap().bounds();
        let b2 = desktop.windows().child_at(2).unwrap().bounds();

        assert_eq!(b0.y, db.y + db.height - 1);
        assert_eq!(b1.y, db.y + db.height - 1);
        assert_eq!(b2.y, db.y + db.height - 1);
        assert_eq!(b0.x, 0, "first minimized starts at x=0");
        assert!(b1.x > b0.x, "second starts after first");
        assert!(b2.x > b1.x, "third starts after second");
    }

    #[test]
    fn test_desktop_shelf_restore_recalculates() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        let original_bounds = Rect::new(5, 5, 30, 10);
        desktop.add_window(Window::new(original_bounds, "W1"));

        // Minimize
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();
        assert_eq!(desktop.task_shelf_height(), 1);

        // Restore
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.restore();
            }
        }
        desktop.recalculate_shelf();
        assert_eq!(desktop.task_shelf_height(), 0, "shelf clears after restore");

        // Bounds should be restored
        let b = desktop.windows().child_at(0).unwrap().bounds();
        assert_eq!(b, original_bounds, "window restored to original bounds");
    }

    #[test]
    fn test_desktop_shelf_close_minimized_recalculates() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        let id2 = desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));

        // Minimize both
        for i in 0..2 {
            if let Some(child) = desktop.windows_mut().child_at_mut(i) {
                if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                    win.minimize();
                }
            }
        }
        desktop.recalculate_shelf();
        assert_eq!(desktop.task_shelf_height(), 1);

        // Close one
        desktop.close_window(id2);
        // close_window already calls recalculate_shelf
        assert_eq!(desktop.task_shelf_height(), 1, "still 1 minimized window");
    }

    #[test]
    fn test_desktop_tile_skips_minimized() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(15, 15, 30, 10), "W3"));

        // Minimize W3 (index 2)
        if let Some(child) = desktop.windows_mut().child_at_mut(2) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();

        // Tile — should only arrange W1 and W2
        desktop.tile();

        // W1 and W2 should fill the effective area (24 - 1 shelf = 23 rows)
        let c0 = desktop.windows().child_at(0).unwrap().bounds();
        let c1 = desktop.windows().child_at(1).unwrap().bounds();
        let eff = desktop.effective_area();

        // With 2 windows: 1x2 or 2x1 grid in the effective area
        assert!(c0.height > 1, "tiled window should have height > 1");
        assert!(c1.height > 1, "tiled window should have height > 1");
        // The tiled windows should fit within effective_area
        assert!(
            c0.y + c0.height <= eff.y + eff.height,
            "W1 within effective area"
        );
        assert!(
            c1.y + c1.height <= eff.y + eff.height,
            "W2 within effective area"
        );
    }

    #[test]
    fn test_desktop_cascade_skips_minimized() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(15, 15, 30, 10), "W3"));

        // Minimize W1 (index 0)
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();

        // Cascade — should only arrange W2 and W3
        desktop.cascade();

        // W2 and W3 should start at (0,0) and (2,1) in the effective area
        let c1 = desktop.windows().child_at(1).unwrap().bounds();
        let c2 = desktop.windows().child_at(2).unwrap().bounds();

        assert_eq!(c1.x, 0, "first cascaded window at x=0");
        assert_eq!(c1.y, 0, "first cascaded window at y=0");
        assert_eq!(c2.x, 2, "second cascaded window at x=2");
        assert_eq!(c2.y, 1, "second cascaded window at y=1");
    }

    #[test]
    fn test_desktop_effective_area_with_shelf() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 1, 80, 22));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));

        // No shelf initially
        assert_eq!(desktop.effective_area(), Rect::new(0, 1, 80, 22));

        // Minimize
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();

        assert_eq!(
            desktop.effective_area(),
            Rect::new(0, 1, 80, 21),
            "effective area shrinks by shelf height"
        );
    }

    #[test]
    fn test_desktop_close_all_windows() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));
        desktop.add_window(Window::new(Rect::new(15, 15, 30, 10), "W3"));
        assert_eq!(desktop.window_count(), 3);

        desktop.close_all_windows();
        assert_eq!(desktop.window_count(), 0);
        assert_eq!(desktop.task_shelf_height(), 0);
    }

    #[test]
    fn test_desktop_close_all_empty_is_noop() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        assert_eq!(desktop.window_count(), 0);

        desktop.close_all_windows(); // Should not panic
        assert_eq!(desktop.window_count(), 0);
    }

    #[test]
    fn test_desktop_close_all_with_minimized() {
        setup_theme();
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.add_window(Window::new(Rect::new(5, 5, 30, 10), "W1"));
        desktop.add_window(Window::new(Rect::new(10, 10, 30, 10), "W2"));

        // Minimize one
        if let Some(child) = desktop.windows_mut().child_at_mut(0) {
            if let Some(win) = child.as_any_mut().downcast_mut::<Window>() {
                win.minimize();
            }
        }
        desktop.recalculate_shelf();
        assert_eq!(desktop.task_shelf_height(), 1);

        desktop.close_all_windows();
        assert_eq!(desktop.window_count(), 0);
        assert_eq!(desktop.task_shelf_height(), 0);
    }

    #[test]
    fn test_desktop_set_bounds_updates_drag_limits() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        let win = Window::new(Rect::new(5, 5, 30, 10), "Test");
        desktop.add_window(win);

        // Initial drag limits should be the desktop bounds
        let child = desktop.windows().child_at(0).unwrap();
        let win_ref = child.as_any().downcast_ref::<Window>().unwrap();
        assert_eq!(win_ref.drag_limits(), Some(Rect::new(0, 0, 80, 24)));

        // Resize desktop
        desktop.set_bounds(Rect::new(0, 0, 120, 40));

        // Drag limits should be updated
        let child = desktop.windows().child_at(0).unwrap();
        let win_ref = child.as_any().downcast_ref::<Window>().unwrap();
        assert_eq!(win_ref.drag_limits(), Some(Rect::new(0, 0, 120, 40)));
    }
}
