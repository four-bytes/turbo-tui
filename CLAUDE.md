# turbo-tui — Claude Code Configuration

## Project Overview

**turbo-tui** is a Ratatui extension crate that brings Borland Turbo Vision windowing patterns to modern Rust terminal applications.

- **Language:** Rust
- **Status:** v0.2.2 — Released (v0.2 merged to `main`)
- **Org:** four-bytes (Four\ namespace)
- **Crate type:** Library (no binary)
- **Repo:** https://github.com/four-bytes/turbo-tui
- **Consumer:** `four-code` terminal IDE (`~/four-code`)
- **Deps:** ratatui 0.29, crossterm 0.28

## Architecture

Single crate, 21 source files (~14,600 lines total):

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
cargo test                      # Run all 284 tests
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
- **v0.2.1 Progression Plan:** [`docs/PLAN-v0.2.1.md`](docs/PLAN-v0.2.1.md) — Window handling, scrollbar fixes, Builder Lite, task shelf, lifecycle hooks
- **Reference Research:** [`docs/RES-0002-reference-projects-architecture.md`](docs/RES-0002-reference-projects-architecture.md) — Ratatui patterns, TachyonFX, tui-rs demo analysis

## Key Design Decisions

1. **turbo-vision-4-rust is NOT built on Ratatui** — it's a standalone framework on crossterm at the same level as Ratatui. That's why we port patterns instead of using it as a dependency.
2. **Crate name `turbo-tui`** is reserved on crates.io.
3. **Three-phase dispatch** is critical: StatusLine uses `OF_PRE_PROCESS` to intercept F-keys before they reach focused windows. Without this, F1/F5/F10 would go to the focused window first.
4. **Mouse events** route front-to-back (reverse Z-order) with hit-testing. Keyboard/command events use three-phase dispatch through the focused child.

## Architecture Principles (from Reference Analysis — 2026-03-22)

These principles were established after reviewing Ratatui's official patterns and should guide ALL future development:

1. **View trait stays unified** — Do NOT split into separate Widget + EventHandler traits. turbo-tui's `View` (state + events + render) IS the component architecture. Ratatui widgets are stateless renderers for data-display apps; turbo-tui components are stateful interactive views. Reviewed: [Component Architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/), [Widgets](https://ratatui.rs/concepts/widgets/).

