# turbo-tui

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

🟢 **v0.2.2 — Released** — 335 tests, 21 source files, ~15,000 lines. Clippy pedantic clean, zero unsafe.

## Quick Start

```toml
[dependencies]
turbo-tui = { git = "https://github.com/four-bytes/turbo-tui" }
```

## Architecture

turbo-tui does **not** replace Ratatui — it extends it. All rendering goes through Ratatui's `Frame` and `Buffer`.

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

## Inspiration

Having grown up with Turbo Pascal 5, TASM, and later Borland Pascal for Protected Mode — the Turbo Vision UI framework left a lasting impression. This crate brings those [Borland Turbo Vision](https://en.wikipedia.org/wiki/Turbo_Vision) patterns (1991) to modern Rust terminal applications. The original TV powered Turbo Pascal and Turbo C++ IDEs. Norton Commander's dual-panel paradigm will also influence future development.

## License

MIT — see [LICENSE](LICENSE)
