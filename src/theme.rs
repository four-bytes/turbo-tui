//! Theme — global color palette for all widgets.
//!
//! turbo-tui uses a thread-local global theme that all widgets read during
//! rendering. This follows the Borland Turbo Vision palette chain pattern
//! but simplified for Ratatui's `Style` system.
//!
//! # Usage
//!
//! ```ignore
//! use turbo_tui::theme;
//!
//! // Use dark theme (default)
//! let t = theme::current();
//!
//! // Switch to Borland classic at runtime
//! theme::set(Theme::borland_classic());
//! ```

use ratatui::style::{Color, Modifier, Style};
use std::cell::RefCell;

// ============================================================================
// Theme struct
// ============================================================================

/// Complete color theme for all turbo-tui widgets.
///
/// Every style field includes both foreground AND background colors to prevent
/// background bleed-through from underlying views.
#[derive(Debug, Clone)]
pub struct Theme {
    // ── Desktop ────────────────────────────────────────────────────────
    /// Desktop background style (fills the entire desktop area).
    pub desktop_bg: Style,
    /// Desktop background character (default: `' '`).
    pub desktop_char: char,

    // ── Window Frame ───────────────────────────────────────────────────
    /// Window frame border when active/focused.
    pub window_frame_active: Style,
    /// Window frame border when inactive.
    pub window_frame_inactive: Style,
    /// Window title text (active).
    pub window_title_active: Style,
    /// Window title text (inactive).
    pub window_title_inactive: Style,
    /// Window interior background.
    pub window_interior: Style,
    /// Close button `[■]`.
    pub window_close_button: Style,
    /// Resize handle `⋱`.
    pub window_resize_handle: Style,

    // ── Dialog Frame ───────────────────────────────────────────────────
    /// Dialog frame border (always bright).
    pub dialog_frame: Style,
    /// Dialog title text.
    pub dialog_title: Style,
    /// Dialog interior background.
    pub dialog_interior: Style,

    // ── Single Frame ───────────────────────────────────────────────────
    /// Single-line frame border.
    pub single_frame: Style,

    // ── Menu Bar ───────────────────────────────────────────────────────
    /// Menu bar background (entire row).
    pub menu_bar_normal: Style,
    /// Menu bar: selected/open menu name.
    pub menu_bar_selected: Style,
    /// Menu bar: hotkey character in normal state.
    pub menu_bar_hotkey: Style,
    /// Menu bar: hotkey character in selected state.
    pub menu_bar_hotkey_selected: Style,

    // ── Menu Dropdown ──────────────────────────────────────────────────
    /// Menu dropdown: background and border.
    pub menu_box_normal: Style,
    /// Menu dropdown: selected item.
    pub menu_box_selected: Style,
    /// Menu dropdown: disabled item.
    pub menu_box_disabled: Style,
    /// Menu dropdown: separator line.
    pub menu_box_separator: Style,
    /// Menu dropdown: hotkey character (normal).
    pub menu_box_hotkey: Style,
    /// Menu dropdown: hotkey character (selected).
    pub menu_box_hotkey_selected: Style,

    // ── Status Line ────────────────────────────────────────────────────
    /// Status line background.
    pub status_normal: Style,
    /// Status line: hotkey character.
    pub status_hotkey: Style,
    /// Status line: hovered/selected item.
    pub status_selected: Style,

    // ── Button ─────────────────────────────────────────────────────────
    /// Button: normal state.
    pub button_normal: Style,
    /// Button: default (responds to Enter).
    pub button_default: Style,
    /// Button: focused/selected.
    pub button_focused: Style,
    /// Button: disabled.
    pub button_disabled: Style,

    // ── Static Text ────────────────────────────────────────────────────
    /// Static text label.
    pub static_text: Style,

    // ── Scrollbar ──────────────────────────────────────────────────────
    /// Scrollbar track (empty area).
    pub scrollbar_track: Style,
    /// Scrollbar thumb (position indicator).
    pub scrollbar_thumb: Style,
    /// Scrollbar arrow buttons.
    pub scrollbar_arrows: Style,
}

