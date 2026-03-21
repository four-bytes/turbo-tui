# turbo-tui — Claude Code Configuration

## Project Overview

**turbo-tui** is a Ratatui extension crate that brings Borland Turbo Vision windowing patterns to modern Rust terminal applications.

- **Language:** Rust
- **Status:** v0.1.0 — Core Complete (14 modules, 157 tests)
- **Org:** four-bytes (Four\ namespace)
- **Crate type:** Library (no binary)
- **Repo:** https://github.com/four-bytes/turbo-tui
- **Consumer:** `four-code` terminal IDE (`~/four-code`)
- **Deps:** ratatui 0.29, crossterm 0.28

## Architecture

Single crate, 14 modules (~8,100 lines total):

```
src/
├── lib.rs              # Public API + prelude exports (57 lines)
├── command.rs          # CommandId (u16), CommandSet bitfield, 25+ CM_* constants
├── view.rs             # View trait, ViewId (atomic), StateFlags, OptionFlags, Event system (698 lines, 17 tests)
├── group.rs            # Container with Z-order, three-phase event dispatch (887 lines, 11 tests)
├── frame.rs            # Window borders, 3 frame types (Window/Dialog/Single), close/resize handles (606 lines, 19 tests)
├── window.rs           # Overlapping windows: drag, resize, zoom toggle (735 lines, 13 tests)
├── desktop.rs          # Window manager: tile, cascade, click-to-focus, background (477 lines, 10 tests)
├── dialog.rs           # Modal dialogs: Esc/Enter/command handling (570 lines, 12 tests)
├── menu_bar.rs         # Horizontal menu bar: dropdown activation, ~X~ hotkeys, Alt+letter (1021 lines, 12 tests)
├── menu_box.rs         # Standalone dropdown menu box (572 lines, 8 tests)
├── status_line.rs      # Context-sensitive status bar with OF_PRE_PROCESS (646 lines, 7 tests)
├── scrollbar.rs        # Vertical/horizontal scrollbar with thumb drag (712 lines, 11 tests)
├── button.rs           # Clickable button with hotkey support (369 lines, 11 tests)
├── static_text.rs      # Non-interactive text label (247 lines, 8 tests)
└── msgbox.rs           # Pre-built message/confirm/error dialog factories (354 lines, 9 tests)
examples/
└── demo.rs             # Interactive demo: Desktop + MenuBar + Windows + Buttons + StatusLine (334 lines)
```

## Development Workflow

```bash
cargo check                     # Quick syntax check (fastest)
cargo test                      # Run all 157 tests
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
| Change event routing | `src/group.rs` (dispatch_event, lines 362-436) |
| Change menu behavior | `src/menu_bar.rs`, `src/menu_box.rs` |
| Change background/theming | `src/desktop.rs` (draw_background), `src/frame.rs` (styles) |
| Add commands | `src/command.rs` (CM_* constants) |
| Demo changes | `examples/demo.rs` |

## Known Bugs (v0.1.0 — To Fix)

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
