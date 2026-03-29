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
//! // Use Turbo Vision theme (default)
//! let t = theme::current();
//!
//! // Switch theme at runtime
//! theme::set(Theme::turbo_vision());
//! ```

use ratatui::style::{Color, Modifier, Style};
use std::cell::RefCell;
use std::collections::BTreeMap;

// ============================================================================
// ButtonSide enum
// ============================================================================

/// Side of the title bar for button placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSide {
    /// Left side (Borland default for close button).
    #[default]
    Left,
    /// Right side (Windows default for close button).
    Right,
}

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
    /// Resize handle `◢`.
    pub window_resize_handle: Style,
    /// Close button `[■]` when window is inactive/unfocused.
    pub window_close_button_inactive: Style,
    /// Resize handle when window is inactive/unfocused.
    pub window_resize_handle_inactive: Style,
    /// Window frame border during drag/resize operation (highlighted).
    pub window_frame_dragging: Style,

    // ── Border Characters ─────────────────────────────────────────────
    /// Window border: top-left corner.
    pub border_tl: char,
    /// Window border: top-right corner.
    pub border_tr: char,
    /// Window border: bottom-left corner.
    pub border_bl: char,
    /// Window border: bottom-right corner.
    pub border_br: char,
    /// Window border: horizontal line.
    pub border_h: char,
    /// Window border: vertical line.
    pub border_v: char,
    /// Menu dropdown border: top-left corner.
    pub menu_border_tl: char,
    /// Menu dropdown border: top-right corner.
    pub menu_border_tr: char,
    /// Menu dropdown border: bottom-left corner.
    pub menu_border_bl: char,
    /// Menu dropdown border: bottom-right corner.
    pub menu_border_br: char,
    /// Menu dropdown border: horizontal line.
    pub menu_border_h: char,
    /// Menu dropdown border: vertical line.
    pub menu_border_v: char,
    /// Menu dropdown: separator left junction.
    pub menu_sep_l: char,
    /// Menu dropdown: separator right junction.
    pub menu_sep_r: char,

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
    /// Scrollbar track when hovered.
    pub scrollbar_track_hover: Style,
    /// Scrollbar track when parent is inactive/unfocused.
    pub scrollbar_track_inactive: Style,
    /// Scrollbar thumb when parent is inactive/unfocused.
    pub scrollbar_thumb_inactive: Style,
    /// Scrollbar arrows when parent is inactive/unfocused.
    pub scrollbar_arrows_inactive: Style,

    // ── Hover Styles ───────────────────────────────────────────────────
    /// Close button hover style.
    pub window_close_button_hover: Style,
    /// Resize handle hover style.
    pub window_resize_handle_hover: Style,
    /// Button hover style.
    pub button_hover: Style,
    /// Scrollbar thumb hover style.
    pub scrollbar_thumb_hover: Style,
    /// Scrollbar arrows hover style.
    pub scrollbar_arrows_hover: Style,

    // ── Title Bar ──────────────────────────────────────────────────────
    /// Title bar background style (if different from frame).
    /// When set to a non-default value, the entire title row gets this background.
    pub title_bar_bg: Option<Style>,

    // ── Close Button Configuration ─────────────────────────────────────
    /// Close button text (default: "[■]").
    pub close_button_text: String,
    /// Which side the close button sits on. Default: `Left` (Borland style).
    pub close_button_side: ButtonSide,
    /// Which side minimize/maximize controls sit on. Default: `Right`.
    pub controls_side: ButtonSide,
    /// Margin (in columns) from the left border corner to the first button. Default: 2 (1 corner + 1 gap).
    pub button_margin_left: u16,
    /// Margin (in columns) from the right border corner to the first button. Default: 2 (1 corner + 1 gap).
    pub button_margin_right: u16,

    // ── Title Button Configuration ─────────────────────────────────────
    /// Minimize button text (e.g. "🗕", " ─ ", "[▼]"). Empty string = no minimize button.
    pub minimize_button_text: String,
    /// Maximize/restore button text (e.g. "🗖", " □ ", "[▲]"). Empty string = no maximize button.
    pub maximize_button_text: String,
    /// Maximize button text when window is zoomed (e.g. "🗗", " ◻ ", "[◻]"). Falls back to `maximize_button_text` if empty.
    pub maximize_restore_text: String,

    // ── Title Button Styles ────────────────────────────────────────────
    /// Minimize button style (active).
    pub window_minimize_button: Style,
    /// Minimize button hover style.
    pub window_minimize_button_hover: Style,
    /// Minimize button style when window is inactive.
    pub window_minimize_button_inactive: Style,
    /// Maximize button style (active).
    pub window_maximize_button: Style,
    /// Maximize button hover style.
    pub window_maximize_button_hover: Style,
    /// Maximize button style when window is inactive.
    pub window_maximize_button_inactive: Style,

    // ── Resize Grip Character ──────────────────────────────────────────
    /// Resize grip character (default: '◢').
    pub resize_grip_char: char,
}

