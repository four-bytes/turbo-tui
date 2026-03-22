//! JSON theme serialization/deserialization.
//!
//! This module is only available when the `json-themes` feature is enabled.
//! It provides a serializable data model for themes and conversion to/from
//! the internal `Theme` struct.
//!
//! # JSON Format
//!
//! Colors can be specified as:
//! - Named CGA colors: `"Black"`, `"Blue"`, `"Green"`, `"Cyan"`, `"Red"`,
//!   `"Magenta"`, `"Yellow"`, `"Gray"`, `"DarkGray"`, `"LightBlue"`,
//!   `"LightGreen"`, `"LightCyan"`, `"LightRed"`, `"LightMagenta"`,
//!   `"White"`
//! - RGB hex: `"#1E1E1E"` or `"#ff0000"`
//!
//! Border character sets can be:
//! - Named presets: `"double"`, `"single"`, `"bold"`, `"round"`
//! - Custom: `{ "tl": "╔", "tr": "╗", "bl": "╚", "br": "╝", "h": "═", "v": "║" }`
//!
//! # Example
//!
//! ```json
//! {
//!   "name": "My Theme",
//!   "desktop": {
//!     "bg": { "fg": "DarkGray", "bg": "#181818" },
//!     "char": " "
//!   },
//!   "borders": {
//!     "window": "double",
//!     "menu": {
//!       "chars": "single",
//!       "sep_l": "├",
//!       "sep_r": "┤"
//!     }
//!   },
//!   "window": { ... },
//!   "dialog": { ... },
//!   "single_frame": { ... },
//!   "menu_bar": { ... },
//!   "menu_box": { ... },
//!   "status_line": { ... },
//!   "button": { ... },
//!   "static_text": { "fg": "Yellow", "bg": "Blue" },
//!   "scrollbar": { ... }
//! }
//! ```

use crate::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Color representation
// ============================================================================

/// A color value that can be serialized to/from JSON.
///
/// Supports named CGA colors and RGB hex values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ColorValue {
    /// Named color: `"Blue"`, `"White"`, `"#1E1E1E"`, etc.
    Named(String),
}

impl ColorValue {
    /// Convert to a ratatui `Color`.
    ///
    /// Returns `None` if the color name is invalid.
    fn to_color(&self) -> Option<Color> {
        let ColorValue::Named(name) = self;
        let name_lower = name.to_lowercase();

        // RGB hex: #RRGGBB
        if let Some(hex) = name.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(Color::Rgb(r, g, b));
            }
            return None;
        }

        // Named CGA colors
        match name_lower.as_str() {
            "black" => Some(Color::Black),
            "blue" => Some(Color::Blue),
            "green" => Some(Color::Green),
            "cyan" => Some(Color::Cyan),
            "red" => Some(Color::Red),
            "magenta" => Some(Color::Magenta),
            "yellow" => Some(Color::Yellow),
            "gray" | "grey" | "lightgray" | "lightgrey" => Some(Color::Gray),
            "darkgray" | "darkgrey" => Some(Color::DarkGray),
            "lightblue" => Some(Color::LightBlue),
            "lightgreen" => Some(Color::LightGreen),
            "lightcyan" => Some(Color::LightCyan),
            "lightred" => Some(Color::LightRed),
            "lightmagenta" => Some(Color::LightMagenta),
            "lightyellow" => Some(Color::LightYellow),
            "white" => Some(Color::White),
            _ => None,
        }
    }

    /// Create from a ratatui `Color`.
    fn from_color(color: Color) -> Self {
        let name = match color {
            Color::Black => "Black".to_owned(),
            Color::Blue => "Blue".to_owned(),
            Color::Green => "Green".to_owned(),
            Color::Cyan => "Cyan".to_owned(),
            Color::Red => "Red".to_owned(),
            Color::Magenta => "Magenta".to_owned(),
            Color::Yellow => "Yellow".to_owned(),
            Color::Gray => "Gray".to_owned(),
            Color::DarkGray => "DarkGray".to_owned(),
            Color::LightBlue => "LightBlue".to_owned(),
            Color::LightGreen => "LightGreen".to_owned(),
            Color::LightCyan => "LightCyan".to_owned(),
            Color::LightRed => "LightRed".to_owned(),
            Color::LightMagenta => "LightMagenta".to_owned(),
            Color::LightYellow => "LightYellow".to_owned(),
            Color::White | Color::Reset | Color::Indexed(_) => "White".to_owned(),
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        };
        ColorValue::Named(name)
    }
}

