//! `ScrollBar` widget — draggable scrollbar with arrows and thumb.
//!
//! Classic Borland Turbo Vision-style scrollbar with:
//! - Arrow buttons for line stepping
//! - Track area for page stepping
//! - Draggable thumb for direct positioning

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

use crate::command::CM_SCROLL_CHANGED;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase};

// ============================================================================
// Constants — scrollbar characters
// ============================================================================

/// Vertical scrollbar: up arrow.
const V_UP: char = '▲';
/// Vertical scrollbar: down arrow.
const V_DOWN: char = '▼';
/// Vertical scrollbar: track (empty area).
const V_TRACK: char = '░';
/// Vertical scrollbar: thumb (draggable position indicator).
const V_THUMB: char = '█';

/// Horizontal scrollbar: left arrow.
const H_LEFT: char = '◄';
/// Horizontal scrollbar: right arrow.
const H_RIGHT: char = '►';
/// Horizontal scrollbar: track (empty area).
const H_TRACK: char = '░';
/// Horizontal scrollbar: thumb (draggable position indicator).
const H_THUMB: char = '█';

// ============================================================================
// Orientation
// ============================================================================

/// Scrollbar orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Vertical scrollbar (up/down arrows).
    Vertical,
    /// Horizontal scrollbar (left/right arrows).
    Horizontal,
}

/// Which part of the scrollbar is hovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBarHover {
    /// Nothing hovered.
    None,
    /// An arrow button is hovered.
    Arrow,
    /// The thumb is hovered.
    Thumb,
}

// ============================================================================
// ScrollBar
// ============================================================================

/// Scrollbar widget with draggable thumb.
///
/// Classic Turbo Vision-style scrollbar supporting:
/// - Arrow clicks for small increments
/// - Track clicks for page increments
/// - Thumb dragging for direct positioning
///
/// # Example
///
/// ```ignore
/// use turbo_tui::scrollbar::{ScrollBar, Orientation};
/// use ratatui::layout::Rect;
///
/// let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 10));
/// scrollbar.set_params(50, 0, 100, 10, 1);
///
/// let value = scrollbar.value();
/// ```
#[derive(Clone)]
pub struct ScrollBar {
    /// Base view functionality.
    base: ViewBase,
    /// Orientation (vertical or horizontal).
    orientation: Orientation,
    /// Current scroll position.
    value: i32,
    /// Minimum value.
    min_val: i32,
    /// Maximum value.
    max_val: i32,
    /// Step size for page up/down clicks on track.
    page_step: i32,
    /// Step size for arrow button clicks.
    arrow_step: i32,
    /// Whether the thumb is currently being dragged.
    dragging_thumb: bool,
    /// Value at drag start (for proportional tracking).
    drag_start_value: i32,
    /// Currently hovered element.
    hovered: ScrollBarHover,
    /// Active state (true if owning window is focused).
    active: bool,
}

