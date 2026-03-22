//! Horizontal bar — unified menu bar and status line widget.
//!
//! Combines the behaviour of [`MenuBar`] and [`StatusLine`] into a single
//! [`HorizontalBar`] struct that can be used at the top *or* bottom of the
//! screen.  Entries are either direct actions (fire a command immediately on
//! click / hotkey) or dropdown triggers (open a bordered menu box).
//!
//! Use the [`HorizontalBar::menu_bar`] constructor for a top-of-screen bar
//! (dropdowns open downward) and [`HorizontalBar::status_line`] for a
//! bottom-of-screen bar (dropdowns open upward).
//!
//! [`MenuBar`]: crate::menu_bar::MenuBar
//! [`StatusLine`]: crate::status_line::StatusLine

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

use crate::command::CommandId;
use crate::menu_bar::MenuItem;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, OF_PRE_PROCESS};

// Re-export DropDirection from overlay so users of horizontal_bar can access it.
pub use crate::overlay::DropDirection;

// ============================================================================
// BarEntry
// ============================================================================

/// A single entry in a horizontal bar.
///
/// Each entry is either a direct action (fires a command immediately) or a
/// dropdown trigger (opens a menu box with sub-items).
#[derive(Debug, Clone)]
pub enum BarEntry {
    /// Direct command — click or hotkey fires immediately.
    ///
    /// Example: `"~Alt+X~ Quit"` in a status bar, or `"~H~elp"` as a
    /// menu bar item without a submenu.
    Action {
        /// Display text with `~X~` hotkey markers.
        label: String,
        /// Command to emit when activated.
        command: CommandId,
        /// Keyboard shortcut code (0 = no shortcut, activated by label hotkey or mouse only).
        key_code: u16,
    },
    /// Dropdown trigger — click or hotkey opens a bordered dropdown box.
    ///
    /// Example: `"~F~ile"` with sub-items New, Open, Save, Quit.
    Dropdown {
        /// Display text with `~X~` hotkey markers.
        label: String,
        /// Items shown in the dropdown.
        items: Vec<MenuItem>,
        /// Keyboard shortcut code (0 = no shortcut, activated by label hotkey only).
        key_code: u16,
    },
}

impl BarEntry {
    /// Return the label text (with `~X~` markers intact).
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Action { label, .. } | Self::Dropdown { label, .. } => label,
        }
    }

    /// Return the key code for this entry.
    #[must_use]
    pub fn key_code(&self) -> u16 {
        match self {
            Self::Action { key_code, .. } | Self::Dropdown { key_code, .. } => *key_code,
        }
    }

    /// Extract the hotkey letter from the `~X~` marker in the label.
    ///
    /// Returns the letter as a lowercase `char`, or `None` if no marker found.
    #[must_use]
    pub fn hotkey(&self) -> Option<char> {
        extract_hotkey(self.label())
    }

    /// Return the display label with all `~X~` markers stripped.
    #[must_use]
    pub fn display_label(&self) -> String {
        strip_hotkey_markers(self.label())
    }
}

// ============================================================================
// Public helper functions
// ============================================================================

