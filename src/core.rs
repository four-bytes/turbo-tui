//! Core cell, geometry and style types.
//!
//! During the backend migration (epic #10) these are re-exports of Ratatui.
//! Later sub-issues replace them with turbo-tui's own implementations behind
//! the same names, so downstream code only imports from here.

pub use ratatui::{
    buffer::{Buffer, Cell},
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};
