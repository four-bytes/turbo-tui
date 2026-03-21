//! Menu bar — horizontal top bar with dropdown activation.
//!
//! Implements a classic Borland Turbo Vision-style menu bar with:
//! - Horizontal top bar showing menu names
//! - Dropdown boxes on activation
//! - Keyboard navigation (F10, Alt+hotkey, arrows, Enter, Escape)
//! - Mouse click support

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

use crate::command::CommandId;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, OF_PRE_PROCESS};

// ============================================================================
// MenuItem
// ============================================================================

/// A single menu item in a dropdown.
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// Display text with `~X~` hotkey markers (e.g., `"~O~pen  Ctrl+O"`).
    pub label: String,
    /// Command to emit when selected (`0` = separator).
    pub command: CommandId,
    /// Whether this item is enabled.
    pub enabled: bool,
    /// Optional submenu (for cascading menus).
    pub submenu: Option<Vec<MenuItem>>,
}

impl MenuItem {
    /// Create a new enabled menu item.
    #[must_use]
    pub fn new(label: &str, command: CommandId) -> Self {
        Self {
            label: label.to_owned(),
            command,
            enabled: true,
            submenu: None,
        }
    }

    /// Create a separator item (command = 0, disabled).
    #[must_use]
    pub fn separator() -> Self {
        Self {
            label: "─".to_owned(),
            command: 0,
            enabled: false,
            submenu: None,
        }
    }

    /// Create a disabled menu item.
    #[must_use]
    pub fn disabled(label: &str, command: CommandId) -> Self {
        Self {
            label: label.to_owned(),
            command,
            enabled: false,
            submenu: None,
        }
    }

    /// Returns `true` if this item is a separator (command == 0 and disabled).
    #[must_use]
    pub fn is_separator(&self) -> bool {
        self.command == 0 && !self.enabled
    }

    /// Extract the hotkey letter from `~X~` marker in the label.
    ///
    /// Returns the letter as a lowercase `char`, or `None` if no marker found.
    #[must_use]
    pub fn hotkey(&self) -> Option<char> {
        extract_hotkey(&self.label)
    }

    /// Return the display label with `~X~` markers stripped.
    #[must_use]
    pub fn display_label(&self) -> String {
        strip_hotkey_markers(&self.label)
    }
}

// ============================================================================
// Menu
// ============================================================================

/// A top-level menu entry shown in the menu bar.
#[derive(Debug, Clone)]
pub struct Menu {
    /// Display name with `~X~` hotkey marker (e.g., `"~F~ile"`).
    pub name: String,
    /// Items in the dropdown.
    pub items: Vec<MenuItem>,
}

impl Menu {
    /// Create a new top-level menu.
    #[must_use]
    pub fn new(name: &str, items: Vec<MenuItem>) -> Self {
        Self {
            name: name.to_owned(),
            items,
        }
    }

    /// Extract the hotkey letter from the menu name's `~X~` marker.
    #[must_use]
    pub fn hotkey(&self) -> Option<char> {
        extract_hotkey(&self.name)
    }

    /// Return the display name with `~X~` markers stripped.
    #[must_use]
    pub fn display_name(&self) -> String {
        strip_hotkey_markers(&self.name)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract a hotkey letter from a `~X~` marker in `text`.
fn extract_hotkey(text: &str) -> Option<char> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'~' {
            // Look for closing ~
            if let Some(close) = bytes[i + 1..].iter().position(|&b| b == b'~') {
                let inner = &text[i + 1..i + 1 + close];
                if let Some(ch) = inner.chars().next() {
                    return Some(ch.to_ascii_lowercase());
                }
            }
        }
        i += 1;
    }
    None
}