impl Theme {
    /// Create the Borland Turbo Vision classic theme.
    ///
    /// Colors match the original Borland C++ Turbo Vision palette:
    /// - Blue desktop background
    /// - Blue windows with cyan/white frames
    /// - `LightGray` menu bar and status line
    /// - Green selection highlights
    #[must_use]
    pub fn borland_classic() -> Self {
        Self {
            // ── Desktop ────────────────────────────────────────────────
            desktop_bg: Style::default().fg(Color::DarkGray).bg(Color::Blue),
            desktop_char: ' ',

            // ── Window Frame ───────────────────────────────────────────
            window_frame_active: Style::default().fg(Color::White).bg(Color::Blue),
            window_frame_inactive: Style::default().fg(Color::LightCyan).bg(Color::Blue),
            window_title_active: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_title_inactive: Style::default().fg(Color::LightCyan).bg(Color::Blue),
            window_interior: Style::default().fg(Color::Gray).bg(Color::Blue),
            window_close_button: Style::default().fg(Color::LightCyan).bg(Color::Blue),
            window_resize_handle: Style::default().fg(Color::LightCyan).bg(Color::Blue),

            // ── Dialog Frame ───────────────────────────────────────────
            dialog_frame: Style::default().fg(Color::White).bg(Color::Gray),
            dialog_title: Style::default()
                .fg(Color::White)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            dialog_interior: Style::default().fg(Color::Black).bg(Color::Gray),

            // ── Single Frame ───────────────────────────────────────────
            single_frame: Style::default().fg(Color::Gray).bg(Color::Blue),

            // ── Menu Bar ───────────────────────────────────────────────
            menu_bar_normal: Style::default().fg(Color::Black).bg(Color::Gray),
            menu_bar_selected: Style::default().fg(Color::Black).bg(Color::Green),
            menu_bar_hotkey: Style::default()
                .fg(Color::Red)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            menu_bar_hotkey_selected: Style::default()
                .fg(Color::Red)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),

            // ── Menu Dropdown ──────────────────────────────────────────
            menu_box_normal: Style::default().fg(Color::Black).bg(Color::Gray),
            menu_box_selected: Style::default().fg(Color::Black).bg(Color::Green),
            menu_box_disabled: Style::default().fg(Color::DarkGray).bg(Color::Gray),
            menu_box_separator: Style::default().fg(Color::DarkGray).bg(Color::Gray),
            menu_box_hotkey: Style::default()
                .fg(Color::Red)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            menu_box_hotkey_selected: Style::default()
                .fg(Color::Red)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),

            // ── Status Line ────────────────────────────────────────────
            status_normal: Style::default().fg(Color::Black).bg(Color::Gray),
            status_hotkey: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            status_selected: Style::default().fg(Color::Black).bg(Color::Green),

            // ── Button ─────────────────────────────────────────────────
            button_normal: Style::default().fg(Color::White).bg(Color::Blue),
            button_default: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            button_focused: Style::default().fg(Color::Blue).bg(Color::Cyan),
            button_disabled: Style::default().fg(Color::DarkGray).bg(Color::Blue),

            // ── Static Text ────────────────────────────────────────────
            static_text: Style::default().fg(Color::Gray).bg(Color::Blue),

