//! turbo-tui — Ratatui extension crate bringing Borland Turbo Vision windowing
//! patterns to modern Rust terminal applications.

// Forbid unsafe code in the entire crate.
#![forbid(unsafe_code)]
// Enable pedantic Clippy lints.
#![warn(clippy::pedantic)]

// Level 0: Foundation
pub mod command;
pub mod theme;

// JSON theme serialization (optional, requires `json-themes` feature)
#[cfg(feature = "json-themes")]
pub mod theme_json;

// Level 1: View abstraction
pub mod view;

// Clip-aware rendering utilities (crate-internal)
pub(crate) mod clip;

// Level 2: Container
pub mod container;

// Level 3: Window system
pub mod desktop;
pub mod frame;
pub mod window;

// Level 4: Application + Overlay
pub mod application;
pub mod dialog;
pub mod overlay;

// Level 5: Widgets
pub mod button;
pub mod menu_bar;
pub mod menu_box;
pub mod scrollbar;
pub mod static_text;
pub mod status_line;

// Level 5: Compositions
pub mod msgbox;

// Unified horizontal bar (menu bar + status line)
pub mod horizontal_bar;

/// Prelude — import this for quick access to common types.
pub mod prelude {
    pub use crate::application::Application;
    pub use crate::command::*;
    pub use crate::container::Container;
    pub use crate::desktop::Desktop;
    pub use crate::dialog::Dialog;
    pub use crate::frame::{Frame, FrameType};
    pub use crate::msgbox::{confirm_box, confirm_cancel_box, error_box, message_box};
    pub use crate::overlay::{calculate_overlay_bounds, DropDirection, Overlay, OverlayManager};
    pub use crate::theme;
    pub use crate::view::{
        Event, EventKind, OwnerType, View, ViewBase, ViewId, OF_POST_PROCESS, OF_PRE_PROCESS,
        OF_SELECTABLE, OF_TILEABLE, OF_TOP_SELECT, SF_ACTIVE, SF_DISABLED, SF_DRAGGING, SF_FOCUSED,
        SF_MODAL, SF_RESIZING, SF_SHADOW, SF_VISIBLE,
    };
    pub use crate::window::Window;
}