// ============================================================================
// Style representation
// ============================================================================

/// A style value with foreground, background, and modifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleValue {
    /// Foreground color.
    pub fg: ColorValue,
    /// Background color.
    pub bg: ColorValue,
    /// Bold modifier.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(v: &bool) -> bool {
    !v
}

impl StyleValue {
    /// Convert to a ratatui `Style`.
    fn to_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg.to_color() {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg.to_color() {
            style = style.bg(bg);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }

    /// Create from a ratatui `Style`.
    fn from_style(style: Style) -> Self {
        Self {
            fg: style.fg.map_or_else(
                || ColorValue::Named("White".to_owned()),
                ColorValue::from_color,
            ),
            bg: style.bg.map_or_else(
                || ColorValue::Named("Black".to_owned()),
                ColorValue::from_color,
            ),
            bold: style.add_modifier.contains(Modifier::BOLD),
        }
    }
}

// ============================================================================
// Border character sets
// ============================================================================

/// Named border character set presets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BorderPreset {
    /// Double-line: ╔═╗║╚╝
    Double,
    /// Single-line: ┌─┐│└┘
    Single,
    /// Bold/thick: ┏━┓┃┗┛
    Bold,
    /// Rounded: ╭─╮│╰╯
    Round,
    /// No visible borders (spaces).
    None,
}

impl BorderPreset {
    /// Get the (tl, tr, bl, br, h, v) characters for this preset.
    fn chars(&self) -> (char, char, char, char, char, char) {
        match self {
            Self::Double => ('╔', '╗', '╚', '╝', '═', '║'),
            Self::Single => ('┌', '┐', '└', '┘', '─', '│'),
            Self::Bold => ('┏', '┓', '┗', '┛', '━', '┃'),
            Self::Round => ('╭', '╮', '╰', '╯', '─', '│'),
            Self::None => (' ', ' ', ' ', ' ', ' ', ' '),
        }
    }
}

/// Custom border characters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomBorderChars {
    /// Top-left corner.
    pub tl: String,
    /// Top-right corner.
    pub tr: String,
    /// Bottom-left corner.
    pub bl: String,
    /// Bottom-right corner.
    pub br: String,
    /// Horizontal line.
    pub h: String,
    /// Vertical line.
    pub v: String,
}

/// Border character configuration — preset name or custom characters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BorderChars {
    /// A named preset.
    Preset(BorderPreset),
    /// Custom characters.
    Custom(CustomBorderChars),
}

impl BorderChars {
    /// Get the (tl, tr, bl, br, h, v) characters.
    fn chars(&self) -> (char, char, char, char, char, char) {
        match self {
            Self::Preset(p) => p.chars(),
            Self::Custom(c) => {
                let tl = c.tl.chars().next().unwrap_or('┌');
                let tr = c.tr.chars().next().unwrap_or('┐');
                let bl = c.bl.chars().next().unwrap_or('└');
                let br = c.br.chars().next().unwrap_or('┘');
                let h = c.h.chars().next().unwrap_or('─');
                let v = c.v.chars().next().unwrap_or('│');
                (tl, tr, bl, br, h, v)
            }
        }
    }
}