/// Strip all `~X~` marker tildes from `text`, keeping the letter itself.
fn strip_hotkey_markers(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '~' {
            // Collect until next ~
            let mut found_close = false;
            for inner in chars.by_ref() {
                if inner == '~' {
                    found_close = true;
                    break;
                }
                result.push(inner);
            }
            if !found_close {
                // Unclosed tilde — push it literally
                result.push('~');
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Calculate the visual width of a menu item label (without `~` tilde chars).
fn label_display_width(label: &str) -> usize {
    strip_hotkey_markers(label).chars().count()
}

// ============================================================================
// MenuBar
// ============================================================================

/// Horizontal menu bar with dropdown activation.
///
/// Renders a full-width top bar with menu names. When a menu is opened
/// (via F10, Alt+hotkey, or mouse click) the dropdown is drawn directly
/// below the menu name.
pub struct MenuBar {
    /// Common view state.
    base: ViewBase,
    /// Top-level menus.
    menus: Vec<Menu>,
    /// X start position for each menu name in the bar.
    menu_positions: Vec<u16>,
    /// Which menu is currently open (`None` = all closed).
    active_menu: Option<usize>,
    /// Which item in the open dropdown is highlighted.
    selected_item: Option<usize>,
}

impl MenuBar {
    /// Create a new `MenuBar`.
    ///
    /// The `bounds` should span the full width of the screen and have height 1.
    #[must_use]
    pub fn new(bounds: Rect, menus: Vec<Menu>) -> Self {
        let menu_positions = Self::compute_positions(&menus);
        Self {
            base: ViewBase::with_options(bounds, OF_PRE_PROCESS),
            menus,
            menu_positions,
            active_menu: None,
            selected_item: None,
        }
    }

    /// Compute the X start position of each menu name in the bar.
    ///
    /// Format: ` Name  Name  Name ` (leading space, two spaces between).
    fn compute_positions(menus: &[Menu]) -> Vec<u16> {
        let mut positions = Vec::with_capacity(menus.len());
        let mut x: u16 = 1; // Start after one leading space
        for menu in menus {
            positions.push(x);
            // Each menu name takes its display width + 2 spaces padding
            #[allow(clippy::cast_possible_truncation)]
            let width = menu.display_name().chars().count() as u16 + 2;
            x += width;
        }
        positions
    }

    /// Return a slice of all menus.
    #[must_use]
    pub fn menus(&self) -> &[Menu] {
        &self.menus
    }

    /// Return a slice of the computed menu X positions.
    #[must_use]
    pub fn menu_positions(&self) -> &[u16] {
        &self.menu_positions
    }

    /// Whether any menu dropdown is currently open.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active_menu.is_some()
    }

    /// Index of the currently open menu, if any.
    #[must_use]
    pub fn active_menu(&self) -> Option<usize> {
        self.active_menu
    }

    /// Close all menus.
    pub fn close(&mut self) {
        self.active_menu = None;
        self.selected_item = None;
    }

    /// Open the menu at `index`, resetting item selection.
    fn open_menu(&mut self, index: usize) {
        if index < self.menus.len() {
            self.active_menu = Some(index);
            self.selected_item = self.first_selectable_item(index);
        }
    }

    /// Find the first selectable (non-separator, enabled) item in a menu.
    fn first_selectable_item(&self, menu_idx: usize) -> Option<usize> {
        self.menus.get(menu_idx).and_then(|menu| {
            menu.items
                .iter()
                .position(|item| !item.is_separator() && item.enabled)
        })
    }

    /// Move item selection down, skipping separators/disabled items.
    fn move_down(&mut self) {
        let Some(menu_idx) = self.active_menu else {
            return;
        };
        let Some(menu) = self.menus.get(menu_idx) else {
            return;
        };
        let items = &menu.items;
        let current = self.selected_item.unwrap_or(0);
        let next = (current + 1..items.len())
            .find(|&i| !items[i].is_separator() && items[i].enabled)
            .or_else(|| {
                // Wrap around
                (0..=current).find(|&i| !items[i].is_separator() && items[i].enabled)
            });
        if next.is_some() {
            self.selected_item = next;
        }
    }

    /// Move item selection up, skipping separators/disabled items.
    fn move_up(&mut self) {
        let Some(menu_idx) = self.active_menu else {
            return;
        };
        let Some(menu) = self.menus.get(menu_idx) else {
            return;
        };
        let items = &menu.items;
        let current = self.selected_item.unwrap_or(0);
        let prev = (0..current)
            .rev()
            .find(|&i| !items[i].is_separator() && items[i].enabled)
            .or_else(|| {
                // Wrap around
                (current..items.len())
                    .rev()
                    .find(|&i| !items[i].is_separator() && items[i].enabled)
            });
        if prev.is_some() {
            self.selected_item = prev;
        }
    }

    /// Switch to the menu `delta` steps to the left/right.
    fn move_menu(&mut self, delta: isize) {
        #[allow(clippy::cast_possible_wrap)]
        let count = self.menus.len() as isize;
        if count == 0 {
            return;
        }
        #[allow(clippy::cast_possible_wrap)]
        let current = self.active_menu.unwrap_or(0) as isize;
        #[allow(clippy::cast_sign_loss)]
        let next = ((current + delta).rem_euclid(count)) as usize;
        self.open_menu(next);
    }

    /// Return the command for the currently selected item, if any.
    fn selected_command(&self) -> Option<CommandId> {
        let menu_idx = self.active_menu?;
        let item_idx = self.selected_item?;
        let menu = self.menus.get(menu_idx)?;
        let item = menu.items.get(item_idx)?;
        if item.enabled && !item.is_separator() {
            Some(item.command)
        } else {
            None
        }
    }

    /// Determine which menu (if any) the bar column `x` belongs to.
    fn menu_at_column(&self, x: u16) -> Option<usize> {
        for (idx, (&pos, menu)) in self
            .menu_positions
            .iter()
            .zip(self.menus.iter())
            .enumerate()
        {
            #[allow(clippy::cast_possible_truncation)]
            let name_width = menu.display_name().chars().count() as u16;
            // The clickable area spans pos..pos+name_width
            if x >= pos && x < pos + name_width {
                return Some(idx);
            }
        }
        None
    }

    /// Determine which dropdown item the absolute row/col hits.
    ///
    /// Returns `Some(item_index)` when the click is inside the dropdown box.
    fn item_at_position(&self, col: u16, row: u16) -> Option<usize> {
        let menu_idx = self.active_menu?;
        let menu = self.menus.get(menu_idx)?;
        let bar_bounds = self.base.bounds();
        let drop_x = bar_bounds.x + self.menu_positions[menu_idx];
        let drop_y = bar_bounds.y + 1; // Row below the bar

        let drop_width = Self::dropdown_width(menu);
        #[allow(clippy::cast_possible_truncation)]
        let drop_height = menu.items.len() as u16 + 2; // +2 for border

        if col < drop_x || col >= drop_x + drop_width {
            return None;
        }
        if row < drop_y || row >= drop_y + drop_height {
            return None;
        }

        // Border rows return None
        if row == drop_y || row == drop_y + drop_height - 1 {
            return None;
        }
        // Row inside the border (border rows are 0 and last)
        let inner_row = row - drop_y - 1;
        Some(inner_row as usize)
    }

    /// Compute the width of the dropdown box for a menu.
    fn dropdown_width(menu: &Menu) -> u16 {
        let max_label = menu
            .items
            .iter()
            .map(|item| label_display_width(&item.label))
            .max()
            .unwrap_or(0);
        // border (2) + space (2) + label + right padding (1)
        #[allow(clippy::cast_possible_truncation)]
        let w = (max_label as u16).saturating_add(4);
        w.max(6)
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    /// Draw the menu bar row.
    fn draw_bar(&self, buf: &mut Buffer, area: Rect) {
        let (bar_style, active_style, hotkey_style, active_hotkey_style) =
            theme::with_current(|t| {
                (
                    t.menu_bar_normal,
                    t.menu_bar_selected,
                    t.menu_bar_hotkey,
                    t.menu_bar_hotkey_selected,
                )
            });

        // Fill background
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bar_style);
        }

        for (idx, (menu, &pos)) in self
            .menus
            .iter()
            .zip(self.menu_positions.iter())
            .enumerate()
        {
            let is_active = self.active_menu == Some(idx);
            let base_style = if is_active { active_style } else { bar_style };
            let hk_style = if is_active {
                active_hotkey_style
            } else {
                hotkey_style
            };

            let draw_x = area.x + pos;
            // Leading space before the name
            buf.set_string(draw_x.saturating_sub(0), area.y, " ", base_style);

            // Draw each character of the name, applying hotkey style to the
            // character between `~` markers.
            let mut cur_x = draw_x;
            let name = &menu.name;
            let mut in_marker = false;
            for ch in name.chars() {
                if ch == '~' {
                    in_marker = !in_marker;
                    continue;
                }
                let style = if in_marker { hk_style } else { base_style };
                buf.set_string(cur_x, area.y, ch.to_string(), style);
                cur_x += 1;
            }

            // Trailing space after the name
            buf.set_string(cur_x, area.y, " ", base_style);
        }
    }

    /// Draw the open dropdown box below the active menu.
    fn draw_dropdown(&self, buf: &mut Buffer, bar_area: Rect) {
        let Some(menu_idx) = self.active_menu else {
            return;
        };
        let Some(menu) = self.menus.get(menu_idx) else {
            return;
        };
        let Some(&bar_pos) = self.menu_positions.get(menu_idx) else {
            return;
        };

        let drop_x = bar_area.x + bar_pos;
        let drop_y = bar_area.y + 1;
        let drop_width = Self::dropdown_width(menu);
        #[allow(clippy::cast_possible_truncation)]
        let drop_height = menu.items.len() as u16 + 2;

        let (
            box_style,
            border_style,
            selected_style,
            disabled_style,
            sep_style,
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

        // Top border
        buf.set_string(drop_x, drop_y, "┌", border_style);
        for x in 1..drop_width - 1 {
            buf.set_string(drop_x + x, drop_y, "─", border_style);
        }
        buf.set_string(drop_x + drop_width - 1, drop_y, "┐", border_style);

        // Items
        for (item_idx, item) in menu.items.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let row = drop_y + 1 + item_idx as u16;
            let is_selected = self.selected_item == Some(item_idx);

            if item.is_separator() {
                // Separator line
                buf.set_string(drop_x, row, "├", sep_style);
                for x in 1..drop_width - 1 {
                    buf.set_string(drop_x + x, row, "─", sep_style);
                }
                buf.set_string(drop_x + drop_width - 1, row, "┤", sep_style);
            } else {
                // Normal item row
                let (row_style, hk_style) = if is_selected {
                    (selected_style, hotkey_selected_style)
                } else if !item.enabled {
                    (disabled_style, disabled_style)
                } else {
                    (box_style, hotkey_style)
                };

                // Left border
                buf.set_string(drop_x, row, "│", border_style);
                // Fill background
                for x in 1..drop_width - 1 {
                    buf.set_string(drop_x + x, row, " ", row_style);
                }
                // Right border
                buf.set_string(drop_x + drop_width - 1, row, "│", border_style);

                // Draw label text
                let mut cur_x = drop_x + 1;
                let mut in_marker = false;
                for ch in item.label.chars() {
                    if ch == '~' {
                        in_marker = !in_marker;
                        continue;
                    }
                    let style = if in_marker { hk_style } else { row_style };
                    buf.set_string(cur_x, row, ch.to_string(), style);
                    cur_x += 1;
                    if cur_x >= drop_x + drop_width - 1 {
                        break;
                    }
                }
            }
        }

        // Bottom border
        let bottom_y = drop_y + drop_height - 1;
        buf.set_string(drop_x, bottom_y, "└", border_style);
        for x in 1..drop_width - 1 {
            buf.set_string(drop_x + x, bottom_y, "─", border_style);
        }
        buf.set_string(drop_x + drop_width - 1, bottom_y, "┘", border_style);
    }
}

// ============================================================================
// View impl
// ============================================================================

impl View for MenuBar {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, _area: Rect) {
        let bounds = self.base.bounds();
        if bounds.height == 0 {
            return;
        }
        self.draw_bar(buf, bounds);
        if self.active_menu.is_some() {
            self.draw_dropdown(buf, bounds);
        }
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match event.kind.clone() {
            EventKind::Key(key) => self.handle_key(key, event),
            EventKind::Mouse(mouse) => self.handle_mouse(mouse, event),
            _ => {}
        }
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

impl MenuBar {
    /// Check whether item at `item_idx` within `menu_idx` is selectable.
    fn is_item_selectable(&self, menu_idx: usize, item_idx: usize) -> bool {
        self.menus
            .get(menu_idx)
            .and_then(|menu| menu.items.get(item_idx))
            .is_some_and(|item| item.enabled && !item.is_separator())
    }

    /// Handle a keyboard event.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent, event: &mut Event) {
        match key.code {
            // F10 — toggle menu bar (open first menu or close)
            KeyCode::F(10) => {
                if self.is_active() {
                    self.close();
                } else {
                    self.open_menu(0);
                }
                event.clear();
            }

            // Escape — close active menu
            KeyCode::Esc if self.is_active() => {
                self.close();
                event.clear();
            }

            // Arrow navigation (only when active)
            KeyCode::Left if self.is_active() => {
                self.move_menu(-1);
                event.clear();
            }
            KeyCode::Right if self.is_active() => {
                self.move_menu(1);
                event.clear();
            }
            KeyCode::Up if self.is_active() => {
                self.move_up();
                event.clear();
            }
            KeyCode::Down if self.is_active() => {
                self.move_down();
                event.clear();
            }

            // Enter — select highlighted item
            KeyCode::Enter if self.is_active() => {
                if let Some(cmd) = self.selected_command() {
                    self.close();
                    event.kind = EventKind::Command(cmd);
                    event.handled = true;
                }
            }

            // Alt+letter — open matching menu
            KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::ALT) => {
                let ch_lower = ch.to_ascii_lowercase();
                let idx = self.menus.iter().position(|m| m.hotkey() == Some(ch_lower));
                if let Some(menu_idx) = idx {
                    self.open_menu(menu_idx);
                    event.clear();
                }
            }

            _ => {}
        }
    }

    /// Handle a mouse event.
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent, event: &mut Event) {
        let bar_bounds = self.base.bounds();
        let col = mouse.column;
        let row = mouse.row;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if row == bar_bounds.y {
                    // Click on the bar row
                    if let Some(idx) = self.menu_at_column(col.saturating_sub(bar_bounds.x)) {
                        if self.active_menu == Some(idx) {
                            self.close();
                        } else {
                            self.open_menu(idx);
                        }
                        event.clear();
                    }
                } else if self.is_active() {
                    // Click in the dropdown
                    if let Some(item_idx) = self.item_at_position(col, row) {
                        let cmd = self
                            .active_menu
                            .and_then(|m| self.menus.get(m))
                            .and_then(|menu| menu.items.get(item_idx))
                            .filter(|item| item.enabled && !item.is_separator())
                            .map(|item| item.command);
                        if let Some(cmd) = cmd {
                            self.close();
                            event.kind = EventKind::Command(cmd);
                            event.handled = true;
                        }
                    } else {
                        // Click outside — close
                        self.close();
                        event.clear();
                    }
                }
            }

            MouseEventKind::Moved => {
                if self.is_active() {
                    // Hover over dropdown items — update selection
                    if let Some(item_idx) = self.item_at_position(col, row) {
                        if self
                            .active_menu
                            .is_some_and(|m| self.is_item_selectable(m, item_idx))
                        {
                            self.selected_item = Some(item_idx);
                        }
                    }
                    // Always consume mouse move when menu is active
                    event.clear();
                }
            }

            _ => {
                // When menu is active, consume ALL mouse events (Up, Drag, Scroll, etc.)
                // to prevent them from reaching windows underneath.
                if self.is_active() {
                    event.clear();
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_NEW, CM_OPEN, CM_QUIT, CM_SAVE};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_file_menu() -> Menu {
        Menu::new(
            "~F~ile",
            vec![
                MenuItem::new("~N~ew", CM_NEW),
                MenuItem::new("~O~pen  F3", CM_OPEN),
                MenuItem::separator(),
                MenuItem::new("~S~ave  F2", CM_SAVE),
                MenuItem::new("~Q~uit", CM_QUIT),
            ],
        )
    }

    fn make_edit_menu() -> Menu {
        Menu::new(
            "~E~dit",
            vec![
                MenuItem::new("~U~ndo  Ctrl+Z", crate::command::CM_UNDO),
                MenuItem::new("~R~edo  Ctrl+Y", crate::command::CM_REDO),
            ],
        )
    }

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn make_bar() -> MenuBar {
        let area = Rect::new(0, 0, 80, 1);
        MenuBar::new(area, vec![make_file_menu(), make_edit_menu()])
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_new
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_new() {
        let bar = make_bar();
        assert_eq!(bar.menus().len(), 2);
        assert_eq!(bar.menus()[0].name, "~F~ile");
        assert_eq!(bar.menus()[1].name, "~E~dit");

        // Positions: first menu at x=1
        let positions = bar.menu_positions();
        assert_eq!(positions[0], 1);
        // "File" display = "File" (4 chars) + 2 spaces = 6 → second menu at 1+6=7
        assert_eq!(positions[1], 7);
    }

    // -----------------------------------------------------------------------
    // test_menu_item_separator
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_item_separator() {
        let sep = MenuItem::separator();
        assert_eq!(sep.command, 0);
        assert!(!sep.enabled);
        assert!(sep.is_separator());
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_open_close
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_open_close() {
        let mut bar = make_bar();

        assert!(!bar.is_active());
        assert_eq!(bar.active_menu(), None);

        bar.open_menu(0);
        assert!(bar.is_active());
        assert_eq!(bar.active_menu(), Some(0));

        bar.close();
        assert!(!bar.is_active());
        assert_eq!(bar.active_menu(), None);
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_draw
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_draw() {
        let area = Rect::new(0, 0, 80, 1);
        let bar = MenuBar::new(area, vec![make_file_menu(), make_edit_menu()]);
        let mut buf = Buffer::empty(area);
        bar.draw(&mut buf, area);

        // Menu names should be visible in the bar
        // "File" starts at position 1
        assert_eq!(buf[(1, 0)].symbol(), "F");
        assert_eq!(buf[(2, 0)].symbol(), "i");
        assert_eq!(buf[(3, 0)].symbol(), "l");
        assert_eq!(buf[(4, 0)].symbol(), "e");

        // "Edit" starts at position 7
        assert_eq!(buf[(7, 0)].symbol(), "E");
        assert_eq!(buf[(8, 0)].symbol(), "d");
        assert_eq!(buf[(9, 0)].symbol(), "i");
        assert_eq!(buf[(10, 0)].symbol(), "t");
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_f10_toggles
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_f10_toggles() {
        let mut bar = make_bar();

        // F10 opens first menu
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_menu(), Some(0));
        assert!(event.is_cleared());

        // F10 again closes
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_escape_closes
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_escape_closes() {
        let mut bar = make_bar();
        bar.open_menu(0);
        assert!(bar.is_active());

        let mut event = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_enter_selects
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_enter_selects() {
        let mut bar = make_bar();
        bar.open_menu(0);
        // selected_item should be the first enabled item (index 0 = CM_NEW)
        assert_eq!(bar.selected_item, Some(0));

        let mut event = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        bar.handle_event(&mut event);

        // Menu should be closed
        assert!(!bar.is_active());
        // Event should contain the command
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_NEW));
    }

    // -----------------------------------------------------------------------
    // test_menu_bar_arrow_navigation
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_arrow_navigation() {
        let mut bar = make_bar();
        bar.open_menu(0);

        // Right → switch to menu 1 (Edit)
        let mut event = make_key_event(KeyCode::Right, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_eq!(bar.active_menu(), Some(1));
        assert!(event.is_cleared());

        // Left → back to menu 0 (File)
        let mut event = make_key_event(KeyCode::Left, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_eq!(bar.active_menu(), Some(0));
        assert!(event.is_cleared());

        // Down → move to next item (from index 0 to index 1)
        let initial_item = bar.selected_item;
        let mut event = make_key_event(KeyCode::Down, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_ne!(bar.selected_item, initial_item, "Down should advance item");
        assert!(event.is_cleared());

        // Up → move back
        let after_down = bar.selected_item;
        let mut event = make_key_event(KeyCode::Up, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        // Should have moved up (back toward initial or wrapped)
        assert_ne!(bar.selected_item, after_down, "Up should retreat item");
        assert!(event.is_cleared());
    }

    // -----------------------------------------------------------------------
    // Helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_hotkey() {
        assert_eq!(extract_hotkey("~F~ile"), Some('f'));
        assert_eq!(extract_hotkey("~E~dit"), Some('e'));
        assert_eq!(extract_hotkey("No hotkey"), None);
        assert_eq!(extract_hotkey("~O~pen  F3"), Some('o'));
    }

    #[test]
    fn test_strip_hotkey_markers() {
        assert_eq!(strip_hotkey_markers("~F~ile"), "File");
        assert_eq!(strip_hotkey_markers("~E~dit"), "Edit");
        assert_eq!(strip_hotkey_markers("~O~pen  F3"), "Open  F3");
        assert_eq!(strip_hotkey_markers("No markers"), "No markers");
    }

    #[test]
    fn test_menu_display_name() {
        let menu = make_file_menu();
        assert_eq!(menu.display_name(), "File");
        assert_eq!(menu.hotkey(), Some('f'));
    }

    #[test]
    fn test_menu_item_display_label() {
        let item = MenuItem::new("~O~pen  F3", CM_OPEN);
        assert_eq!(item.display_label(), "Open  F3");
        assert_eq!(item.hotkey(), Some('o'));
    }
}