impl Theme {
    /// Create the Turbo Vision classic theme.
    ///
    /// Colors match the original Borland C++ Turbo Vision 2.0 `cpAppColor` palette:
    /// - Darkest grey desktop (pattern override for terminal compatibility)
    /// - Blue windows with `LightGreen` icons, Yellow interior text
    /// - `LightGray` menu bar and status line with Green selection
    /// - Green buttons (Black/White/LightCyan text variants)
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn turbo_vision() -> Self {
        Self {
            // ── Desktop ────────────────────────────────────────────────
            // Original TV: 0x71 = Blue on LightGray with ░ pattern
            // Override: darkest grey background (pattern doesn't work well in terminals)
            desktop_bg: Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(24, 24, 24)),
            desktop_char: ' ',

            // ── Window Frame ───────────────────────────────────────────
            // Passive: 0x17 = LightGray on Blue
            window_frame_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            // Active: 0x1F = White on Blue
            window_frame_active: Style::default().fg(Color::White).bg(Color::Blue),
            // Title active: White on Blue, bold
            window_title_active: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            // Title passive: LightGray on Blue
            window_title_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            // Interior: 0x1E = Yellow on Blue (scroller normal text color)
            window_interior: Style::default().fg(Color::Yellow).bg(Color::Blue),
            // Close button / icons: 0x1A = LightGreen on Blue
            window_close_button: Style::default().fg(Color::LightGreen).bg(Color::Blue),
            // Resize handle: same as icons = LightGreen on Blue
            window_resize_handle: Style::default().fg(Color::LightGreen).bg(Color::Blue),
            // Inactive close button: same color as inactive frame (Gray on Blue)
            window_close_button_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            // Inactive resize handle: same as inactive frame
            window_resize_handle_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            // Dragging: Yellow on Blue, bold (highlighted)
            window_frame_dragging: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),

            // ── Border Characters (double-line, classic Borland) ─────
            border_tl: '╔',
            border_tr: '╗',
            border_bl: '╚',
            border_br: '╝',
            border_h: '═',
            border_v: '║',
            menu_border_tl: '┌',
            menu_border_tr: '┐',
            menu_border_bl: '└',
            menu_border_br: '┘',
            menu_border_h: '─',
            menu_border_v: '│',
            menu_sep_l: '├',
            menu_sep_r: '┤',

            // ── Dialog Frame ───────────────────────────────────────────
            // 0x7F = White on LightGray (active)
            dialog_frame: Style::default().fg(Color::White).bg(Color::Gray),
            dialog_title: Style::default()
                .fg(Color::White)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            // Interior: 0x70 = Black on LightGray
            dialog_interior: Style::default().fg(Color::Black).bg(Color::Gray),

            // ── Single Frame ───────────────────────────────────────────
            single_frame: Style::default().fg(Color::Gray).bg(Color::Blue),

