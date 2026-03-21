//! Status Line — context-sensitive status bar with clickable shortcuts.
//!
//! The status bar displays at the bottom of the screen and shows keyboard
//! shortcuts that can be activated by mouse click or hotkey. Uses `~X~`
//! markers to highlight hotkey letters.

use crate::command::CommandId;
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, OF_PRE_PROCESS};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;
use std::cell::RefCell;

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
// StatusLine
// ============================================================================

/// Context-sensitive status bar with clickable shortcuts.
///
/// Displays at the bottom of the screen with keyboard shortcuts. Items
/// can be activated by:
///
/// - Mouse click
/// - Hotkey (F-keys or other shortcuts)
///
/// Uses `OF_PRE_PROCESS` to intercept key events before the focused view.
///
/// # Example
///
/// ```ignore
/// use four_turbo_tui::status_line::{StatusLine, StatusItem, KB_F1};
/// use four_turbo_tui::command::CM_CLOSE;
///
/// let items = vec![
///     StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
///     StatusItem::new("~F2~ Open", CM_OPEN, KB_F2),
/// ];
///
/// let mut status = StatusLine::new(Rect::new(0, 23, 80, 1), items);
/// status.set_hint(Some("Ready".into()));
/// ```
#[derive(Debug, Clone)]
pub struct StatusLine {
    /// Base view implementation (has `OF_PRE_PROCESS`).
    base: ViewBase,
    /// Status items.
    items: Vec<StatusItem>,
    /// (`start_x`, `end_x`) for each item — computed on draw.
    item_positions: RefCell<Vec<(u16, u16)>>,
    /// Right-aligned context text.
    hint_text: Option<String>,
    /// Currently hovered item (for mouse highlight).
    hovered_item: Option<usize>,
}

impl StatusLine {
    /// Create a new status line with the given items.
    ///
    /// The bounds should typically be a single row at the bottom of the screen.
    #[must_use]
    pub fn new(bounds: Rect, items: Vec<StatusItem>) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_PRE_PROCESS),
            items,
            item_positions: RefCell::new(Vec::new()),
            hint_text: None,
            hovered_item: None,
        }
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

    /// Get the status items.
    #[must_use]
    pub fn items(&self) -> &[StatusItem] {
        &self.items
    }

    /// Find which item (if any) contains the given x coordinate.
    #[must_use]
    fn item_at_x(&self, x: u16) -> Option<usize> {
        let positions = self.item_positions.borrow();
        for (idx, &(start, end)) in positions.iter().enumerate() {
            if x >= start && x < end {
                return Some(idx);
            }
        }
        None
    }

    /// Draw the status line to the buffer.
    fn draw_status(&self, buf: &mut Buffer, area: Rect) {
        // Get theme styles
        let (style, hotkey_style, selected_style) = theme::with_current(|t| {
            (t.status_normal, t.status_hotkey, t.status_selected)
        });

        // Clear the line
        for x in area.left()..area.right() {
            buf[(x, area.y)].set_style(style);
        }

        let mut x = area.x;
        self.item_positions.borrow_mut().clear();

        // Draw each item
        for (idx, item) in self.items.iter().enumerate() {
            let segments = parse_hotkey_text(&item.text);
            let start_x = x;

            // Hover style check
            let is_hovered = self.hovered_item == Some(idx);

            for (text, highlighted) in &segments {
                let seg_style = if is_hovered {
                    selected_style
                } else if *highlighted {
                    hotkey_style
                } else {
                    style
                };

                buf.set_string(x, area.y, text, seg_style);
                x += u16::try_from(text.len()).unwrap_or(u16::MAX);
            }

            // Add space between items
            if idx < self.items.len() - 1 {
                buf.set_string(x, area.y, "  ", style);
                x += 2;
            }

            let end_x = x;
            self.item_positions.borrow_mut().push((start_x, end_x));
        }

        // Draw hint text right-aligned
        if let Some(hint) = &self.hint_text {
            let hint_len = u16::try_from(hint.len()).unwrap_or(u16::MAX);
            if hint_len < area.width {
                let hint_x = area.x + area.width - hint_len - 1;
                buf.set_string(hint_x, area.y, hint, style);
            }
        }
    }
}