/// Extract a hotkey letter from a `~X~` marker in `text`.
///
/// Returns the letter as a lowercase `char`, or `None` if no marker found.
#[must_use]
pub fn extract_hotkey(text: &str) -> Option<char> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'~' {
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
#[must_use]
pub fn strip_hotkey_markers(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '~' {
            let mut found_close = false;
            for inner in chars.by_ref() {
                if inner == '~' {
                    found_close = true;
                    break;
                }
                result.push(inner);
            }
            if !found_close {
                result.push('~');
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parse text with `~X~` markers into segments.
///
/// Returns segments as `(text, highlighted)` tuples.  The text between `~`
/// markers is marked as highlighted (`true`).
///
/// # Examples
///
/// ```
/// # use turbo_tui::horizontal_bar::parse_hotkey_text;
/// let segments = parse_hotkey_text("~F1~ Help");
/// assert_eq!(segments, vec![("F1".to_string(), true), (" Help".to_string(), false)]);
/// ```
#[must_use]
pub fn parse_hotkey_text(text: &str) -> Vec<(String, bool)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_hotkey = false;

    for ch in text.chars() {
        if ch == '~' {
            if !current.is_empty() {
                segments.push((current.clone(), in_hotkey));
                current.clear();
            }
            in_hotkey = !in_hotkey;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        segments.push((current, false));
    }

    segments
}

/// Compute display width of `text`, stripping `~` markers.
///
/// Counts every character that is not a tilde.
#[must_use]
pub fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut in_tilde = false;
    for ch in text.chars() {
        if ch == '~' {
            in_tilde = !in_tilde;
        } else {
            width += 1;
        }
    }
    // Suppress unused variable warning — in_tilde tracks tilde state, result is width
    let _ = in_tilde;
    width
}

/// Compute the visual width of a label (without `~` tilde chars).
fn label_display_width(label: &str) -> usize {
    strip_hotkey_markers(label).chars().count()
}

// ============================================================================
// HorizontalBar
// ============================================================================

/// Unified horizontal bar for menu bars and status lines.
///
/// A single-row bar that displays entries horizontally.  Each entry can be
/// either a direct action (fires a command on click / hotkey) or a dropdown
/// trigger (opens a bordered menu box).
///
/// Use [`menu_bar`] and [`status_line`] constructors for common
/// configurations.
///
/// [`menu_bar`]: HorizontalBar::menu_bar
/// [`status_line`]: HorizontalBar::status_line
pub struct HorizontalBar {
    /// Common view state (carries `OF_PRE_PROCESS`).
    base: ViewBase,
    /// Entries displayed in the bar.
    entries: Vec<BarEntry>,
    /// X start position for each entry (relative to bar origin).
    entry_positions: Vec<u16>,
    /// Direction dropdowns open.
    drop_direction: DropDirection,
    /// Currently hovered entry (when no dropdown is open).
    hovered_entry: Option<usize>,
    /// Which dropdown entry is currently open (`None` = all closed).
    active_dropdown: Option<usize>,
    /// Which item in the open dropdown is highlighted.
    selected_item: Option<usize>,
    /// Right-aligned hint text (typically used in status bars).
    hint_text: Option<String>,
}

impl HorizontalBar {
    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Create a horizontal bar that drops **down** (top-of-screen menu bar).
    ///
    /// The `bounds` should span the full width of the screen and have height 1.
    #[must_use]
    pub fn menu_bar(bounds: Rect, entries: Vec<BarEntry>) -> Self {
        Self::new_with_direction(bounds, entries, DropDirection::Down)
    }

    /// Create a horizontal bar that drops **up** (bottom-of-screen status bar).
    ///
    /// The `bounds` should span the full width of the screen and have height 1.
    #[must_use]
    pub fn status_line(bounds: Rect, entries: Vec<BarEntry>) -> Self {
        Self::new_with_direction(bounds, entries, DropDirection::Up)
    }

    /// Shared constructor — sets up the bar for the given drop direction.
    fn new_with_direction(bounds: Rect, entries: Vec<BarEntry>, direction: DropDirection) -> Self {
        let entry_positions = Self::compute_positions(&entries);
        Self {
            base: ViewBase::with_options(bounds, OF_PRE_PROCESS),
            entries,
            entry_positions,
            drop_direction: direction,
            hovered_entry: None,
            active_dropdown: None,
            selected_item: None,
            hint_text: None,
        }
    }

    // -----------------------------------------------------------------------
    // Public getters
    // -----------------------------------------------------------------------

    /// Return a slice of all entries.
    #[must_use]
    pub fn entries(&self) -> &[BarEntry] {
        &self.entries
    }

    /// Return a slice of the computed entry X positions.
    #[must_use]
    pub fn entry_positions(&self) -> &[u16] {
        &self.entry_positions
    }

    /// Return the direction this bar's dropdowns open.
    #[must_use]
    pub fn drop_direction(&self) -> DropDirection {
        self.drop_direction
    }

    /// Whether any dropdown is currently open.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active_dropdown.is_some()
    }

    /// Index of the currently open dropdown, if any.
    #[must_use]
    pub fn active_dropdown(&self) -> Option<usize> {
        self.active_dropdown
    }

    /// Index of the entry being hovered (when no dropdown is open).
    #[must_use]
    pub fn hovered_entry(&self) -> Option<usize> {
        self.hovered_entry
    }

    /// Set the right-aligned hint text.
    pub fn set_hint(&mut self, hint: Option<String>) {
        self.hint_text = hint;
    }

    /// Get the current hint text.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint_text.as_deref()
    }

    /// Close all open dropdowns and reset hover state.
    pub fn close(&mut self) {
        self.active_dropdown = None;
        self.selected_item = None;
        self.hovered_entry = None;
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Compute the X start position of each entry in the bar.
    ///
    /// Format: ` Label  Label  Label ` (1 leading space + label + 1 trailing
    /// space per entry).
    fn compute_positions(entries: &[BarEntry]) -> Vec<u16> {
        let mut positions = Vec::with_capacity(entries.len());
        let mut x: u16 = 0;
        for entry in entries {
            positions.push(x);
            #[allow(clippy::cast_possible_truncation)]
            let width = label_display_width(entry.label()) as u16 + 2;
            x += width;
        }
        positions
    }

    /// Open the dropdown at `index`.  Does nothing if the entry is not a
    /// `Dropdown` variant or the index is out of bounds.
    fn open_dropdown(&mut self, index: usize) {
        if let Some(BarEntry::Dropdown { .. }) = self.entries.get(index) {
            self.active_dropdown = Some(index);
            self.selected_item = self.first_selectable_item(index);
            self.hovered_entry = None;
        }
    }

    /// Find the first selectable (non-separator, enabled) item in a dropdown.
    fn first_selectable_item(&self, dropdown_idx: usize) -> Option<usize> {
        if let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(dropdown_idx) {
            items
                .iter()
                .position(|item| !item.is_separator() && item.enabled)
        } else {
            None
        }
    }

    /// Move item selection down in the active dropdown, skipping
    /// separators / disabled items with wrap-around.
    fn move_down(&mut self) {
        let Some(dropdown_idx) = self.active_dropdown else {
            return;
        };
        let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(dropdown_idx) else {
            return;
        };
        let current = self.selected_item.unwrap_or(0);
        let next = (current + 1..items.len())
            .find(|&i| !items[i].is_separator() && items[i].enabled)
            .or_else(|| (0..=current).find(|&i| !items[i].is_separator() && items[i].enabled));
        if next.is_some() {
            self.selected_item = next;
        }
    }

    /// Move item selection up in the active dropdown, skipping
    /// separators / disabled items with wrap-around.
    fn move_up(&mut self) {
        let Some(dropdown_idx) = self.active_dropdown else {
            return;
        };
        let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(dropdown_idx) else {
            return;
        };
        let current = self.selected_item.unwrap_or(0);
        let prev = (0..current)
            .rev()
            .find(|&i| !items[i].is_separator() && items[i].enabled)
            .or_else(|| {
                (current..items.len())
                    .rev()
                    .find(|&i| !items[i].is_separator() && items[i].enabled)
            });
        if prev.is_some() {
            self.selected_item = prev;
        }
    }

    /// Switch focus `delta` steps left (`-1`) or right (`+1`), skipping
    /// `Action` entries — only `Dropdown` entries stop the navigation.
    ///
    /// When landing on a `Dropdown`, it is opened automatically.
    fn move_entry(&mut self, delta: isize) {
        // Collect indices of Dropdown entries only (Borland style: arrows
        // skip Action entries when navigating between open menus).
        let dropdown_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if matches!(e, BarEntry::Dropdown { .. }) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if dropdown_indices.is_empty() {
            return;
        }

        // Find the position of the current dropdown in the dropdown-only list.
        let current_pos = self
            .active_dropdown
            .and_then(|idx| dropdown_indices.iter().position(|&d| d == idx))
            .unwrap_or(0);

        #[allow(clippy::cast_possible_wrap)]
        let count = dropdown_indices.len() as isize;
        #[allow(clippy::cast_possible_wrap)]
        #[allow(clippy::cast_sign_loss)]
        let next_pos = ((current_pos as isize + delta).rem_euclid(count)) as usize;
        let next_idx = dropdown_indices[next_pos];
        self.open_dropdown(next_idx);
    }

    /// Return the command for the currently selected dropdown item, if any.
    fn selected_command(&self) -> Option<CommandId> {
        let dropdown_idx = self.active_dropdown?;
        let item_idx = self.selected_item?;
        if let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(dropdown_idx) {
            let item = items.get(item_idx)?;
            if item.enabled && !item.is_separator() {
                return Some(item.command);
            }
        }
        None
    }

    /// Determine which entry (if any) the bar-local column `x` belongs to.
    fn entry_at_column(&self, x: u16) -> Option<usize> {
        for (idx, (&pos, entry)) in self
            .entry_positions
            .iter()
            .zip(self.entries.iter())
            .enumerate()
        {
            #[allow(clippy::cast_possible_truncation)]
            let label_width = label_display_width(entry.label()) as u16;
            let clickable_width = label_width + 2;
            if x >= pos && x < pos + clickable_width {
                return Some(idx);
            }
        }
        None
    }

    /// Determine which dropdown item the absolute (col, row) position hits.
    ///
    /// Returns `Some(item_index)` when the coordinates are inside the dropdown
    /// box, adjusted for [`DropDirection`].
    fn item_at_position(&self, col: u16, row: u16) -> Option<usize> {
        let dropdown_idx = self.active_dropdown?;
        let BarEntry::Dropdown { items, .. } = self.entries.get(dropdown_idx)? else {
            return None;
        };
        let bar_bounds = self.base.bounds();
        let drop_x = bar_bounds.x + self.entry_positions[dropdown_idx];
        let drop_width = Self::dropdown_width(items);
        #[allow(clippy::cast_possible_truncation)]
        let drop_height = items.len() as u16 + 2; // +2 for border rows

        let drop_y = match self.drop_direction {
            DropDirection::Down => bar_bounds.y + 1,
            DropDirection::Up => bar_bounds.y.saturating_sub(drop_height),
        };

        if col < drop_x || col >= drop_x + drop_width {
            return None;
        }
        if row < drop_y || row >= drop_y + drop_height {
            return None;
        }
        // Border rows
        if row == drop_y || row == drop_y + drop_height - 1 {
            return None;
        }
        let inner_row = row - drop_y - 1;
        Some(inner_row as usize)
    }

    /// Compute the visual width of the dropdown box for a set of items.
    fn dropdown_width(items: &[MenuItem]) -> u16 {
        let max_label = items
            .iter()
            .map(|item| label_display_width(&item.label))
            .max()
            .unwrap_or(0);
        // border (2) + space (2) + label + right-padding (1) → at least 6
        #[allow(clippy::cast_possible_truncation)]
        let w = (max_label as u16).saturating_add(4);
        w.max(6)
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    /// Draw the bar row.
    fn draw_bar(&self, buf: &mut Buffer, clip: Rect) {
        let (bar_style, active_style, hotkey_style, active_hotkey_style) =
            theme::with_current(|t| {
                (
                    t.menu_bar_normal,
                    t.menu_bar_selected,
                    t.menu_bar_hotkey,
                    t.menu_bar_hotkey_selected,
                )
            });

        let bounds = self.base.bounds();

        // Fill background
        for x in bounds.x..bounds.x + bounds.width {
            crate::clip::set_string_clipped(buf, x, bounds.y, " ", bar_style, clip);
        }

        for (idx, (entry, &pos)) in self
            .entries
            .iter()
            .zip(self.entry_positions.iter())
            .enumerate()
        {
            let is_active = self.active_dropdown == Some(idx);
            let is_hovered = !self.is_active() && self.hovered_entry == Some(idx);
            let base_style = if is_active || is_hovered {
                active_style
            } else {
                bar_style
            };
            let hk_style = if is_active || is_hovered {
                active_hotkey_style
            } else {
                hotkey_style
            };

            let draw_x = bounds.x + pos;

            // Leading space
            crate::clip::set_string_clipped(buf, draw_x, bounds.y, " ", base_style, clip);

            // Label characters (with hotkey marker handling)
            let mut cur_x = draw_x + 1;
            let mut in_marker = false;
            for ch in entry.label().chars() {
                if ch == '~' {
                    in_marker = !in_marker;
                    continue;
                }
                let style = if in_marker { hk_style } else { base_style };
                crate::clip::set_string_clipped(buf, cur_x, bounds.y, &ch.to_string(), style, clip);
                cur_x += 1;
            }

            // Trailing space
            crate::clip::set_string_clipped(buf, cur_x, bounds.y, " ", base_style, clip);
        }

        // Draw hint text right-aligned (status bar feature)
        if let Some(hint) = &self.hint_text {
            #[allow(clippy::cast_possible_truncation)]
            let hint_len = hint.chars().count() as u16;
            if hint_len < bounds.width {
                let hint_x = bounds.x + bounds.width - hint_len - 1;
                crate::clip::set_string_clipped(buf, hint_x, bounds.y, hint, bar_style, clip);
            }
        }
    }

    /// Draw a horizontal border row for a dropdown.
    fn draw_dropdown_border_row(
        buf: &mut Buffer,
        drop_x: u16,
        row: u16,
        drop_width: u16,
        style: ratatui::style::Style,
        clip: Rect,
        is_top: bool,
    ) {
        let (left, right) = if is_top {
            ("┌", "┐")
        } else {
            ("└", "┘")
        };
        crate::clip::set_string_clipped(buf, drop_x, row, left, style, clip);
        for x in 1..drop_width - 1 {
            crate::clip::set_string_clipped(buf, drop_x + x, row, "─", style, clip);
        }
        crate::clip::set_string_clipped(buf, drop_x + drop_width - 1, row, right, style, clip);
    }

    /// Draw an item row in a dropdown (non-separator).
    #[allow(clippy::too_many_arguments)]
    fn draw_dropdown_item_row(
        buf: &mut Buffer,
        drop_x: u16,
        row: u16,
        drop_width: u16,
        item: &MenuItem,
        is_selected: bool,
        box_style: ratatui::style::Style,
        disabled_style: ratatui::style::Style,
        selected_style: ratatui::style::Style,
        hotkey_style: ratatui::style::Style,
        hotkey_selected_style: ratatui::style::Style,
        clip: Rect,
    ) {
        let (row_style, hk_style) = if is_selected {
            (selected_style, hotkey_selected_style)
        } else if !item.enabled {
            (disabled_style, disabled_style)
        } else {
            (box_style, hotkey_style)
        };

        crate::clip::set_string_clipped(buf, drop_x, row, "│", box_style, clip);
        for x in 1..drop_width - 1 {
            crate::clip::set_string_clipped(buf, drop_x + x, row, " ", row_style, clip);
        }
        crate::clip::set_string_clipped(buf, drop_x + drop_width - 1, row, "│", box_style, clip);

        let mut cur_x = drop_x + 1;
        let mut in_marker = false;
        for ch in item.label.chars() {
            if ch == '~' {
                in_marker = !in_marker;
                continue;
            }
            let style = if in_marker { hk_style } else { row_style };
            crate::clip::set_string_clipped(buf, cur_x, row, &ch.to_string(), style, clip);
            cur_x += 1;
            if cur_x >= drop_x + drop_width - 1 {
                break;
            }
        }
    }

    /// Draw the open dropdown box, positioned according to [`DropDirection`].
    fn draw_dropdown(&self, buf: &mut Buffer, clip: Rect) {
        let Some(dropdown_idx) = self.active_dropdown else {
            return;
        };
        let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(dropdown_idx) else {
            return;
        };
        let Some(&bar_pos) = self.entry_positions.get(dropdown_idx) else {
            return;
        };

        let bounds = self.base.bounds();
        let drop_x = bounds.x + bar_pos;
        let drop_width = Self::dropdown_width(items);
        #[allow(clippy::cast_possible_truncation)]
        let drop_height = items.len() as u16 + 2;

        let drop_y = match self.drop_direction {
            DropDirection::Down => bounds.y + 1,
            DropDirection::Up => bounds.y.saturating_sub(drop_height),
        };

        let (
            box_style,
            border_style,
            selected_style,
            disabled_style,
            hotkey_style,
            hotkey_selected_style,
        ) = theme::with_current(|t| {
            (
                t.menu_box_normal,
                t.menu_box_normal,
                t.menu_box_selected,
                t.menu_box_disabled,
                t.menu_box_hotkey,
                t.menu_box_hotkey_selected,
            )
        });

        // Top border
        Self::draw_dropdown_border_row(buf, drop_x, drop_y, drop_width, border_style, clip, true);

        // Items
        for (item_idx, item) in items.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let row = drop_y + 1 + item_idx as u16;
            let is_selected = self.selected_item == Some(item_idx);

            if item.is_separator() {
                crate::clip::set_string_clipped(buf, drop_x, row, "├", border_style, clip);
                for x in 1..drop_width - 1 {
                    crate::clip::set_string_clipped(buf, drop_x + x, row, "─", border_style, clip);
                }
                crate::clip::set_string_clipped(
                    buf,
                    drop_x + drop_width - 1,
                    row,
                    "┤",
                    border_style,
                    clip,
                );
            } else {
                Self::draw_dropdown_item_row(
                    buf,
                    drop_x,
                    row,
                    drop_width,
                    item,
                    is_selected,
                    box_style,
                    disabled_style,
                    selected_style,
                    hotkey_style,
                    hotkey_selected_style,
                    clip,
                );
            }
        }

        // Bottom border
        let bottom_y = drop_y + drop_height - 1;
        Self::draw_dropdown_border_row(
            buf,
            drop_x,
            bottom_y,
            drop_width,
            border_style,
            clip,
            false,
        );
    }

    // -----------------------------------------------------------------------
    // Event handling
    // -----------------------------------------------------------------------

    /// Handle key-code matching for entries with non-zero `key_code` values.
    fn handle_key_code_match(&mut self, event: &mut Event) {
        for entry in &self.entries {
            let kc = entry.key_code();
            if kc != 0 {
                let matches = if let EventKind::Key(ref k) = event.kind {
                    crate::status_line::key_matches(k, kc)
                } else {
                    false
                };
                if matches {
                    match entry {
                        BarEntry::Action { command, .. } => {
                            let cmd = *command;
                            event.kind = EventKind::Command(cmd);
                            // Leave handled=false so command propagates
                            return;
                        }
                        BarEntry::Dropdown { .. } => {
                            // We need the index to call open_dropdown.
                            // Find by matching key_code again after breaking borrow.
                            break;
                        }
                    }
                }
            }
        }
        // Second pass — open dropdown if a Dropdown entry matched
        // (avoids borrow conflict above)
        let kc_match = if let EventKind::Key(ref k) = event.kind {
            self.entries.iter().position(|e| {
                let kc = e.key_code();
                kc != 0
                    && matches!(e, BarEntry::Dropdown { .. })
                    && crate::status_line::key_matches(k, kc)
            })
        } else {
            None
        };
        if let Some(idx) = kc_match {
            self.open_dropdown(idx);
            event.clear();
        }
    }

    /// Handle a keyboard event.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent, event: &mut Event) {
        match key.code {
            // F10 — toggle: open first dropdown or close if already active
            KeyCode::F(10) => {
                if self.is_active() {
                    self.close();
                } else {
                    // Open the first Dropdown entry
                    let first_dropdown = self
                        .entries
                        .iter()
                        .position(|e| matches!(e, BarEntry::Dropdown { .. }));
                    if let Some(idx) = first_dropdown {
                        self.open_dropdown(idx);
                    }
                }
                event.clear();
            }

            // Escape — close active dropdown
            KeyCode::Esc if self.is_active() => {
                self.close();
                event.clear();
            }

            // Left / Right — switch between dropdown entries (only when active)
            KeyCode::Left if self.is_active() => {
                self.move_entry(-1);
                event.clear();
            }
            KeyCode::Right if self.is_active() => {
                self.move_entry(1);
                event.clear();
            }

            // Up / Down — navigate within the open dropdown
            KeyCode::Up if self.is_active() => {
                self.move_up();
                event.clear();
            }
            KeyCode::Down if self.is_active() => {
                self.move_down();
                event.clear();
            }

            // Enter — select highlighted item and emit its command
            KeyCode::Enter if self.is_active() => {
                if let Some(cmd) = self.selected_command() {
                    self.close();
                    event.kind = EventKind::Command(cmd);
                    event.handled = true;
                }
            }

            // Alt+letter — open matching dropdown or fire matching action
            KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::ALT) => {
                let ch_lower = ch.to_ascii_lowercase();
                let idx = self
                    .entries
                    .iter()
                    .position(|e| e.hotkey() == Some(ch_lower));
                if let Some(entry_idx) = idx {
                    match &self.entries[entry_idx] {
                        BarEntry::Dropdown { .. } => {
                            self.open_dropdown(entry_idx);
                            event.clear();
                        }
                        BarEntry::Action { command, .. } => {
                            let cmd = *command;
                            self.close();
                            event.kind = EventKind::Command(cmd);
                            event.handled = true;
                        }
                    }
                }
            }

            // Key-code matching — iterate entries for matching key codes
            _ => self.handle_key_code_match(event),
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
                    let local_col = col.saturating_sub(bar_bounds.x);
                    if let Some(idx) = self.entry_at_column(local_col) {
                        match &self.entries[idx] {
                            BarEntry::Dropdown { .. } => {
                                if self.active_dropdown == Some(idx) {
                                    self.close();
                                } else {
                                    self.open_dropdown(idx);
                                }
                                event.clear();
                            }
                            BarEntry::Action { command, .. } => {
                                let cmd = *command;
                                self.close();
                                event.kind = EventKind::Command(cmd);
                                event.handled = true;
                            }
                        }
                    }
                } else if self.is_active() {
                    // Click inside the open dropdown
                    if let Some(item_idx) = self.item_at_position(col, row) {
                        // Retrieve command while honouring borrow rules
                        let cmd = if let Some(BarEntry::Dropdown { items, .. }) =
                            self.entries.get(self.active_dropdown.unwrap_or(0))
                        {
                            items
                                .get(item_idx)
                                .filter(|item| item.enabled && !item.is_separator())
                                .map(|item| item.command)
                        } else {
                            None
                        };
                        if let Some(cmd) = cmd {
                            self.close();
                            event.kind = EventKind::Command(cmd);
                            event.handled = true;
                        }
                    } else {
                        // Click outside dropdown and bar — close
                        self.close();
                        event.clear();
                    }
                }
            }

            MouseEventKind::Moved => {
                if self.is_active() {
                    // Hover over dropdown items — update selection
                    if let Some(item_idx) = self.item_at_position(col, row) {
                        let selectable = self.active_dropdown.is_some_and(|d_idx| {
                            if let Some(BarEntry::Dropdown { items, .. }) = self.entries.get(d_idx)
                            {
                                items
                                    .get(item_idx)
                                    .is_some_and(|i| i.enabled && !i.is_separator())
                            } else {
                                false
                            }
                        });
                        if selectable {
                            self.selected_item = Some(item_idx);
                        }
                    }
                    // Switch dropdown on bar-row hover
                    if row == bar_bounds.y {
                        let local_col = col.saturating_sub(bar_bounds.x);
                        if let Some(idx) = self.entry_at_column(local_col) {
                            if self.active_dropdown != Some(idx)
                                && matches!(self.entries.get(idx), Some(BarEntry::Dropdown { .. }))
                            {
                                self.open_dropdown(idx);
                            }
                        }
                    }
                    event.clear();
                } else {
                    // Track hover over entries when no dropdown is open
                    let new_hover = if row == bar_bounds.y {
                        let local_col = col.saturating_sub(bar_bounds.x);
                        self.entry_at_column(local_col)
                    } else {
                        None
                    };
                    if new_hover != self.hovered_entry {
                        self.hovered_entry = new_hover;
                        self.base.mark_dirty();
                    }
                }
            }

            _ => {
                // Consume all other mouse events when a dropdown is open
                // (prevents click-through to windows underneath).
                if self.is_active() {
                    event.clear();
                }
            }
        }
    }
}

