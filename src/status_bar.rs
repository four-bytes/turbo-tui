//! Status Bar — backward-compatibility wrapper around [`HorizontalBar`].
//!
//! The [`StatusBar`] type is now a type alias for
//! [`HorizontalBar`](crate::horizontal_bar::HorizontalBar) configured with
//! [`DropDirection::Up`](crate::overlay::DropDirection::Up).
//!
//! Use [`status_bar_from_items`] to build a `StatusBar` from the legacy
//! [`StatusItem`] list.  All [`KB_*`](KB_F1) constants and [`StatusItem`]
//! remain available for backward compatibility.

use crate::command::CommandId;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

// ============================================================================
// Constants for key codes
// ============================================================================

/// Key code for F1.
pub const KB_F1: u16 = 0x3B00;
/// Key code for F2.
pub const KB_F2: u16 = 0x3C00;
/// Key code for F3.
pub const KB_F3: u16 = 0x3D00;
/// Key code for F4.
pub const KB_F4: u16 = 0x3E00;
/// Key code for F5.
pub const KB_F5: u16 = 0x3F00;
/// Key code for F6.
pub const KB_F6: u16 = 0x4000;
/// Key code for F7.
pub const KB_F7: u16 = 0x4100;
/// Key code for F8.
pub const KB_F8: u16 = 0x4200;
/// Key code for F9.
pub const KB_F9: u16 = 0x4300;
/// Key code for F10.
pub const KB_F10: u16 = 0x4400;
/// Key code for F11.
pub const KB_F11: u16 = 0x5700;
/// Key code for F12.
pub const KB_F12: u16 = 0x5800;
/// Key code for Alt+X (example shortcut).
pub const KB_ALT_X: u16 = 0x2D00;

// ============================================================================
// StatusItem
// ============================================================================

/// A single item in the status bar.
///
/// Items display text with optional `~X~` hotkey markers and respond to
/// mouse clicks or keyboard shortcuts.
#[derive(Debug, Clone)]
pub struct StatusItem {
    /// Display text with `~X~` hotkey markers (e.g., "~F1~ Help").
    pub text: String,
    /// Command to execute when clicked or hotkey pressed.
    pub command: CommandId,
    /// Key code that triggers this item (0 = mouse only).
    pub key_code: u16,
}

impl StatusItem {
    /// Create a new status item.
    #[must_use]
    pub fn new(text: impl Into<String>, command: CommandId, key_code: u16) -> Self {
        Self {
            text: text.into(),
            command,
            key_code,
        }
    }

    /// Create a mouse-only item (no key shortcut).
    #[must_use]
    pub fn mouse_only(text: impl Into<String>, command: CommandId) -> Self {
        Self {
            text: text.into(),
            command,
            key_code: 0,
        }
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Check if a key event matches the given key code.
///
/// Maps F-key codes to `KeyCode::F(n)` values and `KB_ALT_X` to
/// `Alt+x` / `Alt+X`.
#[must_use]
pub fn key_matches(key: &KeyEvent, code: u16) -> bool {
    // Extract F-key number from our code constants
    let f_key = match code {
        KB_F1 => Some(1),
        KB_F2 => Some(2),
        KB_F3 => Some(3),
        KB_F4 => Some(4),
        KB_F5 => Some(5),
        KB_F6 => Some(6),
        KB_F7 => Some(7),
        KB_F8 => Some(8),
        KB_F9 => Some(9),
        KB_F10 => Some(10),
        KB_F11 => Some(11),
        KB_F12 => Some(12),
        _ => None,
    };

    // For Alt shortcuts, check Alt modifier + char
    if code == KB_ALT_X && key.modifiers.contains(KeyModifiers::ALT) {
        return matches!(key.code, KeyCode::Char('x' | 'X'));
    }

    // For F-keys, check key code
    if let Some(n) = f_key {
        return key.code == KeyCode::F(n);
    }

    false
}

/// Parse text with `~X~` markers.
///
/// Returns segments with `(text, highlighted)` tuples.
/// The text between `~` markers is marked as highlighted.
///
/// Delegates to [`crate::horizontal_bar::parse_hotkey_text`].
///
/// # Examples
///
/// ```
/// # use turbo_tui::status_bar::parse_hotkey_text;
/// let segments = parse_hotkey_text("~F1~ Help");
/// assert_eq!(segments, vec![("F1".to_string(), true), (" Help".to_string(), false)]);
/// ```
#[must_use]
pub fn parse_hotkey_text(text: &str) -> Vec<(String, bool)> {
    crate::horizontal_bar::parse_hotkey_text(text)
}

/// Compute display width, stripping `~` markers.
///
/// Delegates to [`crate::horizontal_bar::display_width`].
#[must_use]
pub fn display_width(text: &str) -> usize {
    crate::horizontal_bar::display_width(text)
}

/// Compute item positions starting at a given x offset.
///
/// Returns `(start_x, end_x)` for each item.  Each item occupies
/// 1 leading space + text display width + 1 trailing space.
#[must_use]
pub fn compute_positions(items: &[StatusItem], start_x: u16) -> Vec<(u16, u16)> {
    let mut positions = Vec::new();
    let mut x = start_x;

    for item in items {
        let width = u16::try_from(display_width(&item.text)).unwrap_or(u16::MAX);
        let start = x;
        // Each item: 1 leading space + text width + 1 trailing space
        let end = x + width + 2;
        positions.push((start, end));
        x = end;
    }

    positions
}

// ============================================================================
// Type alias + From impl + convenience constructor
// ============================================================================

/// Type alias for backward compatibility.
///
/// `StatusBar` is now implemented by [`HorizontalBar`] with [`DropDirection::Up`].
///
/// [`DropDirection::Up`]: crate::overlay::DropDirection::Up
pub type StatusBar = crate::horizontal_bar::HorizontalBar;

impl From<StatusItem> for crate::horizontal_bar::BarEntry {
    fn from(item: StatusItem) -> Self {
        Self::Action {
            label: item.text,
            command: item.command,
            key_code: item.key_code,
        }
    }
}

/// Create a new status bar from a list of [`StatusItem`] entries.
///
/// This is a convenience wrapper around [`HorizontalBar::status_bar`] that
/// converts each `StatusItem` into a [`BarEntry::Action`].
///
/// [`BarEntry::Action`]: crate::horizontal_bar::BarEntry::Action
#[must_use]
pub fn status_bar_from_items(bounds: Rect, items: Vec<StatusItem>) -> StatusBar {
    let entries = items
        .into_iter()
        .map(crate::horizontal_bar::BarEntry::from)
        .collect();
    crate::horizontal_bar::HorizontalBar::status_bar(bounds, entries)
}

// ============================================================================
// Re-exports
// ============================================================================

pub use crate::horizontal_bar::{BarEntry, HorizontalBar};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CM_CLOSE;

    #[test]
    fn test_status_item_new() {
        let item = StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1);
        assert_eq!(item.text, "~F1~ Help");
        assert_eq!(item.command, CM_CLOSE);
        assert_eq!(item.key_code, KB_F1);
    }

