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
        }
        removed
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
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn tile(&mut self) {
        let count = self.windows.child_count();
        if count == 0 {
            return;
        }

        let area = self.base.bounds();

        // Calculate grid dimensions
        let cols = (count as f64).sqrt().ceil() as u16;
        let rows = ((count as f64) / f64::from(cols)).ceil() as u16;

        let w = area.width / cols;
        let h = area.height / rows;

        for i in 0..count {
            #[allow(clippy::cast_possible_truncation)]
            let col = (i as u16) % cols;
            #[allow(clippy::cast_possible_truncation)]
            let row = (i as u16) / cols;

            let x = area.x + col * w;
            let y = area.y + row * h;

            // Last column/row gets remaining space
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

            if let Some(child) = self.windows.child_at_mut(i) {
                child.set_bounds(Rect::new(x, y, actual_w, actual_h));
            }
        }
        self.base.mark_dirty();
    }

    /// Cascade windows from top-left with offset.
    ///
    /// Each window gets a 2-column, 1-row offset from the previous one.
    /// Windows are resized to approximately 60% of the desktop area.
    pub fn cascade(&mut self) {
        let count = self.windows.child_count();
        if count == 0 {
            return;
        }

        let area = self.base.bounds();
        let w = (area.width * 3) / 5; // 60% width
        let h = (area.height * 3) / 5; // 60% height

        for i in 0..count {
            #[allow(clippy::cast_possible_truncation)]
            let offset_x = (i as u16) * 2;
            #[allow(clippy::cast_possible_truncation)]
            let offset_y = i as u16;

            let x = area.x + offset_x;
            let y = area.y + offset_y;

            // Clamp to area
            let actual_w = w.min(area.x + area.width - x);
            let actual_h = h.min(area.y + area.height - y);

            if let Some(child) = self.windows.child_at_mut(i) {
                child.set_bounds(Rect::new(x, y, actual_w, actual_h));
            }
        }
        self.base.mark_dirty();
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

        match &event.kind.clone() {
            EventKind::Mouse(mouse) => {
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
            }

            // Key/Command events → delegate to container (three-phase dispatch)
            _ => {
                self.windows.handle_event(event);
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
        crate::theme::set(Theme::borland_classic());
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
        let (theme_bg, theme_char) =
            theme::with_current(|t| (t.desktop_bg, t.desktop_char));
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
}