            // ── Menu Bar ───────────────────────────────────────────────
            // Normal: 0x70 = Black on LightGray
            menu_bar_normal: Style::default().fg(Color::Black).bg(Color::Gray),
            // Selected: 0x20 = Black on Green
            menu_bar_selected: Style::default().fg(Color::Black).bg(Color::Green),
            // Hotkey: 0x74 = Red on LightGray
            menu_bar_hotkey: Style::default()
                .fg(Color::Red)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            // Hotkey selected: 0x24 = Red on Green
            menu_bar_hotkey_selected: Style::default()
                .fg(Color::Red)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),

            // ── Menu Dropdown ──────────────────────────────────────────
            // Same palette as menu bar
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
            // Same as menu bar: 0x70 normal, 0x74 hotkey, 0x20 selected
            status_normal: Style::default().fg(Color::Black).bg(Color::Gray),
            status_hotkey: Style::default()
                .fg(Color::Yellow)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            status_selected: Style::default().fg(Color::Black).bg(Color::Green),

            // ── Button ─────────────────────────────────────────────────
            // Normal: 0x20 = Black on Green
            button_normal: Style::default().fg(Color::Black).bg(Color::Green),
            // Default: 0x2F = White on Green, bold
            button_default: Style::default()
                .fg(Color::White)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
            // Focused: 0x2B = LightCyan on Green
            button_focused: Style::default().fg(Color::LightCyan).bg(Color::Green),
            // Disabled: 0x78 = DarkGray on LightGray
            button_disabled: Style::default().fg(Color::DarkGray).bg(Color::Gray),

            // ── Static Text ────────────────────────────────────────────
            // In window context: 0x1E = Yellow on Blue
            static_text: Style::default().fg(Color::Yellow).bg(Color::Blue),

            // ── Scrollbar ──────────────────────────────────────────────
            // TV original: 0x31 = Blue on Cyan for all scrollbar parts
            scrollbar_track: Style::default().fg(Color::Blue).bg(Color::Cyan),
            scrollbar_thumb: Style::default().fg(Color::Blue).bg(Color::Cyan),
            scrollbar_arrows: Style::default().fg(Color::Blue).bg(Color::Cyan),
            // Inactive scrollbar: muted colors (Gray on Blue matches inactive frame)
            scrollbar_track_inactive: Style::default().fg(Color::DarkGray).bg(Color::Blue),
            scrollbar_thumb_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            scrollbar_arrows_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),

            // ── Hover ──────────────────────────────────────────────────
            window_close_button_hover: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_resize_handle_hover: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            button_hover: Style::default()
                .fg(Color::White)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
            scrollbar_track_hover: Style::default().fg(Color::White).bg(Color::Cyan),
            scrollbar_thumb_hover: Style::default().fg(Color::White).bg(Color::Cyan),
            scrollbar_arrows_hover: Style::default().fg(Color::White).bg(Color::Cyan),

            // ── Title bar ──────────────────────────────────────────────
            title_bar_bg: None,

            // ── Close button config ─────────────────────────────────────
            close_button_text: "[■]".to_owned(),
            close_button_side: ButtonSide::Left,
            controls_side: ButtonSide::Right,
            button_margin_left: 2,
            button_margin_right: 2,

            // ── Title button config ─────────────────────────────────────
            minimize_button_text: String::new(),
            maximize_button_text: String::new(),
            maximize_restore_text: String::new(),

            // ── Title button styles (same as close button by default) ───
            window_minimize_button: Style::default().fg(Color::LightGreen).bg(Color::Blue),
            window_minimize_button_hover: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_minimize_button_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            window_maximize_button: Style::default().fg(Color::LightGreen).bg(Color::Blue),
            window_maximize_button_hover: Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            window_maximize_button_inactive: Style::default().fg(Color::Gray).bg(Color::Blue),
            // ── Resize grip ─────────────────────────────────────────────
            resize_grip_char: '◢',
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::turbo_vision()
    }
}

// ============================================================================
// Thread-local global theme
// ============================================================================

