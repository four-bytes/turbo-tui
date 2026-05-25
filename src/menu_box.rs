//! Menu box — standalone dropdown menu widget.
//!
//! A `MenuBox` can be used independently or by [`MenuBar`] to render
//! a bordered dropdown list with keyboard and mouse navigation.
//!
//! [`MenuBar`]: crate::menu_bar::MenuBar

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

use crate::command::{CommandId, CM_DROPDOWN_CLOSED, CM_DROPDOWN_NAVIGATE};
use crate::menu_bar::MenuItem;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId};

// ============================================================================
// MenuBox
// ============================================================================

/// Standalone dropdown menu box.
///
/// Renders a single-line bordered box with menu items.
/// Navigation wraps around, skipping separators and disabled items.
/// After the user presses Enter or clicks an item, the selected command
/// is stored in `result()`.
///
/// # Example
///
/// ```ignore
/// let items = vec![
///     MenuItem::new("~N~ew", CM_NEW),
///     MenuItem::separator(),
///     MenuItem::new("~Q~uit", CM_QUIT),
/// ];
/// let bounds = MenuBox::calculate_bounds(10, 5, &items);
/// let mut menu = MenuBox::new(bounds, items);
///
/// // In event loop:
/// menu.handle_event(&mut event);
/// if let Some(cmd) = menu.result() {
///     // User selected `cmd`
/// }
/// ```
pub struct MenuBox {
    /// Common view state.
    base: ViewBase,
    /// The items to display.
    items: Vec<MenuItem>,
    /// Currently highlighted item index.
    selected: Option<usize>,
    /// Command selected by the user (`None` = nothing selected yet).
    result: Option<CommandId>,
    /// If set, this `MenuBox` is owned by a `HorizontalBar` and will emit
    /// commands through the event system instead of just storing `result`.
    owner_bar_id: Option<ViewId>,
    /// Direction for a pending navigate request: -1 (left) or 1 (right).
    /// Set when Left/Right is pressed while owned by a bar.
    navigate_direction: Option<isize>,
}

impl MenuBox {
    /// Create a new `MenuBox` with `bounds` and `items`.
    ///
    /// The first enabled, non-separator item is highlighted by default.
    #[must_use]
    pub fn new(bounds: Rect, items: Vec<MenuItem>) -> Self {
        let selected = items
            .iter()
            .position(|item| !item.is_separator() && item.enabled);
        Self {
            base: ViewBase::new(bounds),
            items,
            selected,
            result: None,
            owner_bar_id: None,
            navigate_direction: None,
        }
    }

    /// Currently highlighted item index.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The command chosen by the user, or `None` if nothing selected yet.
    #[must_use]
    pub fn result(&self) -> Option<CommandId> {
        self.result
    }

    /// Calculate the `Rect` needed to display `items` starting at `(x, y)`.
    ///
    /// Width = longest item label + 4 (borders + padding), minimum 6.
    /// Height = number of items + 2 (top and bottom borders).
    #[must_use]
    pub fn calculate_bounds(x: u16, y: u16, items: &[MenuItem]) -> Rect {
        let max_label = items
            .iter()
            .map(|item| item.display_label().chars().count())
            .max()
            .unwrap_or(0);
        #[allow(clippy::cast_possible_truncation)]
        let width = (max_label as u16).saturating_add(4).max(6);
        #[allow(clippy::cast_possible_truncation)]
        let height = items.len() as u16 + 2;
        Rect::new(x, y, width, height)
    }

    /// Set the owning bar's `ViewId`. When set, the `MenuBox` will emit
    /// command events and navigation requests through the event system
    /// instead of only storing the result internally.
    #[must_use]
    pub fn with_owner(mut self, bar_id: ViewId) -> Self {
        self.owner_bar_id = Some(bar_id);
        self
    }

    /// Get the owning bar's `ViewId`, if set.
    #[must_use]
    pub fn owner_bar_id(&self) -> Option<ViewId> {
        self.owner_bar_id
    }

    /// Get the pending navigate direction set when Left/Right was pressed.
    ///
    /// Returns `-1` for Left, `1` for Right, or `None` if no navigation pending.
    #[must_use]
    pub fn navigate_direction(&self) -> Option<isize> {
        self.navigate_direction
    }

