//! Message Box — pre-built dialog factories for common dialogs.
//!
//! This module provides factory functions that create common message box
//! patterns using [`Dialog`], [`StaticText`], and [`Button`].
//!
//! # Available Dialogs
//!
//! - [`message_box()`] — OK-only information box
//! - [`confirm_box()`] — Yes/No confirmation
//! - [`confirm_cancel_box()`] — Yes/No/Cancel confirmation
//! - [`error_box()`] — Error message box
//!
//! # Example
//!
//! ```ignore
//! use four_turbo_tui::{message_box, Rect};
//!
//! // Create and run an OK message box
//! let dialog = message_box(
//!     Rect::new(10, 5, 40, 10),
//!     "Information",
//!     "Operation completed successfully."
//! );
//!
//! // In your event loop:
//! while dialog.is_running() {
//!     dialog.handle_event(&mut event);
//! }
//!
//! if dialog.result() == CM_OK {
//!     // User clicked OK
//! }
//! ```

use crate::button::Button;
use crate::command::{CM_CANCEL, CM_NO, CM_OK, CM_YES};
use crate::dialog::Dialog;
use crate::static_text::StaticText;
use ratatui::layout::Rect;

/// Create an OK message box.
///
/// Returns a [`Dialog`] with:
/// - A [`StaticText`] message (centered)
/// - An OK button (default)
///
/// # Arguments
///
/// * `bounds` — Dialog bounds (recommended: width >= 30, height >= 10).
/// * `title` — Dialog title.
/// * `message` — Message text to display.
///
/// # Example
///
/// ```ignore
/// let dialog = message_box(
///     Rect::new(10, 5, 40, 10),
///     "Information",
///     "File saved successfully."
/// );
/// ```
#[must_use]
pub fn message_box(bounds: Rect, title: &str, message: &str) -> Dialog {
    let mut dialog = Dialog::new(bounds, title);

    // Add message text (centered, row 2 of interior)
    let interior = dialog.interior_rect();
    let text_bounds = Rect::new(
        interior.x,
        interior.y + 1, // Row 2 (row 1 is border, row 0 would be first interior row)
        interior.width,
        1,
    );
    dialog.add(Box::new(StaticText::centered(text_bounds, message)));

    // Add OK button at bottom center
    let button_width = 10u16;
    let button_x = interior.x + interior.width.saturating_sub(button_width) / 2;
    let button_y = interior.y + interior.height.saturating_sub(2);
    let button_bounds = Rect::new(button_x, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(button_bounds, "OK", CM_OK, true)));

    dialog
}

/// Create a Yes/No confirmation box.
///
/// Returns a [`Dialog`] with:
/// - A [`StaticText`] message (centered)
/// - A Yes button (default) and a No button
///
/// # Arguments
///
/// * `bounds` — Dialog bounds (recommended: width >= 40, height >= 10).
/// * `title` — Dialog title.
/// * `message` — Question text to display.
///
/// # Example
///
/// ```ignore
/// let dialog = confirm_box(
///     Rect::new(10, 5, 40, 10),
///     "Confirm",
///     "Are you sure you want to delete this file?"
/// );
/// ```
#[must_use]
pub fn confirm_box(bounds: Rect, title: &str, message: &str) -> Dialog {
    let mut dialog = Dialog::new(bounds, title);

    // Add message text
    let interior = dialog.interior_rect();
    let text_bounds = Rect::new(interior.x, interior.y + 1, interior.width, 1);
    dialog.add(Box::new(StaticText::centered(text_bounds, message)));

    // Add Yes/No buttons side by side at bottom
    let button_width = 10u16;
    let spacing = 2u16;
    let total_button_width = button_width * 2 + spacing;
    let start_x = interior.x + interior.width.saturating_sub(total_button_width) / 2;
    let button_y = interior.y + interior.height.saturating_sub(2);

    // Yes button (default)
    let yes_bounds = Rect::new(start_x, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(yes_bounds, "Yes", CM_YES, true)));

    // No button
    let no_bounds = Rect::new(start_x + button_width + spacing, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(no_bounds, "No", CM_NO, false)));

    dialog
}

/// Create a Yes/No/Cancel confirmation box.
///
/// Returns a [`Dialog`] with:
/// - A [`StaticText`] message (centered)
/// - Three buttons: Yes (default), No, Cancel
///
/// # Arguments
///
/// * `bounds` — Dialog bounds (recommended: width >= 50, height >= 10).
/// * `title` — Dialog title.
/// * `message` — Question text to display.
///
/// # Example
///
/// ```ignore
/// let dialog = confirm_cancel_box(
///     Rect::new(5, 3, 50, 10),
///     "Save Changes?",
///     "Do you want to save changes before closing?"
/// );
/// ```
#[must_use]
pub fn confirm_cancel_box(bounds: Rect, title: &str, message: &str) -> Dialog {
    let mut dialog = Dialog::new(bounds, title);

    // Add message text
    let interior = dialog.interior_rect();
    let text_bounds = Rect::new(interior.x, interior.y + 1, interior.width, 1);
    dialog.add(Box::new(StaticText::centered(text_bounds, message)));

    // Add Yes/No/Cancel buttons at bottom
    let button_width = 10u16;
    let spacing = 2u16;
    let total_button_width = button_width * 3 + spacing * 2;
    let start_x = interior.x + interior.width.saturating_sub(total_button_width) / 2;
    let button_y = interior.y + interior.height.saturating_sub(2);

    // Yes button (default)
    let yes_bounds = Rect::new(start_x, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(yes_bounds, "Yes", CM_YES, true)));

    // No button
    let no_bounds = Rect::new(start_x + button_width + spacing, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(no_bounds, "No", CM_NO, false)));

    // Cancel button
    let cancel_bounds = Rect::new(
        start_x + (button_width + spacing) * 2,
        button_y,
        button_width,
        1,
    );
    dialog.add(Box::new(Button::new(
        cancel_bounds,
        "Cancel",
        CM_CANCEL,
        false,
    )));

    dialog
}

