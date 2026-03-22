//! Menu bar — backward-compatibility wrapper around [`HorizontalBar`].
//!
//! The full menu bar implementation now lives in [`crate::horizontal_bar`].
//! This module retains the [`MenuItem`] and [`Menu`] types (used by
//! `horizontal_bar.rs`) and re-exports the relevant types for existing
//! consumers.
//!
//! [`HorizontalBar`]: crate::horizontal_bar::HorizontalBar

use ratatui::layout::Rect;

use crate::command::CommandId;

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
// Private helpers — delegate to horizontal_bar to avoid duplication
// ============================================================================

fn extract_hotkey(text: &str) -> Option<char> {
    crate::horizontal_bar::extract_hotkey(text)
}

fn strip_hotkey_markers(text: &str) -> String {
    crate::horizontal_bar::strip_hotkey_markers(text)
}

// ============================================================================
// Backward-compatibility type alias
// ============================================================================

/// Type alias for backward compatibility.
///
/// `MenuBar` is now implemented by [`HorizontalBar`] with [`DropDirection::Down`].
///
/// [`HorizontalBar`]: crate::horizontal_bar::HorizontalBar
/// [`DropDirection::Down`]: crate::horizontal_bar::DropDirection::Down
pub type MenuBar = crate::horizontal_bar::HorizontalBar;

// ============================================================================
// From<Menu> for BarEntry
// ============================================================================

impl From<Menu> for crate::horizontal_bar::BarEntry {
    fn from(menu: Menu) -> Self {
        Self::Dropdown {
            label: menu.name,
            items: menu.items,
            key_code: 0,
        }
    }
}

// ============================================================================
// Convenience constructor
// ============================================================================

/// Create a new menu bar from a list of [`Menu`] entries.
///
/// This is a convenience wrapper around [`HorizontalBar::menu_bar`] that
/// converts each `Menu` into a [`BarEntry::Dropdown`].
///
/// [`HorizontalBar::menu_bar`]: crate::horizontal_bar::HorizontalBar::menu_bar
/// [`BarEntry::Dropdown`]: crate::horizontal_bar::BarEntry::Dropdown
#[must_use]
pub fn menu_bar_from_menus(bounds: Rect, menus: Vec<Menu>) -> MenuBar {
    let entries = menus
        .into_iter()
        .map(crate::horizontal_bar::BarEntry::from)
        .collect();
    crate::horizontal_bar::HorizontalBar::menu_bar(bounds, entries)
}

// ============================================================================
// Re-exports
// ============================================================================

// Re-exports from horizontal_bar for users who import from menu_bar
pub use crate::horizontal_bar::{BarEntry, HorizontalBar};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_NEW, CM_OPEN, CM_QUIT, CM_SAVE};

    #[test]
    fn test_menu_item_new() {
        let item = MenuItem::new("~O~pen  F3", CM_OPEN);
        assert_eq!(item.label, "~O~pen  F3");
        assert_eq!(item.command, CM_OPEN);
        assert!(item.enabled);
        assert!(!item.is_separator());
    }

    #[test]
    fn test_menu_item_separator() {
        let sep = MenuItem::separator();
        assert_eq!(sep.command, 0);
        assert!(!sep.enabled);
        assert!(sep.is_separator());
    }

    #[test]
    fn test_menu_item_disabled() {
        let item = MenuItem::disabled("~S~ave", CM_SAVE);
        assert!(!item.enabled);
        assert!(!item.is_separator());
    }

    #[test]
    fn test_menu_item_hotkey() {
        let item = MenuItem::new("~O~pen  F3", CM_OPEN);
        assert_eq!(item.hotkey(), Some('o'));
        assert_eq!(item.display_label(), "Open  F3");
    }

    #[test]
    fn test_menu_display() {
        let menu = Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)]);
        assert_eq!(menu.display_name(), "File");
        assert_eq!(menu.hotkey(), Some('f'));
    }

    #[test]
    fn test_from_menu_to_bar_entry() {
        let menu = Menu::new(
            "~F~ile",
            vec![
                MenuItem::new("~N~ew", CM_NEW),
                MenuItem::separator(),
                MenuItem::new("~Q~uit", CM_QUIT),
            ],
        );
        let entry: crate::horizontal_bar::BarEntry = menu.into();
        assert_eq!(entry.label(), "~F~ile");
        assert_eq!(entry.hotkey(), Some('f'));
        if let crate::horizontal_bar::BarEntry::Dropdown { items, .. } = &entry {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected Dropdown variant");
        }
    }

    #[test]
    fn test_menu_bar_from_menus() {
        let menus = vec![
            Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)]),
            Menu::new("~E~dit", vec![MenuItem::new("~C~ut", 1010)]),
        ];
        let bar = menu_bar_from_menus(Rect::new(0, 0, 80, 1), menus);
        assert_eq!(bar.entries().len(), 2);
        assert!(!bar.is_active());
    }

    #[test]
    fn test_type_alias_usable() {
        let entries = vec![crate::horizontal_bar::BarEntry::Action {
            label: "~H~elp".into(),
            command: 1030,
            key_code: 0,
        }];
        let bar: MenuBar =
            crate::horizontal_bar::HorizontalBar::menu_bar(Rect::new(0, 0, 80, 1), entries);
        assert_eq!(bar.entries().len(), 1);
    }
}
