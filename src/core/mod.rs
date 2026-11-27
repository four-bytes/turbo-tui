//! Core cell, geometry and style types.
//!
//! turbo-tui owns these types so the rest of the crate never depends on an
//! external TUI library's core types.

mod buffer;
mod geometry;
mod style;

pub use buffer::{Buffer, Cell};
pub use geometry::{Position, Rect, Size};
pub use style::{Color, Modifier, Style};
