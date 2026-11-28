# turbo-tui

[![CI](https://github.com/four-bytes/turbo-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/four-bytes/turbo-tui/actions)
[![Crates.io](https://img.shields.io/crates/v/turbo-tui.svg)](https://crates.io/crates/turbo-tui)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Standalone Rust terminal UI crate — Borland Turbo Vision windowing patterns.

Overlapping windows, modal dialogs, dropdown menus, and more — rendered through turbo-tui's own `core`/`Canvas`/`Terminal` stack (crossterm cell diff).

## Features

- **Command System** — `u16` command IDs with bitfield enable/disable
- **View Trait** — Base trait with `ViewId`, focus management, state flags
- **Container** — Z-order management, three-phase event dispatch
- **Overlapping Windows** — Drag by title bar, resize from corner, zoom toggle
- **Modal Dialogs** — Self-contained event loop, OK/Cancel/Yes/No
- **Menu Bar** — Dropdown submenus via OverlayManager, `~X~` hotkey markers, Alt+Letter
- **Status Bar** — Context-sensitive shortcuts, click-to-execute
- **Scrollbars** — Vertical/horizontal with draggable thumb, active/inactive styling
- **JSON Themes** — 7 built-in themes with full JSON serialization
- **Inline Links** — in-app link activation plus optional OSC 8 (off by default under mouse capture)

## Hyperlinks and underline policy

turbo-tui captures the mouse (terminal mode 1003) for drag, resize and in-app
link activation. In that mode Windows Terminal keeps its hyperlink **hover
decoration** — dotted normally, solid while hovered — on the old cells after a
redraw, and OSC 8 cannot force the terminal to invalidate it. Emission is
therefore **off by default while mouse capture is active**. Links still render
with the theme underline and remain activatable in-app: a plain click or
Ctrl+click (or Enter on a focused `LinkLabel`) posts `CM_OPEN_LINK`.

Detection is unchanged: `Application::hyperlink_support()` still reports whether
the terminal is capable. Emission applies the mouse-capture policy on top:

- `TURBO_TUI_HYPERLINKS=on` (or `1`/`true`) forces OSC 8 emission.
- `Application::with_hyperlinks(HyperlinkMode::On)` forces it from the builder.
- `Application::with_mouse_capture(false)` keeps `Auto` detection active for
  consumers that do not capture the mouse.
- `TURBO_TUI_HYPERLINKS=off` disables emission when no builder override is set.

Consumers pass the resulting flag to
`Terminal::set_hyperlinks(app.hyperlink_support().enabled)`. When emission is
on, every link is written as `ESC ] 8 ; id=<id> ; <url> ESC \`, using the
buffer's stable link id so the terminal can track a link region exactly. The
sequence is closed before every cursor move and at the end of every frame.
Plain, non-ASCII and wide cells are always positioned with an explicit cursor
move; the cursor-advance shortcut is used only for adjacent ASCII single-width
linked cells, so terminals that render an ambiguous-width glyph (such as the em
dash) wider than the buffer can never shift the following text.

When a cell is actually wrapped in OSC 8, turbo-tui suppresses that cell's SGR
underline (`SGR 4`), so the terminal's own decoration is the only underline.
Links that are not emitted — the default under mouse capture, non-URL targets
(`FilePos`, `IssueRef`, `Command`), or a URL rejected by the scheme allowlist —
keep the `link` / `link_hover` / `link_visited` theme pens, including their SGR
underline.

Link hover is transient: the `link_hover` pen is cleared on pointer leave, window
move and focus change, and a focused link uses the plain/visited pen. Focus is an
activation state (Enter/Space), not a hover state, so a focused link cannot leave
a stale hover underline behind.

## Capturing a terminal trace

Set `TURBO_TUI_TRACE=<file>` to record every byte turbo-tui sends to the terminal,
one framed block per frame, together with the back-buffer symbols, modifiers and
link ids. The trace can be replayed into the VT screen model used by the terminal
tests.

The demo has a **Control** window with plain underlined text (no link, no OSC 8).
Drag it and the **Links** window the same way: if the Control window leaves the
same line artefacts, the terminal is rendering them, not turbo-tui's link path.

From WSL/Linux, capture once with:

```bash
TURBO_TUI_TRACE=/tmp/turbo-tui-trace.bin TURBO_TUI_HYPERLINKS=on cargo run --example demo
```

From PowerShell:

```powershell
$env:TURBO_TUI_TRACE="$PWD\turbo-tui-trace.bin"; $env:TURBO_TUI_HYPERLINKS="on"; cargo run --example demo
```

Reproduce the artefact, quit (Alt+X), then drop the file at
`tests/fixtures/wt-trace.bin` (or replace the committed
`tests/fixtures/wt-trace-frame1.bin`) and run
`cargo test --all-features trace_fixture`.
The replay test parses each frame, applies the terminal bytes to the VT model and
compares it cell by cell with the captured back-buffer snapshot; the first
diverging frame/cell identifies the bug deterministically.

## Status

🟢 **v0.4.0 — Released** — 565 unit + 8 integration + 4 doc tests, 34 source files, ~24,000 lines. Clippy pedantic clean, zero unsafe.

## Quick Start

```toml
[dependencies]
turbo-tui = { git = "https://github.com/four-bytes/turbo-tui" }
```

## Usage Examples

Imports mirror `examples/demo.rs` (the canonical example). `turbo_tui::prelude::*`
exports the core types (`Application`, `Window`, `Dialog`, `Desktop`, `Rect`,
`Terminal`, `CellCanvas`, `CM_*`, …) but **not** widgets like `Button` /
`StaticText` or the menu/status constructors — those live in their own modules.

### Window with Builder Lite

```rust
use turbo_tui::button::Button;
use turbo_tui::command::CM_OK;
use turbo_tui::core::Rect;
use turbo_tui::desktop::Desktop;
use turbo_tui::window::Window;

let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

// Builder Lite pattern
let mut win = Window::new(Rect::new(5, 5, 40, 15), "Editor")
    .with_min_size(20, 8)
    .with_scrollbars(true, false)
    .with_closeable(true)
    .with_resizable(true)
    .with_content_size(120, 40);

win.add(Box::new(Button::new(Rect::new(5, 10, 10, 1), "OK", CM_OK, true)));
desktop.add_window(win); // Window by value -> ViewId

// Preset constructors
let _editor = Window::editor(Rect::new(5, 3, 35, 12), "Editor");
let _palette = Window::palette(Rect::new(50, 2, 20, 10), "Colors");
let _tool = Window::tool(Rect::new(1, 20, 15, 5), "Tools");
```

### Menu Bar

```rust
use turbo_tui::command::{CM_OPEN, CM_QUIT, CM_SAVE};
use turbo_tui::core::Rect;
use turbo_tui::menu_bar::{menu_bar_from_menus, Menu, MenuItem};

let menu = Menu::new("~F~ile", vec![
    MenuItem::new("~O~pen", CM_OPEN),
    MenuItem::new("~S~ave", CM_SAVE),
    MenuItem::separator(),
    MenuItem::new("E~x~it", CM_QUIT),
]);

let menu_bar = menu_bar_from_menus(Rect::new(0, 0, 80, 1), vec![menu]);
```

### Status Bar

```rust
use turbo_tui::command::{CM_NEXT_THEME, CM_QUIT};
use turbo_tui::core::Rect;
use turbo_tui::status_bar::{status_bar_from_items, StatusItem, KB_ALT_X, KB_F2};

let status_bar = status_bar_from_items(
    Rect::new(0, 23, 80, 1),
    vec![
        StatusItem::new("~Alt+X~ Quit", CM_QUIT, KB_ALT_X),
        StatusItem::new("~F2~ Theme", CM_NEXT_THEME, KB_F2),
    ],
);
```

### Modal Dialog

```rust
use turbo_tui::button::Button;
use turbo_tui::command::{CM_NO, CM_YES};
use turbo_tui::core::Rect;
use turbo_tui::dialog::Dialog;
use turbo_tui::static_text::StaticText;

let mut dlg = Dialog::new(Rect::new(20, 8, 40, 10), "Confirm");
dlg.add(Box::new(StaticText::new(Rect::new(5, 3, 30, 1), "Save changes?")));
dlg.add(Box::new(Button::new(Rect::new(5, 6, 10, 1), "~Y~es", CM_YES, true)));
dlg.add(Box::new(Button::new(Rect::new(20, 6, 10, 1), "~N~o", CM_NO, false)));
```

There is no `Dialog::run` — the dialog is a `View`. Your event loop draws it
via a `Canvas` and dispatches events to it; once a button closes the dialog,
`dlg.result()` yields the closing command:

```rust
if let Some(cmd) = dlg.result() {
    // cmd == CM_YES or CM_NO — the command that closed the dialog
}
```

### Application Event Loop

```rust
use turbo_tui::application::Application;
use turbo_tui::canvas::CellCanvas;
use turbo_tui::command::CM_QUIT;
use turbo_tui::core::Rect;
use turbo_tui::terminal::Terminal;

let mut app = Application::new(Rect::new(0, 0, 80, 24)); // plain value, not a Result
app.set_menu_bar(menu_bar);
app.set_status_bar(status_bar);
app.add_window(win); // Window by value -> ViewId

let mut terminal = Terminal::new(std::io::stdout(), 80, 24);

// The event loop is consumer-owned. Simplified below — see examples/demo.rs
// for the full crossterm setup (raw mode, alternate screen, mouse capture,
// event coalescing, frame pacing).
loop {
    // 1. Poll crossterm yourself -> Vec<crossterm::event::Event>, then dispatch:
    app.handle_crossterm_events(&events);

    // 2. Drain app-level commands (CM_QUIT, CM_TILE, …):
    if let Some(cmd) = app.take_unhandled_command() {
        if cmd == CM_QUIT {
            app.quit();
        }
    }

    // 3. Render one frame; the closure gets &mut Buffer and returns the cursor:
    terminal.draw(|buf| app.draw(&mut CellCanvas::new(buf)))?;

    if !app.is_running() {
        break;
    }
}
```

## API Documentation

### `View` Trait

Every interactive element implements `View`:

- `draw(&self, canvas: &mut dyn Canvas, clip: Rect)` — Render the view
- `handle_event(&mut self, event: &mut Event)` — Handle input events (set `event.handled = true` to consume)
- `state(&self) -> u16` / `set_state(&mut self, state: u16)` — Query/set state flags (`SF_*`)
- `options(&self) -> u16` — Option flags (`OF_*`, e.g. `OF_PRE_PROCESS`)
- `id(&self) -> ViewId` — Stable identity (atomic counter, survives moves)
- `bounds(&self) -> Rect` / `set_bounds(&mut self, bounds: Rect)` — Geometry
- `cursor_position(&self) -> Option<Position>` — Optional cursor position
- `on_focus(&mut self)` / `on_blur(&mut self)` — Lifecycle hooks

### `Application` Struct

Orchestrates windows, menu bar, status bar and event dispatch. The event loop
itself is consumer-owned:

- `Application::new(screen_size: Rect) -> Application` — Construct (plain value, not a `Result`)
- `add_window(&mut self, window: Window) -> ViewId` — Add a window to the desktop
- `set_menu_bar(&mut self, menu_bar: MenuBar)` / `set_status_bar(&mut self, status_bar: StatusBar)` — Attach optional bars
- `handle_crossterm_event(&mut self, event: &crossterm::event::Event)` / `handle_crossterm_events(&mut self, events: &[crossterm::event::Event])` — Dispatch input (coalesces mouse motion)
- `take_unhandled_command(&mut self) -> Option<CommandId>` — Drain app-level commands
- `is_running(&self) -> bool` / `quit(&mut self)` — Loop control
- `draw(&mut self, canvas: &mut dyn Canvas) -> Option<Position>` — Render one frame, returns the cursor position
- `resize(&mut self, width: u16, height: u16)` — Apply a terminal resize

### `Window` Struct

Overlapping window with drag, resize, and zoom:

- `Window::new(bounds: Rect, title: &str)` — Base constructor
- `Window::editor/palette/tool(bounds: Rect, title: &str)` — Presets
- `Window::with_config(bounds: Rect, title: &str, config: impl Into<FrameConfig>)` — From config
- `with_min_size(self, w: u16, h: u16) -> Self` — Builder: minimum size
- `with_drag_limits(self, limits: Rect) -> Self` — Builder: drag bounds
- `with_scrollbars(self, v: bool, h: bool) -> Self` — Builder: scrollbars
- `with_closeable(self, v: bool) -> Self` — Builder: close button
- `with_resizable(self, v: bool) -> Self` — Builder: resize handle
- `with_content_size(self, w: u16, h: u16) -> Self` — Builder: logical content size for scrolling
- `add(&mut self, child: Box<dyn View>) -> ViewId` — Add a child view
- `minimize(&mut self)` — Collapse to task shelf (managed by Desktop)
- `minimized_width(&self) -> u16` — Width when minimized

## Architecture

turbo-tui is a standalone terminal UI crate with its own cell-based rendering backend: `turbo_tui::core` (`Buffer`, `Cell`, `Rect`, `Style`, …), `turbo_tui::terminal::Terminal` (persistent front/back buffers, cell diff via crossterm) and `turbo_tui::canvas::Canvas`. The windowing patterns are adapted from [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) (MIT licensed); no source code was copied.

```
Application
└── turbo-tui Terminal (crossterm diff)
    └── turbo-tui views
        ├── Desktop (background + window manager)
        │   ├── Window 1 (back)
        │   ├── Window 2
        │   └── Window N (front/focused)
        ├── MenuBar (optional)
        └── StatusBar (optional)
```

## Contributing

### Running Tests

```bash
cargo test                      # Run all 565 tests
cargo clippy -- -D warnings     # Lint (pedantic enabled)
cargo fmt                       # Format code
cargo run --example demo        # Run the interactive demo
```

### Submitting PRs

- Branch naming: `feature/<name>`, `fix/<name>`, or `refactor/<name>`
- Include tests for new functionality
- Ensure `cargo clippy -- -D warnings` passes
- Update `HISTORY.md` with a changelog entry
- Keep doc comments on all public items

## Links

- [VISION.md](VISION.md) — Product vision: stack, principles, target groups, milestones
- [ROADMAP.md](ROADMAP.md) — Version roadmap and future phases
- [HISTORY.md](HISTORY.md) — Detailed change log
- [docs/](docs/) — Architecture plans and design documents
  - [PLAN-v0.2.md](docs/PLAN-v0.2.md) — v0.2 rebuild architecture plan
  - [PLAN-v0.2.1.md](docs/PLAN-v0.2.1.md) — v0.2.1 progression plan
  - [PLAN-v0.2.2.md](docs/PLAN-v0.2.2.md) — MenuBar→Overlay refactor plan
- [ARCHITECTURE.md](ARCHITECTURE.md) — System diagram and module dependencies
- [STANDARDS.md](STANDARDS.md) — Coding standards and conventions
- [TESTING.md](TESTING.md) — Test architecture and patterns

## Inspiration

Having grown up with Turbo Pascal 5, TASM, and later Borland Pascal for Protected Mode — the Turbo Vision UI framework left a lasting impression. This crate brings those [Borland Turbo Vision](https://en.wikipedia.org/wiki/Turbo_Vision) patterns (1991) to modern Rust terminal applications. The original TV powered Turbo Pascal and Turbo C++ IDEs. Norton Commander's dual-panel paradigm will also influence future development.

## License

Apache-2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE). Versions up to and including v0.4.0 were released under MIT.