/// Menu border additional characters (separator junctions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuBorderConfig {
    /// Border character set (preset name or custom chars).
    pub chars: BorderChars,
    /// Separator left junction (default: ├).
    #[serde(default = "default_menu_sep_l")]
    pub sep_l: String,
    /// Separator right junction (default: ┤).
    #[serde(default = "default_menu_sep_r")]
    pub sep_r: String,
}

fn default_menu_sep_l() -> String {
    "├".to_owned()
}

fn default_menu_sep_r() -> String {
    "┤".to_owned()
}

// ============================================================================
// Section models
// ============================================================================

/// Desktop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopSection {
    /// Desktop background style.
    pub bg: StyleValue,
    /// Background character (default: space).
    #[serde(default = "default_space_char")]
    pub char: String,
}

fn default_space_char() -> String {
    " ".to_owned()
}

fn default_close_button_text() -> String {
    "[■]".to_owned()
}

fn default_resize_grip_char() -> String {
    "⋱".to_owned()
}

/// Border configuration for window and menu borders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderSection {
    /// Window border character set.
    pub window: BorderChars,
    /// Menu border character set with separators.
    pub menu: MenuBorderConfig,
}

/// Window styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSection {
    /// Frame border when active.
    pub frame_active: StyleValue,
    /// Frame border when inactive.
    pub frame_inactive: StyleValue,
    /// Title text (active).
    pub title_active: StyleValue,
    /// Title text (inactive).
    pub title_inactive: StyleValue,
    /// Interior background.
    pub interior: StyleValue,
    /// Close button style.
    pub close_button: StyleValue,
    /// Resize handle style.
    pub resize_handle: StyleValue,
    /// Frame during drag/resize.
    pub frame_dragging: StyleValue,
    /// Close button hover style.
    pub close_button_hover: StyleValue,
    /// Resize handle hover style.
    pub resize_handle_hover: StyleValue,
    /// Close button style when window is inactive/unfocused.
    pub close_button_inactive: StyleValue,
    /// Resize handle style when window is inactive/unfocused.
    pub resize_handle_inactive: StyleValue,
    /// Title bar background (overrides frame style for the title row).
    /// If null/missing, no separate title bar background is used.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title_bar_bg: Option<StyleValue>,
    /// Close button text (e.g. "[■]" or " × ").
    #[serde(default = "default_close_button_text")]
    pub close_button_text: String,
    /// Close button alignment: false = left (Borland), true = right (Windows).
    #[serde(default)]
    pub close_button_right: bool,
    /// Resize grip character (default: '⋱').
    #[serde(default = "default_resize_grip_char")]
    pub resize_grip_char: String,
}

/// Dialog styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogSection {
    /// Dialog frame border.
    pub frame: StyleValue,
    /// Dialog title.
    pub title: StyleValue,
    /// Dialog interior.
    pub interior: StyleValue,
}

/// Single frame style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleFrameSection {
    /// Single-line frame border style.
    pub frame: StyleValue,
}

/// Menu bar styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuBarSection {
    /// Normal text.
    pub normal: StyleValue,
    /// Selected item.
    pub selected: StyleValue,
    /// Hotkey character (normal).
    pub hotkey: StyleValue,
    /// Hotkey character (selected).
    pub hotkey_selected: StyleValue,
}

/// Menu dropdown styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuBoxSection {
    /// Normal text.
    pub normal: StyleValue,
    /// Selected item.
    pub selected: StyleValue,
    /// Disabled item.
    pub disabled: StyleValue,
    /// Separator line.
    pub separator: StyleValue,
    /// Hotkey character (normal).
    pub hotkey: StyleValue,
    /// Hotkey character (selected).
    pub hotkey_selected: StyleValue,
}

/// Status line styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLineSection {
    /// Normal text.
    pub normal: StyleValue,
    /// Hotkey character.
    pub hotkey: StyleValue,
    /// Selected/hovered item.
    pub selected: StyleValue,
}

