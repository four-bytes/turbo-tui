# turbo-tui — Claude Code Configuration

## Project Overview

**turbo-tui** is a Ratatui extension crate that brings Borland Turbo Vision windowing patterns to modern Rust terminal applications.

- **Language:** Rust
- **Status:** v0.2.0-dev — Rebuild in progress (v0.1 on `main`, v0.2 on `v0.2-rebuild`)
- **Org:** four-bytes (Four\ namespace)
- **Crate type:** Library (no binary)
- **Repo:** https://github.com/four-bytes/turbo-tui
- **Consumer:** `four-code` terminal IDE (`~/four-code`)
- **Deps:** ratatui 0.29, crossterm 0.28

## Architecture

Single crate, 21 source files (~10,000+ lines total):

```
src/
├── lib.rs              # Public API + prelude exports
├── command.rs          # CommandId (u16), CommandSet bitfield, 25+ CM_* constants
├── theme.rs            # Theme struct, 4 Themes (dark, borland_classic, modern, matrix)
├── theme_json.rs       # JSON theme serialization, border presets, color roundtrip
├── view.rs             # View trait, ViewBase, StateFlags, OptionFlags, Event system, deferred queue, lifecycle hooks
├── clip.rs             # ClipRegion for intersection clipping
├── container/
│   ├── mod.rs          # Container struct + public API (renamed from Group)
│   ├── dispatch.rs     # Three-phase event dispatch + mouse capture
│   └── draw.rs         # draw_children + intersection clipping
├── frame.rs            # Smart Border: borders, title, close, resize, optional ScrollBars
├── window.rs           # Overlapping windows: drag/resize state machine, zoom toggle
├── desktop.rs          # Window manager: tile, cascade, click-to-focus, background
├── overlay.rs          # OverlayManager for dropdowns/tooltips above all windows
├── application.rs      # Event Loop, dispatch chain, deferred events, screen resize
├── dialog.rs           # Modal dialogs: Esc/Enter/command handling
├── horizontal_bar.rs   # Unified HorizontalBar: BarEntry (Action/Dropdown), DropDirection (Up/Down)
├── menu_bar.rs         # Thin wrapper: MenuItem, Menu types, MenuBar = HorizontalBar alias
├── menu_box.rs         # Standalone dropdown menu box
├── status_line.rs      # Thin wrapper: StatusItem, KB_* constants, StatusLine = HorizontalBar alias
├── scrollbar.rs        # Vertical/horizontal scrollbar with thumb drag
├── button.rs           # Clickable button with hotkey support
├── static_text.rs      # Non-interactive text label
└── msgbox.rs           # Pre-built message/confirm/error dialog factories
examples/
└── demo.rs             # Interactive demo: Application + MenuBar + Windows + Buttons + StatusLine
```

## Development Workflow

```bash
cargo check                     # Quick syntax check (fastest)
cargo test                      # Run all 255 tests
cargo clippy -- -D warnings     # Lint (pedantic enabled)
cargo fmt                       # Format
cargo run --example demo        # Run the interactive demo
```

No Makefile — use cargo directly.

## Conventions

### Code Style
- `unsafe_code = "forbid"` — zero unsafe, no exceptions
- Clippy pedantic enabled (`clippy::pedantic = warn`)
- All public items must have doc comments
- Tests next to implementation (`#[cfg(test)] mod tests { }`)
- Descriptive test names: `test_window_resize_clamps_to_min_size`
- LF line endings only, real umlauts (ä, ö, ü, ß)

