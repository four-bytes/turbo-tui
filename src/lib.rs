//! turbo-tui — a Rust terminal UI crate bringing Borland Turbo Vision windowing
//! patterns to modern terminal applications.

// Forbid unsafe code in the entire crate.
#![forbid(unsafe_code)]
// Enable pedantic Clippy lints.
#![warn(clippy::pedantic)]

// Core type seam (own cell/geometry/style types)
pub mod core;

// Level 0: Foundation
pub mod command;
pub mod theme;

// JSON theme serialization (optional, requires `json-themes` feature)
#[cfg(feature = "json-themes")]
pub mod theme_json;

// Level 1: View abstraction
pub mod canvas;
pub mod view;

// Cell-based fallback for the semantic Canvas primitives (crate-internal)
pub(crate) mod cell_skin;

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

// Terminal backend: persistent buffers, diffing and crossterm output
pub mod terminal;

// OSC 8 hyperlink capability detection and URL sanitisation
pub mod hyperlink;

// Platform opener for external links (http/https only)
pub mod opener;

// Level 5: Widgets
pub mod button;
pub mod input_line;
pub mod link_label;
pub mod listbox;
pub mod menu_bar;
pub mod menu_box;
pub mod scrollbar;
pub mod static_text;
pub mod status_bar;

// Level 5: Compositions
pub mod msgbox;

// Unified horizontal bar (menu bar + status bar)
pub mod horizontal_bar;

// Level 6: Text editor
pub mod editor;
pub mod memo;
pub mod syntax;

/// Prelude — import this for quick access to common types.
pub mod prelude {
    pub use crate::application::Application;
    pub use crate::canvas::{
        Bevel, Canvas, CellCanvas, Direction, FrameGadgets, FrameKind, FrameSpec, Gadget,
        GadgetState, Orientation, ScrollHover, ScrollSpec,
    };
    pub use crate::command::*;
    pub use crate::container::Container;
    pub use crate::core::{
        Buffer, Cell, Color, Link, LinkId, LinkTable, LinkTarget, Modifier, Position, Rect, Style,
    };
    pub use crate::desktop::Desktop;
    pub use crate::dialog::Dialog;
    pub use crate::editor::Editor;
    pub use crate::frame::{Frame, FrameConfig, FrameHover, FrameType};
    pub use crate::hyperlink::{HyperlinkMode, HyperlinkSupport};
    pub use crate::input_line::InputLine;
    pub use crate::link_label::LinkLabel;
    pub use crate::listbox::ListBox;
    pub use crate::memo::Memo;
    pub use crate::msgbox::{confirm_box, confirm_cancel_box, error_box, message_box};
    pub use crate::opener::open_url;
    pub use crate::overlay::{calculate_overlay_bounds, DropDirection, Overlay, OverlayManager};
    pub use crate::scrollbar::{ScrollBar, ScrollBarVisibility};
    pub use crate::syntax::{PlainTextHighlighter, RustHighlighter, SyntaxHighlighter};
    pub use crate::terminal::{Terminal, TestTerminal};
    pub use crate::theme::{self, ButtonSide};
    pub use crate::view::{
        Event, EventKind, OwnerType, View, ViewBase, ViewId, OF_DROP_TARGET, OF_POST_PROCESS,
        OF_PRE_PROCESS, OF_SELECTABLE, OF_TILEABLE, OF_TOP_SELECT, SF_ACTIVE, SF_DISABLED,
        SF_DRAGGING, SF_FOCUSED, SF_MINIMIZED, SF_MODAL, SF_RESIZING, SF_SHADOW, SF_VISIBLE,
    };
    pub use crate::window::Window;
}