/// Button styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonSection {
    /// Normal state.
    pub normal: StyleValue,
    /// Default button (responds to Enter).
    #[serde(rename = "default")]
    pub default_btn: StyleValue,
    /// Focused/selected.
    pub focused: StyleValue,
    /// Disabled.
    pub disabled: StyleValue,
    /// Hover state.
    pub hover: StyleValue,
}

/// Scrollbar styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollbarSection {
    /// Track (empty area).
    pub track: StyleValue,
    /// Thumb (position indicator).
    pub thumb: StyleValue,
    /// Arrow buttons.
    pub arrows: StyleValue,
    /// Thumb hover style.
    pub thumb_hover: StyleValue,
    /// Arrow buttons hover style.
    pub arrows_hover: StyleValue,
}

// ============================================================================
// Top-level theme data model
// ============================================================================

/// JSON-serializable theme data model.
///
/// This is the top-level structure that maps to a JSON theme file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeData {
    /// Theme name (for display).
    pub name: String,
    /// Desktop configuration.
    pub desktop: DesktopSection,
    /// Border character sets.
    pub borders: BorderSection,
    /// Window styles.
    pub window: WindowSection,
    /// Dialog styles.
    pub dialog: DialogSection,
    /// Single frame style.
    pub single_frame: SingleFrameSection,
    /// Menu bar styles.
    pub menu_bar: MenuBarSection,
    /// Menu dropdown styles.
    pub menu_box: MenuBoxSection,
    /// Status line styles.
    pub status_line: StatusLineSection,
    /// Button styles.
    pub button: ButtonSection,
    /// Static text style.
    pub static_text: StyleValue,
    /// Scrollbar styles.
    pub scrollbar: ScrollbarSection,
}

// ============================================================================
// Conversion: ThemeData → Theme
// ============================================================================

impl ThemeData {
    /// Convert this JSON data model into a `Theme`.
    #[must_use]
    pub fn to_theme(&self) -> Theme {
        let (wtl, wtr, wbl, wbr, wh, wv) = self.borders.window.chars();
        let (mtl, mtr, mbl, mbr, mh, mv) = self.borders.menu.chars.chars();
        let menu_sep_l = self.borders.menu.sep_l.chars().next().unwrap_or('├');
        let menu_sep_r = self.borders.menu.sep_r.chars().next().unwrap_or('┤');

        Theme {
            desktop_bg: self.desktop.bg.to_style(),
            desktop_char: self.desktop.char.chars().next().unwrap_or(' '),

            window_frame_active: self.window.frame_active.to_style(),
            window_frame_inactive: self.window.frame_inactive.to_style(),
            window_title_active: self.window.title_active.to_style(),
            window_title_inactive: self.window.title_inactive.to_style(),
            window_interior: self.window.interior.to_style(),
            window_close_button: self.window.close_button.to_style(),
            window_resize_handle: self.window.resize_handle.to_style(),
            window_frame_dragging: self.window.frame_dragging.to_style(),
            window_close_button_hover: self.window.close_button_hover.to_style(),
            window_resize_handle_hover: self.window.resize_handle_hover.to_style(),
            window_close_button_inactive: self.window.close_button_inactive.to_style(),
            window_resize_handle_inactive: self.window.resize_handle_inactive.to_style(),
            title_bar_bg: self.window.title_bar_bg.as_ref().map(StyleValue::to_style),
            close_button_text: self.window.close_button_text.clone(),
            close_button_right: self.window.close_button_right,
            resize_grip_char: self.window.resize_grip_char.chars().next().unwrap_or('⋱'),

            border_tl: wtl,
            border_tr: wtr,
            border_bl: wbl,
            border_br: wbr,
            border_h: wh,
            border_v: wv,
            menu_border_tl: mtl,
            menu_border_tr: mtr,
            menu_border_bl: mbl,
            menu_border_br: mbr,
            menu_border_h: mh,
            menu_border_v: mv,
            menu_sep_l,
            menu_sep_r,

            dialog_frame: self.dialog.frame.to_style(),
            dialog_title: self.dialog.title.to_style(),
            dialog_interior: self.dialog.interior.to_style(),

            single_frame: self.single_frame.frame.to_style(),

            menu_bar_normal: self.menu_bar.normal.to_style(),
            menu_bar_selected: self.menu_bar.selected.to_style(),
            menu_bar_hotkey: self.menu_bar.hotkey.to_style(),
            menu_bar_hotkey_selected: self.menu_bar.hotkey_selected.to_style(),

            menu_box_normal: self.menu_box.normal.to_style(),
            menu_box_selected: self.menu_box.selected.to_style(),
            menu_box_disabled: self.menu_box.disabled.to_style(),
            menu_box_separator: self.menu_box.separator.to_style(),
            menu_box_hotkey: self.menu_box.hotkey.to_style(),
            menu_box_hotkey_selected: self.menu_box.hotkey_selected.to_style(),

            status_normal: self.status_line.normal.to_style(),
            status_hotkey: self.status_line.hotkey.to_style(),
            status_selected: self.status_line.selected.to_style(),

            button_normal: self.button.normal.to_style(),
            button_default: self.button.default_btn.to_style(),
            button_focused: self.button.focused.to_style(),
            button_disabled: self.button.disabled.to_style(),
            button_hover: self.button.hover.to_style(),

            static_text: self.static_text.to_style(),

            scrollbar_track: self.scrollbar.track.to_style(),
            scrollbar_thumb: self.scrollbar.thumb.to_style(),
            scrollbar_arrows: self.scrollbar.arrows.to_style(),
            scrollbar_thumb_hover: self.scrollbar.thumb_hover.to_style(),
            scrollbar_arrows_hover: self.scrollbar.arrows_hover.to_style(),
        }
    }

