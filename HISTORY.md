# turbo-tui — Change History

## v0.1.0 (2026-03-21)

### Initial Setup
- Project created: Borland Turbo Vision windowing patterns for Ratatui
- Command system: `CommandId` (u16), `CommandSet` (bitfield), standard command constants
- Standard commands: CM_QUIT, CM_OK, CM_CANCEL, CM_YES, CM_NO, CM_CLOSE, CM_SAVE, etc.
- INTERNAL_COMMAND_BASE (1000) convention: commands >= 1000 don't close dialogs
- 4 tests passing
- ADR-002 written in four-code documenting architecture decisions
- Pattern reference: turbo-vision-4-rust (MIT licensed)

### Complete Widget Library
- view.rs: View trait, ViewId (atomic counter), StateFlags, OptionFlags, Event system
- group.rs: Container with Z-order management, three-phase event dispatch
- frame.rs: Window borders with 3 frame types (Window, Dialog, Single), Ratatui rendering
- window.rs: Overlapping windows with drag, resize, zoom toggle
- desktop.rs: Window manager with tile, cascade, click-to-focus
- dialog.rs: Modal dialogs with Escape/Enter handling, INTERNAL_COMMAND_BASE convention
- menu_bar.rs: Horizontal menu bar with dropdown activation, ~X~ hotkeys, Alt+letter
- menu_box.rs: Dropdown menu box with keyboard navigation
- status_line.rs: Context-sensitive status bar with OF_PRE_PROCESS, clickable shortcuts
- scrollbar.rs: Vertical/horizontal scrollbar with draggable thumb
- button.rs: Clickable button with Space/Enter/mouse support
- static_text.rs: Non-interactive text label (left-aligned or centered)
- msgbox.rs: Pre-built message_box, confirm_box, confirm_cancel_box, error_box factories
- 157 tests passing, clippy pedantic clean, zero unsafe code

## v0.2.0-dev (2026-03-21)

### Architecture Rebuild
- **Container:** Renamed `Group` → `Container`, split into submodules (`container/mod.rs`, `dispatch.rs`, `draw.rs`)
- **View trait:** Added deferred event queue (`Event.deferred: Vec<Event>`, `Event::post()`), lifecycle hooks (`on_insert`, `on_remove`, `on_resize`)
- **Frame:** New Smart Border with optional `ScrollBar` integration (no `Box<dyn View>`), hit-test methods
- **Window:** Drag/resize state machine, zoom toggle, interior fill, SF_FOCUSED propagation to Frame
- **Desktop:** Window manager with click-to-front, tile, cascade, background rendering
- **Overlay:** OverlayManager with dismiss-on-escape/click, `calculate_overlay_bounds()` for overflow detection
- **Application:** Central orchestrator — dispatch chain (Overlay→MenuBar→StatusLine→Desktop), deferred event processing, screen resize handling
- **Dialog:** Modal window with Escape→CM_CANCEL, Enter→CM_OK, commands<1000 close dialog

### MsgBox + Demo
- msgbox.rs: Factory functions `message_box()`, `confirm_box()`, `confirm_cancel_box()`, `error_box()`
- demo.rs: Interactive demo using Application struct, MenuBar, StatusLine, 3 windows with buttons

### Stats
- 222 tests passing, clippy pedantic clean, zero unsafe code
- 18 source files, ~8,500+ lines

### Deferred
- MenuBar→Overlay dropdown refactor (using Overlay system instead of self-draw) — planned for v0.2.1
