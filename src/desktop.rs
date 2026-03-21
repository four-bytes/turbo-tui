//! Desktop — Container that manages overlapping windows.
//!
//! A `Desktop` is a specialized [`Group`] that:
//! - Fills the whole terminal area
//! - Draws a background pattern
//! - Manages windows with Z-order, click-to-focus, tiling, and cascading

use crate::group::Group;
use crate::view::{Event, EventKind, View, ViewId};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use std::any::Any;

/// Desktop container that manages overlapping windows.
///
/// A `Desktop` fills the terminal area and provides:
/// - Background pattern rendering
/// - Window management (add, close, tile, cascade)
/// - Click-to-focus (clicking a window brings it to front)
///
/// # Example
///
/// ```ignore
/// let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
/// desktop.set_background('░', Style::default().bg(Color::DarkGray));
///
/// // Add windows
/// let win1 = Box::new(Window::new(Rect::new(5, 3, 30, 15), "Document 1"));
/// desktop.add_window(win1);
///
/// // Tile all windows
/// desktop.tile_windows();
/// ```
pub struct Desktop {
    /// Interior group handling Z-order and event dispatch.
    group: Group,
    /// Background pattern character.
    background_char: char,
    /// Background pattern style.
    background_style: Style,
}

impl Desktop {
    /// Create a new desktop with the given bounds.
    ///
    /// Defaults:
    /// - Background character: `'░'` (light shade)
    /// - Background style: `Style::default()`
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            group: Group::new(bounds),
            background_char: '░',
            background_style: Style::default(),
        }
    }

    /// Set the background pattern.
    pub fn set_background(&mut self, ch: char, style: Style) {
        self.background_char = ch;
        self.background_style = style;
    }

    /// Add a window to the desktop (on top).
    ///
    /// Returns the [`ViewId`] of the added window.
    pub fn add_window(&mut self, window: Box<dyn View>) -> ViewId {
        let id = self.group.add(window);
        // Set focus to the new window
        let last_idx = self.group.child_count().saturating_sub(1);
        self.group.set_focus_to(last_idx);
        id
    }

    /// Close a window by [`ViewId`].
    ///
    /// Returns `true` if a window was removed.
    pub fn close_window(&mut self, id: ViewId) -> bool {
        self.group.remove_by_id(id).is_some()
    }

    /// Bring a window to front and set it as active.
    ///
    /// Does nothing if the window is not found.
    pub fn activate_window(&mut self, id: ViewId) {
        if let Some(idx) = self.find_window_index(id) {
            let last_idx = self.group.child_count().saturating_sub(1);
            if idx == last_idx {
                self.group.set_focus_to(idx);
            } else {
                self.group.bring_to_front(idx);
                // After bring_to_front, the window is at the end
                let new_last_idx = self.group.child_count().saturating_sub(1);
                self.group.set_focus_to(new_last_idx);
            }
        }
    }

    /// Get the active (front-most, focused) window's [`ViewId`].
    ///
    /// Returns `None` if there are no windows.
    #[must_use]
    pub fn active_window_id(&self) -> Option<ViewId> {
        self.group
            .focused_index()
            .and_then(|idx| self.group.child_at(idx).map(View::id))
    }

    /// Get the number of windows.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.group.child_count()
    }

    /// Tile all windows evenly across the desktop.
    ///
    /// Arranges windows in a grid pattern, calculating columns and rows
    /// to fill the desktop area.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn tile_windows(&mut self) {
        let count = self.group.child_count();
        if count == 0 {
            return;
        }

        let area = self.group.bounds();
        let count_u16 = count as u16;
        // Calculate grid dimensions: cols = ceil(sqrt(count))
        let cols = (count as f64).sqrt().ceil() as u16;
        let rows = count_u16.div_ceil(cols);
        let w = area.width / cols.max(1);
        let h = area.height / rows.max(1);

        for (i, child) in self.group.children_mut().iter_mut().enumerate() {
            let col = i as u16 % cols;
            let row = i as u16 / cols;
            let x = area.x + col * w;
            let y = area.y + row * h;
            // Last column/row gets remaining space
            let actual_w = if col == cols.saturating_sub(1) {
                area.width.saturating_sub(col * w)
            } else {
                w
            };
            let actual_h = if row == rows.saturating_sub(1) {
                area.height.saturating_sub(row * h)
            } else {
                h
            };
            child.set_bounds(Rect::new(x, y, actual_w, actual_h));
        }
    }

    /// Cascade windows (offset each by 1,1 from previous).
    ///
    /// Each window is positioned at (i, i) with the same size,
    /// creating a cascading effect.
    #[allow(clippy::cast_possible_truncation)]
    pub fn cascade_windows(&mut self) {
        let area = self.group.bounds();
        let count = self.group.child_count();
        let count_u16 = count as u16;
        let w = area.width.saturating_sub(count_u16).max(20);
        let h = area.height.saturating_sub(count_u16).max(8);

        for (i, child) in self.group.children_mut().iter_mut().enumerate() {
            let x = area.x + i as u16;
            let y = area.y + i as u16;
            child.set_bounds(Rect::new(x, y, w, h));
        }
    }

    /// Cycle to next window (F6 / Ctrl+Tab style).
    pub fn next_window(&mut self) {
        self.group.focus_next();
    }

    /// Cycle to previous window.
    pub fn prev_window(&mut self) {
        self.group.focus_prev();
    }

    /// Find the index of a window by ID.
    fn find_window_index(&self, id: ViewId) -> Option<usize> {
        self.group.children().iter().position(|c| c.id() == id)
    }

    /// Draw background pattern.
    fn draw_background(&self, buf: &mut Buffer, area: Rect) {
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                buf.set_string(
                    x,
                    y,
                    self.background_char.to_string(),
                    self.background_style,
                );
            }
        }
    }
}