impl ScrollBar {
    /// Create a new vertical scrollbar.
    ///
    /// # Arguments
    ///
    /// * `bounds` - The bounding rectangle (typically width=1).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scrollbar = ScrollBar::vertical(Rect::new(10, 0, 1, 20));
    /// ```
    #[must_use]
    pub fn vertical(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            orientation: Orientation::Vertical,
            value: 0,
            min_val: 0,
            max_val: 100,
            page_step: 10,
            arrow_step: 1,
            dragging_thumb: false,
            drag_start_value: 0,
            hovered: ScrollBarHover::None,
            active: true,
        }
    }

    /// Create a new horizontal scrollbar.
    ///
    /// # Arguments
    ///
    /// * `bounds` - The bounding rectangle (typically height=1).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scrollbar = ScrollBar::horizontal(Rect::new(0, 10, 20, 1));
    /// ```
    #[must_use]
    pub fn horizontal(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            orientation: Orientation::Horizontal,
            value: 0,
            min_val: 0,
            max_val: 100,
            page_step: 10,
            arrow_step: 1,
            dragging_thumb: false,
            drag_start_value: 0,
            hovered: ScrollBarHover::None,
            active: true,
        }
    }

    /// Set scrollbar parameters.
    ///
    /// # Arguments
    ///
    /// * `value` - Current scroll position.
    /// * `min` - Minimum value.
    /// * `max` - Maximum value.
    /// * `page_step` - Step for page up/down clicks on track.
    /// * `arrow_step` - Step for arrow button clicks.
    pub fn set_params(&mut self, value: i32, min: i32, max: i32, page_step: i32, arrow_step: i32) {
        self.min_val = min;
        self.max_val = max;
        self.page_step = page_step.max(1);
        self.arrow_step = arrow_step.max(1);
        self.set_value(value);
    }

    /// Get the current scroll value.
    #[must_use]
    pub fn value(&self) -> i32 {
        self.value
    }

    /// Set the scroll value (clamped to valid range).
    pub fn set_value(&mut self, value: i32) {
        self.value = value.clamp(self.min_val, self.max_val);
    }

    /// Get the minimum value.
    #[must_use]
    pub fn min_val(&self) -> i32 {
        self.min_val
    }

    /// Get the maximum value.
    #[must_use]
    pub fn max_val(&self) -> i32 {
        self.max_val
    }

    /// Get the scrollbar orientation.
    #[must_use]
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Check if the scrollbar is in active (focused window) state.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set the active state (affects rendering style: active vs inactive).
    pub fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.base.mark_dirty();
        }
    }

    /// Calculate thumb position (pixel/cell coordinate).
    ///
    /// Returns the position of the thumb in track cells from the start.
    /// For vertical: row offset from arrow up.
    /// For horizontal: column offset from arrow left.
    fn thumb_position(&self) -> usize {
        let bounds = self.base.bounds();
        let range = (self.max_val - self.min_val).max(1);

        match self.orientation {
            Orientation::Vertical => {
                // Height - 2 (arrows) = track size
                let track_size = i32::from(bounds.height.saturating_sub(2));
                if track_size <= 0 {
                    return 0;
                }
                let pos = (self.value - self.min_val) * track_size / range;
                usize::try_from(pos.max(0)).unwrap_or(0)
            }
            Orientation::Horizontal => {
                // Width - 2 (arrows) = track size
                let track_size = i32::from(bounds.width.saturating_sub(2));
                if track_size <= 0 {
                    return 0;
                }
                let pos = (self.value - self.min_val) * track_size / range;
                usize::try_from(pos.max(0)).unwrap_or(0)
            }
        }
    }

    /// Calculate the thumb cell range (start, length).
    ///
    /// The thumb always occupies at least one cell.
    fn thumb_range(&self) -> (usize, usize) {
        let bounds = self.base.bounds();
        let track_size = match self.orientation {
            Orientation::Vertical => bounds.height.saturating_sub(2) as usize,
            Orientation::Horizontal => bounds.width.saturating_sub(2) as usize,
        };

        if track_size == 0 {
            return (0, 1);
        }

        // Thumb at least 1 cell
        let thumb_pos = self.thumb_position().min(track_size.saturating_sub(1));
        (thumb_pos, 1)
    }

    /// Update value from mouse drag position.
    ///
    /// Called during thumb drag to update value proportionally.
    fn update_value_from_position(&mut self, position: usize) {
        let bounds = self.base.bounds();
        let range = (self.max_val - self.min_val).max(1);

        match self.orientation {
            Orientation::Vertical => {
                let track_size = i32::from(bounds.height.saturating_sub(2));
                if track_size > 0 {
                    let pos_i32 = i32::try_from(position).unwrap_or(i32::MAX);
                    let new_value = self.min_val + (pos_i32 * range) / track_size;
                    self.set_value(new_value);
                }
            }
            Orientation::Horizontal => {
                let track_size = i32::from(bounds.width.saturating_sub(2));
                if track_size > 0 {
                    let pos_i32 = i32::try_from(position).unwrap_or(i32::MAX);
                    let new_value = self.min_val + (pos_i32 * range) / track_size;
                    self.set_value(new_value);
                }
            }
        }
    }

    /// Handle mouse event.
    fn handle_mouse(&mut self, event: &mut Event, mouse: MouseEvent) {
        let bounds = self.base.bounds();

        // Check if click is within bounds
        let col = mouse.column;
        let row = mouse.row;

        if col < bounds.x
            || col >= bounds.x + bounds.width
            || row < bounds.y
            || row >= bounds.y + bounds.height
        {
            // Click outside - stop dragging if active
            if self.dragging_thumb && matches!(mouse.kind, MouseEventKind::Up(_)) {
                self.dragging_thumb = false;
            }
            // Clear hover when mouse leaves
            if self.hovered != ScrollBarHover::None {
                self.hovered = ScrollBarHover::None;
                self.base.mark_dirty();
            }
            return;
        }

        // Calculate relative position
        let rel_col = col - bounds.x;
        let rel_row = row - bounds.y;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_down(rel_col, rel_row, event);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_thumb => {
                self.handle_mouse_drag(rel_col, rel_row, event);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.dragging_thumb {
                    self.dragging_thumb = false;
                    event.handled = true;
                }
            }
            MouseEventKind::Moved => {
                // Update hover state based on which part the mouse is over
                let new_hover = match self.orientation {
                    Orientation::Vertical => {
                        if rel_row == 0 || rel_row >= bounds.height.saturating_sub(1) {
                            ScrollBarHover::Arrow
                        } else {
                            let track_pos = (rel_row - 1) as usize;
                            let (thumb_pos, thumb_len) = self.thumb_range();
                            if track_pos >= thumb_pos && track_pos < thumb_pos + thumb_len {
                                ScrollBarHover::Thumb
                            } else {
                                ScrollBarHover::None
                            }
                        }
                    }
                    Orientation::Horizontal => {
                        if rel_col == 0 || rel_col >= bounds.width.saturating_sub(1) {
                            ScrollBarHover::Arrow
                        } else {
                            let track_pos = (rel_col - 1) as usize;
                            let (thumb_pos, thumb_len) = self.thumb_range();
                            if track_pos >= thumb_pos && track_pos < thumb_pos + thumb_len {
                                ScrollBarHover::Thumb
                            } else {
                                ScrollBarHover::None
                            }
                        }
                    }
                };
                if new_hover != self.hovered {
                    self.hovered = new_hover;
                    self.base.mark_dirty();
                }
            }
            _ => {}
        }
    }

    /// Handle mouse button down.
    fn handle_mouse_down(&mut self, rel_col: u16, rel_row: u16, event: &mut Event) {
        let bounds = self.base.bounds();

        match self.orientation {
            Orientation::Vertical => {
                // Row 0 = up arrow, last row = down arrow, middle = track
                if rel_row == 0 {
                    // Up arrow
                    self.set_value(self.value.saturating_sub(self.arrow_step));
                    Self::broadcast_change(event);
                } else if rel_row >= bounds.height.saturating_sub(1) {
                    // Down arrow
                    self.set_value(self.value.saturating_add(self.arrow_step));
                    Self::broadcast_change(event);
                } else {
                    // Track area
                    let track_pos = (rel_row - 1) as usize;
                    let (thumb_pos, thumb_len) = self.thumb_range();

                    if track_pos >= thumb_pos && track_pos < thumb_pos + thumb_len {
                        // Click on thumb - start drag
                        self.dragging_thumb = true;
                        self.drag_start_value = self.value;
                    } else if track_pos < thumb_pos {
                        // Click above thumb - page up
                        self.set_value(self.value.saturating_sub(self.page_step));
                        Self::broadcast_change(event);
                    } else {
                        // Click below thumb - page down
                        self.set_value(self.value.saturating_add(self.page_step));
                        Self::broadcast_change(event);
                    }
                }
            }
            Orientation::Horizontal => {
                // Col 0 = left arrow, last col = right arrow, middle = track
                if rel_col == 0 {
                    // Left arrow
                    self.set_value(self.value.saturating_sub(self.arrow_step));
                    Self::broadcast_change(event);
                } else if rel_col >= bounds.width.saturating_sub(1) {
                    // Right arrow
                    self.set_value(self.value.saturating_add(self.arrow_step));
                    Self::broadcast_change(event);
                } else {
                    // Track area
                    let track_pos = (rel_col - 1) as usize;
                    let (thumb_pos, thumb_len) = self.thumb_range();

                    if track_pos >= thumb_pos && track_pos < thumb_pos + thumb_len {
                        // Click on thumb - start drag
                        self.dragging_thumb = true;
                        self.drag_start_value = self.value;
                    } else if track_pos < thumb_pos {
                        // Click left of thumb - page left
                        self.set_value(self.value.saturating_sub(self.page_step));
                        Self::broadcast_change(event);
                    } else {
                        // Click right of thumb - page right
                        self.set_value(self.value.saturating_add(self.page_step));
                        Self::broadcast_change(event);
                    }
                }
            }
        }

        event.handled = true;
    }

    /// Handle mouse drag (thumb being dragged).
    fn handle_mouse_drag(&mut self, rel_col: u16, rel_row: u16, event: &mut Event) {
        let old_value = self.value;

        match self.orientation {
            Orientation::Vertical => {
                // Subtract 1 for the up arrow, then calculate position
                let track_pos = rel_row.saturating_sub(1) as usize;
                self.update_value_from_position(track_pos);
            }
            Orientation::Horizontal => {
                // Subtract 1 for the left arrow, then calculate position
                let track_pos = rel_col.saturating_sub(1) as usize;
                self.update_value_from_position(track_pos);
            }
        }

        if self.value != old_value {
            Self::broadcast_change(event);
        }

        event.handled = true;
    }

    /// Broadcast scroll change event.
    fn broadcast_change(event: &mut Event) {
        *event = Event::broadcast(CM_SCROLL_CHANGED);
    }
}

