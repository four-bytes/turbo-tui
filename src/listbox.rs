//! `ListBox` — scrollable selectable list widget.
//!
//! A `ListBox` displays a vertical list of items with keyboard and mouse
//! navigation. One item is highlighted at a time (single-selection model).
//! If content exceeds the visible area, a scrollbar is shown.
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{ListBox, Rect};
//!
//! let list = ListBox::new(Rect::new(5, 3, 20, 10))
//!     .with_items(vec![
//!         "Apple".into(),
//!         "Banana".into(),
//!         "Cherry".into(),
//!     ])
//!     .with_selected(1);
//! ```

use crate::clip;
use crate::theme;
use crate::view::{
    Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE,
};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Scrollbar characters used when content exceeds visible area.
const SB_UP: char = '▲';
const SB_DOWN: char = '▼';
const SB_TRACK: char = '░';
const SB_THUMB: char = '█';

/// Scrollable selectable list widget with single-selection model.
///
/// # Keyboard Navigation
///
/// | Key | Action |
/// |-----|--------|
/// | Up / Down | Select previous/next item |
/// | Page Up / Page Down | Scroll by one page |
/// | Home | Select first item |
/// | End | Select last item |
///
/// # Mouse Navigation
///
/// - Click on an item to select it
/// - Scroll wheel to move selection up/down
///
/// # Visual Style
///
/// ```text
/// ┌─────────────────────┐
/// │ Item 1              │  ← normal style
/// │ ▓ Item 2 ▓          │  ← selected style (highlighted)
/// │ Item 3              │
/// │ ▲                   │  ← scrollbar (when content overflows)
/// │ █                   │
/// │ ▼                   │
/// └─────────────────────┘
/// ```
pub struct ListBox {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// List of display items.
    items: Vec<String>,
    /// Index of the currently selected item (0-based).
    selected: usize,
    /// Scroll offset (first visible item index).
    scroll: usize,
}

