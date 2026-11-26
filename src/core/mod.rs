//! Core cell, geometry and style types.
//!
//! turbo-tui owns these types so the rest of the crate never sees Ratatui's
//! core types directly. During the backend migration (epic #10) temporary
//! `From` conversions in [`convert`] bridge to Ratatui for
//! [`Application::draw`](crate::application::Application::draw).

mod buffer;
mod convert;
mod geometry;
mod style;

pub use buffer::{Buffer, Cell};
pub use geometry::{Position, Rect, Size};
pub use style::{Color, Modifier, Style};
