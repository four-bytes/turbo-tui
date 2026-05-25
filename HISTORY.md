# turbo-tui — Change History

## v0.1.0 (2026-03-21)

### Initial Setup
- Project created: Borland Turbo Vision windowing patterns for Ratatui
- Command system: `CommandId` (u16), `CommandSet` (bitfield), standard command constants
- Standard commands: CM_QUIT, CM_OK, CM_CANCEL, CM_YES, CM_NO, CM_CLOSE, CM_SAVE, etc.
- INTERNAL_COMMAND_BASE (1000) convention: commands >= 1000 don't close dialogs
- 4 tests passing
- ADR-002 written in four-code documenting architecture decisions
- Pattern reference: [turbo-vision-4-rust](https://github.com/aovestdipaperino/turbo-vision-4-rust) (MIT licensed)

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

### HorizontalBar Unification (2026-03-22)
- **NEW** `src/horizontal_bar.rs`: Unified `HorizontalBar` struct replacing separate `MenuBar` and `StatusLine`
  - `BarEntry` enum: `Action` (direct command) or `Dropdown` (opens menu box)
  - `DropDirection`: reuses `overlay::DropDirection` — `Down` for menu bar, `Up` for status bar
  - Supports mixed entries: menu bars can have direct actions, status bars can have dropdowns
  - Full keyboard (F10, Esc, arrows, Enter, Alt+letter, key codes) + mouse handling
  - 26 tests
- **Refactored** `menu_bar.rs` → thin backward-compat wrapper (1070→276 lines)
  - `MenuItem`, `Menu` types retained
  - `pub type MenuBar = HorizontalBar` alias
  - `From<Menu> for BarEntry`, `menu_bar_from_menus()` convenience constructor
- **Refactored** `status_line.rs` → thin backward-compat wrapper (664→325 lines)
  - `StatusItem`, `KB_*` constants, `key_matches()` retained
  - `pub type StatusLine = HorizontalBar` alias
  - `From<StatusItem> for BarEntry`, `status_line_from_items()` convenience constructor
- Updated `application.rs` and `examples/demo.rs` to use new API
- Design doc: `docs/DESIGN-horizontal-bar.md`
- 255 tests passing, clippy pedantic clean, zero unsafe code

### Theme Loading Report + Resize Grip Fix (2026-03-22)
- **NEW** `ThemeLoadReport` struct in `theme.rs` — per-file success/error tracking for JSON theme loading
  - `has_errors()`, `loaded_count()`, `error_summary()` helpers
  - `Display` impl for formatted output
- **BREAKING** `load_themes_from_dir()` now returns `Result<ThemeLoadReport, io::Error>` instead of `usize`
  - Directory read failures are propagated as `Err` (no longer silently swallowed)
  - Individual file parse errors collected in `ThemeLoadReport::errors`
  - Theme loading never fails silently anymore
- **FIX** Resize grip character: changed default from '⋱' (U+22F1) to '◢' (U+25E2) everywhere
  - Built-in `turbo_vision()` theme
  - `default_resize_grip_char()` in `theme_json.rs`
  - Fallback in `to_theme()` conversion
  - All 6 JSON theme files already used '◢'
- **NEW** Integration tests: `test_load_dark_json_from_disk`, `test_load_all_theme_files_from_disk`
- Demo updated: `panic!` on theme load failures instead of silent fallback
- 280 tests passing, clippy pedantic clean

### Scrollbar Fixes + Reference Analysis (2026-03-22)
- **Phase 1:** Scrollbar inactive styling
  - 3 new theme fields: `scrollbar_track_inactive`, `scrollbar_thumb_inactive`, `scrollbar_arrows_inactive`
  - `ScrollBar::set_active(bool)` / `is_active()` — controls active vs inactive rendering
  - `Window::set_state()` propagates `SF_FOCUSED` to frame scrollbars via `set_active()`
  - `theme_json.rs`: `ScrollbarSection` gets 3 `#[serde(default)]` inactive fields + `Default` impl for `StyleValue`
- **Phase 2:** Scrollbar hover fix
  - `Frame::update_scrollbar_hover(col, row)` — forwards MouseMoved to scrollbars with correct bounds
  - `Frame::clear_scrollbar_hover()` — clears hover on all scrollbars
  - `Frame::handle_scrollbar_click(col, row, event) -> bool` — handles Down/Drag on scrollbars
  - `Window::handle_event` routes Moved, Down, Drag to frame scrollbars
- **Reference Analysis:** Reviewed Ratatui Component Architecture, Event Handling, Widgets, Builder Lite, TachyonFX, tui-rs demo, gping
  - Architecture decision: View trait stays unified (NOT split into Widget + EventHandler)
  - Adopted: Builder Lite pattern for Window/FrameConfig construction
  - Saved to `docs/RES-0002-reference-projects-architecture.md`
- **Documentation:** Created ARCHITECTURE.md, STANDARDS.md, TESTING.md, ROADMAP.md, PLAN-v0.2.1.md
- 284 tests passing, clippy pedantic clean

### Scrollbar Inactive Fix + Task Bar Shelf (2026-03-22)
- **Scrollbar Inactive Fix:** JSON theme files were missing `track_inactive`, `thumb_inactive`, `arrows_inactive` fields
  - Serde defaults hard-coded Turbo Vision colors (Blue background) — broke all non-TV themes
  - Added proper inactive scrollbar colors to all 6 JSON theme files matching each theme's palette
  - Fixed color typos: `dark.json` `#3c3c3cOD` → `#3c3c3c`, `matrix.json` `#ff505` → `#ff5050`
- **Phase 3: Task Bar Shelf** (PLAN-v0.2.1)
  - Desktop gets `task_shelf_height: u16` — tracks shelf rows for minimized windows
  - `Desktop::recalculate_shelf()` — positions minimized windows left-to-right at desktop bottom
  - Shelf wraps to multiple rows if minimized windows overflow one row
  - `Desktop::effective_area()` — returns desktop bounds minus shelf rows
  - `Window::minimize()` simplified — no longer self-positions, Desktop manages shelf layout
  - `Window::minimized_width()` — public helper for shelf layout calculation
  - `tile()` and `cascade()` skip minimized windows, use `effective_area()`
  - `recalculate_shelf()` called after add_window, close_window, and all event handling
  - 8 new tests: shelf empty, one minimized, multiple tiled, restore, close, tile/cascade skip, effective area
- 292 tests passing, clippy pedantic clean

### Phase 4 + 5: Builder Lite + Lifecycle Hooks (2026-03-22)
- **Phase 4a: FrameConfig struct** (`src/frame.rs`)
  - `FrameConfig` struct with `frame_type`, `closeable`, `resizable`, `minimizable`, `maximizable`, `v_scrollbar`, `h_scrollbar`
  - Named constructors: `FrameConfig::window()`, `FrameConfig::dialog()`, `FrameConfig::panel()`
  - Builder methods: `with_v_scrollbar()`, `with_h_scrollbar()`, `with_closeable()`, `with_resizable()`, `with_minimizable()`, `with_maximizable()`
  - `Frame::from_config(bounds, title, config)` — creates Frame from FrameConfig
  - `Default` impl returns `window()` config
  - Exported in prelude
- **Phase 4b: Window Builder Lite** (`src/window.rs`)
  - `Window::with_config(bounds, title, config)` — constructor from FrameConfig
  - Self-consuming builder methods: `with_min_size()`, `with_drag_limits()`, `with_scrollbars()`, `with_closeable()`, `with_resizable()`, `with_minimized_max_width()`
  - Existing `set_*()` mutators retained for runtime changes
- **Phase 4c: Window Presets** (`src/window.rs`)
  - `Window::editor(bounds, title)` — vertical scrollbar, min 20×8
  - `Window::palette(bounds, title)` — not resizable, not closeable
  - `Window::tool(bounds, title)` — compact min 10×5
- **Phase 5: View Lifecycle Hooks** (`src/view.rs`, `src/container/mod.rs`)
  - `on_focus(&mut self)` — called when view receives focus (SF_FOCUSED set)
  - `on_blur(&mut self)` — called when view loses focus (SF_FOCUSED cleared)
  - Default implementations are no-ops
  - `Container::set_focus_to()` calls `on_blur()` on old child, `on_focus()` on new child
- **Phase 7: JSON Theme Files** — already complete (inactive scrollbar fields added in previous session)
- **Demo updated** to use Builder Lite, Window presets, and showcase scrollbar focus styling
- 313 tests passing, clippy pedantic clean

### Frame Title Centering Fix (2026-03-22)
- **FIX** Title now centers within full frame width, then clips to button tray boundaries
  - Previously centered within available tray space only — looked off-center with close/zoom buttons
  - Adds `'…'` ellipsis character when title is truncated on left or right edge
  - Handles edge cases: very narrow frames, titles shorter than available space
- 4 new tests: centered title, right-truncation ellipsis, no-ellipsis-when-fits, very-narrow-no-crash
- 321 tests passing, clippy pedantic clean

## v0.2.2-dev (2026-03-22)

### F1: MenuBar → Overlay Dropdown Refactor
- **REFACTOR** Dropdown rendering moved from `HorizontalBar` self-draw to `OverlayManager` + `MenuBox`
  - Dropdowns now render above all windows (not clipped by bar's clip area)
  - ~200 lines of duplicate drawing/event code removed from `HorizontalBar`
  - `MenuBox` used as the actual overlay view (already existed as standalone widget)
- **Phase 1: MenuBox Enhancement**
  - Added `owner_bar_id: Option<ViewId>` — when set, MenuBox emits commands via event system
  - `confirm_selection()` now sets `event.kind = EventKind::Command(cmd)` when owned by a bar
  - Left/Right arrows post `CM_DROPDOWN_NAVIGATE` deferred event (with direction stored in `navigate_direction`)
  - Backward compat: standalone `MenuBox::result()` still works for non-overlay usage
- **Phase 2: HorizontalBar Simplification**
  - Removed: `draw_dropdown()`, `draw_dropdown_border_row()`, `draw_dropdown_item_row()` (~170 lines)
  - Removed: `dropdown_width()`, `item_at_position()`, `move_down()`, `move_up()`, `selected_command()`, `first_selectable_item()`
  - Removed: `selected_item` field (dropdown item selection now handled by MenuBox)
  - Added: `request_dropdown()` posts `CM_OPEN_DROPDOWN` + stores `pending_dropdown` for Application
  - Added: `navigate_dropdown()` (public) replaces `move_entry()`, called by Application on `CM_DROPDOWN_NAVIGATE`
  - Added: `take_pending_dropdown()`, `dropdown_items_for()`, `dropdown_anchor()` public API for Application
  - F10/Escape/close now post `CM_DROPDOWN_CLOSED` so Application can clean up overlays
- **Phase 3: Application Orchestration**
  - Intercepts `CM_OPEN_DROPDOWN`: creates `MenuBox` overlay via `OverlayManager` with `calculate_overlay_bounds()`
  - Intercepts `CM_DROPDOWN_CLOSED`: pops overlay, resets bar state
  - Intercepts `CM_DROPDOWN_NAVIGATE`: reads direction from MenuBox, pops current overlay, navigates bar, opens next
  - Supports both MenuBar (drops down) and StatusLine (drops up)
- **Phase 4: OverlayManager Dismiss Callback**
  - Outside-click dismiss now posts `CM_DROPDOWN_CLOSED` so owning bar resets `active_dropdown`
  - Escape dismiss already posted `CM_DROPDOWN_CLOSED` (added in Phase 3)
  - Added `overlays_iter()` for Application to inspect overlay contents
- **New Commands:** `CM_OPEN_DROPDOWN` (1010), `CM_DROPDOWN_CLOSED` (1011), `CM_DROPDOWN_NAVIGATE` (1012)
- 331 tests passing (was 321), clippy pedantic clean
- Plan: `docs/PLAN-v0.2.2.md`

### F2: Minimized Window Tray Fix (2026-03-22)
- **FIX** Frame now draws at `height=1` — minimized windows visible in task shelf
  - `Frame::draw()` guard changed from `height < 2` to `height < 1`
  - 1-row frame: draws only top border (corners, horizontal line, close button, title)
  - Side borders, scrollbars, bottom border skipped at `height < 2`
- **FIX** `ButtonTray` suppresses minimize/maximize buttons at `height ≤ 1`
  - Both `draw()` and `build_button_tray()` (hit-testing) consistent
  - Minimized windows show only: `[■] Title [×]` (close + title)
- **FIX** Hit-test methods guard against `height ≤ 1`:
  - `is_minimize_button()`, `is_maximize_button()`, `is_resize_handle()` → return `false`
  - `is_close_button()` still works (close button valid at any height)
- Added `has_close_button()` public accessor on Frame
- 4 new tests: height=1 draw, no min/max buttons, close button works, title visible
- 335 tests passing, clippy pedantic clean

## v0.3-dev (2026-03-29)

### Ratatui 0.30 Upgrade
- Upgraded ratatui 0.29 → 0.30, crossterm 0.28 retained — zero API breaks

### Bug Fixes
- **Bug 1:** HorizontalBar Alt+Char fallthrough — `Alt+X` (and other Alt+letter combos) now falls through to `handle_key_code_match()` when no hotkey match found. Previously the Alt+Char arm matched but silently dropped the event.
- **Bug 2:** Window resize propagation — `update_bounds()` now resizes all interior children to fill the new interior rect. Previously `Container::set_bounds()` only propagated position deltas, not size changes.
- **Bug 3:** Container auto-focus — `Container::add()` now auto-focuses the first focusable child. Previously `self.focused` was never set, so Key events never reached any child.

### Cursor Position Support
- **View trait:** Added `cursor_position() -> Option<Position>` (default `None`)
- **Container:** `cursor_position()` queries focused child
- **Window:** View impl delegates `cursor_position()` to interior container
- **Desktop:** `cursor_position()` queries windows container
- **Application:** `draw()` calls `frame.set_cursor_position()` from desktop (only when no overlays active)
- **Window::editor() preset:** Changed to `with_scrollbars(true, true)` — both vertical and horizontal scrollbars

### Tests
- 353 tests passing (was 335), clippy pedantic clean
- New tests: `test_alt_x_falls_through_to_key_code_match`, `test_window_resize_updates_child_bounds`, `test_container_add_auto_focuses_first_focusable_child`, `test_container_add_non_focusable_child_does_not_set_focus`

### Desktop Drag Limits Fix (2026-03-29)
- **FIX** `Desktop::set_bounds()` now updates `drag_limits` on all existing windows when terminal resizes
  - Previously, windows created before a terminal resize kept stale drag limits (zero drag range if window filled entire desktop)
  - New test: `test_desktop_set_bounds_updates_drag_limits`
- 357 tests passing (was 353), clippy pedantic clean

## [0.1.0] - 2026-03-21

### Added
- Initial release with full widget library for Borland Turbo Vision windowing patterns on Ratatui.
- Command system: `CommandId` (u16), `CommandSet` (bitfield), and 25+ standard command constants (`CM_QUIT`, `CM_OK`, `CM_CANCEL`, `CM_CLOSE`, etc.).
- `View` trait with `ViewId`, `StateFlags`, `OptionFlags`, and Event system.
- `Container` (originally `Group`) with Z-order management and three-phase event dispatch.
- `Frame` with Smart Border and optional `ScrollBar` integration.
- `Window` with drag, resize, and zoom toggle support.
- `Desktop` window manager with tile, cascade, and click-to-focus.
- `Dialog` for modal windows with Escape/Enter handling.
- `MenuBar`, `MenuBox`, and `StatusLine` widgets.
- `Scrollbar` (vertical/horizontal) with draggable thumb.
- `Button`, `StaticText`, and `MsgBox` factory functions.
- 157 tests passing, clippy pedantic clean, zero unsafe code.

### Known Issues
- Background noise caused by `'░'` desktop character.
- Interior bleed-through: windows did not fill their interior area.
- Resize only shrinks: no mouse capture during resize operations.
- Drag lag due to 50 ms poll interval.
- Escape quits application instead of Alt+X (Borland convention).
- Alt+letter inactive for menus.
- Resize grip invisible in some terminals.

## [0.2.0] - 2026-03-22

### Added
- Architecture rebuild across 10 implementation steps.
- `Container` renamed from `Group`, split into submodules (`mod.rs`, `dispatch.rs`, `draw.rs`).
- `View` trait extensions: deferred event queue (`Event::post()`), lifecycle hooks (`on_insert`, `on_remove`, `on_resize`).
- `Frame` (Smart Border) with hit-testing and stack-allocated `FrameStyles`.
- `Window` drag/resize state machine, zoom toggle, and interior fill.
- `OverlayManager` for dropdowns and tooltips above all windows.
- `Application` orchestrator: dispatch chain, deferred events, screen resize handling.
- `HorizontalBar` unification: single struct replacing separate `MenuBar` and `StatusLine` via `BarEntry` (Action/Dropdown).
- `MsgBox` factory functions (`message_box`, `confirm_box`, `confirm_cancel_box`, `error_box`).
- JSON theme serialization with `ThemeLoadReport` and per-file error tracking.
- Demo application (`examples/demo.rs`) showcasing windows, menus, buttons, and status line.
- 284 tests passing, clippy pedantic clean.

### Fixed
- **Background noise:** Changed desktop fill from `'░'` to `' '` with dark RGB background via theme.
- **Interior bleed-through:** `Window` now calls `fill_interior()` to paint the interior with the theme background color.
- **Resize only shrinks:** `Container::dispatch.rs` routes `Drag`/`Up` to the focused child using `SF_DRAGGING`/`SF_RESIZING` flags with mouse capture.
- **Drag lag:** Reduced poll interval to 16 ms and switched to event-driven redraw.
- **Escape quits:** Changed quit key to Alt+X (Borland convention).
- **Alt+letter inactive:** `HorizontalBar` handles Alt+letter directly via `OF_PRE_PROCESS` dispatch.
- **Resize grip invisible:** Changed default grip character to `'◢'` (U+25E2), theme-configurable via `resize_grip_char`.

## [0.2.1] - 2026-03-22

### Added
- **Scrollbar inactive styling:** 3 new theme fields (`scrollbar_track_inactive`, `scrollbar_thumb_inactive`, `scrollbar_arrows_inactive`) and `ScrollBar::set_active(bool)`; focus state propagates from `Window` to frame scrollbars.
- **Scrollbar hover fix:** `Frame::update_scrollbar_hover()`, `handle_scrollbar_click()`, `clear_scrollbar_hover()`; `Window` routes `Moved`/`Down`/`Drag` events to frame scrollbars.
- **Task bar shelf:** `Desktop::recalculate_shelf()` positions minimized windows left-to-right at the desktop bottom; `effective_area()` excludes shelf rows; `tile()` and `cascade()` skip minimized windows.
- **Window Builder Lite pattern:** Self-consuming methods on `Window` (`with_min_size`, `with_drag_limits`, `with_scrollbars`, `with_closeable`, `with_resizable`) and `FrameConfig` struct (`FrameConfig::window()`, `dialog()`, `panel()`).
- **Window presets:** `Window::editor()`, `Window::palette()`, `Window::tool()` for common window configurations.
- **View lifecycle hooks:** `on_focus()` and `on_blur()` on `View` trait; `Container::set_focus_to()` calls blur on old child and focus on new child.
- Frame title centering within full width with `'…'` ellipsis truncation.
- 321 tests passing, clippy pedantic clean.

### Fixed
- Scrollbar inactive colors in JSON themes: added proper inactive scrollbar colors to all 6 JSON theme files (was using hard-coded Turbo Vision colors via serde defaults).
- Color typos in `dark.json` (`#3c3c3cOD`) and `matrix.json` (`#ff505`).
- Minimized windows no longer overlap or hide under the status line; Desktop manages shelf positioning via `recalculate_shelf()`.

## [0.2.2] - 2026-03-22

### Added
- **MenuBar → Overlay dropdown refactor:** Dropdown rendering moved from `HorizontalBar` self-draw to `OverlayManager` + `MenuBox`.
  - Dropdowns now render above all windows and are not clipped by the bar's clip area.
  - New commands: `CM_OPEN_DROPDOWN` (1010), `CM_DROPDOWN_CLOSED` (1011), `CM_DROPDOWN_NAVIGATE` (1012).
  - `MenuBox` emits commands via the event system when owned by a bar; supports Left/Right arrow navigation across dropdowns.
  - `OverlayManager` dismiss callback posts `CM_DROPDOWN_CLOSED` on outside-click.
  - ~170–200 lines of duplicate drawing/event code removed from `HorizontalBar`.
- 335 tests passing, clippy pedantic clean.

### Fixed
- **Minimized window tray visibility:** `Frame::draw()` now renders at `height = 1` (changed guard from `height < 2` to `height < 1`); draws top border, corners, close button, and title at 1-row height.
- **Minimized window buttons:** `ButtonTray` suppresses minimize/maximize buttons when `height ≤ 1`; hit-test methods (`is_minimize_button`, `is_maximize_button`, `is_resize_handle`) return `false` at `height ≤ 1`.
- **Close button on minimized windows:** `is_close_button()` remains functional at any height; `has_close_button()` accessor added to `Frame`.

[0.1.0]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.1.0
[0.2.0]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.2.0
[0.2.1]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.2.1
[0.2.2]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.2.2

## [0.2.3] - 2026-05-25

### Added
- `Application::post_event()` for background thread event injection into the running application loop.
- Updated demo to showcase scrollbar fix and `post_event()` usage.

### Fixed
- **Scrollbar thumb positioning:** mouse click now correctly maps to the middle of thumb positions; only the area between arrow buttons counts for thumb positioning.
- **Ratatui 0.30 upgrade:** upgraded from 0.29 to 0.30 with zero API breaks.

[0.2.3]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.2.3

## [0.2.4] - 2026-05-26

### Added
- **Drag-and-drop state machine:** New drag-and-drop support with `CM_DRAG_START`, `CM_DRAG_MOVE`, `CM_DRAG_END` commands for tracking drag operations.
  - `Application::start_drag(origin, payload)` — starts a drag with an arbitrary `Box<dyn Any>` payload.
  - `Application::end_drag()` — ends the current drag and clears payload.
  - `Application::drag_payload()` — returns a reference to the current drag payload.
  - `Application::is_dragging()` — returns `true` while a drag is in progress.
  - `Application::drag_origin()` — returns the position where the drag started.
  - `Container::dispatch_event()` posts `CM_DRAG_START`/`CM_DRAG_MOVE`/`CM_DRAG_END` as deferred events on Left mouse button interactions.
  - 435 tests passing, clippy pedantic clean.

[0.2.4]: https://github.com/four-bytes/turbo-tui/releases/tag/v0.2.4