    // -----------------------------------------------------------------------
    // Navigation helpers
    // -----------------------------------------------------------------------

    /// Move selection down, wrapping and skipping separators/disabled items.
    fn move_down(&mut self) {
        let current = self.selected.unwrap_or(0);
        let next = (current + 1..self.items.len())
            .find(|&i| self.is_selectable(i))
            .or_else(|| (0..=current).find(|&i| self.is_selectable(i)));
        if next.is_some() {
            self.selected = next;
        }
    }

    /// Move selection up, wrapping and skipping separators/disabled items.
    fn move_up(&mut self) {
        let current = self.selected.unwrap_or(0);
        let prev = (0..current)
            .rev()
            .find(|&i| self.is_selectable(i))
            .or_else(|| {
                (current..self.items.len())
                    .rev()
                    .find(|&i| self.is_selectable(i))
            });
        if prev.is_some() {
            self.selected = prev;
        }
    }

    /// Whether item at `index` can be selected (enabled, not separator).
    fn is_selectable(&self, index: usize) -> bool {
        self.items
            .get(index)
            .is_some_and(|item| !item.is_separator() && item.enabled)
    }

    /// Determine which item row the point `(col, row)` hits.
    ///
    /// Returns the item index, or `None` if the click is on a border.
    fn item_at(&self, col: u16, row: u16) -> Option<usize> {
        let b = self.base.bounds();
        if col <= b.x || col >= b.x + b.width - 1 {
            return None; // Border columns
        }
        if row <= b.y || row >= b.y + b.height - 1 {
            return None; // Border rows
        }
        #[allow(clippy::cast_possible_truncation)]
        Some((row - b.y - 1) as usize)
    }

    /// Commit the currently selected item as the result.
    /// If an event is provided and this `MenuBox` has an owner, also emit the
    /// command through the event system so it propagates through dispatch.
    fn confirm_selection(&mut self, event: Option<&mut Event>) {
        if let Some(idx) = self.selected {
            if self.is_selectable(idx) {
                let cmd = self.items[idx].command;
                self.result = Some(cmd);
                // When owned by a bar, emit the command through the event
                // and close the dropdown
                if self.owner_bar_id.is_some() {
                    if let Some(ev) = event {
                        ev.kind = EventKind::Command(cmd);
                        ev.handled = true;
                        // Post deferred close so the overlay is dismissed
                        ev.post(Event::command(CM_DROPDOWN_CLOSED));
                    }
                }
            }
        }
    }
}

// ============================================================================
// View impl
// ============================================================================

impl View for MenuBox {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    #[allow(clippy::similar_names)]
    fn draw(&self, buf: &mut Buffer, _clip: Rect) {
        let area = self.base.bounds();
        if area.width < 4 || area.height < 3 {
            return;
        }

        let (
            box_style,
            border_style,
            selected_style,
            disabled_style,
            _sep_style,
            hotkey_style,
            hotkey_selected_style,
        ) = theme::with_current(|t| {
            (
                t.menu_box_normal,
                t.menu_box_normal,
                t.menu_box_selected,
                t.menu_box_disabled,
                t.menu_box_separator,
                t.menu_box_hotkey,
                t.menu_box_hotkey_selected,
            )
        });

        let (m_tl, m_tr, m_bl, m_br, m_h, m_v, m_sl, m_sr) = theme::with_current(|t| {
            (
                t.menu_border_tl,
                t.menu_border_tr,
                t.menu_border_bl,
                t.menu_border_br,
                t.menu_border_h,
                t.menu_border_v,
                t.menu_sep_l,
                t.menu_sep_r,
            )
        });

        let s_tl = m_tl.to_string();
        let s_tr = m_tr.to_string();
        let s_bl = m_bl.to_string();
        let s_br = m_br.to_string();
        let s_h = m_h.to_string();
        let s_v = m_v.to_string();
        let s_sl = m_sl.to_string();
        let s_sr = m_sr.to_string();

        // Top border
        buf.set_string(area.x, area.y, &s_tl, border_style);
        for x in 1..area.width - 1 {
            buf.set_string(area.x + x, area.y, &s_h, border_style);
        }
        buf.set_string(area.x + area.width - 1, area.y, &s_tr, border_style);

        // Items
        for (item_idx, item) in self.items.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let row = area.y + 1 + item_idx as u16;
            if row >= area.y + area.height - 1 {
                break;
            }
            let is_selected = self.selected == Some(item_idx);

            if item.is_separator() {
                // Separator line: all characters use border style for uniform appearance
                buf.set_string(area.x, row, &s_sl, border_style);
                for x in 1..area.width - 1 {
                    buf.set_string(area.x + x, row, &s_h, border_style);
                }
                buf.set_string(area.x + area.width - 1, row, &s_sr, border_style);
            } else {
                let (row_style, hk_style) = if is_selected {
                    (selected_style, hotkey_selected_style)
                } else if !item.enabled {
                    (disabled_style, disabled_style)
                } else {
                    (box_style, hotkey_style)
                };

                buf.set_string(area.x, row, &s_v, border_style);
                for x in 1..area.width - 1 {
                    buf.set_string(area.x + x, row, " ", row_style);
                }
                buf.set_string(area.x + area.width - 1, row, &s_v, border_style);

                let mut cur_x = area.x + 1;
                let mut in_marker = false;
                for ch in item.label.chars() {
                    if ch == '~' {
                        in_marker = !in_marker;
                        continue;
                    }
                    let style = if in_marker { hk_style } else { row_style };
                    buf.set_string(cur_x, row, ch.to_string(), style);
                    cur_x += 1;
                    if cur_x >= area.x + area.width - 1 {
                        break;
                    }
                }
            }
        }

