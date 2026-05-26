# turbo-tui — Roadmap

> Last updated: 2026-05-26

## Version History

| Version | Status | Tests | Key Features |
|---------|--------|-------|-------------|
| v0.1.0 | ✅ Released | 172 | Full widget library, 7 known bugs |
| v0.2.0-dev | ✅ Complete | 280 | Architecture rebuild: Container, Frame, Window, Desktop, Overlay, Application, Dialog, HorizontalBar, MsgBox, JSON themes |
| v0.2.1 | ✅ Released | 321 | Scrollbar fixes, window handling, Builder Lite, task shelf, lifecycle hooks, title centering |
| v0.2.2 | ✅ Released | 335 | MenuBar → Overlay dropdown refactor, minimized window tray fix |
| v0.2.3 | ✅ Released | 357 | Scrollbar thumb positioning fix, `post_event()`, Ratatui 0.30 upgrade |
| v0.2.4 | ✅ Released | 435 | Drag-and-drop state machine (`CM_DRAG_START`, `CM_DRAG_MOVE`, `CM_DRAG_END`) |

---

## v0.2.1 — Window Handling + Composability (RELEASED)

**Plan:** [`docs/PLAN-v0.2.1.md`](docs/PLAN-v0.2.1.md)
**Branch:** `v0.2-rebuild`

### Completed ✅
- [x] Scrollbar inactive styling (3 theme fields, `set_active()`, focus propagation)
- [x] Scrollbar hover fix (Frame routes mouse events to border scrollbars)
- [x] **Phase 3:** Task bar shelf for minimized windows
- [x] **Phase 4a:** FrameConfig struct
- [x] **Phase 4b:** Window Builder Lite pattern
- [x] **Phase 4c:** Widget presets (Window::editor, ::palette, ::tool)
- [x] **Phase 5:** View lifecycle hooks (on_focus, on_blur)
- [x] **Phase 6:** Demo update
- [x] **Phase 7:** JSON themes update

### Dependencies
```
Phase 3 (task shelf)    ──────────────┐
Phase 4a (FrameConfig)  → 4b (builder) → 4c (presets) ──┐
Phase 5 (lifecycle)     ──────────────┐                  │
Phase 7 (JSON themes)   ──────────────┤                  │
                                      └── Phase 6 (demo) ┘
```

---

## v0.2.2 — MenuBar → Overlay Dropdown Refactor (COMPLETED)

**Plan:** [`docs/PLAN-v0.2.2.md`](docs/PLAN-v0.2.2.md)
**Branch:** `v0.2-rebuild`

### Phases
- [x] **Phase 1:** MenuBox enhancement — command emission on confirm, Left/Right navigation
- [x] **Phase 2:** HorizontalBar simplification — remove self-draw (~170 lines), post deferred events
- [x] **Phase 3:** Application orchestration — intercept CM_OPEN_DROPDOWN, create MenuBox overlays
- [x] **Phase 4:** OverlayManager — dismiss callback (on_dismiss command)
- [x] **Phase 5:** Demo + integration tests
- [x] **F2: Minimized window tray fix** — Frame draws at height=1, task shelf visible + clickable

### Goal
Menu dropdowns render via OverlayManager above all windows, eliminating clip-area limitations. Removes ~200 lines of duplicate drawing/event code from HorizontalBar. Minimized windows visible in task shelf with close button + title, click-to-restore working.

---

## v0.2.3 — Scrollbar Fix + Integration (RELEASED)

- [x] **F3:** Scrollbar thumb positioning fix — mouse click maps to middle of positions; only area between arrow buttons counts for thumb calculation
- [x] **F6:** `Application::post_event()` public API
- [x] Ratatui 0.30 upgrade

---

## v0.2.4 — Drag-and-Drop (RELEASED)

- [x] **Drag-and-drop state machine:** `CM_DRAG_START`, `CM_DRAG_MOVE`, `CM_DRAG_END` commands
- [x] `Application::start_drag()`, `end_drag()`, `drag_payload()`, `is_dragging()`, `drag_origin()`
- [x] `Container::dispatch_event()` posts drag events on Left mouse button interactions

---

## v0.4.0 — SSH Optimization & Animations (PLANNED)

### 1. Partial Invalidation (SSH Optimization)
**Problem:** Full-screen repaint on every event wastes bandwidth over SSH.
**Solution:** Track dirty regions and redraw only changed areas.

