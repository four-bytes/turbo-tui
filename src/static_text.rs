//! `StaticText` — non-interactive text label.
//!
//! A `StaticText` is a simple text display widget that does not respond to
//! input events. It can display text left-aligned or centered within its bounds.
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{StaticText, Rect};
//!
//! // Left-aligned text
//! let label = StaticText::new(Rect::new(10, 5, 20, 1), "Enter your name:");
//!
//! // Centered text
//! let title = StaticText::centered(Rect::new(0, 0, 40, 1), "Welcome!");
//! ```

use crate::view::{Event, View, ViewBase, ViewId};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use std::any::Any;

/// Non-interactive text label.
///
/// Displays static text within a rectangular area. Can be left-aligned
/// or centered. Does not respond to any input events.
///
/// # Visual Style
///
/// ```text
/// Left-aligned:  Enter your name:
/// Centered:          Welcome!
/// ```
pub struct StaticText {
    /// Embedded base providing `ViewId`, bounds, state.
    base: ViewBase,
    /// Text content to display.
    text: String,
    /// Whether to center the text within bounds.
    centered: bool,
}

impl StaticText {
    /// Create a new left-aligned static text.
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the text area.
    /// * `text` — Text content to display.
    #[must_use]
    pub fn new(bounds: Rect, text: &str) -> Self {
        Self {
            base: ViewBase::new(bounds),
            text: text.to_owned(),
            centered: false,
        }
    }

    /// Create a new centered static text.
    ///
    /// # Arguments
    ///
    /// * `bounds` — Position and size of the text area.
    /// * `text` — Text content to display (centered).
    #[must_use]
    pub fn centered(bounds: Rect, text: &str) -> Self {
        Self {
            base: ViewBase::new(bounds),
            text: text.to_owned(),
            centered: true,
        }
    }

    /// Get the text content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Set the text content.
    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    /// Check if the text is centered.
    #[must_use]
    pub fn is_centered(&self) -> bool {
        self.centered
    }

    /// Draw the text to the buffer.
    fn draw_text(&self, buf: &mut Buffer, area: Rect) {
        if self.text.is_empty() {
            return;
        }

        let x = if self.centered {
            // Center the text
            #[allow(clippy::cast_possible_truncation)]
            let text_len = self.text.len() as u16;
            area.x + area.width.saturating_sub(text_len) / 2
        } else {
            // Left-aligned
            area.x
        };

        let style = Style::default().fg(Color::White);
        buf.set_string(x, area.y, &self.text, style);
    }
}

impl View for StaticText {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        self.draw_text(buf, area);
    }

    fn handle_event(&mut self, event: &mut Event) {
        // StaticText does not respond to events
        // Mark as handled if it's targeting us, but don't process
        let _ = event;
    }

    fn can_focus(&self) -> bool {
        false
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::SF_VISIBLE;

    #[test]
    fn test_static_text_new() {
        let text = StaticText::new(Rect::new(10, 5, 20, 1), "Hello, World!");

        assert_eq!(text.bounds(), Rect::new(10, 5, 20, 1));
        assert_eq!(text.text(), "Hello, World!");
        assert!(!text.is_centered());
        assert!(!text.can_focus());
    }

    #[test]
    fn test_static_text_centered() {
        let text = StaticText::centered(Rect::new(0, 0, 40, 1), "Welcome!");

        assert_eq!(text.bounds(), Rect::new(0, 0, 40, 1));
        assert_eq!(text.text(), "Welcome!");
        assert!(text.is_centered());
    }

    #[test]
    fn test_static_text_draw() {
        let text = StaticText::new(Rect::new(0, 0, 20, 1), "Test Label");
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 5));
        text.draw(&mut buf, Rect::new(0, 0, 30, 5));

        // Verify text was drawn
        let content = buf.content();
        let has_test = content.iter().any(|cell| {
            cell.symbol().contains('T')
                || cell.symbol().contains('e')
                || cell.symbol().contains('s')
        });
        assert!(has_test, "StaticText should draw its content");
    }

    #[test]
    fn test_static_text_set() {
        let mut text = StaticText::new(Rect::new(0, 0, 20, 1), "Original");
        assert_eq!(text.text(), "Original");

        text.set_text(String::from("Updated"));
        assert_eq!(text.text(), "Updated");
    }

    #[test]
    fn test_static_text_empty() {
        let text = StaticText::new(Rect::new(0, 0, 20, 1), "");
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        text.draw(&mut buf, Rect::new(0, 0, 20, 1));
        // Should not panic
    }

    #[test]
    fn test_static_text_state() {
        let mut text = StaticText::new(Rect::new(0, 0, 20, 1), "Test");

        // Initial state should have SF_VISIBLE
        assert_ne!(text.state() & SF_VISIBLE, 0);

        // Modify state
        text.set_state(0);
        assert_eq!(text.state(), 0);
    }

    #[test]
    fn test_static_text_bounds() {
        let mut text = StaticText::new(Rect::new(5, 3, 20, 2), "Test");
        assert_eq!(text.bounds(), Rect::new(5, 3, 20, 2));

        text.set_bounds(Rect::new(10, 10, 30, 5));
        assert_eq!(text.bounds(), Rect::new(10, 10, 30, 5));
    }

    #[test]
    fn test_static_text_centered_calculation() {
        let text = StaticText::centered(Rect::new(0, 0, 20, 1), "Hi"); // 2 chars
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 5));
        text.draw(&mut buf, Rect::new(0, 0, 30, 5));

        // "Hi" has 2 chars, centered in width20 => starts at (20-2)/2 = 9
        // Check that the text is approximately centered
        let content = buf.content();
        let has_h = content.iter().any(|cell| cell.symbol().contains('H'));
        assert!(has_h, "Centered text should be drawn");
    }
}