    /// Create a `ThemeData` from an existing `Theme`.
    #[must_use]
    pub fn from_theme(theme: &Theme, name: &str) -> Self {
        let window_preset = detect_border_preset(
            theme.border_tl,
            theme.border_tr,
            theme.border_bl,
            theme.border_br,
            theme.border_h,
            theme.border_v,
        );
        let menu_preset = detect_border_preset(
            theme.menu_border_tl,
            theme.menu_border_tr,
            theme.menu_border_bl,
            theme.menu_border_br,
            theme.menu_border_h,
            theme.menu_border_v,
        );

        Self {
            name: name.to_owned(),
            desktop: DesktopSection {
                bg: StyleValue::from_style(theme.desktop_bg),
                char: theme.desktop_char.to_string(),
            },
            borders: BorderSection {
                window: window_preset,
                menu: MenuBorderConfig {
                    chars: menu_preset,
                    sep_l: theme.menu_sep_l.to_string(),
                    sep_r: theme.menu_sep_r.to_string(),
                },
            },
            window: WindowSection {
                frame_active: StyleValue::from_style(theme.window_frame_active),
                frame_inactive: StyleValue::from_style(theme.window_frame_inactive),
                title_active: StyleValue::from_style(theme.window_title_active),
                title_inactive: StyleValue::from_style(theme.window_title_inactive),
                interior: StyleValue::from_style(theme.window_interior),
                close_button: StyleValue::from_style(theme.window_close_button),
                resize_handle: StyleValue::from_style(theme.window_resize_handle),
                frame_dragging: StyleValue::from_style(theme.window_frame_dragging),
                close_button_hover: StyleValue::from_style(theme.window_close_button_hover),
                resize_handle_hover: StyleValue::from_style(theme.window_resize_handle_hover),
                close_button_inactive: StyleValue::from_style(theme.window_close_button_inactive),
                resize_handle_inactive: StyleValue::from_style(theme.window_resize_handle_inactive),
                title_bar_bg: theme.title_bar_bg.map(StyleValue::from_style),
                close_button_text: theme.close_button_text.clone(),
                close_button_right: theme.close_button_right,
                resize_grip_char: theme.resize_grip_char.to_string(),
            },
            dialog: DialogSection {
                frame: StyleValue::from_style(theme.dialog_frame),
                title: StyleValue::from_style(theme.dialog_title),
                interior: StyleValue::from_style(theme.dialog_interior),
            },
            single_frame: SingleFrameSection {
                frame: StyleValue::from_style(theme.single_frame),
            },
            menu_bar: MenuBarSection {
                normal: StyleValue::from_style(theme.menu_bar_normal),
                selected: StyleValue::from_style(theme.menu_bar_selected),
                hotkey: StyleValue::from_style(theme.menu_bar_hotkey),
                hotkey_selected: StyleValue::from_style(theme.menu_bar_hotkey_selected),
            },
            menu_box: MenuBoxSection {
                normal: StyleValue::from_style(theme.menu_box_normal),
                selected: StyleValue::from_style(theme.menu_box_selected),
                disabled: StyleValue::from_style(theme.menu_box_disabled),
                separator: StyleValue::from_style(theme.menu_box_separator),
                hotkey: StyleValue::from_style(theme.menu_box_hotkey),
                hotkey_selected: StyleValue::from_style(theme.menu_box_hotkey_selected),
            },
            status_line: StatusLineSection {
                normal: StyleValue::from_style(theme.status_normal),
                hotkey: StyleValue::from_style(theme.status_hotkey),
                selected: StyleValue::from_style(theme.status_selected),
            },
            button: ButtonSection {
                normal: StyleValue::from_style(theme.button_normal),
                default_btn: StyleValue::from_style(theme.button_default),
                focused: StyleValue::from_style(theme.button_focused),
                disabled: StyleValue::from_style(theme.button_disabled),
                hover: StyleValue::from_style(theme.button_hover),
            },
            static_text: StyleValue::from_style(theme.static_text),
            scrollbar: ScrollbarSection {
                track: StyleValue::from_style(theme.scrollbar_track),
                thumb: StyleValue::from_style(theme.scrollbar_thumb),
                arrows: StyleValue::from_style(theme.scrollbar_arrows),
                thumb_hover: StyleValue::from_style(theme.scrollbar_thumb_hover),
                arrows_hover: StyleValue::from_style(theme.scrollbar_arrows_hover),
            },
        }
    }