thread_local! {
    static CURRENT_THEME: RefCell<Theme> = RefCell::new(Theme::turbo_vision());
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
/// theme::set(Theme::turbo_vision());
/// ```
pub fn set(theme: Theme) {
    CURRENT_THEME.with(|t| {
        *t.borrow_mut() = theme;
    });
}

// ============================================================================
// Theme Registry — dynamic theme management
// ============================================================================

thread_local! {
    /// Registry of named themes. Populated by `load_themes_from_dir()` or `register()`.
    static THEME_REGISTRY: RefCell<BTreeMap<String, Theme>> = const { RefCell::new(BTreeMap::new()) };
    /// Name of the currently active theme.
    static CURRENT_THEME_NAME: RefCell<String> = RefCell::new("Turbo Vision".to_owned());
}

/// Register a theme by name.
///
/// If a theme with the same name already exists, it is replaced.
pub fn register(name: &str, theme: Theme) {
    THEME_REGISTRY.with(|r| {
        r.borrow_mut().insert(name.to_owned(), theme);
    });
}

/// Get the name of the currently active theme.
#[must_use]
pub fn current_name() -> String {
    CURRENT_THEME_NAME.with(|n| n.borrow().clone())
}

/// Set the active theme by name.
///
/// Looks up the name in the registry. If not found, does nothing and returns `false`.
/// If found, sets it as the current theme and returns `true`.
#[must_use]
pub fn set_by_name(name: &str) -> bool {
    let theme = THEME_REGISTRY.with(|r| r.borrow().get(name).cloned());
    if let Some(t) = theme {
        set(t);
        CURRENT_THEME_NAME.with(|n| {
            name.clone_into(&mut n.borrow_mut());
        });
        true
    } else {
        false
    }
}

/// Get the list of all registered theme names (sorted alphabetically).
#[must_use]
pub fn registered_names() -> Vec<String> {
    THEME_REGISTRY.with(|r| r.borrow().keys().cloned().collect())
}

/// Cycle to the next registered theme.
///
/// Returns the name of the newly activated theme. If the registry is empty,
/// returns the current theme name without changing anything.
#[must_use]
pub fn cycle_next_registered() -> String {
    let names = registered_names();
    if names.is_empty() {
        return current_name();
    }
    let current = current_name();
    let current_idx = names.iter().position(|n| n == &current).unwrap_or(0);
    let next_idx = (current_idx + 1) % names.len();
    let next_name = &names[next_idx];
    let _ = set_by_name(next_name);
    next_name.clone()
}

/// Initialize the registry with the built-in Turbo Vision theme.
///
/// Call this once at startup before loading external themes.
pub fn init_builtin() {
    register("Turbo Vision", Theme::turbo_vision());
}

/// Report from loading themes from a directory.
///
/// Contains the list of successfully loaded theme names and any errors
/// encountered per file. This ensures theme loading failures are never silent.
#[cfg(feature = "json-themes")]
#[derive(Debug)]
pub struct ThemeLoadReport {
    /// Names of themes that were successfully loaded and registered.
    pub loaded: Vec<String>,
    /// Errors encountered, with the file path that caused each error.
    pub errors: Vec<(std::path::PathBuf, crate::theme_json::ThemeLoadError)>,
}

#[cfg(feature = "json-themes")]
impl ThemeLoadReport {
    /// Returns `true` if any theme files failed to load.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns the number of successfully loaded themes.
    #[must_use]
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Format all errors into a single multi-line string for logging/display.
    ///
    /// Returns `None` if there are no errors.
    #[must_use]
    pub fn error_summary(&self) -> Option<String> {
        if self.errors.is_empty() {
            return None;
        }
        let lines: Vec<String> = self
            .errors
            .iter()
            .map(|(path, err)| format!("  {}: {err}", path.display()))
            .collect();
        Some(format!(
            "Failed to load {} theme file(s):\n{}",
            self.errors.len(),
            lines.join("\n")
        ))
    }
}

#[cfg(feature = "json-themes")]
impl std::fmt::Display for ThemeLoadReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Loaded {} theme(s), {} error(s)",
            self.loaded.len(),
            self.errors.len()
        )?;
        for name in &self.loaded {
            write!(f, "\n  ✓ {name}")?;
        }
        for (path, err) in &self.errors {
            write!(f, "\n  ✗ {}: {err}", path.display())?;
        }
        Ok(())
    }
}