### Design Patterns (from Borland Turbo Vision)
- **Rendering:** Through Ratatui `Frame`/`Buffer` — NOT a standalone terminal framework
- **CommandSet:** Owned by Application struct, NOT thread-local/global
- **No raw pointers:** No `*const dyn View` owner pointers — event-based communication instead
- **Command convention:** Commands < 1000 close modal dialogs, >= 1000 (`INTERNAL_COMMAND_BASE`) are internal view commands
- **Event dispatch:** Three-phase: PreProcess → Focused → PostProcess (critical for StatusLine intercepting F-keys)
- **ViewId:** Atomic counter for stable identity across moves/resizes
- **Z-order:** Vec ordering (index 0 = back, last = front)
- **Hotkey markers:** `~X~` in labels marks the hotkey letter (rendered underlined)
- **Borland keys:** Alt+X = quit, F10 = menu, Alt+letter = open specific menu

### Reference
- Pattern source: [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) (MIT) — study patterns, don't copy code
- Architecture decision: [ADR-002](~/four-code/docs/ADR-002-turbo-tui-windowing.md) in four-code
- **v0.2 Architecture Plan:** [`docs/PLAN-v0.2.md`](docs/PLAN-v0.2.md) — Detailed build plan with 10 steps, architecture decisions, new concepts (Deferred Event Queue, Lifecycle Hooks, Overlay System, Event Coalescing)

## Key Design Decisions

1. **turbo-vision-4-rust is NOT built on Ratatui** — it's a standalone framework on crossterm at the same level as Ratatui. That's why we port patterns instead of using it as a dependency.
2. **Crate name `turbo-tui`** is reserved on crates.io.
3. **Three-phase dispatch** is critical: StatusLine uses `OF_PRE_PROCESS` to intercept F-keys before they reach focused windows. Without this, F1/F5/F10 would go to the focused window first.
4. **Mouse events** route front-to-back (reverse Z-order) with hit-testing. Keyboard/command events use three-phase dispatch through the focused child.

## Key Files for Common Tasks

| Task | Files |
|------|-------|
| Add new widget | `src/lib.rs` (module + prelude), new `src/widget_name.rs` |
| Change window behavior | `src/window.rs`, `src/frame.rs` |
| Change event routing | `src/container/dispatch.rs` (dispatch_event, three-phase) |
| Change bar behavior (menu/status) | `src/horizontal_bar.rs` (unified logic) |
| Change menu items/types | `src/menu_bar.rs` (MenuItem, Menu), `src/horizontal_bar.rs` |
| Change background/theming | `src/desktop.rs` (draw_background), `src/frame.rs` (styles) |
| Add commands | `src/command.rs` (CM_* constants) |
| Demo changes | `examples/demo.rs` |

## v0.2 Rebuild — Current State

**Branch:** `v0.2-rebuild` | **Plan:** [`docs/PLAN-v0.2.md`](docs/PLAN-v0.2.md)

### Completed (Steps 1-10)
- Level 0: command.rs + theme.rs — CommandId/CommandSet, Theme with JSON support, theme registry
- Level 1: view.rs — dirty-flag, clip semantics, deferred events, lifecycle hooks (16 tests)
- Level 2: container/ — renamed from Group, split into mod/dispatch/draw, mouse capture (20 tests)
- Level 3: frame.rs — Smart Border with ScrollBars, hit-testing, stack-allocated FrameStyles (23 tests)
- Level 3: window.rs — Drag/resize state machine, zoom toggle, interior fill (20 tests)
- Level 3: desktop.rs — Window manager, tile, cascade, click-to-front (17 tests)
- Level 4: overlay.rs — OverlayManager, dismiss logic, overflow flip calculation (14 tests)
- Level 4: application.rs — Event loop orchestrator, dispatch chain, deferred events (12 tests)
- Level 4: dialog.rs — Modal dialog, Escape/Enter/command handling (12 tests)
- Level 5a: horizontal_bar.rs — Unified MenuBar+StatusLine, BarEntry (Action/Dropdown), Alt+letter (24 tests)
- Level 5b: msgbox.rs — Factory functions for message/confirm/error boxes (9 tests)
- Level 5c: theme_json.rs — JSON theme serialization, border presets, color roundtrip (12 tests)
- Demo: examples/demo.rs — Interactive demo, event-driven redraw, theme switching

