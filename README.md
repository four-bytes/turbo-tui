# turbo-tui

Borland Turbo Vision windowing patterns for [Ratatui](https://github.com/ratatui/ratatui).

Overlapping windows, modal dialogs, dropdown menus, and more — all rendering through Ratatui's `Frame`/`Buffer` system.

## Features

- **Command System** — `u16` command IDs with bitfield enable/disable
- **View Trait** — Base trait with `ViewId`, focus management, state flags
- **Group Container** — Z-order management, three-phase event dispatch
- **Overlapping Windows** — Drag by title bar, resize from corner, zoom toggle
- **Modal Dialogs** — Self-contained event loop, OK/Cancel/Yes/No
- **Menu Bar** — Dropdown submenus, `~X~` hotkey markers, Alt+Letter
- **Status Line** — Context-sensitive shortcuts, click-to-execute
- **Scrollbars** — Vertical/horizontal with draggable thumb

## Status

🚧 **Early Development** — Command system implemented, more coming.

## Quick Start

```toml
[dependencies]
turbo-tui = { git = "https://github.com/four-bytes/turbo-tui" }
```

## Architecture

turbo-tui does **not** replace Ratatui — it extends it. All rendering goes through Ratatui's `Frame` and `Buffer`. The patterns are adapted from [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) (MIT licensed), reimplemented for the Ratatui ecosystem.

```
Application (yours)
└── Ratatui Terminal
    └── turbo-tui widgets
        ├── Desktop (background + window manager)
        │   ├── Window 1 (back)
        │   ├── Window 2
        │   └── Window N (front/focused)
        ├── MenuBar (optional)
        └── StatusLine (optional)
```

## Roadmap

1. ✅ Command System (`command.rs`)
2. ⬜ View Trait + ViewId
3. ⬜ Group Container
4. ⬜ Frame + Window
5. ⬜ Desktop
6. ⬜ Dialog + MessageBox
7. ⬜ MenuBar
8. ⬜ StatusLine
9. ⬜ Scrollbar
10. ⬜ Button + InputLine

## Inspiration

This crate brings [Borland Turbo Vision](https://en.wikipedia.org/wiki/Turbo_Vision) patterns (1991) to modern Rust terminal applications. The original TV powered Turbo Pascal and Turbo C++ IDEs.

## License

MIT — see [LICENSE](LICENSE)
