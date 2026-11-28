# turbo-tui — Testing Guide

> Last updated: 2026-03-22

## Quick Commands

```bash
cargo test                          # Run all tests (284 unit + 2 doc)
cargo test -- --nocapture           # Show println! output
cargo test test_window              # Run tests matching pattern
cargo test -- --test-threads=1      # Sequential (needed if tests share theme state)
cargo clippy -- -D warnings         # Lint check (pedantic enabled)
cargo fmt --check                   # Format check (dry-run)
```

## Test Architecture

### Location
All tests are co-located with their implementation:
```
src/module.rs
    // ... production code ...
    #[cfg(test)]
    mod tests {
        use super::*;
        // tests here
    }
```

Unit tests are co-located in `#[cfg(test)] mod tests`. The single exception is the opt-in end-to-end suite in `tests/e2e.rs` (see "End-to-End tmux Tests" below), which is `#[ignore]`d by default.

### Test Count by Module (as of v0.2.1-dev)

| Module | Tests | Notes |
|--------|-------|-------|
| `command.rs` | 4 | CommandId, CommandSet bitfield |
| `view.rs` | 16 | ViewId, Event, StateFlags, deferred queue |
| `container/mod.rs` | 20 | Z-order, focus, three-phase dispatch, mouse capture |
| `frame.rs` | 23 | Borders, title, close button, resize handle, hover, scrollbars |
| `window.rs` | 20 | Drag/resize state machine, zoom, minimize |
| `desktop.rs` | 17 | Background, tile, cascade, click-to-front |
| `overlay.rs` | 14 | Overlay stack, dismiss logic, overflow flip |
| `application.rs` | 12 | Dispatch chain, resize, deferred events |
| `dialog.rs` | 12 | Modal, Escape/Enter/command handling |
| `horizontal_bar.rs` | 26 | BarEntry, dropdown, keyboard, Alt+letter |
| `menu_bar.rs` | 8 | MenuItem, Menu, backward-compat wrapper |
| `status_line.rs` | 8 | StatusItem, KB_* constants |
| `menu_box.rs` | 15 | Dropdown rendering, keyboard navigation |
| `scrollbar.rs` | 20 | Thumb position, drag, active/inactive |
| `button.rs` | 14 | Click, hotkey, focus |
| `static_text.rs` | 6 | Rendering, centering |
| `msgbox.rs` | 9 | Factory functions, button layout |
| `clip.rs` | 6 | Intersection clipping |
| `theme.rs` | 8 | Style validation, registry, cycling |
| `theme_json.rs` | 12 | JSON roundtrip, color parsing, border presets |
| **Total** | **~284** | |

