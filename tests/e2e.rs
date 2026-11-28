//! Opt-in tmux end-to-end tests for the demo binary.
//!
//! These tests drive `examples/demo.rs` inside a real tmux pane and read the
//! rendered cells back with `capture-pane -e`. They are ignored by default so
//! `cargo test` never needs tmux.
//!
//! Run them with:
//!
//! ```text
//! TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored --test-threads=1 --nocapture
//! ```

#[path = "e2e/harness.rs"]
mod harness;

#[path = "e2e/scenarios.rs"]
mod scenarios;
