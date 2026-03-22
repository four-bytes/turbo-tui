//! `MsgBox` — Factory functions for pre-built dialog boxes.
//!
//! Convenience functions that create [`Dialog`] instances with standard layouts:
//! message boxes, confirmation dialogs, and error dialogs.
//!
//! # Factory Functions
//!
//! - [`message_box()`] — Information message with OK button
//! - [`confirm_box()`] — Yes/No question
//! - [`confirm_cancel_box()`] — Yes/No/Cancel question
//! - [`error_box()`] — Error message with OK button
//!
//! # Usage
//!
//! ```ignore
//! use turbo_tui::msgbox::message_box;
//! use ratatui::layout::Rect;
//!
//! // Create a message box dialog
//! let screen = Rect::new(0, 0, 80, 24);
//! let mut dialog = message_box("Info", "Operation completed.", screen);
//!
//! // Add to application or render directly
//! // dialog is a Dialog with children (StaticText + Button)
//! ```

use crate::button::Button;
use crate::command::{CM_CANCEL, CM_NO, CM_OK, CM_YES};
use crate::dialog::Dialog;
use crate::static_text::StaticText;
use ratatui::layout::Rect;

/// Create a message box dialog with an OK button.
///
/// The dialog is centered relative to the given `screen` area.
/// Content is displayed as a static text label above the button.
///
/// # Arguments
///
/// * `title` — Dialog title
/// * `message` — Message text
/// * `screen` — Screen bounds (for centering the dialog)
///
/// # Returns
///
/// A [`Dialog`] configured with the message and OK button.
#[must_use]
pub fn message_box(title: &str, message: &str, screen: Rect) -> Dialog {
    let (dialog_w, dialog_h) = calculate_dialog_size(message, 1);
    let bounds = center_rect(dialog_w, dialog_h, screen);

    let mut dialog = Dialog::new(bounds, title);

    // Add message text (relative to dialog interior)
    let text = StaticText::new(Rect::new(1, 1, dialog_w.saturating_sub(4), 1), message);
    dialog.add(Box::new(text));

    // Add OK button (centered at bottom)
    let btn_w = 10;
    let btn_x = (dialog_w.saturating_sub(4)).saturating_sub(btn_w) / 2;
    let btn_y = dialog_h.saturating_sub(4);
    let ok_btn = Button::new(Rect::new(btn_x, btn_y, btn_w, 1), "~O~K", CM_OK, true);
    dialog.add(Box::new(ok_btn));

    dialog
}

/// Create a confirmation dialog with Yes and No buttons.
///
/// The dialog is centered relative to the given `screen` area.
/// Returns a [`Dialog`] with the message and two buttons (Yes/No).
///
/// # Arguments
///
/// * `title` — Dialog title
/// * `message` — Question text
/// * `screen` — Screen bounds (for centering the dialog)
///
/// # Returns
///
/// A [`Dialog`] configured with the message and Yes/No buttons.
#[must_use]
pub fn confirm_box(title: &str, message: &str, screen: Rect) -> Dialog {
    let (dialog_w, dialog_h) = calculate_dialog_size(message, 2);
    let bounds = center_rect(dialog_w, dialog_h, screen);

    let mut dialog = Dialog::new(bounds, title);

    let text = StaticText::new(Rect::new(1, 1, dialog_w.saturating_sub(4), 1), message);
    dialog.add(Box::new(text));

    // Yes and No buttons side by side
    let btn_w = 10;
    let total_btn_w = btn_w * 2 + 2; // 2 buttons + gap
    let start_x = (dialog_w.saturating_sub(4)).saturating_sub(total_btn_w) / 2;
    let btn_y = dialog_h.saturating_sub(4);

    let yes_btn = Button::new(Rect::new(start_x, btn_y, btn_w, 1), "~Y~es", CM_YES, true);
    dialog.add(Box::new(yes_btn));

    let no_btn = Button::new(
        Rect::new(start_x + btn_w + 2, btn_y, btn_w, 1),
        "~N~o",
        CM_NO,
        false,
    );
    dialog.add(Box::new(no_btn));

    dialog
}

/// Create a confirmation dialog with Yes, No, and Cancel buttons.
///
/// The dialog is centered relative to the given `screen` area.
/// Returns a [`Dialog`] with the message and three buttons (Yes/No/Cancel).
///
/// # Arguments
///
/// * `title` — Dialog title
/// * `message` — Question text
/// * `screen` — Screen bounds (for centering the dialog)
///
/// # Returns
///
/// A [`Dialog`] configured with the message and Yes/No/Cancel buttons.
#[must_use]
pub fn confirm_cancel_box(title: &str, message: &str, screen: Rect) -> Dialog {
    let (dialog_w, dialog_h) = calculate_dialog_size(message, 3);
    let bounds = center_rect(dialog_w, dialog_h, screen);

    let mut dialog = Dialog::new(bounds, title);

    let text = StaticText::new(Rect::new(1, 1, dialog_w.saturating_sub(4), 1), message);
    dialog.add(Box::new(text));

    let btn_w = 10;
    let total_btn_w = btn_w * 3 + 4; // 3 buttons + 2 gaps
    let start_x = (dialog_w.saturating_sub(4)).saturating_sub(total_btn_w) / 2;
    let btn_y = dialog_h.saturating_sub(4);

    let yes_btn = Button::new(Rect::new(start_x, btn_y, btn_w, 1), "~Y~es", CM_YES, true);
    dialog.add(Box::new(yes_btn));

    let no_btn = Button::new(
        Rect::new(start_x + btn_w + 2, btn_y, btn_w, 1),
        "~N~o",
        CM_NO,
        false,
    );
    dialog.add(Box::new(no_btn));

    let cancel_btn = Button::new(
        Rect::new(start_x + (btn_w + 2) * 2, btn_y, btn_w, 1),
        "Cancel",
        CM_CANCEL,
        false,
    );
    dialog.add(Box::new(cancel_btn));

    dialog
}