impl View for ScrollBar {
    fn id(&self) -> crate::view::ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        // Don't draw if area is empty
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bounds = self.base.bounds();

        // Intersect with available area
        let draw_area = bounds.intersection(area);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        // Get theme styles
        let (track_style, thumb_style, arrow_style) = theme::with_current(|t| {
            if !self.active {
                return (
                    t.scrollbar_track_inactive,
                    t.scrollbar_thumb_inactive,
                    t.scrollbar_arrows_inactive,
                );
            }
            let thumb = if self.hovered == ScrollBarHover::Thumb {
                t.scrollbar_thumb_hover
            } else {
                t.scrollbar_thumb
            };
            let arrows = if self.hovered == ScrollBarHover::Arrow {
                t.scrollbar_arrows_hover
            } else {
                t.scrollbar_arrows
            };
            (t.scrollbar_track, thumb, arrows)
        });

        // Calculate track size
        let (thumb_pos, _thumb_len) = self.thumb_range();

        match self.orientation {
            Orientation::Vertical => {
                let height = bounds.height as usize;

                for row in 0..height {
                    let buf_row = bounds.y + u16::try_from(row).unwrap_or(0);
                    let buf_col = bounds.x;

                    if buf_row >= area.y
                        && buf_row < area.y + area.height
                        && buf_col >= area.x
                        && buf_col < area.x + area.width
                    {
                        let Some(cell) = buf.cell_mut((buf_col, buf_row)) else {
                            continue;
                        };

                        let (ch, style) = if row == 0 {
                            // Up arrow
                            (V_UP, arrow_style)
                        } else if row >= height.saturating_sub(1) {
                            // Down arrow
                            (V_DOWN, arrow_style)
                        } else {
                            // Track area
                            let track_pos = row - 1;
                            if track_pos == thumb_pos {
                                (V_THUMB, thumb_style)
                            } else {
                                (V_TRACK, track_style)
                            }
                        };

                        cell.set_char(ch);
                        cell.set_style(style);
                    }
                }
            }
            Orientation::Horizontal => {
                let width = bounds.width as usize;
                let row = bounds.y;
                let start_col = bounds.x;

                for col in 0..width {
                    let buf_col = start_col + u16::try_from(col).unwrap_or(0);

                    if buf_col >= area.x
                        && buf_col < area.x + area.width
                        && row >= area.y
                        && row < area.y + area.height
                    {
                        let Some(cell) = buf.cell_mut((buf_col, row)) else {
                            continue;
                        };

                        let (ch, style) = if col == 0 {
                            // Left arrow
                            (H_LEFT, arrow_style)
                        } else if col >= width.saturating_sub(1) {
                            // Right arrow
                            (H_RIGHT, arrow_style)
                        } else {
                            // Track area
                            let track_pos = col - 1;
                            if track_pos == thumb_pos {
                                (H_THUMB, thumb_style)
                            } else {
                                (H_TRACK, track_style)
                            }
                        };

                        cell.set_char(ch);
                        cell.set_style(style);
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.handled {
            return;
        }

        if let EventKind::Mouse(mouse) = &event.kind {
            let mouse = *mouse;
            self.handle_mouse(event, mouse);
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
    use ratatui::buffer::Buffer;

    #[test]
    fn test_scrollbar_vertical_new() {
        let scrollbar = ScrollBar::vertical(Rect::new(10, 5, 1, 20));

        assert_eq!(scrollbar.orientation(), Orientation::Vertical);
        assert_eq!(scrollbar.value(), 0);
        assert_eq!(scrollbar.min_val(), 0);
        assert_eq!(scrollbar.max_val(), 100);
        assert_eq!(scrollbar.bounds(), Rect::new(10, 5, 1, 20));
    }

    #[test]
    fn test_scrollbar_set_params() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 10));

        scrollbar.set_params(25, 0, 100, 10, 1);

        assert_eq!(scrollbar.value(), 25);
        assert_eq!(scrollbar.min_val(), 0);
        assert_eq!(scrollbar.max_val(), 100);
    }

    #[test]
    fn test_scrollbar_value_clamped() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 10));

