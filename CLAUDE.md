# turbo-tui — Claude Code Configuration

## Project Overview

**turbo-tui** is a Ratatui extension crate providing Borland Turbo Vision windowing patterns.

- **Language:** Rust
- **Status:** Early Development
- **Org:** four-bytes
- **Binary:** Library crate (no binary)

## Architecture

Single crate with modules:

```
src/
├── lib.rs          # Public API + Prelude
├── command.rs      # Command IDs + CommandSet bitfield
├── view.rs         # View trait + ViewId (planned)
├── group.rs        # Container + Z-order (planned)
├── window.rs       # Overlapping windows (planned)
├── frame.rs        # Window borders (planned)
├── dialog.rs       # Modal dialogs (planned)
├── menu_bar.rs     # Menu bar (planned)
├── status_line.rs  # Status line (planned)
└── scrollbar.rs    # Scrollbar (planned)
```

## Development Workflow

```bash
cargo check         # Quick syntax check
cargo test          # Run all tests
cargo clippy -- -D warnings  # Lint (pedantic enabled)
cargo fmt           # Format
```

## Conventions

### Code Style
- `unsafe_code = "forbid"` — no unsafe allowed
- Clippy pedantic enabled
- All public items documented
- Tests next to implementation (`#[cfg(test)]`)

### Design Patterns
- Adapted from turbo-vision-4-rust (MIT), not copied
- Rendering through Ratatui Frame/Buffer
- CommandSet owned by Application, not global/thread-local
- View trait with ViewId (atomic counter) for stable identity
- Three-phase event dispatch: PreProcess → Focused → PostProcess
- Commands < 1000 close dialogs, >= 1000 are internal

### Reference
- [ADR-002](../four-code/docs/ADR-002-turbo-tui-windowing.md) in four-code
- [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) — pattern source