    /// Parse from a JSON string.
    ///
    /// # Errors
    /// Returns a serde error if the JSON is invalid or doesn't match the schema.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to a pretty-printed JSON string.
    ///
    /// # Errors
    /// Returns a serde error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load from a JSON file.
    ///
    /// # Errors
    /// Returns `ThemeLoadError::Io` if the file cannot be read,
    /// or `ThemeLoadError::Json` if the content is invalid JSON.
    pub fn load_from_file(path: &Path) -> Result<Self, ThemeLoadError> {
        let content = std::fs::read_to_string(path).map_err(ThemeLoadError::Io)?;
        Self::from_json(&content).map_err(ThemeLoadError::Json)
    }

    /// Save to a JSON file.
    ///
    /// # Errors
    /// Returns `ThemeLoadError::Json` if serialization fails,
    /// or `ThemeLoadError::Io` if the file cannot be written.
    pub fn save_to_file(&self, path: &Path) -> Result<(), ThemeLoadError> {
        let json = self.to_json().map_err(ThemeLoadError::Json)?;
        std::fs::write(path, json).map_err(ThemeLoadError::Io)
    }
}

/// Error type for theme loading operations.
#[derive(Debug)]
pub enum ThemeLoadError {
    /// I/O error reading/writing the file.
    Io(std::io::Error),
    /// JSON parsing error.
    Json(serde_json::Error),
}

impl std::fmt::Display for ThemeLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for ThemeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
        }
    }
}

// ============================================================================
// Helper: detect border preset from characters
// ============================================================================