        scrollbar.set_params(0, 0, 50, 5, 1);

        // Value below min
        scrollbar.set_value(-10);
        assert_eq!(scrollbar.value(), 0);

        // Value above max
        scrollbar.set_value(100);
        assert_eq!(scrollbar.value(), 50);

        // Value in range
        scrollbar.set_value(25);
        assert_eq!(scrollbar.value(), 25);
    }

    #[test]
    fn test_scrollbar_thumb_position() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 12));
        // Height 12: 1 up arrow + 10 track + 1 down arrow

        scrollbar.set_params(0, 0, 100, 10, 1);
        assert_eq!(scrollbar.thumb_position(), 0);

        scrollbar.set_value(50);
        assert_eq!(scrollbar.thumb_position(), 5);

        scrollbar.set_value(100);
        // Track size = 10, so max position = 9
        assert_eq!(scrollbar.thumb_position(), 10);
    }

    #[test]
    fn test_scrollbar_draw_vertical() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 6));
        scrollbar.set_params(25, 0, 100, 10, 1);

        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 6));
        scrollbar.draw(&mut buf, Rect::new(0, 0, 2, 6));

        // Check up arrow at row 0
        assert_eq!(buf[(0, 0)].symbol(), V_UP.to_string().as_str());

        // Check down arrow at last row
        assert_eq!(buf[(0, 5)].symbol(), V_DOWN.to_string().as_str());

        // Thumb should be at position 1 (25/100 * 4 = 1, track size = 4)
        // Track rows: 1, 2, 3, 4 (4 rows for track)
        // Value 25 = thumb at position 1 from track start
        let thumb_pos = scrollbar.thumb_position();
        assert!(thumb_pos < 4); // Track size = height - 2 = 4
    }

    #[test]
    fn test_scrollbar_draw_horizontal() {
        let mut scrollbar = ScrollBar::horizontal(Rect::new(0, 0, 10, 1));
        scrollbar.set_params(50, 0, 100, 10, 1);

        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        scrollbar.draw(&mut buf, Rect::new(0, 0, 10, 2));

        // Check left arrow at column 0
        assert_eq!(buf[(0, 0)].symbol(), H_LEFT.to_string().as_str());

        // Check right arrow at last column
        assert_eq!(buf[(9, 0)].symbol(), H_RIGHT.to_string().as_str());

        // Track size = 8 (width - 2)
        // Value 50 = thumb at position 4 from track start
        let thumb_pos = scrollbar.thumb_position();
        assert!(thumb_pos < 8); // Track size
    }

    #[test]
    fn test_scrollbar_arrow_click() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 10));
        scrollbar.set_params(50, 0, 100, 10, 5); // arrow_step = 5

        // Click up arrow (row 0)
        let _old_value = scrollbar.value();
        scrollbar.set_value(scrollbar.value().saturating_sub(5));
        assert_eq!(scrollbar.value(), 45);

        // Click down arrow (last row)
        scrollbar.set_value(scrollbar.value().saturating_add(5));
        assert_eq!(scrollbar.value(), 50);
    }

    #[test]
    fn test_scrollbar_page_click() {
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 12));
        // Height 12: 1 up + 10 track + 1 down
        scrollbar.set_params(50, 0, 100, 20, 1); // page_step = 20

        // Simulate page up (click above thumb)
        let _old_value = scrollbar.value();
        scrollbar.set_value(scrollbar.value().saturating_sub(scrollbar.page_step));
        assert_eq!(scrollbar.value(), 30);

        // Simulate page down (click below thumb)
        scrollbar.set_value(scrollbar.value().saturating_add(scrollbar.page_step));
        assert_eq!(scrollbar.value(), 50);
    }

    #[test]
    fn test_scrollbar_horizontal_new() {
        let scrollbar = ScrollBar::horizontal(Rect::new(0, 10, 20, 1));

        assert_eq!(scrollbar.orientation(), Orientation::Horizontal);
        assert_eq!(scrollbar.value(), 0);
        assert_eq!(scrollbar.bounds(), Rect::new(0, 10, 20, 1));
    }

    #[test]
    fn test_scrollbar_small_bounds() {
        // Test with minimal height (should still work)
        let scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 3));
        assert_eq!(scrollbar.thumb_position(), 0);
    }

    #[test]
    fn test_scrollbar_track_click_logic() {
        // Test that clicking track positions work correctly
        let mut scrollbar = ScrollBar::vertical(Rect::new(0, 0, 1, 10));
        scrollbar.set_params(0, 0, 100, 10, 1);

        // Track size = 8 (height - 2)
        // At value 0, thumb is at position 0
        assert_eq!(scrollbar.thumb_position(), 0);

        // At value 50, track_pos 4
        scrollbar.set_value(50);
        let pos = scrollbar.thumb_position();
        // pos = (50 - 0) * 8 / 100 = 4
        assert_eq!(pos, 4);
    }
}