/// Create an error message box.
///
/// Returns a [`Dialog`] with:
/// - A [`StaticText`] error message (centered)
/// - An OK button
///
/// # Arguments
///
/// * `bounds` — Dialog bounds (recommended: width >= 40, height >= 10).
/// * `message` — Error message to display.
///
/// # Example
///
/// ```ignore
/// let dialog = error_box(
///     Rect::new(10, 5, 40, 10),
///     "Failed to open file: permission denied."
/// );
/// ```
#[must_use]
pub fn error_box(bounds: Rect, message: &str) -> Dialog {
    let mut dialog = Dialog::new(bounds, "Error");

    // Add error message
    let interior = dialog.interior_rect();
    let text_bounds = Rect::new(interior.x, interior.y + 1, interior.width, 1);
    dialog.add(Box::new(StaticText::centered(text_bounds, message)));

    // Add OK button at bottom center
    let button_width = 10u16;
    let button_x = interior.x + interior.width.saturating_sub(button_width) / 2;
    let button_y = interior.y + interior.height.saturating_sub(2);
    let button_bounds = Rect::new(button_x, button_y, button_width, 1);
    dialog.add(Box::new(Button::new(button_bounds, "OK", CM_OK, true)));

    dialog
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{View, OF_SELECTABLE};

    fn count_buttons(dialog: &Dialog) -> usize {
        let interior = dialog.interior();
        let mut count = 0;
        for i in 0..interior.child_count() {
            if let Some(child) = interior.child_at(i) {
                // Buttons are selectable
                if child.options() & OF_SELECTABLE != 0 {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn test_message_box_has_ok() {
        let dialog = message_box(Rect::new(0, 0, 40, 10), "Info", "Test message");

        assert_eq!(dialog.title(), "Info");
        // Should have message text + 1 OK button
        assert_eq!(dialog.child_count(), 2);
        assert_eq!(count_buttons(&dialog), 1);
    }

    #[test]
    fn test_message_box_result() {
        let dialog = message_box(Rect::new(0, 0, 40, 10), "Info", "Test message");

        // Result should be 0 (still running)
        assert_eq!(dialog.result(), 0);
        assert!(dialog.is_running());
    }

    #[test]
    fn test_confirm_box_has_yes_no() {
        let dialog = confirm_box(Rect::new(0, 0, 40, 10), "Confirm", "Are you sure?");

        assert_eq!(dialog.title(), "Confirm");
        // Should have message text + Yes + No buttons
        assert_eq!(dialog.child_count(), 3);
        assert_eq!(count_buttons(&dialog), 2);
    }

    #[test]
    fn test_confirm_cancel_box_has_three_buttons() {
        let dialog = confirm_cancel_box(Rect::new(0, 0, 50, 10), "Question", "Save changes?");

        assert_eq!(dialog.title(), "Question");
        // Should have message text + Yes + No + Cancel buttons
        assert_eq!(dialog.child_count(), 4);
        assert_eq!(count_buttons(&dialog), 3);
    }

    #[test]
    fn test_error_box() {
        let dialog = error_box(Rect::new(0, 0, 40, 10), "File not found.");

        assert_eq!(dialog.title(), "Error");
        // Should have error text + 1 OK button
        assert_eq!(dialog.child_count(), 2);
        assert_eq!(count_buttons(&dialog), 1);
    }

    #[test]
    fn test_message_box_small_bounds() {
        // Test with minimum bounds (should not panic)
        let dialog = message_box(Rect::new(0, 0, 30, 8), "Title", "Message");
        assert_eq!(dialog.child_count(), 2);
    }

    #[test]
    fn test_confirm_box_can_focus() {
        let dialog = confirm_box(Rect::new(0, 0, 40, 10), "Confirm", "Test");

        // Check that dialog can be focused
        assert!(dialog.can_focus());
    }

    #[test]
    fn test_message_box_escape_closes() {
        use crate::view::Event;
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut dialog = message_box(Rect::new(0, 0, 40, 10), "Test", "Message");

        // Press Escape
        let mut event = Event::key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::empty(),
        ));
        dialog.handle_event(&mut event);

        // Dialog should close with CM_CANCEL
        assert_eq!(dialog.result(), CM_CANCEL);
        assert!(!dialog.is_running());
    }

    #[test]
    fn test_message_box_enter_closes() {
        use crate::view::Event;
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut dialog = message_box(Rect::new(0, 0, 40, 10), "Test", "Message");

        // Press Enter
        let mut event = Event::key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::empty(),
        ));
        dialog.handle_event(&mut event);

        // Dialog should close with CM_OK
        assert_eq!(dialog.result(), CM_OK);
        assert!(!dialog.is_running());
    }
}