// ============================================================================
// View impl
// ============================================================================

impl View for HorizontalBar {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        if self.base.bounds().height == 0 {
            return;
        }
        self.draw_bar(buf, clip);
        if self.active_dropdown.is_some() {
            self.draw_dropdown(buf, clip);
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_CLOSE, CM_NEW, CM_OPEN, CM_QUIT, CM_SAVE};
    use crate::status_line::{KB_F1, KB_F2};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_test_entries() -> Vec<BarEntry> {
        vec![
            BarEntry::Dropdown {
                label: "~F~ile".into(),
                items: vec![
                    MenuItem::new("~N~ew", CM_NEW),
                    MenuItem::new("~O~pen", CM_OPEN),
                    MenuItem::separator(),
                    MenuItem::new("~Q~uit", CM_QUIT),
                ],
                key_code: 0,
            },
            BarEntry::Dropdown {
                label: "~E~dit".into(),
                items: vec![MenuItem::new("~C~ut", 1010), MenuItem::new("~P~aste", 1012)],
                key_code: 0,
            },
            BarEntry::Action {
                label: "~H~elp".into(),
                command: 1030,
                key_code: 0,
            },
        ]
    }

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn make_menu_bar() -> HorizontalBar {
        HorizontalBar::menu_bar(Rect::new(0, 0, 80, 1), make_test_entries())
    }

