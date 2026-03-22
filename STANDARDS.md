# turbo-tui — Coding Standards

> Last updated: 2026-03-22

## Rust Configuration

### Cargo.toml Lints
```toml
[lints.rust]
unsafe_code = "forbid"      # Zero unsafe, no exceptions

[lints.clippy]
pedantic = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
```

### Edition & Dependencies
- **Edition:** 2021
- **ratatui:** 0.29
- **crossterm:** 0.28
- **serde/serde_json:** 1.x (optional, behind `json-themes` feature)
- **No dev-dependencies** — all tests use only std + production deps

---

## Code Style

### Formatting
- `cargo fmt` — default rustfmt settings
- LF line endings only (never CRLF)
- Real umlauts (ä, ö, ü, ß) in German text — never ae/oe/ue
- Max line length: not enforced, but keep reasonable (~100-120)

### Documentation
- **All public items** must have doc comments (`///`)
- Module-level doc comments (`//!`) at top of each file explaining purpose
- Include `# Example` sections for complex public APIs (use `/// ```ignore` if they need terminal context)
- Reference other types with `[`backtick links`]`

### Naming
- Test names: `test_{module}_{what}_{condition}` — e.g. `test_window_resize_clamps_to_min_size`
- Constants: `SCREAMING_SNAKE_CASE` — e.g. `SF_FOCUSED`, `CM_QUIT`, `OF_SELECTABLE`
- State flags: `SF_` prefix (State Flag)
- Option flags: `OF_` prefix (Option Flag)
- Commands: `CM_` prefix (CoMmand)
- Keyboard constants: `KB_` prefix

### Imports
- Group by: std → external crates → crate-internal
- Use specific imports, not glob (except in test modules where `use super::*` is OK)
- `use crate::` for internal, `use ratatui::` / `use crossterm::` for external

---

## Architecture Patterns

### View Trait (Component Model)
Every interactive UI element implements `View`:
```rust
pub trait View {
    fn id(&self) -> ViewId;
    fn bounds(&self) -> Rect;
    fn set_bounds(&mut self, bounds: Rect);
    fn draw(&self, buf: &mut Buffer, clip: Rect);
    fn handle_event(&mut self, event: &mut Event);
    fn can_focus(&self) -> bool;
    fn state(&self) -> u16;
    fn set_state(&mut self, state: u16);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    // ... lifecycle hooks with default impls
}
```

**DO NOT** split into separate Widget + EventHandler traits. The unified View IS the component architecture.

### ViewBase Embedding
Every widget embeds `ViewBase` for boilerplate:
```rust
struct MyWidget {
    base: ViewBase,
    // widget-specific fields
}
```

### Builder Lite Pattern (for construction)
Use self-consuming methods returning `Self`. NOT a separate Builder struct:
```rust
impl Window {
    pub fn new(bounds: Rect, title: &str) -> Self { ... }
    #[must_use]
    pub fn scrollbars(mut self, v: bool, h: bool) -> Self { ... }
    #[must_use]
    pub fn min_size(mut self, w: u16, h: u16) -> Self { ... }
}
```

Builder Lite methods must have `#[must_use]` attribute. Existing `set_*` methods stay for runtime mutation.

### Event System
- **Three-phase dispatch:** PreProcess → Focused → PostProcess
- **Event consumption:** `event.clear()` when handled
- **Deferred events:** `event.post(Event::command(CM_SOMETHING))` for child→parent communication
- **Commands < 1000** close modal dialogs, **≥ 1000** (`INTERNAL_COMMAND_BASE`) are internal
- **Mouse events:** Route front-to-back (reverse Z-order) with hit-testing
- **Keyboard events:** Route through focused child only

### Container Pattern
- Children stored as `Vec<Box<dyn View>>`
- Z-order: index 0 = back, last = front
- Coordinates: children added with **relative** coords, converted to absolute in `Container::add()`
- Focus: `focused: Option<usize>` index into children vec

### Theme System
- Thread-local global: `theme::with_current(|t| { ... })`
- Every style must include **both fg AND bg** colors (prevents bleed-through)
- JSON themes via `json-themes` feature flag
- Theme registry for named themes + cycling

---

## File Structure for New Widgets

When adding a new widget (e.g., `gauge.rs`):

1. Create `src/gauge.rs`
2. Add `pub mod gauge;` to `src/lib.rs` (in Level 5: Widgets section)
3. Add public types to prelude in `src/lib.rs`
4. Implement `View` trait
5. Embed `ViewBase`
6. Add `#[cfg(test)] mod tests` at bottom
7. Add theme fields if needed (in `theme.rs` + `theme_json.rs`)
8. Update `CLAUDE.md` architecture tree
9. Update `TESTING.md` test count table

### Widget Template
```rust
//! Gauge — progress bar widget.
//!
//! Displays a horizontal progress bar with optional label.

use crate::theme;
use crate::view::{View, ViewBase, ViewId};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

/// A horizontal progress bar.
pub struct Gauge {
    base: ViewBase,
    progress: f64, // 0.0 .. 1.0
}

impl Gauge {
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            progress: 0.0,
        }
    }
    
    // Builder Lite
    #[must_use]
    pub fn progress(mut self, value: f64) -> Self {
        self.progress = value.clamp(0.0, 1.0);
        self
    }
}

impl View for Gauge {
    fn id(&self) -> ViewId { self.base.id() }
    fn bounds(&self) -> Rect { self.base.bounds() }
    fn set_bounds(&mut self, bounds: Rect) { self.base.set_bounds(bounds); }
    fn draw(&self, buf: &mut Buffer, clip: Rect) { /* render */ }
    fn handle_event(&mut self, _event: &mut crate::view::Event) {}
    fn can_focus(&self) -> bool { false }
    fn state(&self) -> u16 { self.base.state() }
    fn set_state(&mut self, state: u16) { self.base.set_state(state); }
    fn options(&self) -> u16 { self.base.options() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gauge_new_defaults() {
        let g = Gauge::new(Rect::new(0, 0, 20, 1));
        assert_eq!(g.progress, 0.0);
    }
}
```

---

## Commit Standards

- Version bump + HISTORY.md entry for every release
- HISTORY.md is **append-only** — never overwrite existing entries
- No Claude references, no Co-Authored-By in public commits
- Commit message format: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`

---

## Performance Guidelines

- **Zero heap allocations in draw paths** — use stack-allocated arrays (see `FrameStyles`)
- **Theme access** via `theme::with_current()` is a cheap `RefCell::borrow()` — not a bottleneck
- **Dirty flags** on `ViewBase` — only redraw when marked dirty
- **16ms poll timeout** + event-driven redraw — only draws when events arrive
- **No `clone()` in hot paths** — scrollbar draw clones scrollbar to set bounds; acceptable for now but noted
- **24-bit RGB colors** may render slower than CGA 16-color on some terminals (Windows Terminal vs Alacritty)