/// Load all `.json` theme files from the given directory and register them.
///
/// Each file's `"name"` field is used as the registry key.
/// Returns a [`ThemeLoadReport`] with details about which themes loaded
/// and which files had errors. **Check `report.has_errors()`** — theme
/// loading should never fail silently.
///
/// Only available when the `json-themes` feature is enabled.
///
/// # Errors
///
/// Returns an `io::Error` if the directory itself cannot be read.
/// Individual file errors are collected in `ThemeLoadReport::errors`.
#[cfg(feature = "json-themes")]
pub fn load_themes_from_dir(dir: &std::path::Path) -> Result<ThemeLoadReport, std::io::Error> {
    let mut report = ThemeLoadReport {
        loaded: Vec::new(),
        errors: Vec::new(),
    };
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            match crate::theme_json::ThemeData::load_from_file(&path) {
                Ok(theme_data) => {
                    let name = theme_data.name.clone();
                    register(&name, theme_data.to_theme());
                    report.loaded.push(name);
                }
                Err(e) => {
                    report.errors.push((path, e));
                }
            }
        }
    }
    Ok(report)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turbo_vision_has_bg_on_all_styles() {
        let t = Theme::turbo_vision();

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
            (
                "window_close_button_inactive",
                t.window_close_button_inactive,
            ),
            (
                "window_resize_handle_inactive",
                t.window_resize_handle_inactive,
            ),
            ("window_minimize_button", t.window_minimize_button),
            ("window_maximize_button", t.window_maximize_button),
            (
                "window_minimize_button_inactive",
                t.window_minimize_button_inactive,
            ),
            (
                "window_maximize_button_inactive",
                t.window_maximize_button_inactive,
            ),
            ("window_frame_dragging", t.window_frame_dragging),
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
            ("scrollbar_track_hover", t.scrollbar_track_hover),
            ("scrollbar_thumb", t.scrollbar_thumb),
            ("scrollbar_arrows", t.scrollbar_arrows),
            ("scrollbar_track_inactive", t.scrollbar_track_inactive),
            ("scrollbar_thumb_inactive", t.scrollbar_thumb_inactive),
            ("scrollbar_arrows_inactive", t.scrollbar_arrows_inactive),
        ];

        for (name, style) in styles {
            assert!(
                style.bg.is_some(),
                "Style '{name}' is missing a background color — this causes bleed-through"
            );
        }
    }

    #[test]
    fn test_default_is_turbo_vision() {
        let default = Theme::default();
        let tv = Theme::turbo_vision();

        // Spot-check a few styles
        assert_eq!(default.desktop_bg, tv.desktop_bg);
        assert_eq!(default.window_frame_active, tv.window_frame_active);
        assert_eq!(default.menu_bar_normal, tv.menu_bar_normal);
    }

    #[test]
    fn test_set_and_get_theme() {
        // Save original
        let original_bg = with_current(|t| t.desktop_char);

        // Set a modified theme
        let mut custom = Theme::turbo_vision();
        custom.desktop_char = '▓';
        set(custom);

        let new_bg = with_current(|t| t.desktop_char);
        assert_eq!(new_bg, '▓');

        // Restore
        set(Theme::turbo_vision());
        let restored = with_current(|t| t.desktop_char);
        assert_eq!(restored, original_bg);
    }

    #[test]
    fn test_theme_clone() {
        let t1 = Theme::turbo_vision();
        let t2 = t1.clone();
        assert_eq!(t1.desktop_bg, t2.desktop_bg);
        assert_eq!(t1.window_frame_active, t2.window_frame_active);
    }

    #[test]
    fn test_turbo_vision_has_double_borders() {
        let t = Theme::turbo_vision();
        assert_eq!(t.border_tl, '╔');
        assert_eq!(t.border_tr, '╗');
        assert_eq!(t.border_bl, '╚');
        assert_eq!(t.border_br, '╝');
        assert_eq!(t.border_h, '═');
        assert_eq!(t.border_v, '║');
    }

    #[test]
    fn test_registry_init_builtin() {
        init_builtin();
        let names = registered_names();
        assert!(names.contains(&"Turbo Vision".to_owned()));
    }

    #[test]
    fn test_set_by_name() {
        init_builtin();
        assert!(set_by_name("Turbo Vision"));
        assert_eq!(current_name(), "Turbo Vision");
        assert!(!set_by_name("Nonexistent Theme"));
    }

    #[test]
    fn test_register_and_cycle() {
        init_builtin();
        // Register a second theme for cycling
        let mut custom = Theme::turbo_vision();
        custom.desktop_char = '▓';
        register("Custom", custom);

        let names = registered_names();
        assert!(names.len() >= 2);
    }

    #[test]
    fn test_turbo_vision_has_inactive_styles() {
        let t = Theme::turbo_vision();
        // Inactive close button should have a background
        assert!(t.window_close_button_inactive.bg.is_some());
        assert!(t.window_resize_handle_inactive.bg.is_some());
    }
}
