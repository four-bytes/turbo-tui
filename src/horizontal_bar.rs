//! Horizontal bar — unified menu bar and status bar widget.
//!
//! Combines the behaviour of [`MenuBar`] and [`StatusBar`] into a single
//! [`HorizontalBar`] struct that can be used at the top *or* bottom of the
//! screen.  Entries are either direct actions (fire a command immediately on
//! click / hotkey) or dropdown triggers (open a bordered menu box).
//!
//! Use the [`HorizontalBar::menu_bar`] constructor for a top-of-screen bar
//! (dropdowns open downward) and [`HorizontalBar::status_bar`] for a
//! bottom-of-screen bar (dropdowns open upward).
//!
//! [`MenuBar`]: crate::menu_bar::MenuBar
//! [`StatusBar`]: crate::status_bar::StatusBar

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

use crate::command::{CommandId, CM_DROPDOWN_CLOSED, CM_OPEN_DROPDOWN};
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
/// trigger (opens a bordered menu box via `OverlayManager`).
///
/// Use [`menu_bar`] and [`status_bar`] constructors for common
/// configurations.
///
/// [`menu_bar`]: HorizontalBar::menu_bar
/// [`status_bar`]: HorizontalBar::status_bar
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
    /// Pending dropdown request: the index Application should open as overlay.
    /// Set by `request_dropdown()`, consumed by `Application` via `take_pending_dropdown()`.
    pending_dropdown: Option<usize>,
    /// Pending navigate direction: -1 (left) or +1 (right).
    /// Set when Left/Right is pressed while dropdown active, consumed by Application.
    pending_navigate_direction: Option<isize>,
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
    pub fn status_bar(bounds: Rect, entries: Vec<BarEntry>) -> Self {
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
            pending_dropdown: None,
            pending_navigate_direction: None,
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
        self.hovered_entry = None;
        self.pending_dropdown = None;
        self.pending_navigate_direction = None;
    }

    // -----------------------------------------------------------------------
    // Application coordination API
    // -----------------------------------------------------------------------

    /// Take the pending dropdown index. Returns `Some(index)` if a dropdown
    /// should be opened, then clears the pending state.
    pub fn take_pending_dropdown(&mut self) -> Option<usize> {
        self.pending_dropdown.take()
    }

    /// Take the pending navigation direction. Returns `Some(delta)` if
    /// Left/Right was pressed, then clears the pending state.
    pub fn take_pending_navigate(&mut self) -> Option<isize> {
        self.pending_navigate_direction.take()
    }

    /// Get the items for a dropdown at `index`. Returns `None` if the
    /// index is out of range or the entry is not a Dropdown.
    #[must_use]
    pub fn dropdown_items_for(&self, index: usize) -> Option<&[MenuItem]> {
        match self.entries.get(index)? {
            BarEntry::Dropdown { items, .. } => Some(items),
            BarEntry::Action { .. } => None,
        }
    }

    /// Get the anchor point for positioning a dropdown overlay at `index`.
    /// Returns `(x, y)` where:
    /// - `x` = absolute X position of the entry on screen
    /// - `y` = the bar row (for `DropDirection` calculation)
    #[must_use]
    pub fn dropdown_anchor(&self, index: usize) -> Option<(u16, u16)> {
        let pos = self.entry_positions.get(index)?;
        let bounds = self.base.bounds();
        Some((bounds.x + pos, bounds.y))
    }

    /// Switch to the next/previous dropdown entry.
    /// Called by Application in response to `CM_DROPDOWN_NAVIGATE`.
    pub fn navigate_dropdown(&mut self, delta: isize, event: &mut Event) {
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
        self.request_dropdown(next_idx, event);
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

    /// Open the dropdown at `index`. Sets `pending_dropdown` so Application
    /// can create a `MenuBox` overlay. Posts `CM_OPEN_DROPDOWN` as a deferred event.
    fn request_dropdown(&mut self, index: usize, event: &mut Event) {
        if let Some(BarEntry::Dropdown { .. }) = self.entries.get(index) {
            self.active_dropdown = Some(index);
            self.hovered_entry = None;
            self.pending_dropdown = Some(index);
            event.post(Event::command(CM_OPEN_DROPDOWN));
        }
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

    // -----------------------------------------------------------------------
    // Event handling
    // -----------------------------------------------------------------------

    /// Handle key-code matching for entries with non-zero `key_code` values.
    fn handle_key_code_match(&mut self, event: &mut Event) {
        for entry in &self.entries {
            let kc = entry.key_code();
            if kc != 0 {
                let matches = if let EventKind::Key(ref k) = event.kind {
                    crate::status_bar::key_matches(k, kc)
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
                            // We need the index to call request_dropdown.
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
                    && crate::status_bar::key_matches(k, kc)
            })
        } else {
            None
        };
        if let Some(idx) = kc_match {
            self.request_dropdown(idx, event);
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
                    event.post(Event::command(CM_DROPDOWN_CLOSED));
                } else {
                    // Open the first Dropdown entry
                    let first_dropdown = self
                        .entries
                        .iter()
                        .position(|e| matches!(e, BarEntry::Dropdown { .. }));
                    if let Some(idx) = first_dropdown {
                        self.request_dropdown(idx, event);
                    }
                }
                event.clear();
            }

            // Escape — close active dropdown (also handled by OverlayManager,
            // but HorizontalBar must reset its own state)
            KeyCode::Esc if self.is_active() => {
                self.close();
                event.post(Event::command(CM_DROPDOWN_CLOSED));
                event.clear();
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
                            self.request_dropdown(entry_idx, event);
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
                                    event.post(Event::command(CM_DROPDOWN_CLOSED));
                                } else {
                                    self.request_dropdown(idx, event);
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
                }
                // Clicks outside bar when active — don't handle here.
                // OverlayManager dismiss-on-outside-click handles this.
            }

            MouseEventKind::Moved => {
                if self.is_active() {
                    // Switch dropdown on bar-row hover
                    if row == bar_bounds.y {
                        let local_col = col.saturating_sub(bar_bounds.x);
                        if let Some(idx) = self.entry_at_column(local_col) {
                            if self.active_dropdown != Some(idx)
                                && matches!(self.entries.get(idx), Some(BarEntry::Dropdown { .. }))
                            {
                                self.request_dropdown(idx, event);
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
        // Dropdown is now rendered by OverlayManager, not self-drawn
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
    use crate::status_bar::{KB_F1, KB_F2};
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
    fn test_status_bar_new() {
        let entries = vec![BarEntry::Action {
            label: "~F1~ Help".into(),
            command: CM_CLOSE,
            key_code: KB_F1,
        }];
        let bar = HorizontalBar::status_bar(Rect::new(0, 23, 80, 1), entries);
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

        // Use F10 to open (request_dropdown requires event param)
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));

        bar.close();
        assert!(!bar.is_active());
        assert_eq!(bar.active_dropdown(), None);
    }

    #[test]
    fn test_open_dropdown_on_action_entry_does_nothing() {
        let mut bar = make_menu_bar();
        // Entry 2 is an Action — requesting it should be a no-op
        let mut event = Event::default();
        bar.request_dropdown(2, &mut event);
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
        // Open via F10
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());

        let mut event = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
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
        let mut bar = HorizontalBar::status_bar(Rect::new(0, 23, 80, 1), entries);

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
        // Should have posted CM_OPEN_DROPDOWN
        assert!(event
            .deferred
            .iter()
            .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_OPEN_DROPDOWN)));
    }

    // -----------------------------------------------------------------------
    // New deferred-event / Application-coordination tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_request_dropdown_posts_command() {
        let mut bar = make_menu_bar();
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        assert_eq!(bar.active_dropdown(), Some(0));
        // Should have posted CM_OPEN_DROPDOWN
        assert!(event
            .deferred
            .iter()
            .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_OPEN_DROPDOWN)));
        // Should have pending dropdown
        assert_eq!(bar.take_pending_dropdown(), Some(0));
    }

    #[test]
    fn test_close_posts_dropdown_closed() {
        let mut bar = make_menu_bar();
        // Open first
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        // Press F10 again to close
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event
            .deferred
            .iter()
            .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_DROPDOWN_CLOSED)));
    }

    #[test]
    fn test_dropdown_items_for() {
        let bar = make_menu_bar();
        let items = bar.dropdown_items_for(0);
        assert!(items.is_some());
        assert!(!items.unwrap().is_empty());
        // Action entry should return None
        assert!(bar.dropdown_items_for(2).is_none());
        // Out of range
        assert!(bar.dropdown_items_for(99).is_none());
    }

    #[test]
    fn test_dropdown_anchor() {
        let bar = make_menu_bar();
        let anchor = bar.dropdown_anchor(0);
        assert!(anchor.is_some());
        let (x, y) = anchor.unwrap();
        assert_eq!(y, 0); // bar at y=0
        assert_eq!(x, 0); // first entry at x=0
    }

    #[test]
    fn test_navigate_dropdown() {
        let mut bar = make_menu_bar();
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert_eq!(bar.active_dropdown(), Some(0));
        // Navigate right
        let mut event = Event::default();
        bar.navigate_dropdown(1, &mut event);
        assert_eq!(bar.active_dropdown(), Some(1)); // Edit dropdown
        assert!(event
            .deferred
            .iter()
            .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_OPEN_DROPDOWN)));
    }

    #[test]
    fn test_escape_when_active_posts_closed() {
        let mut bar = make_menu_bar();
        let mut event = make_key_event(KeyCode::F(10), KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(bar.is_active());
        let mut event = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        bar.handle_event(&mut event);
        assert!(!bar.is_active());
        assert!(event
            .deferred
            .iter()
            .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_DROPDOWN_CLOSED)));
    }
}