impl ListBox {
    /// Create a new empty `ListBox` with the given bounds.
    ///
    /// The list is selectable (can receive focus).
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the list area.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            items: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }

    /// Set the list items, replacing any existing items.
    ///
    /// The selected index is clamped to the new item count.
    #[must_use]
    pub fn with_items(mut self, items: Vec<String>) -> Self {
        self.items = items;
        let max_idx = self.items.len().saturating_sub(1);
        if self.selected > max_idx {
            self.selected = max_idx;
        }
        self.clamp_scroll();
        self
    }

    /// Set the initially selected item index.
    ///
    /// Clamped to the valid range `0..items.len()`.
    #[must_use]
    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        let max_idx = self.items.len().saturating_sub(1);
        if self.selected > max_idx {
            self.selected = max_idx;
        }
        self.clamp_scroll();
        self
    }

    /// Get the list of items.
    #[must_use]
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Get the currently selected index.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Get the text of the currently selected item, if any.
    #[must_use]
    pub fn selected_text(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }

    /// Get the current scroll offset (first visible item index).
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Set the list items (mutable setter).
    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        let max_idx = self.items.len().saturating_sub(1);
        if self.selected > max_idx {
            self.selected = max_idx;
        }
        self.clamp_scroll();
        self.base.mark_dirty();
    }

    /// Set the selected index directly.
    pub fn set_selected(&mut self, selected: usize) {
        self.selected = selected.min(self.items.len().saturating_sub(1));
        self.clamp_scroll();
        self.base.mark_dirty();
    }

    /// Push a new item onto the list.
    ///
    /// The item is added at the end. Scroll is clamped and the view is marked
    /// dirty so the next draw will show the new item.
    pub fn push_item(&mut self, item: String) {
        self.items.push(item);
        self.clamp_scroll();
        self.base.mark_dirty();
    }

    /// Get the current option flags.
    #[must_use]
    pub fn get_options(&self) -> u16 {
        self.base.options()
    }

    /// Set the option flags directly.
    pub fn set_options(&mut self, options: u16) {
        self.base.set_options(options);
    }

    /// Select the previous item (wrapping not allowed).
    fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            // Scroll up if selection moved above visible area
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
        }
    }

    /// Select the next item (wrapping not allowed).
    fn select_next(&mut self) {
        if self.selected < self.items.len().saturating_sub(1) {
            self.selected += 1;
            // Scroll down if selection moved below visible area
            let visible_lines = self.visible_lines();
            if self.selected >= self.scroll + visible_lines {
                self.scroll = self.selected.saturating_sub(visible_lines.saturating_sub(1));
            }
        }
    }

    /// Scroll one page up.
    fn page_up(&mut self) {
        let page_size = self.visible_lines();
        if self.selected > page_size {
            self.selected -= page_size;
            self.scroll = self.scroll.saturating_sub(page_size);
        } else {
            self.selected = 0;
            self.scroll = 0;
        }
    }

    /// Scroll one page down.
    fn page_down(&mut self) {
        let page_size = self.visible_lines();
        let max_idx = self.items.len().saturating_sub(1);
        if self.selected + page_size < max_idx {
            self.selected += page_size;
            self.scroll += page_size;
        } else {
            self.selected = max_idx;
            // Adjust scroll so the last item is visible
            self.scroll = self.selected.saturating_sub(page_size.saturating_sub(1));
        }
        self.clamp_scroll();
    }

    /// Number of visible lines in the list area (height of bounds).
    fn visible_lines(&self) -> usize {
        let bounds = self.base.bounds();
        // Reserve 1 column for optional scrollbar
        usize::from(bounds.height)
    }

    /// Whether the scrollbar should be shown.
    fn needs_scrollbar(&self) -> bool {
        let bounds = self.base.bounds();
        self.items.len() > usize::from(bounds.height)
    }

    /// Clamp scroll offset to valid range.
    fn clamp_scroll(&mut self) {
        let bounds = self.base.bounds();
        let visible = usize::from(bounds.height);
        let max_scroll = self.items.len().saturating_sub(visible);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        // Also ensure selected stays in scroll range
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if !self.items.is_empty() && self.selected >= self.scroll + visible {
            self.scroll = self.selected.saturating_sub(visible.saturating_sub(1));
        }
    }

    /// Calculate scrollbar thumb position.
    fn thumb_position(&self) -> usize {
        let bounds = self.base.bounds();
        let visible = usize::from(bounds.height);
        if visible == 0 || self.items.len() <= visible {
            return 0;
        }
        let track_size = visible.saturating_sub(2); // up + down arrows
        if track_size == 0 {
            return 0;
        }
        let range = self.items.len().saturating_sub(visible);
        if range == 0 {
            return 0;
        }
        let pos = self.scroll * track_size / range;
        pos.min(track_size.saturating_sub(1))
    }

    /// Check if a mouse position is inside the list bounds.
    fn is_inside(&self, col: u16, row: u16) -> bool {
        let b = self.base_bounds();
        col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height
    }

    fn base_bounds(&self) -> Rect {
        self.base.bounds()
    }
}

impl View for ListBox {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
        self.clamp_scroll();
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        let bounds = self.base_bounds();
        let draw_area = bounds.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let show_scrollbar = self.needs_scrollbar();

        // Determine content width (reserve 1 column for scrollbar if needed)
        let content_width = if show_scrollbar {
            bounds.width.saturating_sub(1)
        } else {
            bounds.width
        };
        if content_width == 0 {
            return;
        }

        let (normal_style, selected_style) = theme::with_current(|t| {
            (t.list_box_normal, t.list_box_selected)
        });

        // Draw visible items
        let visible = usize::from(bounds.height);
        for i in 0..visible.min(self.items.len().saturating_sub(self.scroll)) {
            let item_idx = self.scroll + i;
            let row = bounds.y + u16::try_from(i).unwrap_or(0);

            // Skip rows outside clip
            if row < clip.y || row >= clip.y + clip.height {
                continue;
            }

            let is_selected = item_idx == self.selected;
            let style = if is_selected { selected_style } else { normal_style };

            // Fill entire row background
            for col in bounds.x..bounds.x + content_width {
                if col >= clip.x && col < clip.x + clip.width {
                    if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                        cell.set_char(' ').set_style(style);
                    }
                }
            }