### Doc Tests
- 2 runnable doc tests, 17 ignored (require terminal context: `ignore`)
- Ignored doc tests use `/// ```ignore` because they need a live terminal

## Test Patterns

### Theme Setup
Most widget tests need the theme initialized. Use a helper function:

```rust
fn setup_theme() {
    crate::theme::set(Theme::turbo_vision());
}
```

Call `setup_theme()` at the start of each test that renders or checks theme-dependent behavior. **Thread-local theme** means tests can run in parallel without interference.

### Mouse Event Helpers
Window and container tests use helper functions for mouse events:

```rust
fn mouse_down(col: u16, row: u16) -> Event {
    Event::mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn mouse_drag(col: u16, row: u16) -> Event { ... }
fn mouse_up(col: u16, row: u16) -> Event { ... }
```

### Buffer-Based Rendering Tests
Frame and widget tests render to a `Buffer` and check cell contents:

```rust
let bounds = Rect::new(0, 0, 20, 10);
let mut buf = Buffer::empty(bounds);
let frame = Frame::new(bounds, "Test", FrameType::Window);
frame.draw(&mut buf, bounds);

// Check specific cell
let cell = buf.cell(Position::new(0, 0)).unwrap();
assert_eq!(cell.symbol(), "╔");
```

### Event Dispatch Tests
Test event handling by creating events, passing to `handle_event`, and checking if cleared:

```rust
let mut win = Window::new(bounds, "Test");
let mut event = mouse_down(close_col, title_row);
win.handle_event(&mut event);
assert!(event.is_cleared()); // Event was consumed
```

### State Flag Tests
Verify state transitions using bitfield checks:

```rust
win.set_state(win.state() | SF_FOCUSED);
assert_ne!(win.state() & SF_FOCUSED, 0);
```

## Test Naming Convention

Pattern: `test_{module}_{what}_{condition}`

Examples:
- `test_window_new_defaults` — defaults after construction
- `test_window_resize_clamps_to_min_size` — resize behavior
- `test_frame_hover_close_button` — hover state
- `test_container_focus_cycle` — focus management
- `test_scrollbar_thumb_position_at_zero` — specific state

## What to Test for New Widgets

When adding a new widget, ensure tests cover:

1. **Construction defaults** — `test_{widget}_new_defaults`
2. **View trait basics** — `id()`, `bounds()`, `set_bounds()`, `can_focus()`
3. **Rendering** — Buffer-based: check expected characters appear
4. **Event handling** — Mouse clicks, keyboard, commands
5. **State transitions** — Focus, hover, disabled
6. **Edge cases** — Zero-size bounds, empty content, boundary positions
7. **Theme interaction** — Active vs inactive styling (if applicable)

## End-to-End tmux Tests

The `e2e` integration test (`tests/e2e.rs`) drives `examples/demo.rs` in a real
tmux pane. It is ignored by default, so `cargo test` never needs tmux. Run it
explicitly:

```bash
TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored --test-threads=1 --nocapture
```

Running serially is recommended because each scenario starts its own tmux server
and shares the demo build; the harness itself is parallel-safe thanks to a
per-`Harness` socket and scratch directory.

The flag is required: without it the harness returns `None` and every test
skips. With `TURBO_TUI_E2E=1` a missing tmux or a failed demo build is a hard
panic, never a silent skip.

Without `TURBO_TUI_E2E=1` the tests are ignored during a normal `cargo test`, and
forcing them with `--ignored` still returns immediately from
`Harness::start()` -> `None` without starting tmux. A green run with `--ignored`
and no env var therefore proves nothing.

### Harness

`tests/e2e/harness.rs` starts a dedicated tmux server (own socket under a
per-process scratch directory) with an exactly 80×24 pane, a stub `xdg-open`
that logs its argument, `TERM=xterm-256color` and `TURBO_TUI_HYPERLINKS=off`.
The `Drop` guard kills the server and removes the scratch directory, so a
panicking test leaks neither a session nor files. Tests are parallel-safe
because every `Harness` gets its own socket and scratch directory.

`tmux capture-pane -e -p` is parsed into a cell grid: SGR colours (ANSI and
24-bit RGB), bold/underline/reverse, wide graphemes via `unicode-width`, and
padded short rows. Helpers find text, window frames by title, attribute cells,
background colours, and the hardware cursor (`display -p`). Mouse input is sent
as SGR sequences; coordinates are 0-based in the API and converted to the
1-based SGR wire format internally.

On WSL hosts the harness unsets `WSL_DISTRO_NAME`/`WSL_INTEROP` for the tmux
server so `open_url` takes the Linux `xdg-open` path and the test stub is
authoritative.

### Scenarios

| Test | Covers | Status on main |
|------|--------|----------------|
| `scenario_1_drag_leaves_no_underline_streaks` | Dragging a window leaves no underlined cells outside the moved frame | passes |
| `scenario_2_scrollbars_scroll_demo` | `Scroll Demo` vertical and horizontal bars: arrow = one step, track = page, thumb drag = further | passes; acceptance test for #70 on this window |
| `scenario_2b_scrollbars_editor` | Editor vertical bar: down arrow scrolls the first code line by one | passes; acceptance test for #70 on the Editor window |
| `scenario_3_editor_no_black_holes_expected_fail_73` | Editor interior keeps its background after wheel/page scrolling | #73 reproduces (expected failure): scrolled cells fall back to the window background |
| `scenario_4_cursor_visible_per_theme_expected_fail_71` | Cursor is visible and placed inside the editor for every theme | #71 reproduces (expected failure) |
| `scenario_5_link_click_reports_opened` | A link click reports `opened <url>` and runs the opener stub | passes |

Known-bug scenarios use `expect_known_bug`: the body asserts the **correct**
behaviour and fails loudly (`... no longer reproduces — remove expect_known_bug
...`) as soon as the fix lands. When that happens, delete the wrapper and keep
the assertion.

## CI Expectations

No CI pipeline yet (private repo). Before committing:
```bash
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

All three must pass. Zero warnings policy.