    // -----------------------------------------------------------------------
    // BarEntry tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bar_entry_action_hotkey() {
        let entry = BarEntry::Action {
            label: "~H~elp".into(),
            command: 1030,
            key_code: 0,
        };
        assert_eq!(entry.hotkey(), Some('h'));
    }

    #[test]
    fn test_bar_entry_dropdown_hotkey() {
        let entry = BarEntry::Dropdown {
            label: "~F~ile".into(),
            items: vec![],
            key_code: 0,
        };
        assert_eq!(entry.hotkey(), Some('f'));
    }

    #[test]
    fn test_bar_entry_display_label() {
        let entry = BarEntry::Action {
            label: "~Alt+X~ Quit".into(),
            command: CM_QUIT,
            key_code: 0,
        };
        assert_eq!(entry.display_label(), "Alt+X Quit");
    }

    // -----------------------------------------------------------------------
    // Helper function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_hotkey() {
        assert_eq!(extract_hotkey("~F~ile"), Some('f'));
        assert_eq!(extract_hotkey("~E~dit"), Some('e'));
        assert_eq!(extract_hotkey("No hotkey"), None);
        assert_eq!(extract_hotkey("~O~pen  F3"), Some('o'));
        assert_eq!(extract_hotkey(""), None);
    }

    #[test]
    fn test_strip_hotkey_markers() {
        assert_eq!(strip_hotkey_markers("~F~ile"), "File");
        assert_eq!(strip_hotkey_markers("~E~dit"), "Edit");
        assert_eq!(strip_hotkey_markers("~O~pen  F3"), "Open  F3");
        assert_eq!(strip_hotkey_markers("No markers"), "No markers");
        assert_eq!(strip_hotkey_markers(""), "");
    }

    #[test]
    fn test_parse_hotkey_text() {
        let segments = parse_hotkey_text("~F1~ Help");
        assert_eq!(
            segments,
            vec![("F1".to_string(), true), (" Help".to_string(), false)]
        );

        let segments = parse_hotkey_text("No Hotkey");
        assert_eq!(segments, vec![("No Hotkey".to_string(), false)]);

        let segments = parse_hotkey_text("~Alt~+~X~");
        assert_eq!(
            segments,
            vec![
                ("Alt".to_string(), true),
                ("+".to_string(), false),
                ("X".to_string(), true),
            ]
        );
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("~F1~ Help"), 7);
        assert_eq!(display_width("No hotkey"), 9);
        assert_eq!(display_width("~F1~~F2~"), 4);
        assert_eq!(display_width(""), 0);
    }

    // -----------------------------------------------------------------------
    // Constructor tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_menu_bar_new() {
        let bar = make_menu_bar();
        assert_eq!(bar.entries().len(), 3);
        assert_eq!(bar.drop_direction(), DropDirection::Down);
        assert!(!bar.is_active());
        assert!(bar.hint().is_none());
    }

    #[test]
    fn test_status_line_new() {
        let entries = vec![BarEntry::Action {
            label: "~F1~ Help".into(),
            command: CM_CLOSE,
            key_code: KB_F1,
        }];
        let bar = HorizontalBar::status_line(Rect::new(0, 23, 80, 1), entries);
        assert_eq!(bar.drop_direction(), DropDirection::Up);
        assert!(!bar.is_active());
    }

    // -----------------------------------------------------------------------
    // Position calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_positions() {
        let bar = make_menu_bar();
        let positions = bar.entry_positions();
        // "~F~ile" → "File" = 4 chars + 2 = 6 wide; starts at 0
        assert_eq!(positions[0], 0);
        // "~E~dit" → "Edit" = 4 chars + 2 = 6 wide; starts at 6
        assert_eq!(positions[1], 6);
        // "~H~elp" → "Help" = 4 chars + 2 = 6 wide; starts at 12
        assert_eq!(positions[2], 12);
    }

    // -----------------------------------------------------------------------
    // Open / close tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_close_dropdown() {
        let mut bar = make_menu_bar();
        assert!(!bar.is_active());

        bar.open_dropdown(0);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));

        bar.close();
        assert!(!bar.is_active());
        assert_eq!(bar.active_dropdown(), None);
    }

    #[test]
    fn test_open_dropdown_on_action_entry_does_nothing() {
        let mut bar = make_menu_bar();
        // Entry 2 is an Action — opening it should be a no-op
        bar.open_dropdown(2);
        assert!(!bar.is_active());
    }

    // -----------------------------------------------------------------------
    // Keyboard tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_f10_toggles() {
        let mut bar = make_menu_bar();

        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert!(event.is_cleared());

        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_cleared());
    }

    #[test]
    fn test_escape_closes() {
        let mut bar = make_menu_bar();
        bar.open_dropdown(0);
        assert!(bar.is_active());

        let mut event = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_cleared());
    }

    #[test]
    fn test_enter_selects_item() {
        let mut bar = make_menu_bar();
        bar.open_dropdown(0);
        // First selectable item in "File" dropdown is CM_NEW (index 0)
        assert_eq!(bar.selected_item, Some(0));

        let mut event = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        bar.handle_event(&mut event);

        assert!(!bar.is_active());
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_NEW));
    }

    #[test]
    fn test_arrow_navigation() {
        let mut bar = make_menu_bar();
        bar.open_dropdown(0);
        assert_eq!(bar.active_dropdown(), Some(0));

        // Right → switch to dropdown 1 (Edit)
        let mut event = make_key_event(KeyCode::Right, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_eq!(bar.active_dropdown(), Some(1));
        assert!(event.is_cleared());

        // Left → back to dropdown 0 (File)
        let mut event = make_key_event(KeyCode::Left, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_eq!(bar.active_dropdown(), Some(0));
        assert!(event.is_cleared());

        // Down → move to next item (from index 0 to index 1)
        let initial_item = bar.selected_item;
        let mut event = make_key_event(KeyCode::Down, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_ne!(bar.selected_item, initial_item);
        assert!(event.is_cleared());

        // Up → move back
        let after_down = bar.selected_item;
        let mut event = make_key_event(KeyCode::Up, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_ne!(bar.selected_item, after_down);
        assert!(event.is_cleared());
    }

    #[test]
    fn test_alt_letter_opens_dropdown() {
        let mut bar = make_menu_bar();

        let mut event = make_key_event(KeyCode::Char('f'), KeyModifiers::ALT);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));
        assert!(event.is_cleared());
    }

    #[test]
    fn test_alt_letter_fires_action() {
        let mut bar = make_menu_bar();

        // Alt+H should fire the Help action (entry 2)
        let mut event = make_key_event(KeyCode::Char('h'), KeyModifiers::ALT);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(1030));
    }

    // -----------------------------------------------------------------------
    // Mouse tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_mouse_click_opens_dropdown() {
        let mut bar = make_menu_bar();

        // Click on the first entry (x=0, y=0) — "File"
        let mut event = Event::mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1, // inside first entry (leading-space at 0, label at 1..5, trailing at 5)
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));
    }

    #[test]
    fn test_mouse_click_fires_action() {
        // Bar with a single Action entry at x=0
        let entries = vec![BarEntry::Action {
            label: "~Q~uit".into(),
            command: CM_QUIT,
            key_code: 0,
        }];
        let mut bar = HorizontalBar::menu_bar(Rect::new(0, 0, 40, 1), entries);

        let mut event = Event::mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1, // inside the "Quit" entry
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_QUIT));
    }

    // -----------------------------------------------------------------------
    // DropDirection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_dropdown_direction_down() {
        // Down: dropdown row = bar.y + 1 = 1
        let mut bar = make_menu_bar(); // y=0, direction=Down
        bar.open_dropdown(0);
        // dropdown items start at row 2 (row 1 = top border, row 2 = first item)
        let hit = bar.item_at_position(1, 2); // col inside dropdown, row = first item
        assert!(hit.is_some());
        assert_eq!(hit, Some(0));
    }

    #[test]
    fn test_dropdown_direction_up() {
        // Up: bar at y=10, dropdown opens above
        let entries = vec![BarEntry::Dropdown {
            label: "~F~ile".into(),
            items: vec![
                MenuItem::new("~N~ew", CM_NEW),
                MenuItem::new("~O~pen", CM_OPEN),
            ],
            key_code: 0,
        }];
        let mut bar = HorizontalBar::status_line(Rect::new(0, 10, 80, 1), entries);
        bar.open_dropdown(0);
        // drop_height = 2 items + 2 border = 4
        // drop_y = 10 - 4 = 6
        // First item row = drop_y + 1 = 7
        let hit = bar.item_at_position(1, 7);
        assert!(hit.is_some());
        assert_eq!(hit, Some(0));
    }

    // -----------------------------------------------------------------------
    // Hint text
    // -----------------------------------------------------------------------

    #[test]
    fn test_hint_text() {
        let mut bar = make_menu_bar();
        assert!(bar.hint().is_none());

        bar.set_hint(Some("Ready".into()));
        assert_eq!(bar.hint(), Some("Ready"));

        bar.set_hint(None);
        assert!(bar.hint().is_none());
    }

    // -----------------------------------------------------------------------
    // Draw test
    // -----------------------------------------------------------------------

    #[test]
    fn test_draw_bar() {
        let bar = make_menu_bar();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        bar.draw(&mut buf, area);

        // "File" starts at column 1 (column 0 = leading space)
        assert_eq!(buf[(0, 0)].symbol(), " "); // leading space
        assert_eq!(buf[(1, 0)].symbol(), "F");
        assert_eq!(buf[(2, 0)].symbol(), "i");
        assert_eq!(buf[(3, 0)].symbol(), "l");
        assert_eq!(buf[(4, 0)].symbol(), "e");

        // "Edit" starts at column 7 (column 6 = leading space)
        assert_eq!(buf[(6, 0)].symbol(), " ");
        assert_eq!(buf[(7, 0)].symbol(), "E");
        assert_eq!(buf[(8, 0)].symbol(), "d");
        assert_eq!(buf[(9, 0)].symbol(), "i");
        assert_eq!(buf[(10, 0)].symbol(), "t");
    }

    // -----------------------------------------------------------------------
    // Key-code matching (status-line style)
    // -----------------------------------------------------------------------

    #[test]
    fn test_action_key_code_match() {
        let entries = vec![BarEntry::Action {
            label: "~F1~ Help".into(),
            command: CM_CLOSE,
            key_code: KB_F1,
        }];
        let mut bar = HorizontalBar::status_line(Rect::new(0, 23, 80, 1), entries);

        let mut event = Event::key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        bar.handle_event(&mut event);
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_CLOSE));
    }

    #[test]
    fn test_dropdown_key_code_match() {
        let entries = vec![BarEntry::Dropdown {
            label: "~F~ile".into(),
            items: vec![MenuItem::new("~S~ave", CM_SAVE)],
            key_code: KB_F2,
        }];
        let mut bar = HorizontalBar::menu_bar(Rect::new(0, 0, 80, 1), entries);

        let mut event = Event::key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));
        assert!(event.is_cleared());
    }
}