**Tasks**:
- Add `dirty_rect: Option<Rect>` to `ViewBase`.
- Modify `Application::draw()` to clip rendering to dirty regions.
- Update `View::draw()` to set `dirty_rect` when content changes.
- Add `invalidate_rect(&mut self, rect: Rect)` to `Application`.

**Files**:
- `src/view.rs` (add `dirty_rect` to `ViewBase`)
- `src/application.rs` (clip rendering to dirty regions)

**Verification**:
- Test with `cargo run --example demo` over SSH (reduced bandwidth).
- Ensure `cargo test` passes.

### 2. TachyonFX Integration (Animations)
**Problem:** No support for animations or visual effects.
**Solution:** Add post-render effects via `EffectManager`.

**Tasks**:
- Add `EffectManager` trait and implementations (fade, slide, pulse).
- Add `effect_manager: Option<Box<dyn EffectManager>>` to `Application`.
- Modify `Application::draw()` to apply effects after rendering.
- Add `with_effect(&mut self, effect: Box<dyn EffectManager>)` to `Application`.

**Files**:
- `src/effect.rs` (new file, `EffectManager` trait + implementations)
- `src/application.rs` (add `effect_manager` field + methods)

**Verification**:
- Test with `cargo run --example demo` (animations visible).
- Ensure `cargo test` passes.

### 3. Clipboard Integration
**Problem:** No copy/paste support between views.
**Solution:** Add `Clipboard` trait and platform backends.

**Tasks**:
- Add `Clipboard` trait (`get_text()`, `set_text()`).
- Add platform backends (`ArboardClipboard`, `NullClipboard`).
- Add `clipboard: Box<dyn Clipboard>` to `Application`.
- Add `with_clipboard(&mut self, clipboard: Box<dyn Clipboard>)` to `Application`.

**Files**:
- `src/clipboard.rs` (new file, `Clipboard` trait + backends)
- `src/application.rs` (add `clipboard` field + methods)

**Verification**:
- Test copy/paste in input widgets.
- Ensure `cargo test` passes.

### 4. Gauge/ProgressBar
**Problem:** No progress indicators for background tasks.
**Solution:** Add `Gauge` widget with theme integration.

**Tasks**:
- Add `Gauge` widget with `percent: u16` and optional label.
- Theme fields: `gauge_track`, `gauge_fill`, `gauge_label`.
- Builder Lite: `with_percent()`, `with_label()`, `with_direction(Horizontal/Vertical)`.

**Files**:
- `src/gauge.rs` (new file)
- `src/theme.rs` (add gauge theme fields)
- `src/lib.rs` (module + prelude export)

**Verification**:
- Test in demo with simulated background task.
- Ensure `cargo test` passes.

---

## Future

- **Tree widget:** Hierarchical list/tree view with expand/collapse.
- **Multi-document interface (MDI):** Tabbed windows support.
- **Channel-Based Events:** Async UI updates via `tokio::mpsc`.
- **Runtime Theme Editor:** Edit themes at runtime.
- **Community Controls pattern:** Documented extension pattern for community-contributed View implementations (guide + example).

---

## Architecture Principles (PERMANENT — guide all versions)

1. **View trait stays unified** — state + events + render in one trait. NOT separate Widget + EventHandler. turbo-tui is a component framework, not a widget library.
2. **Builder Lite for construction** — `self`-consuming methods returning `Self`. No separate Builder struct.
3. **Deferred events over Action returns** — Keep deferred event queue. Action enum doesn't support three-phase dispatch.
4. **Frame owns scrollbars** — `Option<ScrollBar>` on Frame, not Container children. Scrollbars sit on the border.
5. **Post-render effects = future** — TachyonFX-style transforms. Design must not prevent it.
6. **Centralized catch + three-phase dispatch** — Three-phase: PreProcess → Focused → PostProcess.

---

## Key Documentation

| Document | Path | Purpose |
|----------|------|---------|
| CLAUDE.md | `CLAUDE.md` | Agent configuration, conventions, current state |
| HISTORY.md | `HISTORY.md` | Change log (append-only) |
| v0.2 Plan | `docs/PLAN-v0.2.md` | v0.2 architecture rebuild plan (completed) |
| v0.2.1 Plan | `docs/PLAN-v0.2.1.md` | v0.2.1 sprint plan (completed) |
| HorizontalBar Design | `docs/DESIGN-horizontal-bar.md` | Unified MenuBar+StatusBar design |
| ADR-002 | `~/four-code/docs/ADR-002-turbo-tui-windowing.md` | Architecture decision record |
