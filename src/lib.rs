//! turbo-tui — Borland Turbo Vision windowing patterns for Ratatui
//!
//! This crate brings classic Borland Turbo Vision UI patterns to the Ratatui ecosystem:
//!
//! - **Overlapping Windows** with Z-order, drag & resize
//! - **Modal Dialogs** with OK/Cancel/Yes/No
//! - **Menu Bar** with dropdown submenus and keyboard hotkeys
//! - **Status Line** with context-sensitive shortcuts
//! - **Command System** with enable/disable states
//! - **Scrollbars** with draggable thumb
//!
//! # Architecture
//!
//! turbo-tui renders through Ratatui's `Frame`/`Buffer` system — it does NOT
//! replace Ratatui, it extends it with windowing capabilities.
//!
//! Patterns are adapted from [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust)
//! (MIT licensed), reimplemented for Ratatui.

pub mod button;
pub mod command;
pub mod theme;
pub mod desktop;
pub mod dialog;
pub mod frame;
pub mod group;
pub mod menu_bar;
pub mod menu_box;
pub mod msgbox;
pub mod scrollbar;
pub mod static_text;
pub mod status_line;
pub mod view;
pub mod window;

pub mod prelude {
    pub use crate::button::Button;
    pub use crate::command::{CommandId, CommandSet, CM_SCROLL_CHANGED};
    pub use crate::theme::Theme;
    pub use crate::desktop::Desktop;
    pub use crate::dialog::Dialog;
    pub use crate::frame::{Frame, FrameType};
    pub use crate::group::Group;
    pub use crate::menu_bar::{Menu, MenuBar, MenuItem};
    pub use crate::menu_box::MenuBox;
    pub use crate::msgbox::{confirm_box, confirm_cancel_box, error_box, message_box};
    pub use crate::scrollbar::{Orientation, ScrollBar};
    pub use crate::static_text::StaticText;
    pub use crate::status_line::{
        StatusItem, StatusLine, KB_ALT_X, KB_F1, KB_F10, KB_F11, KB_F12, KB_F2, KB_F3, KB_F4,
        KB_F5, KB_F6, KB_F7, KB_F8, KB_F9,
    };
    pub use crate::view::{
        Event, EventKind, OwnerType, View, ViewBase, ViewId, OF_POST_PROCESS, OF_PRE_PROCESS,
        OF_SELECTABLE, OF_TILEABLE, OF_TOP_SELECT, SF_ACTIVE, SF_DISABLED, SF_DRAGGING, SF_FOCUSED,
        SF_MODAL, SF_RESIZING, SF_SHADOW, SF_VISIBLE,
    };
    pub use crate::window::Window;
}
