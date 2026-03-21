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

pub mod command;

pub mod prelude {
    pub use crate::command::{CommandId, CommandSet};
}
