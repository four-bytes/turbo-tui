# turbo-tui

[![CI](https://github.com/four-bytes/turbo-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/four-bytes/turbo-tui/actions)
[![Crates.io](https://img.shields.io/crates/v/turbo-tui.svg)](https://crates.io/crates/turbo-tui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Borland Turbo Vision windowing patterns for [Ratatui](https://github.com/ratatui/ratatui).

Overlapping windows, modal dialogs, dropdown menus, and more — all rendering through Ratatui's `Frame`/`Buffer` system.

## Features

- **Command System** — `u16` command IDs with bitfield enable/disable
- **View Trait** — Base trait with `ViewId`, focus management, state flags
- **Container** — Z-order management, three-phase event dispatch
- **Overlapping Windows** — Drag by title bar, resize from corner, zoom toggle
- **Modal Dialogs** — Self-contained event loop, OK/Cancel/Yes/No
- **Menu Bar** — Dropdown submenus via OverlayManager, `~X~` hotkey markers, Alt+Letter
- **Status Bar** — Context-sensitive shortcuts, click-to-execute
- **Scrollbars** — Vertical/horizontal with draggable thumb, active/inactive styling
- **JSON Themes** — 6 built-in themes with full JSON serialization

## Status

🟢 **v0.2.3 — Released** — 357 tests, 21 source files, ~15,000 lines. Clippy pedantic clean, zero unsafe.

## Quick Start

```toml
[dependencies]
turbo-tui = { git = "https://github.com/four-bytes/turbo-tui" }
```

## Usage Examples

### Window with Builder Lite

```rust
use turbo_tui::prelude::*;

let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24));

// Basic window
let mut win = Window::new(Rect::new(5, 5, 40, 15), "Editor");

// Builder Lite pattern
let mut win = Window::new(Rect::new(5, 5, 40, 15), "Editor")
    .with_min_size(20, 8)
    .with_scrollbars(true, false)
    .with_closeable(true)
    .with_resizable(true);

// Preset constructors
let mut palette = Window::palette(Rect::new(50, 2, 20, 10), "Colors");
let mut tool = Window::tool(Rect::new(1, 20, 15, 5), "Tools");

win.insert(Box::new(Button::new("OK", CM_OK, Rect::new(5, 10, 10, 1))));
desktop.add_window(Box::new(win));
```

### Menu Bar

```rust
use turbo_tui::prelude::*;

let menu = Menu::new("~F~ile", vec![
    MenuItem::action("~O~pen", CM_OPEN),
    MenuItem::action("~S~ave", CM_SAVE),
    MenuItem::separator(),
    MenuItem::action("E~x~it", CM_QUIT),
]);

let mut menu_bar = MenuBar::new(Rect::new(0, 0, 80, 1));
menu_bar.add_entries(menu_bar_from_menus(vec![menu]));
```

### Modal Dialog

```rust
use turbo_tui::prelude::*;

let mut dlg = Dialog::new(Rect::new(20, 8, 40, 10), "Confirm");
dlg.insert(Box::new(StaticText::new("Save changes?", Rect::new(5, 3, 30, 1))));
dlg.insert(Box::new(Button::new("~Y~es", CM_YES, Rect::new(5, 6, 10, 1))));
dlg.insert(Box::new(Button::new("~N~o", CM_NO, Rect::new(20, 6, 10, 1))));

// Run modal — returns the command that closed it
let result = dlg.run(&mut terminal, &mut theme)?;
```

### Application Event Loop

```rust
use turbo_tui::prelude::*;

let mut app = Application::new()?;
app.set_menu_bar(menu_bar);
app.set_status_bar(status_bar);
app.set_desktop(desktop);

app.run()?; // Blocks until CM_QUIT
```

## API Documentation

### `View` Trait

Every interactive element implements `View`:

- `draw(&self, buf: &mut Buffer, theme: &Theme, clip: ClipRegion)` — Render the view
- `handle_event(&mut self, event: &mut Event) -> bool` — Handle input events (`true` = consumed)
- `state_flags(&self) -> StateFlags` / `set_state_flags(&mut self, flags: StateFlags)` — Query/set state
- `bounds(&self) -> Rect` / `set_bounds(&mut self, bounds: Rect)` — Geometry
- `view_id(&self) -> ViewId` — Stable identity (atomic counter, survives moves)
- `cursor_position(&self) -> Option<Position>` — Optional cursor position for Ratatui

### `Application` Struct

Orchestrates the entire event loop:

- `run(&mut self) -> Result<()>` — Main event loop (blocks until `CM_QUIT`)
- `post_event(&mut self, event: Event)` — Queue deferred events
- `theme(&self) -> &Theme` / `set_theme(&mut self, theme: Theme)` — Global theme
- `set_menu_bar(&mut self, menu_bar: MenuBar)` — Attach optional menu bar
- `set_status_bar(&mut self, status_bar: StatusLine)` — Attach optional status bar
- `set_desktop(&mut self, desktop: Desktop)` — Attach desktop with windows

### `Window` Struct

Overlapping window with drag, resize, and zoom:

- `Window::new(bounds: Rect, title: &str)` — Base constructor
- `Window::with_config(bounds: Rect, title: &str, config: FrameConfig)` — From config
- `with_min_size(self, w: u16, h: u16) -> Self` — Builder: minimum size
- `with_drag_limits(self, limits: Rect) -> Self` — Builder: drag bounds
- `with_scrollbars(self, v: bool, h: bool) -> Self` — Builder: scrollbars
- `with_closeable(self, v: bool) -> Self` — Builder: close button
- `with_resizable(self, v: bool) -> Self` — Builder: resize handle
- `minimize(&mut self)` — Collapse to task shelf (managed by Desktop)
- `minimized_width(&self) -> u16` — Width when minimized

## Architecture

turbo-tui does **not** replace Ratatui — it extends it. All rendering goes through Ratatui's `Frame` and `Buffer`. The patterns are adapted from [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) (MIT licensed), reimplemented for the Ratatui ecosystem.

```
Application
└── Ratatui Terminal
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
cargo test                      # Run all 357 tests
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

MIT — see [LICENSE](LICENSE)
