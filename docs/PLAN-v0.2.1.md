# turbo-tui v0.2.1 — Progression Plan

> **Status:** Complete (2026-03-22)
> **Branch:** `v0.2-rebuild`
> **Basis:** v0.2.0-dev — 313 tests, ~15,000 lines, 21 source files
> **References:** `docs/RES-0002-reference-projects-architecture.md`

---

## Architecture Principles (REMEMBER across ALL sessions)

### 1. Keep View Trait Unified
Do NOT split into Widget + EventHandler. turbo-tui's `View` trait (state + events + render) IS the component architecture, just with a different API shape than Ratatui's template. Ratatui widgets are stateless renderers; turbo-tui components are stateful. This was a deliberate decision after reviewing:
- Ratatui Component Architecture: https://ratatui.rs/concepts/application-patterns/component-architecture/
- Ratatui Widget traits: https://ratatui.rs/concepts/widgets/

### 2. Builder Lite, Not Builder
Use `self`-consuming methods returning `Self` (Ratatui's standard pattern). No separate `WindowBuilder` struct.
- Source: https://ratatui.rs/concepts/builder-lite-pattern/
- Example: `Window::new(bounds, "Title").scrollbars(true, false).min_size(20, 8)`

### 3. Centralized Catch + Three-Phase Dispatch
Already correct. This is approach 2 from Ratatui's event handling docs. The right fit for a windowing framework.
- Source: https://ratatui.rs/concepts/event-handling/

### 4. Deferred Events > Action Returns
Keep the deferred event queue. The `Action` enum return pattern from Ratatui's component template doesn't support three-phase dispatch where multiple views process the same event.

### 5. Post-Render Effects = Future Optional
TachyonFX-style buffer transforms for window animations. NOT part of v0.2.1 — but design should not prevent future integration.
- Source: https://github.com/junkdog/tachyonfx
- Integration point: `Application::draw()` could accept an optional `EffectManager` that runs after widget rendering.

### 6. Frame Owns Scrollbars
Keep scrollbars as `Option<ScrollBar>` on Frame. They sit on the border, not in the interior. This is architecturally correct for the Borland TV pattern.

---

## Completed (this session)

### Phase 1: Scrollbar Inactive Styling ✅
- Added 3 theme fields: `scrollbar_track_inactive`, `scrollbar_thumb_inactive`, `scrollbar_arrows_inactive`
- `ScrollBar::set_active(bool)` + `is_active()` — controls active vs inactive rendering
- `Window::set_state()` propagates `SF_FOCUSED` to frame scrollbars via `set_active()`
- `theme_json.rs` — `ScrollbarSection` gets 3 new `#[serde(default)]` fields
- **Files:** `theme.rs`, `scrollbar.rs`, `window.rs`, `theme_json.rs`

### Phase 2: Scrollbar Hover Fix ✅
- `Frame::update_scrollbar_hover(col, row)` — forwards MouseMoved to scrollbars with correct bounds
- `Frame::clear_scrollbar_hover()` — clears hover on all scrollbars
- `Frame::handle_scrollbar_click(col, row, event) -> bool` — handles Down/Drag on scrollbars
- `Window::handle_event` routes Moved, Down, and Drag to frame scrollbars
- **Files:** `frame.rs`, `window.rs`

---

## Phase 3: Task Bar Shelf for Minimized Windows ✅

**Priority:** High | **Agent:** developer-mid | **Est. ~200 lines**

### Problem
`Window::minimize()` positions minimized windows at `drag_limits.y + drag_limits.height - 1` — under the status line, overlapping each other, essentially invisible.

### Design
- Desktop gets a `task_shelf_height: u16` field (default 0, grows when windows minimized)
- When any window is minimized, Desktop positions it in the shelf area at the bottom of the desktop bounds
- Minimized windows tile left-to-right: `(shelf_x + offset, shelf_y, minimized_width, 1)`
- Each minimized window shows as a clickable title bar (frame top row only)
- If too many to fit in one row, shelf grows to 2 rows
- Desktop's effective window area shrinks by `task_shelf_height` rows
- Shelf is drawn by Desktop between background and windows
- Clicking a minimized window in the shelf → restores it

### Files
- `src/desktop.rs` — Add shelf tracking, `recalculate_shelf()`, positioning logic, draw shelf
- `src/window.rs` — Remove self-positioning from `minimize()` (let Desktop handle it)
- `src/application.rs` — Update desktop bounds when shelf height changes

### Tests
- Minimize 1 window → appears in shelf row, shelf_height = 1
- Minimize 3 windows → tile left-to-right
- Click minimized window → restores, shelf recalculates
- Close minimized window → shelf recalculates
- Shelf grows to 2 rows on overflow
- Tile/cascade ignores minimized windows
- Restore sets bounds back to pre-minimize size

---

## Phase 4a: FrameConfig Struct ✅

**Priority:** Medium | **Agent:** developer-mid | **Est. ~80 lines**

### Design
```rust
/// Configuration for creating a Frame.
#[derive(Debug, Clone)]
pub struct FrameConfig {
    pub frame_type: FrameType,
    pub closeable: bool,
    pub resizable: bool,
    pub minimizable: bool,
    pub maximizable: bool,
    pub v_scrollbar: bool,
    pub h_scrollbar: bool,
}

impl FrameConfig {
    /// Window defaults: closeable + resizable, min/max from theme.
    pub fn window() -> Self { ... }
    /// Dialog defaults: no close, no resize.
    pub fn dialog() -> Self { ... }
    /// Panel defaults: Single frame, no close, no resize.
    pub fn panel() -> Self { ... }
}
```

### Files
- `src/frame.rs` — Add `FrameConfig`, add `Frame::from_config(bounds, title, config)`
- `src/lib.rs` — Export `FrameConfig` in prelude

---

## Phase 4b: Window Builder Lite Pattern ✅

**Priority:** Medium | **Agent:** developer-mid | **Est. ~60 lines** | **Depends on:** 4a

### Design
Self-consuming methods on Window (NOT a separate builder struct).

```rust
impl Window {
    // Existing
    pub fn new(bounds: Rect, title: &str) -> Self { ... }
    
    // Builder Lite methods — consume self, return Self
    #[must_use]
    pub fn scrollbars(mut self, vertical: bool, horizontal: bool) -> Self { ... }
    #[must_use]
    pub fn min_size(mut self, w: u16, h: u16) -> Self { ... }
    #[must_use]
    pub fn drag_limits(mut self, limits: Rect) -> Self { ... }
    #[must_use]
    pub fn frame_config(mut self, config: FrameConfig) -> Self { ... }
    #[must_use]
    pub fn closeable(mut self, yes: bool) -> Self { ... }
    #[must_use]
    pub fn resizable(mut self, yes: bool) -> Self { ... }
}
```

### Usage
```rust
let win = Window::new(bounds, "Editor")
    .scrollbars(true, false)
    .min_size(20, 8)
    .drag_limits(desktop_area);
```

### Files
- `src/window.rs` — Add builder lite methods. Existing `set_*` stay for runtime mutation.

---

## Phase 4c: Widget Presets ✅

**Priority:** Medium | **Agent:** developer-mini | **Est. ~30 lines** | **Depends on:** 4b

### Design
Named constructors for common window configurations.

```rust
impl Window {
    /// Editor window — vertical scrollbar, generous min size.
    pub fn editor(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title).scrollbars(true, false).min_size(20, 8)
    }
    /// Small fixed dialog — no resize, no scrollbars.
    pub fn palette(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title).resizable(false).closeable(false)
    }
    /// Tool window — small, resizable, no min/max buttons.
    pub fn tool(bounds: Rect, title: &str) -> Self {
        Self::new(bounds, title).min_size(10, 5)
    }
}
```

### Files
- `src/window.rs`

---

## Phase 5: View Lifecycle Hooks (on_focus, on_blur) ✅

**Priority:** Medium | **Agent:** developer-mid | **Est. ~80 lines**

### Current State
View trait already has `on_insert()`, `on_remove()`, `on_resize()`. Missing: `on_focus()` and `on_blur()`.

### Why Needed
Focus changes are handled by checking `SF_FOCUSED` in `set_state()`. This is implicit and error-prone. Components that need to react to focus changes (e.g., scrollbar active/inactive, cursor visibility) must override `set_state()` and check bit differences.

### Design
```rust
pub trait View {
    // ... existing ...
    
    /// Called when this view receives focus.
    /// Default implementation does nothing.
    fn on_focus(&mut self) {}
    
    /// Called when this view loses focus.
    /// Default implementation does nothing.
    fn on_blur(&mut self) {}
}
```

### Where Called
- `Container::set_focus_to()` — calls `on_blur()` on old focused child, `on_focus()` on new
- `Window::set_state()` — if SF_FOCUSED bit changes, calls `on_focus()`/`on_blur()` on self

### Files
- `src/view.rs` — Add trait methods with default impls
- `src/container/mod.rs` — Call hooks in `set_focus_to()`, `clear_focus()`
- `src/window.rs` — Simplify `set_state()`, move scrollbar active propagation to `on_focus()`/`on_blur()`

---

## Phase 6: Demo Update ✅

**Priority:** Low | **Agent:** developer-mini | **Est. ~40 lines** | **Depends on:** 3, 4b

- Use Builder Lite pattern for window creation
- Show task bar shelf: start one window minimized
- Add a window with both scrollbars to demonstrate inactive styling
- Show focus change effects on scrollbar appearance

### Files
- `examples/demo.rs`

---

## Phase 7: JSON Theme Schema Update ✅

**Priority:** Low | **Agent:** developer-mini | **Est. ~60 lines**

- Update all 6 JSON theme files with the 3 new scrollbar inactive fields:
  - `themes/dark.json`
  - `themes/matrix.json`
  - `themes/modern.json`
  - `themes/turbo-vision.json`
  - `themes/windows-classic.json`
  - `themes/windows.json`
- Each needs `track_inactive`, `thumb_inactive`, `arrows_inactive` in `scrollbar` section
- Test roundtrip: `Theme → ThemeData → JSON → ThemeData → Theme` still passes

### Files
- `themes/*.json`

---

## Execution Order & Dependencies

```
Phase 3 (task shelf)    ──────────────┐
Phase 4a (FrameConfig)  → 4b (builder) → 4c (presets) ──┐
Phase 5 (lifecycle)     ──────────────┐                  │
Phase 7 (JSON themes)   ──────────────┤                  │
                                      └── Phase 6 (demo) ┘
```

**Parallel groups:**
- Group A: Phase 3 + Phase 5 + Phase 7 (all independent)
- Group B: Phase 4a → 4b → 4c (sequential dependency)
- Group C: Phase 6 (depends on 3 + 4b)

### Agent Strategy
| Phase | Agent | Why |
|-------|-------|-----|
| 3 | developer-mid | State management, Desktop architecture |
| 4a | developer-mid | Struct design with theme interaction |
| 4b | developer-mid | Builder pattern with correct semantics |
| 4c | developer-mini | Mechanical: compose existing builder methods |
| 5 | developer-mid | Trait modification, call-site changes across files |
| 6 | developer-mini | Mechanical: update demo to use new API |
| 7 | developer-mini | Mechanical: add JSON fields to 6 files |

---

## Future Phases (v0.2.2+ — NOT part of v0.2.1)

| ID | Feature | Reference | Notes |
|----|---------|-----------|-------|
| F1 | MenuBar → Overlay dropdown refactor | PLAN-v0.2.md Step 9b | MenuBar self-draws dropdown — should use OverlayManager |
| F2 | Gauge/ProgressBar widget | gping, tui-rs demo | Ring-buffer data model for streaming |
| F3 | ListView/ScrollView widget | gping | Scrollable content area with virtual scrolling |
| F4 | TachyonFX integration point | tachyonfx | `Application::draw()` accepts optional `EffectManager` |
| F5 | Channel-based external events | ratatui-background-process-example | `tokio::mpsc` for background → UI |
| F6 | `Application::post_event()` API | ratatui event patterns | Public API for consumer background tasks |
| F7 | Widget validation framework | Ratatui Component `update()` | Adapted for three-phase dispatch |
| F8 | Tab widget | tui-rs demo | Tab bar for tabbed window content |
| F9 | Tree widget | — | Hierarchical list/tree view |

---

## Reference URLs

- **Ratatui Component Architecture:** https://ratatui.rs/concepts/application-patterns/component-architecture/
- **Ratatui Event Handling:** https://ratatui.rs/concepts/event-handling/
- **Ratatui Widgets:** https://ratatui.rs/concepts/widgets/
- **Ratatui Builder Lite:** https://ratatui.rs/concepts/builder-lite-pattern/
- **TachyonFX:** https://github.com/junkdog/tachyonfx
- **tui-rs demo:** https://github.com/fdehau/tui-rs/tree/master/examples/demo
- **gping:** https://github.com/orf/gping
- **Ratatui Scrollbar example:** https://ratatui.rs/examples/widgets/scrollbar/
- **Research doc:** `docs/RES-0002-reference-projects-architecture.md`
- **v0.2 architecture plan:** `docs/PLAN-v0.2.md`