            // Draw item text (clipped)
            if let Some(item) = self.items.get(item_idx) {
                clip::set_string_clipped(buf, bounds.x, row, item, style, clip);
            }
        }

        // Fill remaining visible rows with background
        let remaining = visible.saturating_sub(self.items.len().saturating_sub(self.scroll));
        for i in 0..remaining {
            let row = bounds.y + u16::try_from(self.items.len().saturating_sub(self.scroll) + i).unwrap_or(0);
            if row < clip.y || row >= clip.y + clip.height {
                continue;
            }
            for col in bounds.x..bounds.x + content_width {
                if col >= clip.x && col < clip.x + clip.width {
                    if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                        cell.set_char(' ').set_style(normal_style);
                    }
                }
            }
        }

        // Draw scrollbar if needed
        if show_scrollbar {
            let sb_col = bounds.x + bounds.width.saturating_sub(1);
            if sb_col >= clip.x && sb_col < clip.x + clip.width {
                let (track_style, thumb_style, arrow_style) = theme::with_current(|t| {
                    (t.scrollbar_track, t.scrollbar_thumb, t.scrollbar_arrows)
                });

                let height = usize::from(bounds.height);

                for row_offset in 0..height {
                    let sb_row = bounds.y + u16::try_from(row_offset).unwrap_or(0);
                    if sb_row < clip.y || sb_row >= clip.y + clip.height {
                        continue;
                    }

                    let (ch, style) = if row_offset == 0 {
                        (SB_UP, arrow_style)
                    } else if row_offset >= height.saturating_sub(1) {
                        (SB_DOWN, arrow_style)
                    } else {
                        let track_pos = row_offset - 1;
                        if track_pos == self.thumb_position() {
                            (SB_THUMB, thumb_style)
                        } else {
                            (SB_TRACK, track_style)
                        }
                    };

                    if let Some(cell) = buf.cell_mut(Position::new(sb_col, sb_row)) {
                        cell.set_char(ch).set_style(style);
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Key(key) => {
                if self.items.is_empty() {
                    return;
                }
                match key.code {
                    KeyCode::Up => {
                        self.select_prev();
                        event.clear();
                    }
                    KeyCode::Down => {
                        self.select_next();
                        event.clear();
                    }
                    KeyCode::PageUp => {
                        self.page_up();
                        event.clear();
                    }
                    KeyCode::PageDown => {
                        self.page_down();
                        event.clear();
                    }
                    KeyCode::Home => {
                        self.selected = 0;
                        self.scroll = 0;
                        event.clear();
                    }
                    KeyCode::End => {
                        self.selected = self.items.len() - 1;
                        self.scroll = self.selected.saturating_sub(
                            usize::from(self.base_bounds().height).saturating_sub(1),
                        );
                        event.clear();
                    }
                    _ => {}
                }
            },
            EventKind::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.is_inside(mouse.column, mouse.row) {
                        let bounds = self.base_bounds();
                        let rel_row = (mouse.row - bounds.y) as usize;
                        let index = self.scroll + rel_row;
                        if index < self.items.len() {
                            self.selected = index;
                        }
                        event.clear();
                    }
                }
                MouseEventKind::ScrollUp => {
                    if self.is_inside(mouse.column, mouse.row) {
                        self.select_prev();
                        event.clear();
                    }
                }
                MouseEventKind::ScrollDown => {
                    if self.is_inside(mouse.column, mouse.row) {
                        self.select_next();
                        event.clear();
                    }
                }
                _ => {}
            },
            _ => {}
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

    fn handle_drop(&mut self, payload: Box<dyn Any>) -> bool {
        match payload.downcast::<String>() {
            Ok(item) => {
                self.push_item((*item).clone());
                true
            }
            Err(_) => false,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{SF_FOCUSED, SF_VISIBLE};
    use crossterm::event::{KeyModifiers, MouseEvent};

    fn sample_items() -> Vec<String> {
        vec![
            "Apple".into(),
            "Banana".into(),
            "Cherry".into(),
            "Date".into(),
            "Elderberry".into(),
            "Fig".into(),
            "Grape".into(),
        ]
    }

    #[test]
    fn test_listbox_new() {
        let list = ListBox::new(Rect::new(10, 5, 20, 10));

        assert_eq!(list.bounds(), Rect::new(10, 5, 20, 10));
        assert!(list.items().is_empty());
        assert_eq!(list.selected(), 0);
        assert_eq!(list.scroll(), 0);
        assert!(list.can_focus());
        assert_ne!(list.options() & OF_SELECTABLE, 0);
    }

    #[test]
    fn test_listbox_with_items() {
        let items = sample_items();
        let list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(items.clone());

        assert_eq!(list.items().len(), 7);
        assert_eq!(list.items()[0], "Apple");
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_with_selected() {
        let items = sample_items();
        let list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(items)
            .with_selected(3);

        assert_eq!(list.selected(), 3);
        assert_eq!(list.selected_text(), Some("Date"));
    }

    #[test]
    fn test_listbox_selected_clamped_to_empty() {
        let list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_selected(5); // No items, clamped to 0

        assert_eq!(list.selected(), 0);
        assert!(list.items().is_empty());
    }

    #[test]
    fn test_listbox_selected_clamped_to_max() {
        let list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(vec!["A".into(), "B".into()])
            .with_selected(99); // Clamped to 1

        assert_eq!(list.selected(), 1);
    }

    #[test]
    fn test_listbox_select_next() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        assert_eq!(list.selected(), 0);

        list.select_next();
        assert_eq!(list.selected(), 1);

        list.select_next();
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn test_listbox_select_next_at_end() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(6); // Last item

        list.select_next();
        assert_eq!(list.selected(), 6); // Stays at end
    }

    #[test]
    fn test_listbox_select_prev() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(4);

        list.select_prev();
        assert_eq!(list.selected(), 3);

        list.select_prev();
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn test_listbox_select_prev_at_start() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        list.select_prev();
        assert_eq!(list.selected(), 0); // Stays at start
    }

    #[test]
    fn test_listbox_select_prev_triggers_scroll() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3)) // visible 3 lines
            .with_items(sample_items())
            .with_selected(3);

        assert_eq!(list.scroll, 1); // scroll adjusted so selected (3) is visible in 3 lines: scroll = selected - (3-1) = 3-2 = 1
        // Wait, let me recalculate. visible_lines = 3, selected = 3
        // clamp_scroll: selected >= scroll + visible => 3 >= 1 + 3 => 3 >= 4 => false
        // Actually scroll was set in with_selected -> clamp_scroll
        // scroll starts at 0. clamp_scroll runs.
        // visible = 3, max_scroll = 7 - 3 = 4. scroll=0 <= 4 ok.
        // selected=3, scroll=0, visible=3 => 0 + 3 = 3, selected(3) >= 3 => scroll = 3-2 = 1
        // So scroll=1 now.
        assert_eq!(list.scroll, 1);
        assert_eq!(list.selected, 3);

        // Move up: selected goes to 2, which is >= scroll(1) so no scroll change
        list.select_prev();
        assert_eq!(list.selected, 2);
        assert_eq!(list.scroll, 1); // 2 >= 1 (scroll) and 2 < 1+3(4) so fine

        // Move up: selected goes to 1, which is < scroll(1) -> scroll goes to 1
        list.select_prev();
        assert_eq!(list.selected, 1);
        assert_eq!(list.scroll, 1); // 1 >= 1 but 1 < 1+3, so stays

        // Move up: selected goes to 0, which is < scroll(1) -> scroll goes to 0
        list.select_prev();
        assert_eq!(list.selected, 0);
        assert_eq!(list.scroll, 0);
    }

    #[test]
    fn test_listbox_select_next_triggers_scroll() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3)) // visible 3 lines
            .with_items(sample_items())
            .with_selected(0);

        assert_eq!(list.scroll, 0);

        // Move down to index 2 (last visible)
        list.select_next();
        list.select_next();
        assert_eq!(list.selected, 2);
        assert_eq!(list.scroll, 0); // still visible

        // Move down once more — selected=3, scroll should move to 1
        list.select_next();
        assert_eq!(list.selected, 3);
        assert_eq!(list.scroll, 1); // 3 >= 0+3=3, so scroll = 3-(3-1)=1

        // Move down — selected=4, scroll=2
        list.select_next();
        assert_eq!(list.selected, 4);
        assert_eq!(list.scroll, 2);
    }

    #[test]
    fn test_listbox_page_up() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items())
            .with_selected(6);

        // page_size = 3, selected = 6
        // page_up: selected = 6 - 3 = 3
        list.page_up();
        assert_eq!(list.selected, 3);
    }

    #[test]
    fn test_listbox_page_up_at_top() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items())
            .with_selected(2);

        list.page_up();
        assert_eq!(list.selected, 0);
        assert_eq!(list.scroll, 0);
    }

    #[test]
    fn test_listbox_page_down() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items())
            .with_selected(0);

        // page_size = 3, selected = 0
        // page_down: selected = 0 + 3 = 3
        list.page_down();
        assert_eq!(list.selected, 3);
    }

    #[test]
    fn test_listbox_page_down_at_end() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items())
            .with_selected(5);

        list.page_down();
        assert_eq!(list.selected, 6); // Last item (max_idx = 6)
    }

    #[test]
    fn test_listbox_home() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(5);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 0);
        assert_eq!(list.scroll(), 0);
    }

    #[test]
    fn test_listbox_end() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        let key = crossterm::event::KeyEvent::new(
            KeyCode::End,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 6); // Last index
    }

    #[test]
    fn test_listbox_home_empty() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10));

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(!event.is_cleared()); // Not consumed — no items
    }

    #[test]
    fn test_listbox_key_up_event() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(3);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn test_listbox_key_down_event() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(3);

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 4);
    }

    #[test]
    fn test_listbox_mouse_click_selects() {
        let mut list = ListBox::new(Rect::new(10, 5, 20, 10))
            .with_items(sample_items());

        // Click on row 7 (relative to bounds: 2) → index 2
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 7,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 2);
        assert_eq!(list.selected_text(), Some("Cherry"));
    }

    #[test]
    fn test_listbox_mouse_click_outside() {
        let mut list = ListBox::new(Rect::new(10, 5, 20, 10))
            .with_items(sample_items());

        // Click outside bounds
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_mouse_click_beyond_items() {
        let mut list = ListBox::new(Rect::new(10, 5, 20, 10))
            .with_items(sample_items());

        // Click on row 14 (relative: 9) — inside bounds but index 9 is beyond 7 items
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 14,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        // Event consumed (inside bounds) but selection unchanged (no item at index 9)
        assert!(event.is_cleared());
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_scroll_wheel_up() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(3);

        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn test_listbox_scroll_wheel_down() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(3);

        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        assert!(event.is_cleared());
        assert_eq!(list.selected(), 4);
    }

    #[test]
    fn test_listbox_scroll_wheel_outside_bounds() {
        let mut list = ListBox::new(Rect::new(10, 10, 20, 10))
            .with_items(sample_items())
            .with_selected(3);

        // Scroll outside bounds
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse);
        list.handle_event(&mut event);

        assert!(!event.is_cleared()); // Not consumed
        assert_eq!(list.selected(), 3);
    }

    #[test]
    fn test_listbox_empty_list() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10));

        // Key events should not crash on empty list
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_single_item() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(vec!["Only".into()]);

        assert_eq!(list.selected(), 0);

        list.select_next(); // At end, shouldn't move
        assert_eq!(list.selected(), 0);

        list.select_prev(); // At start, shouldn't move
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_selected_text() {
        let list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items())
            .with_selected(2);

        assert_eq!(list.selected_text(), Some("Cherry"));

        let empty_list = ListBox::new(Rect::new(0, 0, 20, 10));
        assert_eq!(empty_list.selected_text(), None);
    }

    #[test]
    fn test_listbox_needs_scrollbar() {
        // More items than visible lines
        let list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items()); // 7 items, 3 visible

        assert!(list.needs_scrollbar());

        // Fewer items than visible lines
        let list2 = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items()); // 7 items, 10 visible

        assert!(!list2.needs_scrollbar());
    }

    #[test]
    fn test_listbox_thumb_position() {
        let list = ListBox::new(Rect::new(0, 0, 20, 5)) // visible 5, track = 3 (5-2)
            .with_items(sample_items()); // 7 items, range = 2 (7-5)

        // scroll=0: 0 * 3 / 2 = 0
        assert_eq!(list.thumb_position(), 0);
    }

    #[test]
    fn test_listbox_set_items_updates_list() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(vec!["A".into(), "B".into()]);

        assert_eq!(list.items().len(), 2);

        list.set_items(vec!["X".into(), "Y".into(), "Z".into()]);
        assert_eq!(list.items().len(), 3);
        assert_eq!(list.items()[2], "Z");
    }

    #[test]
    fn test_listbox_set_selected() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        list.set_selected(5);
        assert_eq!(list.selected(), 5);

        list.set_selected(99); // Clamped to 6
        assert_eq!(list.selected(), 6);
    }

    #[test]
    fn test_listbox_draw() {
        let list = ListBox::new(Rect::new(0, 0, 20, 5))
            .with_items(sample_items());
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        list.draw(&mut buf, Rect::new(0, 0, 30, 10));

        // Items should be drawn
        let content = buf.content();
        let has_apple = content.iter().any(|cell| cell.symbol().contains('A'));
        assert!(has_apple, "ListBox should draw its items");
    }

    #[test]
    fn test_listbox_draw_empty() {
        let list = ListBox::new(Rect::new(0, 0, 20, 5));
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        list.draw(&mut buf, Rect::new(0, 0, 20, 5));
        // Should not panic
    }

    #[test]
    fn test_listbox_draw_clipped() {
        let list = ListBox::new(Rect::new(0, 0, 20, 5))
            .with_items(sample_items());
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        // Draw with clip that only covers half the list
        list.draw(&mut buf, Rect::new(0, 0, 20, 2));
        // Should not panic
    }

    #[test]
    fn test_listbox_state() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10));

        // Initial state should have SF_VISIBLE
        assert_ne!(list.state() & SF_VISIBLE, 0);
        assert_eq!(list.state() & SF_FOCUSED, 0);

        // Set focused
        list.set_state(list.state() | SF_FOCUSED);
        assert_ne!(list.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_listbox_bounds() {
        let mut list = ListBox::new(Rect::new(5, 3, 20, 8));
        assert_eq!(list.bounds(), Rect::new(5, 3, 20, 8));

        list.set_bounds(Rect::new(10, 10, 30, 12));
        assert_eq!(list.bounds(), Rect::new(10, 10, 30, 12));
    }

    #[test]
    fn test_listbox_ignores_non_navigation_keys() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        let key = crossterm::event::KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::empty(),
        );
        let mut event = Event::key(key);
        list.handle_event(&mut event);

        assert!(!event.is_cleared());
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn test_listbox_cleared_event_not_processed() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10));

        let mut event = Event::default();
        event.clear();

        list.handle_event(&mut event);
        // Should not panic
    }

    #[test]
    fn test_listbox_clamp_scroll_on_resize() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items())
            .with_selected(6);

        // With 3 visible lines and 7 items, selected=6 => scroll should be 4 (6-2)
        // max_scroll = 7 - 3 = 4
        assert_eq!(list.scroll, 4);

        // Resize to show 10 lines — scroll should be clamped back
        list.set_bounds(Rect::new(0, 0, 20, 10));
        assert_eq!(list.scroll, 0); // visible lines (10) >= items (7), so scroll=0
    }

    #[test]
    fn test_listbox_handle_drop() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        // Drop a string onto the list
        let payload: Box<dyn Any> = Box::new("Dropped Item".to_string());
        let handled = list.handle_drop(payload);

        assert!(handled);
        assert_eq!(list.items().len(), 8);
        assert_eq!(list.items()[7], "Dropped Item");
    }

    #[test]
    fn test_listbox_handle_drop_wrong_type() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 10))
            .with_items(sample_items());

        // Drop a non-string type
        let payload: Box<dyn Any> = Box::new(42i32);
        let handled = list.handle_drop(payload);

        assert!(!handled);
        assert_eq!(list.items().len(), 7); // unchanged
    }

    #[test]
    fn test_listbox_push_item() {
        let mut list = ListBox::new(Rect::new(0, 0, 20, 3))
            .with_items(sample_items());

        list.push_item("New Item".to_string());
        assert_eq!(list.items().len(), 8);
        assert_eq!(list.items()[7], "New Item");
    }
}