2. **Builder Lite pattern for construction** — Self-consuming methods returning `Self`, NOT a separate Builder struct. Example: `Window::new(bounds, "Title").scrollbars(true, false).min_size(20, 8)`. Source: [Builder Lite](https://ratatui.rs/concepts/builder-lite-pattern/).

3. **Deferred events over Action returns** — Keep the deferred event queue. The `Action` enum return pattern doesn't support three-phase dispatch where multiple views process the same event.

4. **Frame owns scrollbars** — Scrollbars are `Option<ScrollBar>` on Frame, sitting on the border. They are NOT Container children. This prevents scrollbars from consuming interior space.

5. **Post-render effects = future** — TachyonFX-style buffer transforms for animations. Not in v0.2.x, but design must not prevent it. Integration point: `Application::draw()` + optional `EffectManager`. Source: [TachyonFX](https://github.com/junkdog/tachyonfx).

6. **Scrollbar inactive styling** — Scrollbars have active/inactive appearance based on owning window's `SF_FOCUSED` state. 3 theme fields: `scrollbar_track_inactive`, `scrollbar_thumb_inactive`, `scrollbar_arrows_inactive`. Propagated via `Window::set_state()`.

7. **Scrollbar hover via Frame** — Frame has `update_scrollbar_hover()`, `handle_scrollbar_click()`, `clear_scrollbar_hover()`. Window routes Moved/Down/Drag to these. Scrollbars on the border receive mouse events through Frame, not directly.

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

## v0.2 Rebuild — Current State (v0.2.0 complete, v0.2.1 in progress)

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

### v0.2.1 Completed
- Scrollbar inactive styling: 3 theme fields + `ScrollBar::set_active()` + Window focus propagation
- Scrollbar hover fix: `Frame::update_scrollbar_hover()`, `handle_scrollbar_click()`, `clear_scrollbar_hover()`
- Window routes Moved/Down/Drag events to frame scrollbars
- JSON theme schema: `ScrollbarSection` gets 3 `#[serde(default)]` inactive fields
- Scrollbar inactive fix: All 6 JSON themes get proper inactive scrollbar colors (was using hard-coded TV colors)
- Color typos fixed: dark.json `#3c3c3cOD`, matrix.json `#ff505`
- Phase 3: Task bar shelf — `Desktop::recalculate_shelf()`, `effective_area()`, `task_shelf_height()`
- `Window::minimize()` simplified — Desktop manages shelf positioning
- `Window::minimized_width()` public helper
- `tile()` and `cascade()` skip minimized windows, use `effective_area()`
- Phase 4a: FrameConfig struct — `FrameConfig::window()`, `dialog()`, `panel()`, `Frame::from_config()`
- Phase 4b: Window Builder Lite — `with_config()`, `with_min_size()`, `with_drag_limits()`, `with_scrollbars()`, `with_closeable()`, `with_resizable()`
- Phase 4c: Window Presets — `Window::editor()`, `Window::palette()`, `Window::tool()`
- Phase 5: View lifecycle hooks — `on_focus()`, `on_blur()` on View trait, called from Container
- Phase 7: JSON theme inactive scrollbar fields — already done (verified)
- Phase 6: Demo updated — Builder Lite, presets, scrollbar focus showcase
- Phase (frame): Frame title centering within full width + `'…'` ellipsis truncation
- 321 tests passing, clippy pedantic clean
- **v0.2.1 released and tagged**

### v0.2.1 In Progress (see `docs/PLAN-v0.2.1.md`)
- All phases complete — **released as v0.2.1**

### v0.2.2 In Progress
- **F1: MenuBar → Overlay dropdown refactor** — move dropdown rendering from HorizontalBar self-draw to OverlayManager + MenuBox
  - Phase 1: MenuBox command emission
  - Phase 2: HorizontalBar simplification (remove ~170 lines self-draw)
  - Phase 3: Application orchestration (CM_OPEN_DROPDOWN coordination)
  - Phase 4: OverlayManager dismiss callback
  - Phase 5: Demo + integration tests
  - Plan: `docs/PLAN-v0.2.2.md`

### Remaining (Deferred tov0.2.3+)
- Step 9b: MenuBar→Overlay dropdown refactor (MenuBar currently self-draws dropdown — works but can't extend beyond clip area)
- Step 9a/c/d: Widget adaptations (minor — existing widgets work but don't use v0.2 patterns fully)
- Future: Gauge/ProgressBar, ListView, TachyonFX integration, channel-based events (see PLAN-v0.2.1.md Future Phases)

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
| #7 Resize grip invisible | `"◘"` in DarkGray | Changed to `'◢'` (U+25E2), theme-configurable via `resize_grip_char` |

## Known Issues (v0.2.0)

### Issue 1: MenuBar dropdown is self-drawn (Step 9b)
- **File:** `src/horizontal_bar.rs` (`draw_dropdown` method)
- **Issue:** MenuBar draws its dropdown box directly instead of using the OverlayManager. This works correctly but dropdowns can't extend beyond the bar's clip area if windows overlap.
- **Plan:** Refactor to delegate dropdown rendering to OverlayManager + MenuBox (v0.2.1).

### Issue 2: JSON theme rendering may feel slower than TV built-in
- **Cause:** NOT disk I/O — themes are loaded once at startup into in-memory registry. The `with_current()` call is a cheap thread-local RefCell borrow. Perceived difference is because JSON themes use `Color::Rgb(r,g,b)` (24-bit) which some terminals render slower than CGA 16-color palette used by the Turbo Vision theme.**Mitigation:** None needed at library level. Terminal emulator choice (e.g., Alacritty/WezTerm vs. Windows Terminal) affects 24-bit color performance.

### Issue 3: FrameStyles clones close_button_text every draw (FIXED)
- Replaced `String` heap allocation with stack-allocated `[char; 8]` array in `FrameStyles`. Zero heap allocations during frame drawing now.

### Issue 4: Minimized windows invisible (v0.2.1 — Phase 3)
- **File:** `src/window.rs` (`minimize()`), `src/desktop.rs`
- **Issue:** Minimized windows collapse to height=1 but position under status line, overlapping each other. Essentially invisible and unclickable.
- **Plan:** Desktop task bar shelf — minimized windows tile at bottom of desktop area. See `docs/PLAN-v0.2.1.md` Phase 3.

### Issue 5: Window creation verbose (v0.2.1 — Phase 4)
- **Files:** `src/window.rs`, `src/frame.rs`
- **Issue:** Creating a configured window requires 5-8 separate setter calls. No fluent API.
- **Plan:** Builder Lite pattern + FrameConfig struct + widget presets. See `docs/PLAN-v0.2.1.md` Phase 4a-4c.

## Reference URLs (for Architecture Decisions)

| Reference | URL | Relevance |
|-----------|-----|-----------|
| Ratatui Component Architecture | https://ratatui.rs/concepts/application-patterns/component-architecture/ | Component trait pattern — turbo-tui's View is equivalent |
| Ratatui Event Handling | https://ratatui.rs/concepts/event-handling/ | 3 event patterns — we use approach 2 |
| Ratatui Widgets | https://ratatui.rs/concepts/widgets/ | Widget/StatefulWidget/WidgetRef traits |
| Ratatui Builder Lite | https://ratatui.rs/concepts/builder-lite-pattern/ | Self-consuming fluent API pattern |
| TachyonFX | https://github.com/junkdog/tachyonfx | Post-render effects, future animation integration |
| tui-rs demo | https://github.com/fdehau/tui-rs/tree/master/examples/demo | Dense dashboard layout, gauge/chart patterns |
| gping | https://github.com/orf/gping | Real-time gauge, ring-buffer data |
| Ratatui Scrollbar | https://ratatui.rs/examples/widgets/scrollbar/ | Scrollbar widget reference |

## Agent Strategy

- **~70% developer-mid** (Kimi K2): Logic, state machines, widget implementations
- **~30% developer** (Sonnet): Ratatui rendering, complex mouse/integration code, debugging
- **developer-mini** (free): Config changes, small mechanical edits, boilerplate

## Project Documentation Map

| File | Purpose | When to Read |
|------|---------|-------------|
| `CLAUDE.md` | Agent config, conventions, current state | Every session (auto-loaded) |
| `ARCHITECTURE.md` | System diagram, module deps, event dispatch, rendering pipeline | When changing event routing, adding widgets, understanding data flow |
| `STANDARDS.md` | Coding standards, patterns, widget template, commit conventions | When writing new code, reviewing PRs |
| `TESTING.md` | Test architecture, patterns, naming, what to test | When writing or debugging tests |
| `ROADMAP.md` | Version roadmap, future phases, reference projects | When planning next steps |
| `HISTORY.md` | Change log (append-only) | Before committing changes |
| `docs/PLAN-v0.2.1.md` | v0.2.1 sprint plan (completed) | Historical reference |
| `docs/PLAN-v0.2.2.md` | v0.2.2 MenuBar→Overlay refactor plan | When implementing v0.2.2 features |
| `docs/PLAN-v0.2.md` | v0.2 architecture rebuild plan (completed) | For historical architecture decisions |
| `docs/RES-0002-reference-projects-architecture.md` | Reference analysis: Ratatui, TachyonFX, tui-rs, gping | When making architecture decisions |
| `docs/RES-0001-performance-research.md` | Performance research (16ms poll, dirty flags, etc.) | When optimizing |
| `docs/DESIGN-horizontal-bar.md` | HorizontalBar unification design | When changing menu/status bar |

## Related Projects

| Project | Path | Relationship |
|---------|------|-------------|
| four-code | `~/four-code` | Consumer — terminal IDE that will use turbo-tui for windowing |
| ADR-002 | `~/four-code/docs/ADR-002-turbo-tui-windowing.md` | Architecture decision record |
| v0.2 Plan | `docs/PLAN-v0.2.md` | Detailed architecture plan for v0.2 rebuild |
| v0.2.1 Plan | `docs/PLAN-v0.2.1.md` | Progression plan with reference analysis findings |
| Research | `docs/RES-0002-reference-projects-architecture.md` | Reference projects analysis |