fn detect_border_preset(tl: char, tr: char, bl: char, br: char, h: char, v: char) -> BorderChars {
    for preset in [
        BorderPreset::Double,
        BorderPreset::Single,
        BorderPreset::Bold,
        BorderPreset::Round,
        BorderPreset::None,
    ] {
        let (ptl, ptr, pbl, pbr, ph, pv) = preset.chars();
        if tl == ptl && tr == ptr && bl == pbl && br == pbr && h == ph && v == pv {
            return BorderChars::Preset(preset);
        }
    }
    BorderChars::Custom(CustomBorderChars {
        tl: tl.to_string(),
        tr: tr.to_string(),
        bl: bl.to_string(),
        br: br.to_string(),
        h: h.to_string(),
        v: v.to_string(),
    })
}

// ============================================================================
// Convenience functions on Theme
// ============================================================================

impl Theme {
    /// Load a theme from a JSON file.
    ///
    /// Only available with the `json-themes` feature.
    ///
    /// # Errors
    /// Returns `ThemeLoadError` if the file cannot be read or parsed.
    pub fn load_json(path: &Path) -> Result<Self, ThemeLoadError> {
        let data = ThemeData::load_from_file(path)?;
        Ok(data.to_theme())
    }

    /// Save the current theme to a JSON file.
    ///
    /// Only available with the `json-themes` feature.
    ///
    /// # Errors
    /// Returns `ThemeLoadError` if serialization or file writing fails.
    pub fn save_json(&self, path: &Path, name: &str) -> Result<(), ThemeLoadError> {
        let data = ThemeData::from_theme(self, name);
        data.save_to_file(path)
    }