    #[test]
    fn test_status_item_mouse_only() {
        let item = StatusItem::mouse_only("Click Me", CM_CLOSE);
        assert_eq!(item.text, "Click Me");
        assert_eq!(item.command, CM_CLOSE);
        assert_eq!(item.key_code, 0);
    }

    #[test]
    fn test_key_matches_f_keys() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert!(key_matches(&key, KB_F1));
        assert!(!key_matches(&key, KB_F2));

        let key = KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE);
        assert!(key_matches(&key, KB_F10));
    }

    #[test]
    fn test_key_matches_alt() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert!(key_matches(&key, KB_ALT_X));

        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert!(!key_matches(&key, KB_ALT_X));
    }

    #[test]
    fn test_parse_hotkey_text_delegated() {
        let segments = parse_hotkey_text("~F1~ Help");
        assert_eq!(
            segments,
            vec![("F1".to_string(), true), (" Help".to_string(), false)]
        );
    }

    #[test]
    fn test_display_width_delegated() {
        assert_eq!(display_width("~F1~ Help"), 7);
    }

    #[test]
    fn test_compute_positions() {
        let items = vec![
            StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
            StatusItem::new("~F2~ Open", CM_CLOSE, KB_F2),
        ];
        let positions = compute_positions(&items, 0);
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], (0, 9));
        assert_eq!(positions[1], (9, 18));
    }

    #[test]
    fn test_from_status_item_to_bar_entry() {
        let item = StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1);
        let entry: crate::horizontal_bar::BarEntry = item.into();
        assert_eq!(entry.label(), "~F1~ Help");
        assert_eq!(entry.key_code(), KB_F1);
        if let crate::horizontal_bar::BarEntry::Action { command, .. } = &entry {
            assert_eq!(*command, CM_CLOSE);
        } else {
            panic!("Expected Action variant");
        }
    }

    #[test]
    fn test_status_bar_from_items() {
        use ratatui::layout::Rect;
        let items = vec![
            StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
            StatusItem::new("~F2~ Open", CM_CLOSE, KB_F2),
        ];
        let bar = status_bar_from_items(Rect::new(0, 23, 80, 1), items);
        assert_eq!(bar.entries().len(), 2);
        assert!(!bar.is_active());
    }

    #[test]
    fn test_type_alias_usable() {
        use ratatui::layout::Rect;
        let entries = vec![crate::horizontal_bar::BarEntry::Action {
            label: "~F1~ Help".into(),
            command: CM_CLOSE,
            key_code: KB_F1,
        }];
        let bar: StatusBar =
            crate::horizontal_bar::HorizontalBar::status_bar(Rect::new(0, 23, 80, 1), entries);
        assert_eq!(bar.entries().len(), 1);
    }
}