impl View for StatusLine {
    fn id(&self) -> crate::view::ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        self.draw_status(buf, area);
    }

    fn handle_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Key(key) => {
                // Check if any item's key_code matches
                for item in &self.items {
                    if item.key_code != 0 && key_matches(key, item.key_code) {
                        event.kind = EventKind::Command(item.command);
                        return; // Don't clear — let command propagate
                    }
                }
            }
            EventKind::Mouse(mouse) => {
                let bounds = self.base.bounds();

                // Only handle if mouse is on our row
                let mouse_in_row = mouse.row >= bounds.y && mouse.row < bounds.y + bounds.height;

                if !mouse_in_row {
                    return;
                }

                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        // Find which item was clicked
                        if let Some(idx) = self.item_at_x(mouse.column) {
                            event.kind = EventKind::Command(self.items[idx].command);
                            event.handled = true;
                        }
                    }
                    MouseEventKind::Moved => {
                        self.hovered_item = self.item_at_x(mouse.column);
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        // Clear hover on mouse up if outside
                        if self.item_at_x(mouse.column).is_none() {
                            self.hovered_item = None;
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

    fn options(&self) -> u16 {
        self.base.options() | OF_PRE_PROCESS
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Check if a key event matches the given key code.
///
/// Maps F-key codes to `KeyCode::F(n)` values.
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
/// Returns segments with (text, highlighted) tuples.
/// The text between `~` markers is marked as highlighted.
///
/// # Examples
///
/// ```
/// # use turbo_tui::status_line::parse_hotkey_text;
/// let segments = parse_hotkey_text("~F1~ Help");
/// assert_eq!(segments, vec![("F1".to_string(), true), (" Help".to_string(), false)]);
/// ```
#[must_use]
pub fn parse_hotkey_text(text: &str) -> Vec<(String, bool)> {
    let mut segments = Vec::new();
    let chars = text.chars();
    let mut current = String::new();
    let mut in_hotkey = false;

    for ch in chars {
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

    // Handle trailing text after last `~`
    if !current.is_empty() {
        segments.push((current, false));
    }

    segments
}

/// Compute display width, stripping `~` markers.
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

    width
}

/// Compute item positions starting at a given x offset.
///
/// Returns (`start_x`, `end_x`) for each item.
#[must_use]
pub fn compute_positions(items: &[StatusItem], start_x: u16) -> Vec<(u16, u16)> {
    let mut positions = Vec::new();
    let mut x = start_x;

    for (idx, item) in items.iter().enumerate() {
        let width = u16::try_from(display_width(&item.text)).unwrap_or(u16::MAX);
        let start = x;
        let end = if idx < items.len() - 1 {
            x + width + 2 // Add 2 for separator
        } else {
            x + width
        };

        positions.push((start, end));
        x = end;
    }

    positions
}

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
    fn test_status_line_new() {
        let items = vec![StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1)];
        let status = StatusLine::new(Rect::new(0, 23, 80, 1), items);

        assert_eq!(status.items().len(), 1);
        assert!(status.hint().is_none());
    }

    #[test]
    fn test_status_line_hint() {
        let items = vec![StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1)];
        let mut status = StatusLine::new(Rect::new(0, 23, 80, 1), items);

        assert!(status.hint().is_none());

        status.set_hint(Some("Ready".into()));
        assert_eq!(status.hint(), Some("Ready"));

        status.set_hint(None);
        assert!(status.hint().is_none());
    }

    #[test]
    fn test_parse_hotkey_text() {
        // Simple hotkey
        let segments = parse_hotkey_text("~F1~ Help");
        assert_eq!(
            segments,
            vec![("F1".to_string(), true), (" Help".to_string(), false),]
        );

        // No hotkey
        let segments = parse_hotkey_text("No Hotkey");
        assert_eq!(segments, vec![("No Hotkey".to_string(), false),]);

        // Multiple hotkeys
        let segments = parse_hotkey_text("~Alt~+~X~");
        assert_eq!(
            segments,
            vec![
                ("Alt".to_string(), true),
                ("+".to_string(), false),
                ("X".to_string(), true),
            ]
        );

        // Hotkey at end
        let segments = parse_hotkey_text("Open ~File~");
        assert_eq!(
            segments,
            vec![("Open ".to_string(), false), ("File".to_string(), true),]
        );
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("~F1~ Help"), 7);
        assert_eq!(display_width("No hotkey"), 9);
        assert_eq!(display_width("~F1~~F2~"), 4);
    }

    #[test]
    fn test_key_matches_f_keys() {
        // F1
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert!(key_matches(&key, KB_F1));
        assert!(!key_matches(&key, KB_F2));

        // F10
        let key = KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE);
        assert!(key_matches(&key, KB_F10));

        // F12
        let key = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
        assert!(key_matches(&key, KB_F12));
    }

    #[test]
    fn test_key_matches_alt() {
        // Alt+X
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert!(key_matches(&key, KB_ALT_X));

        let key = KeyEvent::new(KeyCode::Char('X'), KeyModifiers::ALT);
        assert!(key_matches(&key, KB_ALT_X));

        // Regular X without Alt
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert!(!key_matches(&key, KB_ALT_X));
    }

    #[test]
    fn test_compute_positions() {
        let items = vec![
            StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
            StatusItem::new("~F2~ Open", CM_CLOSE, KB_F2),
        ];

        let positions = compute_positions(&items, 0);
        assert_eq!(positions.len(), 2);
        // "~F1~ Help" = 7 display chars, with 2 space separator = 9
        assert_eq!(positions[0], (0, 9));
        // "~F2~ Open" = 7 display chars, no separator (last item)
        // Start at 9, end at 16
        assert_eq!(positions[1], (9, 16));
    }

    #[test]
    fn test_status_line_has_pre_process() {
        let items = vec![StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1)];
        let status = StatusLine::new(Rect::new(0, 23, 80, 1), items);

        assert_ne!(status.options() & OF_PRE_PROCESS, 0);
    }

    #[test]
    fn test_status_line_draw() {
        let items = vec![StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1)];
        let mut status = StatusLine::new(Rect::new(0, 0, 20, 1), items);
        status.set_hint(Some("Ready".into()));

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        status.draw(&mut buf, Rect::new(0, 0, 20, 1));

        // Check that text was written
        let content: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("F1"));
        assert!(content.contains("Help"));
        assert!(content.contains("Ready"));
    }

    #[test]
    fn test_status_line_click() {
        let items = vec![
            StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
            StatusItem::new("~F2~ Open", CM_CLOSE, KB_F2),
        ];
        let mut status = StatusLine::new(Rect::new(0, 0, 20, 1), items);

        // First draw to compute positions
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        status.draw(&mut buf, Rect::new(0, 0, 20, 1));

        // Now simulate a click on first item (x=0)
        let mut event = Event::mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        status.handle_event(&mut event);
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_CLOSE));
    }

    #[test]
    fn test_status_line_key() {
        let items = vec![StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1)];
        let mut status = StatusLine::new(Rect::new(0, 0, 20, 1), items);

        // Press F1
        let mut event = Event::key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        status.handle_event(&mut event);
        assert!(event.is_command());
        assert_eq!(event.command_id(), Some(CM_CLOSE));
    }

    #[test]
    fn test_status_item_positions_no_overlap() {
        let items = vec![
            StatusItem::new("~F1~ Help", CM_CLOSE, KB_F1),
            StatusItem::new("~F2~ Open", CM_CLOSE, KB_F2),
            StatusItem::new("~F3~ Exit", CM_CLOSE, KB_F3),
        ];

        let positions = compute_positions(&items, 0);

        // Check no overlaps
        for i in 0..positions.len() - 1 {
            assert!(positions[i].1 <= positions[i + 1].0);
        }

        // Check ordering
        for i in 1..positions.len() {
            assert!(positions[i].0 >= positions[i - 1].1);
        }
    }
}
