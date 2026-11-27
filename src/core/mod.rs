//! Core cell, geometry and style types.
//!
//! turbo-tui owns these types so the rest of the crate never depends on an
//! external TUI library's core types.

mod buffer;
mod geometry;
mod link;
mod style;

pub use buffer::{Buffer, Cell};
pub use geometry::{Position, Rect, Size};
pub use link::{Link, LinkId, LinkTable, LinkTarget};
pub use style::{Color, Modifier, Style};