            // ── Scrollbar ──────────────────────────────────────────────
            scrollbar_track: Style::default().fg(Color::DarkGray).bg(Color::Blue),
            scrollbar_thumb: Style::default().fg(Color::White).bg(Color::Blue),
            scrollbar_arrows: Style::default().fg(Color::LightCyan).bg(Color::Blue),
        }
    }

    /// Create a dark theme suitable for modern terminals.
    ///
    /// - Black desktop background (uses terminal default)
    /// - Dark gray window interiors with white/gray text
    /// - Dark gray menu bar and status line
    /// - Cyan accent for selection and highlights
    #[must_use]
    pub fn dark() -> Self {
        Self {
            // ── Desktop ────────────────────────────────────────────────
            desktop_bg: Style::default().fg(Color::DarkGray).bg(Color::Black),
            desktop_char: ' ',

            // ── Window Frame ───────────────────────────────────────────
            window_frame_active: Style::default().fg(Color::Cyan).bg(Color::Black),
            window_frame_inactive: Style::default().fg(Color::DarkGray).bg(Color::Black),
            window_title_active: Style::default()
                .fg(Color::White)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
            window_title_inactive: Style::default().fg(Color::DarkGray).bg(Color::Black),
            window_interior: Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 30)),
            window_close_button: Style::default().fg(Color::Red).bg(Color::Black),
            window_resize_handle: Style::default().fg(Color::DarkGray).bg(Color::Black),

            // ── Dialog Frame ───────────────────────────────────────────
            dialog_frame: Style::default().fg(Color::White).bg(Color::DarkGray),
            dialog_title: Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
            dialog_interior: Style::default().fg(Color::White).bg(Color::DarkGray),

            // ── Single Frame ───────────────────────────────────────────
            single_frame: Style::default().fg(Color::DarkGray).bg(Color::Black),

            // ── Menu Bar ───────────────────────────────────────────────
            menu_bar_normal: Style::default().fg(Color::Gray).bg(Color::Rgb(30, 30, 30)),
            menu_bar_selected: Style::default().fg(Color::White).bg(Color::Rgb(0, 100, 150)),
            menu_bar_hotkey: Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(30, 30, 30))
                .add_modifier(Modifier::BOLD),
            menu_bar_hotkey_selected: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(0, 100, 150))
                .add_modifier(Modifier::BOLD),

            // ── Menu Dropdown ──────────────────────────────────────────
            menu_box_normal: Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
            menu_box_selected: Style::default().fg(Color::White).bg(Color::Rgb(0, 100, 150)),
            menu_box_disabled: Style::default().fg(Color::DarkGray).bg(Color::Rgb(40, 40, 40)),
            menu_box_separator: Style::default().fg(Color::DarkGray).bg(Color::Rgb(40, 40, 40)),
            menu_box_hotkey: Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(40, 40, 40))
                .add_modifier(Modifier::BOLD),
            menu_box_hotkey_selected: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(0, 100, 150))
                .add_modifier(Modifier::BOLD),

            // ── Status Line ────────────────────────────────────────────
            status_normal: Style::default().fg(Color::Gray).bg(Color::Rgb(30, 30, 30)),
            status_hotkey: Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(30, 30, 30))
                .add_modifier(Modifier::BOLD),
            status_selected: Style::default().fg(Color::White).bg(Color::Rgb(0, 100, 150)),

            // ── Button ─────────────────────────────────────────────────
            button_normal: Style::default().fg(Color::Gray).bg(Color::Rgb(50, 50, 50)),
            button_default: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(50, 50, 50))
                .add_modifier(Modifier::BOLD),
            button_focused: Style::default().fg(Color::White).bg(Color::Rgb(0, 100, 150)),
            button_disabled: Style::default().fg(Color::DarkGray).bg(Color::Rgb(50, 50, 50)),

            // ── Static Text ────────────────────────────────────────────
            static_text: Style::default().fg(Color::Gray).bg(Color::Rgb(30, 30, 30)),

            // ── Scrollbar ──────────────────────────────────────────────
            scrollbar_track: Style::default().fg(Color::Rgb(60, 60, 60)).bg(Color::Rgb(30, 30, 30)),
            scrollbar_thumb: Style::default().fg(Color::Gray).bg(Color::Rgb(30, 30, 30)),
            scrollbar_arrows: Style::default().fg(Color::DarkGray).bg(Color::Rgb(30, 30, 30)),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

// ============================================================================
// Thread-local global theme
// ============================================================================

thread_local! {
    static CURRENT_THEME: RefCell<Theme> = RefCell::new(Theme::dark());
}

/// Get the current theme and pass it to a closure.
///
/// This is the primary way widgets access the theme during rendering.
///
/// # Example
///
/// ```ignore
/// use turbo_tui::theme;
///
/// theme::with_current(|t| {
///     let style = t.window_frame_active;
///     // use style for rendering...
/// });
/// ```
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&Theme) -> R,
{
    CURRENT_THEME.with(|t| f(&t.borrow()))
}