impl View for Desktop {
    fn id(&self) -> ViewId {
        self.group.id()
    }

    fn bounds(&self) -> Rect {
        self.group.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.group.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        // 1. Draw background pattern
        self.draw_background(buf, area);

        // 2. Draw all children (windows) via Group in Z-order
        self.group.draw(buf, area);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        // Special: click on a non-front window → bring to front first
        if let EventKind::Mouse(mouse) = &event.kind {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                if let Some(hit_idx) = self.group.child_at_point(mouse.column, mouse.row) {
                    let last_idx = self.group.child_count().saturating_sub(1);
                    if hit_idx != last_idx {
                        // Bring clicked window to front
                        self.group.bring_to_front(hit_idx);
                        // After bring_to_front, the window is at the last index
                        let new_last_idx = self.group.child_count().saturating_sub(1);
                        self.group.set_focus_to(new_last_idx);
                        // Continue to let the window handle the click
                        // (don't clear the event)
                    }
                }
            }
        }

        // Delegate to Group for three-phase dispatch
        self.group.handle_event(event);
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn state(&self) -> u16 {
        self.group.state()
    }

    fn set_state(&mut self, state: u16) {
        self.group.set_state(state);
    }

    fn options(&self) -> u16 {
        self.group.options()
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
    use crate::window::Window;

    /// Simple test window for tests.
    fn make_window(x: u16, y: u16, w: u16, h: u16, title: &str) -> Box<Window> {
        Box::new(Window::new(Rect::new(x, y, w, h), title))
    }

    #[test]
    fn test_desktop_new() {
        let desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        assert_eq!(desktop.bounds(), Rect::new(0, 0, 80, 24));
        assert_eq!(desktop.background_char, '░');
        assert_eq!(desktop.window_count(), 0);
    }

    #[test]
    fn test_desktop_set_background() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));
        desktop.set_background('█', Style::default());

        assert_eq!(desktop.background_char, '█');
    }

    #[test]
    fn test_desktop_add_close_window() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let id1 = desktop.add_window(make_window(5, 5, 20, 10, "Window 1"));
        assert_eq!(desktop.window_count(), 1);

        let id2 = desktop.add_window(make_window(10, 10, 20, 10, "Window 2"));
        assert_eq!(desktop.window_count(), 2);

        // Close first window
        assert!(desktop.close_window(id1));
        assert_eq!(desktop.window_count(), 1);

        // Close non-existent window
        assert!(!desktop.close_window(id1));

        // Close second window
        assert!(desktop.close_window(id2));
        assert_eq!(desktop.window_count(), 0);
    }

    #[test]
    fn test_desktop_active_window() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // No windows → no active
        assert!(desktop.active_window_id().is_none());

        // Add windows
        let _id1 = desktop.add_window(make_window(5, 5, 20, 10, "Window 1"));
        let id2 = desktop.add_window(make_window(10, 10, 20, 10, "Window 2"));
        let id3 = desktop.add_window(make_window(15, 15, 20, 10, "Window 3"));

        // Last added is active (focused)
        let active = desktop.active_window_id();
        assert_eq!(active, Some(id3));

        // Activate first window
        desktop.activate_window(id2);
        let active = desktop.active_window_id();
        assert_eq!(active, Some(id2));
    }

    #[test]
    fn test_desktop_next_prev_window() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        let id1 = desktop.add_window(make_window(5, 5, 20, 10, "Window 1"));
        let id2 = desktop.add_window(make_window(10, 10, 20, 10, "Window 2"));
        let _id3 = desktop.add_window(make_window(15, 15, 20, 10, "Window 3"));

        // Start at last (focused on add)
        assert_eq!(desktop.active_window_id(), Some(_id3));

        // Cycle next → wraps to first
        desktop.next_window();
        assert_eq!(desktop.active_window_id(), Some(id1));

        // Cycle next → second
        desktop.next_window();
        assert_eq!(desktop.active_window_id(), Some(id2));

        // Cycle prev → back to first
        desktop.prev_window();
        assert_eq!(desktop.active_window_id(), Some(id1));
    }

    #[test]
    fn test_desktop_tile_windows() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // Add 4 windows
        desktop.add_window(make_window(0, 0, 20, 10, "W1"));
        desktop.add_window(make_window(0, 0, 20, 10, "W2"));
        desktop.add_window(make_window(0, 0, 20, 10, "W3"));
        desktop.add_window(make_window(0, 0, 20, 10, "W4"));

        // Tile
        desktop.tile_windows();

        // Check layout: 2x2 grid (sqrt(4) = 2)
        // Each window should have non-overlapping bounds
        let children = desktop.group.children();

        // Window 0: (0, 0, 40, 12)
        assert_eq!(children[0].bounds(), Rect::new(0, 0, 40, 12));
        // Window 1: (40, 0, 40, 12)
        assert_eq!(children[1].bounds(), Rect::new(40, 0, 40, 12));
        // Window 2: (0, 12, 40, 12)
        assert_eq!(children[2].bounds(), Rect::new(0, 12, 40, 12));
        // Window 3: (40, 12, 40, 12)
        assert_eq!(children[3].bounds(), Rect::new(40, 12, 40, 12));
    }

    #[test]
    fn test_desktop_tile_windows_one() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // Single window fills the desktop
        desktop.add_window(make_window(5, 5, 20, 10, "W1"));
        desktop.tile_windows();

        let bounds = desktop.group.children()[0].bounds();
        assert_eq!(bounds, Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn test_desktop_cascade_windows() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // Add 3 windows
        desktop.add_window(make_window(0, 0, 20, 10, "W1"));
        desktop.add_window(make_window(0, 0, 20, 10, "W2"));
        desktop.add_window(make_window(0, 0, 20, 10, "W3"));

        // Cascade
        desktop.cascade_windows();

        let children = desktop.group.children();
        let w = 80u16.saturating_sub(3).max(20);
        let h = 24u16.saturating_sub(3).max(8);

        // Each window is offset by (i, i)
        assert_eq!(children[0].bounds(), Rect::new(0, 0, w, h));
        assert_eq!(children[1].bounds(), Rect::new(1, 1, w, h));
        assert_eq!(children[2].bounds(), Rect::new(2, 2, w, h));
    }

    #[test]
    fn test_desktop_click_to_focus() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        // Add two windows at different positions
        let id_back = desktop.add_window(Box::new(Window::new(Rect::new(5, 5, 30, 15), "Back")));
        let id_front =
            desktop.add_window(Box::new(Window::new(Rect::new(10, 10, 30, 15), "Front")));

        // Front is active (last added)
        assert_eq!(desktop.active_window_id(), Some(id_front));

        // Click on back window (at position 6, 6 which is inside back but not front)
        let mut event = Event::mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 6,
            row: 6,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        desktop.handle_event(&mut event);

        // Back window should now be active (brought to front and focused)
        let active = desktop.active_window_id();
        assert_eq!(active, Some(id_back));
    }

    #[test]
    fn test_desktop_bounds_change_propagates() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

        desktop.set_bounds(Rect::new(0, 0, 100, 40));
        assert_eq!(desktop.bounds(), Rect::new(0, 0, 100, 40));
        assert_eq!(desktop.group.bounds(), Rect::new(0, 0, 100, 40));
    }
}