### Remaining (Deferred to v0.2.1)
- Step 9b: MenuBar→Overlay dropdown refactor (MenuBar currently self-draws dropdown — works but can't extend beyond clip area)
- Step 9a/c/d: Widget adaptations (minor — existing widgets work but don't use v0.2 patterns fully)

### Build Order (10 Steps)
1. Container submodules (rename Group→Container, split mod/dispatch/draw)
2. View trait extensions (deferred events + lifecycle hooks)
3. Frame (Smart Border with ScrollBars)
4. Window (Frame + Interior Container, Drag/Resize state machine)
5. Desktop (Window Manager)
6. Overlay system
7. Application (Event Loop, Coalescing)
8. Dialog (Modal)
9. Widget adaptations (MenuBar→Overlay dropdown, MenuBox overflow)
10. MsgBox + Demo

## Resolved Bugs (v0.1.0 → v0.2.0)

All seven v0.1.0 bugs have been resolved in the v0.2 rebuild:

| Bug | Issue | Resolution |
|-----|-------|------------|
| #1 Background noisy | `'░'` desktop char | Changed to `' '` with dark RGB background via theme |
| #2 Interior bleed-through | Window didn't fill interior | `fill_interior()` in `window.rs` fills with theme color |
| #3 Resize only shrinks (CRITICAL) | No mouse capture | `container/dispatch.rs` routes Drag/Up to focused child with SF_DRAGGING/SF_RESIZING |
| #4 Drag lag (50ms poll) | Slow poll interval | Changed to 16ms + event-driven redraw (only redraws on events) |
| #5 Escape quits | Wrong quit key | Alt+X quits (Borland convention) |
| #6 Alt+letter inactive menu | Events not routed | HorizontalBar handles Alt+letter directly via OF_PRE_PROCESS dispatch |
| #7 Resize grip invisible | `"◘"` in DarkGray | Changed to `'⋱'` (U+22F1), theme-configurable via `resize_grip_char` |

## Known Issues (v0.2.0)

### Issue 1: MenuBar dropdown is self-drawn (Step 9b)
- **File:** `src/horizontal_bar.rs` (`draw_dropdown` method)
- **Issue:** MenuBar draws its dropdown box directly instead of using the OverlayManager. This works correctly but dropdowns can't extend beyond the bar's clip area if windows overlap.
- **Plan:** Refactor to delegate dropdown rendering to OverlayManager + MenuBox (v0.2.1).

### Issue 2: JSON theme rendering may feel slower than TV built-in
- **Cause:** NOT disk I/O — themes are loaded once at startup into in-memory registry. The `with_current()` call is a cheap thread-local RefCell borrow. Perceived difference is because JSON themes use `Color::Rgb(r,g,b)` (24-bit) which some terminals render slower than CGA 16-color palette used by the Turbo Vision theme.**Mitigation:** None needed at library level. Terminal emulator choice (e.g., Alacritty/WezTerm vs. Windows Terminal) affects 24-bit color performance.

### Issue 3: FrameStyles clones close_button_text every draw (FIXED)
- Replaced `String` heap allocation with stack-allocated `[char; 8]` array in `FrameStyles`. Zero heap allocations during frame drawing now.

## Agent Strategy

- **~70% developer-mid** (Kimi K2): Logic, state machines, widget implementations
- **~30% developer** (Sonnet): Ratatui rendering, complex mouse/integration code, debugging
- **developer-mini** (free): Config changes, small mechanical edits, boilerplate

## Related Projects

| Project | Path | Relationship |
|---------|------|-------------|
| four-code | `~/four-code` | Consumer — terminal IDE that will use turbo-tui for windowing |
| ADR-002 | `~/four-code/docs/ADR-002-turbo-tui-windowing.md` | Architecture decision record |
| v0.2 Plan | `docs/PLAN-v0.2.md` | Detailed architecture plan for v0.2 rebuild |