        // Bottom border
        let bottom_y = area.y + area.height - 1;
        buf.set_string(area.x, bottom_y, &s_bl, border_style);
        for x in 1..area.width - 1 {
            buf.set_string(area.x + x, bottom_y, &s_h, border_style);
        }
        buf.set_string(area.x + area.width - 1, bottom_y, &s_br, border_style);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match event.kind.clone() {
            EventKind::Key(key) => match key.code {
                KeyCode::Up => {
                    self.move_up();
                    event.clear();
                }
                KeyCode::Down => {
                    self.move_down();
                    event.clear();
                }
                KeyCode::Enter => {
                    self.confirm_selection(Some(event));
                    // Without owner, clear the event; with owner, confirm_selection
                    // sets event.kind to Command and handled=true
                    if self.owner_bar_id.is_none() {
                        event.clear();
                    }
                }
                KeyCode::Esc => {
                    // Escape — close without selection (result stays None)
                    self.result = None;
                    event.clear();
                }
                // Left/Right navigate to adjacent dropdown (only when owned by bar)
                KeyCode::Left if self.owner_bar_id.is_some() => {
                    self.navigate_direction = Some(-1);
                    event.post(Event::command(CM_DROPDOWN_NAVIGATE));
                    event.clear();
                }
                KeyCode::Right if self.owner_bar_id.is_some() => {
                    self.navigate_direction = Some(1);
                    event.post(Event::command(CM_DROPDOWN_NAVIGATE));
                    event.clear();
                }
                _ => {}
            },

            EventKind::Mouse(mouse) => {
                let col = mouse.column;
                let row = mouse.row;

                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(item_idx) = self.item_at(col, row) {
                            if self.is_selectable(item_idx) {
                                self.selected = Some(item_idx);
                                self.confirm_selection(Some(event));
                                // Without owner, clear the event
                                if self.owner_bar_id.is_none() {
                                    event.clear();
                                }
                            }
                        }
                    }
                    MouseEventKind::Moved => {
                        if let Some(item_idx) = self.item_at(col, row) {
                            if self.is_selectable(item_idx) {
                                self.selected = Some(item_idx);
                                event.clear();
                            }
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
    }

    fn can_focus(&self) -> bool {
        true
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
    use crate::command::{CM_NEW, CM_OPEN, CM_QUIT, CM_SAVE};
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent,
    };

    fn make_items() -> Vec<MenuItem> {
        vec![
            MenuItem::new("~N~ew", CM_NEW),
            MenuItem::new("~O~pen  F3", CM_OPEN),
            MenuItem::separator(),
            MenuItem::new("~S~ave  F2", CM_SAVE),
            MenuItem::disabled("~D~isabled", 999),
            MenuItem::new("~Q~uit", CM_QUIT),
        ]
    }

    fn make_key_event(code: KeyCode) -> Event {
        Event::key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn make_mouse_down(col: u16, row: u16) -> Event {
        Event::mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    // -----------------------------------------------------------------------
    // test_menu_box_calculate_bounds
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_calculate_bounds() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(5, 3, &items);

        assert_eq!(bounds.x, 5);
        assert_eq!(bounds.y, 3);

        // Longest label: "Open  F3" = 8 chars → width = 8 + 4 = 12
        // (strip_hotkey_markers of "~O~pen  F3" = "Open  F3" = 8 chars)
        assert_eq!(bounds.width, 12);

        // 6 items + 2 borders = 8
        assert_eq!(bounds.height, 8);
    }

    // -----------------------------------------------------------------------
    // test_menu_box_calculate_bounds_minimum_width
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_calculate_bounds_minimum_width() {
        let items = vec![MenuItem::separator()];
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        // Minimum width = 6
        assert_eq!(bounds.width, 6);
    }

    // -----------------------------------------------------------------------
    // test_menu_box_navigate
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_navigate() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        // Initial selection: first enabled item (index 0 = CM_NEW)
        assert_eq!(menu.selected(), Some(0));

        // Down → index 1 (Open)
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event);
        assert_eq!(menu.selected(), Some(1));
        assert!(event.is_cleared());

        // Down → skip separator at index 2 → index 3 (Save)
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event);
        assert_eq!(menu.selected(), Some(3));
        assert!(event.is_cleared());

        // Down → skip disabled at index 4 → index 5 (Quit)
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event);
        assert_eq!(menu.selected(), Some(5));
        assert!(event.is_cleared());