    /// Create a theme from a JSON string.
    ///
    /// Only available with the `json-themes` feature.
    ///
    /// # Errors
    /// Returns a serde error if the JSON is invalid or doesn't match the schema.
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        let data = ThemeData::from_json(json)?;
        Ok(data.to_theme())
    }

    /// Serialize this theme to a pretty-printed JSON string.
    ///
    /// Only available with the `json-themes` feature.
    ///
    /// # Errors
    /// Returns a serde error if serialization fails.
    pub fn to_json_str(&self, name: &str) -> Result<String, serde_json::Error> {
        let data = ThemeData::from_theme(self, name);
        data.to_json()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_named_roundtrip() {
        let colors = [
            Color::Black,
            Color::Blue,
            Color::Green,
            Color::Cyan,
            Color::Red,
            Color::Magenta,
            Color::Yellow,
            Color::Gray,
            Color::DarkGray,
            Color::LightBlue,
            Color::LightGreen,
            Color::LightCyan,
            Color::LightRed,
            Color::LightMagenta,
            Color::LightYellow,
            Color::White,
        ];
        for color in colors {
            let cv = ColorValue::from_color(color);
            let back = cv.to_color().expect("Should parse back");
            assert_eq!(back, color, "Roundtrip failed for {color:?}");
        }
    }

    #[test]
    fn test_color_rgb_roundtrip() {
        let cv = ColorValue::Named("#1e1e1e".to_owned());
        assert_eq!(cv.to_color(), Some(Color::Rgb(30, 30, 30)));

        let cv2 = ColorValue::from_color(Color::Rgb(30, 30, 30));
        assert_eq!(cv2, ColorValue::Named("#1e1e1e".to_owned()));
    }

    #[test]
    fn test_border_preset_chars() {
        assert_eq!(BorderPreset::Double.chars(), ('╔', '╗', '╚', '╝', '═', '║'));
        assert_eq!(BorderPreset::Single.chars(), ('┌', '┐', '└', '┘', '─', '│'));
        assert_eq!(BorderPreset::Bold.chars(), ('┏', '┓', '┗', '┛', '━', '┃'));
        assert_eq!(BorderPreset::Round.chars(), ('╭', '╮', '╰', '╯', '─', '│'));
    }

    #[test]
    fn test_border_preset_none() {
        assert_eq!(BorderPreset::None.chars(), (' ', ' ', ' ', ' ', ' ', ' '));

        // Roundtrip serialization
        let json = serde_json::to_string(&BorderPreset::None).expect("Should serialize");
        assert_eq!(json, "\"none\"");
        let parsed: BorderPreset = serde_json::from_str("\"none\"").expect("Should parse");
        assert_eq!(parsed, BorderPreset::None);
    }

    #[test]
    fn test_detect_border_preset() {
        let bc = detect_border_preset('╔', '╗', '╚', '╝', '═', '║');
        assert!(matches!(bc, BorderChars::Preset(BorderPreset::Double)));

        let bc2 = detect_border_preset('╭', '╮', '╰', '╯', '─', '│');
        assert!(matches!(bc2, BorderChars::Preset(BorderPreset::Round)));

        // Unknown chars → Custom
        let bc3 = detect_border_preset('X', 'X', 'X', 'X', 'X', 'X');
        assert!(matches!(bc3, BorderChars::Custom(_)));
    }

    #[test]
    fn test_theme_roundtrip_turbo_vision() {
        let original = Theme::turbo_vision();
        let data = ThemeData::from_theme(&original, "Turbo Vision");
        let json = data.to_json().expect("Should serialize");
        let parsed = ThemeData::from_json(&json).expect("Should parse");
        let restored = parsed.to_theme();

        // Spot-check key fields
        assert_eq!(
            restored.window_frame_active.fg,
            original.window_frame_active.fg
        );
        assert_eq!(
            restored.window_frame_active.bg,
            original.window_frame_active.bg
        );
        assert_eq!(restored.border_tl, original.border_tl);
        assert_eq!(restored.border_v, original.border_v);
        assert_eq!(restored.desktop_char, original.desktop_char);
        assert_eq!(restored.menu_sep_l, original.menu_sep_l);
    }

    #[test]
    fn test_theme_roundtrip_dark() {
        let original = Theme::turbo_vision();
        let data = ThemeData::from_theme(&original, "Turbo Vision");
        let json = data.to_json().expect("Should serialize");
        let parsed = ThemeData::from_json(&json).expect("Should parse");
        let restored = parsed.to_theme();

        assert_eq!(restored.border_tl, '╔'); // Double preset
        assert_eq!(restored.window_interior.fg, original.window_interior.fg);
        assert_eq!(restored.window_interior.bg, original.window_interior.bg);
    }

    #[test]
    fn test_theme_roundtrip_modern() {
        let original = Theme::turbo_vision();
        let data = ThemeData::from_theme(&original, "Turbo Vision");
        let json = data.to_json().expect("Should serialize");
        let parsed = ThemeData::from_json(&json).expect("Should parse");
        let restored = parsed.to_theme();

        assert_eq!(restored.border_tl, '╔'); // Double preset
    }

    #[test]
    fn test_border_preset_serialization() {
        let json = serde_json::to_string(&BorderPreset::Double).expect("Should serialize");
        assert_eq!(json, "\"double\"");

        let parsed: BorderPreset = serde_json::from_str("\"round\"").expect("Should parse");
        assert_eq!(parsed, BorderPreset::Round);
    }

    #[test]
    fn test_style_value_no_bold_omitted() {
        let sv = StyleValue {
            fg: ColorValue::Named("White".to_owned()),
            bg: ColorValue::Named("Blue".to_owned()),
            bold: false,
        };
        let json = serde_json::to_string(&sv).expect("serialize");
        // bold: false should be omitted via skip_serializing_if
        assert!(!json.contains("bold"), "bold:false should be skipped");
    }

    #[test]
    fn test_style_value_bold_included() {
        let sv = StyleValue {
            fg: ColorValue::Named("White".to_owned()),
            bg: ColorValue::Named("Blue".to_owned()),
            bold: true,
        };
        let json = serde_json::to_string(&sv).expect("serialize");
        assert!(json.contains("bold"), "bold:true should be included");
    }
}