/// Set a new global theme.
///
/// All subsequent widget rendering will use this theme.
///
/// # Example
///
/// ```ignore
/// use turbo_tui::theme::{self, Theme};
///
/// theme::set(Theme::borland_classic());
/// ```
pub fn set(theme: Theme) {
    CURRENT_THEME.with(|t| {
        *t.borrow_mut() = theme;
    });
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borland_classic_has_bg_on_all_styles() {
        let t = Theme::borland_classic();

        // Verify every style has a background color set (not None).
        // This prevents the "bleed-through" bug where desktop bg shows
        // through window borders.
        let styles = [
            ("desktop_bg", t.desktop_bg),
            ("window_frame_active", t.window_frame_active),
            ("window_frame_inactive", t.window_frame_inactive),
            ("window_title_active", t.window_title_active),
            ("window_title_inactive", t.window_title_inactive),
            ("window_interior", t.window_interior),
            ("window_close_button", t.window_close_button),
            ("window_resize_handle", t.window_resize_handle),
            ("dialog_frame", t.dialog_frame),
            ("dialog_title", t.dialog_title),
            ("dialog_interior", t.dialog_interior),
            ("single_frame", t.single_frame),
            ("menu_bar_normal", t.menu_bar_normal),
            ("menu_bar_selected", t.menu_bar_selected),
            ("menu_bar_hotkey", t.menu_bar_hotkey),
            ("menu_bar_hotkey_selected", t.menu_bar_hotkey_selected),
            ("menu_box_normal", t.menu_box_normal),
            ("menu_box_selected", t.menu_box_selected),
            ("menu_box_disabled", t.menu_box_disabled),
            ("menu_box_separator", t.menu_box_separator),
            ("menu_box_hotkey", t.menu_box_hotkey),
            ("menu_box_hotkey_selected", t.menu_box_hotkey_selected),
            ("status_normal", t.status_normal),
            ("status_hotkey", t.status_hotkey),
            ("status_selected", t.status_selected),
            ("button_normal", t.button_normal),
            ("button_default", t.button_default),
            ("button_focused", t.button_focused),
            ("button_disabled", t.button_disabled),
            ("static_text", t.static_text),
            ("scrollbar_track", t.scrollbar_track),
            ("scrollbar_thumb", t.scrollbar_thumb),
            ("scrollbar_arrows", t.scrollbar_arrows),
        ];

        for (name, style) in styles {
            assert!(
                style.bg.is_some(),
                "Style '{name}' is missing a background color — this causes bleed-through"
            );
        }
    }

    #[test]
    fn test_default_is_dark() {
        let default = Theme::default();
        let dark = Theme::dark();

        // Spot-check a few styles
        assert_eq!(default.desktop_bg, dark.desktop_bg);
        assert_eq!(default.window_frame_active, dark.window_frame_active);
        assert_eq!(default.menu_bar_normal, dark.menu_bar_normal);
    }

    #[test]
    fn test_dark_has_bg_on_all_styles() {
        let t = Theme::dark();

        let styles = [
            ("desktop_bg", t.desktop_bg),
            ("window_frame_active", t.window_frame_active),
            ("window_frame_inactive", t.window_frame_inactive),
            ("window_title_active", t.window_title_active),
            ("window_title_inactive", t.window_title_inactive),
            ("window_interior", t.window_interior),
            ("window_close_button", t.window_close_button),
            ("window_resize_handle", t.window_resize_handle),
            ("dialog_frame", t.dialog_frame),
            ("dialog_title", t.dialog_title),
            ("dialog_interior", t.dialog_interior),
            ("single_frame", t.single_frame),
            ("menu_bar_normal", t.menu_bar_normal),
            ("menu_bar_selected", t.menu_bar_selected),
            ("menu_bar_hotkey", t.menu_bar_hotkey),
            ("menu_bar_hotkey_selected", t.menu_bar_hotkey_selected),
            ("menu_box_normal", t.menu_box_normal),
            ("menu_box_selected", t.menu_box_selected),
            ("menu_box_disabled", t.menu_box_disabled),
            ("menu_box_separator", t.menu_box_separator),
            ("menu_box_hotkey", t.menu_box_hotkey),
            ("menu_box_hotkey_selected", t.menu_box_hotkey_selected),
            ("status_normal", t.status_normal),
            ("status_hotkey", t.status_hotkey),
            ("status_selected", t.status_selected),
            ("button_normal", t.button_normal),
            ("button_default", t.button_default),
            ("button_focused", t.button_focused),
            ("button_disabled", t.button_disabled),
            ("static_text", t.static_text),
            ("scrollbar_track", t.scrollbar_track),
            ("scrollbar_thumb", t.scrollbar_thumb),
            ("scrollbar_arrows", t.scrollbar_arrows),
        ];

        for (name, style) in styles {
            assert!(
                style.bg.is_some(),
                "Dark theme style '{name}' is missing a background color"
            );
        }
    }

    #[test]
    fn test_set_and_get_theme() {
        // Save original
        let original_bg = with_current(|t| t.desktop_char);

        // Set a modified theme
        let mut custom = Theme::dark();
        custom.desktop_char = '▓';
        set(custom);

        let new_bg = with_current(|t| t.desktop_char);
        assert_eq!(new_bg, '▓');

        // Restore
        set(Theme::dark());
        let restored = with_current(|t| t.desktop_char);
        assert_eq!(restored, original_bg);
    }

    #[test]
    fn test_theme_clone() {
        let t1 = Theme::dark();
        let t2 = t1.clone();
        assert_eq!(t1.desktop_bg, t2.desktop_bg);
        assert_eq!(t1.window_frame_active, t2.window_frame_active);
    }
}