/// Create an error message box with OK button.
///
/// Same as `message_box` but with "Error" as default title.
///
/// # Arguments
///
/// * `message` — Error message text
/// * `screen` — Screen bounds (for centering the dialog)
///
/// # Returns
///
/// A [`Dialog`] configured with the error message and OK button.
#[must_use]
pub fn error_box(message: &str, screen: Rect) -> Dialog {
    message_box("Error", message, screen)
}

// ============================================================================
// Private helpers
// ============================================================================

/// Calculate dialog size based on message length and number of buttons.
///
/// Returns `(width, height)` tuple.
fn calculate_dialog_size(message: &str, button_count: u16) -> (u16, u16) {
    #[allow(clippy::cast_possible_truncation)]
    let msg_len = message.len() as u16;
    #[allow(clippy::cast_possible_truncation)]
    let btn_space = button_count * 10 + (button_count.saturating_sub(1)) * 2;

    // Width: max of message + padding or buttons + padding, minimum 30
    let content_w = msg_len.max(btn_space);
    let w = (content_w + 6).clamp(30, 70); // +6 for borders + padding

    // Height: title bar + padding + message + gap + buttons + padding + bottom border
    let h: u16 = 7;

    (w, h)
}

/// Center a rectangle within a screen area.
fn center_rect(w: u16, h: u16, screen: Rect) -> Rect {
    let x = screen.x + (screen.width.saturating_sub(w)) / 2;
    let y = screen.y + (screen.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::view::View;

    fn setup_theme() {
        crate::theme::set(Theme::turbo_vision());
    }

    #[test]
    fn test_message_box_creates_dialog() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = message_box("Info", "Test message", screen);

        assert!(dialog.is_open());
        assert_eq!(dialog.title(), "Info");
        assert!(dialog.result().is_none());
    }

    #[test]
    fn test_message_box_has_ok_button() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = message_box("Info", "Test message", screen);

        // Dialog should have 2 children: text + button
        assert_eq!(dialog.interior().child_count(), 2);
    }

    #[test]
    fn test_confirm_box_creates_dialog() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = confirm_box("Confirm", "Are you sure?", screen);

        assert!(dialog.is_open());
        assert_eq!(dialog.title(), "Confirm");
    }

    #[test]
    fn test_confirm_box_has_two_buttons() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = confirm_box("Confirm", "Are you sure?", screen);

        // Dialog should have 3 children: text + 2 buttons
        assert_eq!(dialog.interior().child_count(), 3);
    }

    #[test]
    fn test_confirm_cancel_box_creates_dialog() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = confirm_cancel_box("Save?", "Save changes?", screen);

        assert!(dialog.is_open());
        assert_eq!(dialog.title(), "Save?");
    }

    #[test]
    fn test_confirm_cancel_box_has_three_buttons() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = confirm_cancel_box("Save?", "Save changes?", screen);

        // Dialog should have 4 children: text + 3 buttons
        assert_eq!(dialog.interior().child_count(), 4);
    }

    #[test]
    fn test_error_box_creates_dialog() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = error_box("Something went wrong", screen);

        assert!(dialog.is_open());
        assert_eq!(dialog.title(), "Error");
    }

    #[test]
    fn test_dialog_centered() {
        setup_theme();
        let screen = Rect::new(0, 0, 80, 24);
        let dialog = message_box("Info", "Test", screen);

        let bounds = dialog.bounds();
        // Dialog should be roughly centered
        // Width should be at least 30 (minimum), height is 7
        assert!(bounds.width >= 30);
        assert_eq!(bounds.height, 7);

        // Center position should be reasonable
        let center_x = screen.x + screen.width / 2;
        let center_y = screen.y + screen.height / 2;
        let dialog_center_x = bounds.x + bounds.width / 2;
        let dialog_center_y = bounds.y + bounds.height / 2;

        // Allow some tolerance for rounding
        assert!(
            (dialog_center_x as i32 - center_x as i32).abs() <= 1,
            "Dialog should be horizontally centered"
        );
        assert!(
            (dialog_center_y as i32 - center_y as i32).abs() <= 1,
            "Dialog should be vertically centered"
        );
    }

    #[test]
    fn test_calculate_dialog_size() {
        // Minimum width is 30
        let (w, h) = calculate_dialog_size("Hi", 1);
        assert!(w >= 30);
        assert_eq!(h, 7);

        // Maximum width is 70
        let (w, _) = calculate_dialog_size(&"x".repeat(100), 1);
        assert_eq!(w, 70);

        // Height is always 7
        let (_, h) = calculate_dialog_size("Test", 2);
        assert_eq!(h, 7);
    }
}
