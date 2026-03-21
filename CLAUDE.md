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

Single crate, 18 source files (~8,500+ lines total):

```
src/
├── lib.rs              # Public API + prelude exports
├── command.rs          # CommandId (u16), CommandSet bitfield, 25+ CM_* constants
├── theme.rs            # Theme struct, 4 Themes (dark, borland_classic, modern, matrix)
├── view.rs             # View trait, ViewBase, StateFlags, OptionFlags, Event system, deferred queue, lifecycle hooks
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
├── menu_bar.rs         # Horizontal menu bar: dropdown activation, ~X~ hotkeys, Alt+letter
├── menu_box.rs         # Standalone dropdown menu box
├── status_line.rs      # Context-sensitive status bar with OF_PRE_PROCESS
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
cargo test                      # Run all 222 tests
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
| Change menu behavior | `src/menu_bar.rs`, `src/menu_box.rs` |
| Change background/theming | `src/desktop.rs` (draw_background), `src/frame.rs` (styles) |
| Add commands | `src/command.rs` (CM_* constants) |
| Demo changes | `examples/demo.rs` |

## v0.2 Rebuild — Current State

**Branch:** `v0.2-rebuild` | **Plan:** [`docs/PLAN-v0.2.md`](docs/PLAN-v0.2.md)

### Completed (Steps 1-8, 10)
- Level 0: command.rs + theme.rs (unchanged from v0.1)
- Level 1: view.rs — dirty-flag, clip semantics, deferred events, lifecycle hooks (21 tests)
- Level 2: container/ — renamed from Group, split into mod/dispatch/draw submodules (16 tests)
- Level 3: frame.rs — Smart Border with ScrollBars, hit-testing (23 tests)
- Level 3: window.rs — Drag/resize state machine, zoom toggle (20 tests)
- Level 3: desktop.rs — Window manager, tile, cascade, click-to-front (17 tests)
- Level 4: overlay.rs — OverlayManager, dismiss logic, overflow calculation (14 tests)
- Level 4: application.rs — Event loop orchestrator, dispatch chain, deferred events (9 tests)
- Level 4: dialog.rs — Modal dialog, Escape/Enter/command handling (12 tests)
- Level 5b: msgbox.rs — Factory functions for message/confirm/error boxes (9 tests)
- Demo: examples/demo.rs — Interactive demo using Application struct

### Remaining (Deferred to v0.2.1)
- Step 9b: MenuBar→Overlay dropdown refactor (MenuBar currently self-draws dropdown)
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

## Known Bugs (v0.1.0 — Addressed by v0.2 Rebuild)

### Bug 1: Background pattern too noisy
- **File:** `src/desktop.rs` line 55
- **Issue:** Default `background_char` is `'░'` (light shade) — looks busy/noisy
- **Fix:** Change to `' '` with `Style::default().bg(Color::Blue)` (Borland classic blue desktop)
- **Also update:** Test `test_desktop_new` at line 302 asserts `background_char == '░'`

### Bug 2: Window interior not filled
- **File:** `src/window.rs` lines 286-298 (`draw()`)
- **Issue:** Window draws frame + children but doesn't fill the interior area. If children don't cover the full interior, the background bleeds through, making text unreadable.
- **Fix:** After drawing frame, fill `interior_area` with background color (e.g., `Color::Blue` or `Color::DarkGray`) before drawing children.

### Bug 3: Resize only works shrinking, not growing (CRITICAL)
- **File:** `src/group.rs` lines 401-421 (`dispatch_event`, `EventKind::Mouse` arm)
- **Issue:** Mouse events are routed via hit-testing (`col >= bounds.x && col < bounds.x + bounds.width`). When resizing a window LARGER, the mouse cursor moves OUTSIDE the window's current bounds. The `Drag` event then doesn't hit-test against the window, so it never reaches the window's resize handler.
- **Root cause:** No mouse capture — drag/resize events must go to the focused child regardless of mouse position.
- **Fix:** In the Mouse arm of `dispatch_event`, check if the event is a `MouseEventKind::Drag(_)` or `MouseEventKind::Up(_)`. If so, route to the focused child FIRST (regardless of hit-test). If the focused child doesn't consume it, fall through to normal hit-testing. This implements "mouse capture" — once a drag starts, the dragging view receives all subsequent drag/up events.

### Bug 4: Drag is slow (input lag)
- **File:** `examples/demo.rs` line 206
- **Issue:** `event::poll(Duration::from_millis(50))` — 50ms poll means max 20 FPS for mouse events
- **Fix:** Change to `Duration::from_millis(16)` (~60 FPS)

### Bug 5: Quit key is Escape instead of Alt+X
- **File:** `examples/demo.rs` lines 214-217
- **Issue:** Escape quits the app. Borland convention is Alt+X to quit.
- **Fix:** Change `key.code == KeyCode::Esc` to check for `KeyCode::Char('x')` with `KeyModifiers::ALT`. Also update the status line text from `"~Esc~ Quit"` to `"~Alt+X~ Quit"` (line 117). Update the doc comment at top of file (line 7).

### Bug 6: Alt+letter doesn't open menu when menu is inactive
- **File:** `examples/demo.rs` lines 226-235
- **Issue:** Alt+key events only reach `menu_bar` when `menu_bar.is_active()` or when `key.code == KeyCode::F(10)`. The menu_bar already has Alt+letter handling (menu_bar.rs lines 697-704), but the demo doesn't route Alt+key events to it when inactive.
- **Fix:** Add a condition: if the key has `KeyModifiers::ALT` modifier, also pass the event to `menu_bar.handle_event()`.

### Bug 7: Resize grip not visible enough
- **File:** `src/frame.rs` line 283 (character), line 202-204 (style)
- **Issue:** Resize grip uses `"◘"` (U+25D8) in `Color::DarkGray` — barely visible
- **Fix:** Change character to `"⋱"` (U+22F1, down-right diagonal ellipsis) and style to `Color::Cyan` or `Color::White` to match the active window border color.

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