        // Down → wrap to index 0 (New)
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event);
        assert_eq!(menu.selected(), Some(0));
        assert!(event.is_cleared());

        // Up → wrap back to index 5 (Quit)
        let mut event = make_key_event(KeyCode::Up);
        menu.handle_event(&mut event);
        assert_eq!(menu.selected(), Some(5));
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_box_select
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_select() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        // Nothing selected yet
        assert_eq!(menu.result(), None);

        // Navigate to CM_SAVE (index 3)
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event); // → 1 Open
        let mut event = make_key_event(KeyCode::Down);
        menu.handle_event(&mut event); // → 3 Save (skip sep at 2)

        // Enter → commit
        let mut event = make_key_event(KeyCode::Enter);
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), Some(CM_SAVE));
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_box_draw
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_draw() {
        let items = vec![MenuItem::new("New", CM_NEW), MenuItem::new("Open", CM_OPEN)];
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut buf = Buffer::empty(bounds);
        let menu = MenuBox::new(bounds, items);
        menu.draw(&mut buf, bounds);

        // Top-left corner
        assert_eq!(buf[(0, 0)].symbol(), "┌");
        // Bottom-left corner
        assert_eq!(buf[(0, bounds.height - 1)].symbol(), "└");
        // Top-right corner
        assert_eq!(buf[(bounds.width - 1, 0)].symbol(), "┐");

        // First item "New" starts at column 1, row 1
        assert_eq!(buf[(1, 1)].symbol(), "N");
        assert_eq!(buf[(2, 1)].symbol(), "e");
        assert_eq!(buf[(3, 1)].symbol(), "w");

        // Second item "Open" at row 2
        assert_eq!(buf[(1, 2)].symbol(), "O");
        assert_eq!(buf[(2, 2)].symbol(), "p");
        assert_eq!(buf[(3, 2)].symbol(), "e");
        assert_eq!(buf[(4, 2)].symbol(), "n");
    }

    // -----------------------------------------------------------------------
    // test_menu_box_mouse_click_selects
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_mouse_click_selects() {
        let items = vec![
            MenuItem::new("New", CM_NEW),
            MenuItem::new("Open", CM_OPEN),
            MenuItem::new("Save", CM_SAVE),
        ];
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        // Click on row 2 (second item "Open", 0-indexed: row=2 is inside border)
        let mut event = make_mouse_down(2, 2);
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), Some(CM_OPEN));
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_box_escape_no_result
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_escape_no_result() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        let mut event = make_key_event(KeyCode::Esc);
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), None);
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_box_disabled_item_not_selectable
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_disabled_item_not_selectable() {
        let items = vec![
            MenuItem::new("Enabled", CM_NEW),
            MenuItem::disabled("Disabled", CM_SAVE),
        ];
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let menu = MenuBox::new(bounds, items);

        // Initial selection should be 0 (Enabled), not 1 (Disabled)
        assert_eq!(menu.selected(), Some(0));
    }

    // -----------------------------------------------------------------------
    // test_menu_box_emits_command_on_enter_with_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_emits_command_on_enter_with_owner() {
        use crate::view::ViewBase;
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let owner_id = ViewBase::new(Rect::new(0, 0, 80, 1)).id();
        let mut menu = MenuBox::new(bounds, items).with_owner(owner_id);

        // Initial selection: index 0 = CM_NEW
        assert_eq!(menu.selected(), Some(0));

        // Enter should emit CM_NEW as command event
        let mut event = make_key_event(KeyCode::Enter);
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), Some(CM_NEW));
        assert!(matches!(event.kind, EventKind::Command(cmd) if cmd == CM_NEW));
    }

    // -----------------------------------------------------------------------
    // test_menu_box_no_command_emission_without_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_no_command_emission_without_owner() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        // Without owner, Enter should store result but clear event (not emit command)
        let mut event = make_key_event(KeyCode::Enter);
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), Some(CM_NEW));
        // Event should be cleared, not a command
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_box_left_right_posts_navigate_with_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_left_right_posts_navigate_with_owner() {
        use crate::view::ViewBase;
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let owner_id = ViewBase::new(Rect::new(0, 0, 80, 1)).id();
        let mut menu = MenuBox::new(bounds, items).with_owner(owner_id);

        // Left arrow should post CM_DROPDOWN_NAVIGATE as deferred event
        let mut event = make_key_event(KeyCode::Left);
        menu.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(event.deferred.len(), 1);
        assert!(matches!(
            event.deferred[0].kind,
            EventKind::Command(cmd) if cmd == CM_DROPDOWN_NAVIGATE
        ));

        // Right arrow should also post CM_DROPDOWN_NAVIGATE
        let mut event = make_key_event(KeyCode::Right);
        menu.handle_event(&mut event);
        assert!(event.is_cleared());
        assert_eq!(event.deferred.len(), 1);
    }

    // -----------------------------------------------------------------------
    // test_menu_box_navigate_direction_set_on_left_right
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_navigate_direction_set_on_left_right() {
        use crate::view::ViewBase;
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let owner_id = ViewBase::new(Rect::new(0, 0, 80, 1)).id();
        let mut menu = MenuBox::new(bounds, items).with_owner(owner_id);

        // Initially no direction pending
        assert_eq!(menu.navigate_direction(), None);

        // Left sets -1
        let mut event = make_key_event(KeyCode::Left);
        menu.handle_event(&mut event);
        assert_eq!(menu.navigate_direction(), Some(-1));

        // Right sets +1
        let mut event = make_key_event(KeyCode::Right);
        menu.handle_event(&mut event);
        assert_eq!(menu.navigate_direction(), Some(1));
    }

    // -----------------------------------------------------------------------
    // test_menu_box_left_right_ignored_without_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_left_right_ignored_without_owner() {
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let mut menu = MenuBox::new(bounds, items);

        // Without owner, Left/Right should NOT be handled
        let mut event = make_key_event(KeyCode::Left);
        menu.handle_event(&mut event);
        assert!(
            !event.is_cleared(),
            "Left should not be handled without owner"
        );
    }

    // -----------------------------------------------------------------------
    // test_menu_box_mouse_click_emits_command_with_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_box_mouse_click_emits_command_with_owner() {
        use crate::view::ViewBase;
        let items = make_items();
        let bounds = MenuBox::calculate_bounds(0, 0, &items);
        let owner_id = ViewBase::new(Rect::new(0, 0, 80, 1)).id();
        let mut menu = MenuBox::new(bounds, items).with_owner(owner_id);

        // Click on first item (row = bounds.y + 1, col inside)
        let mut event = make_mouse_down(1, 1); // inside first item
        menu.handle_event(&mut event);
        assert_eq!(menu.result(), Some(CM_NEW));
        assert!(matches!(event.kind, EventKind::Command(cmd) if cmd == CM_NEW));
    }
}
