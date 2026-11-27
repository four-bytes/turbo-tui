# turbo-tui — Roadmap

> Last updated: 2026-11-27

> Product vision and milestones: [VISION.md](VISION.md)

## Version History

| Version | Status | Tests | Key Features |
|---------|--------|-------|-------------|
| v0.1.0 | ✅ Released | 172 | Full widget library, 7 known bugs |
| v0.2.0-dev | ✅ Complete | 280 | Architecture rebuild: Container, Frame, Window, Desktop, Overlay, Application, Dialog, HorizontalBar, MsgBox, JSON themes |
| v0.2.1 | ✅ Released | 321 | Scrollbar fixes, window handling, Builder Lite, task shelf, lifecycle hooks, title centering |
| v0.2.2 | ✅ Released | 335 | MenuBar → Overlay dropdown refactor, minimized window tray fix |
| v0.2.3 | ✅ Released | 357 | Scrollbar thumb positioning fix, `post_event()`, Ratatui 0.30 upgrade |
| v0.2.4 | ✅ Released | 435 | Drag-and-drop state machine (`CM_DRAG_START`, `CM_DRAG_MOVE`, `CM_DRAG_END`) |
| v0.4.0 | ✅ Released | ~565 | Own cell backend (`turbo_tui::core` types), own `Terminal` (persistent buffers + cell diff), `Canvas` trait, Ratatui dependency removed |

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

## v0.5.0 — SSH Optimization & Animations (PLANNED)

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

**Dependency (resolved in v0.4.0):** Historically, Ratatui reset the back buffer every frame (`Terminal::swap_buffers` → `reset()`), so drawing only the dirty rect left empty cells everywhere else and the diff erased the screen there. That is why `Application` invalidated the full screen on every key/mouse event and the dirty rect had no effect. The own backend (v0.4.0) provides a persistent buffer → see Future "Own backend instead of Ratatui" (completed).

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
- **Runtime Theme Editor:** Edit themes at runtime.
- **Community Controls pattern:** Documented extension pattern for community-contributed View implementations (guide + example).

### Rendering Backend & Runtime — Dual UI (from the FourSix/Sixel discussion, 2026-11)

The former "Dual Render" framing is superseded by **Dual UI** (epic #39): one session, one view
tree, native terminal **and** browser frontends with bidirectional input. Decisions:
[ADR #41 Dual UI](docs/ADR-dual-ui.md) and [ADR #36 pixel geometry](docs/ADR-pixel-geometry.md).
The browser is not a terminal stream; it renders a semantic display list per layer at its own
resolution.

- **Own backend instead of Ratatui:** ✅ **Completed in v0.4.0** (issue #21, S1–S6). From Ratatui, turbo-tui used `Buffer`, `Rect`, `Position`, `Style`/`Color`/`Modifier` (views and themes), `Terminal` (diff + output) and `Frame`, which was part of the public API via `Application::draw(&mut Frame)` and had to be replaced with an own frame/target type (breaking). Widgets, layouts, `Paragraph` and `Block` were not used, because the Turbo Vision views are our own code. Done: own `Cell`/`Buffer`/`Rect`/`Style` types (`turbo_tui::core`), own cell diff, `Terminal` + `Canvas`/`CellCanvas`, crossterm directly. Ratatui was not kept around as an adapter. Migration path for the consumer four-code (used Ratatui types directly) tracked in #10.
- **Canvas trait:** widgets draw via `Canvas` instead of writing buffer cells directly. The canvas provides a metric (width × height of a character) that widgets compute with. Two implementations:
  - `CellCanvas`: unit = cell, metric 1 × 1. This is today's text mode; positioning is cell-wise.
  - `PixelCanvas`: adapter onto the FourSix Engine. Surface unit = pixel, metric from the font (e.g. 8 × 16); layout stays cell-based, sub-cell window positions come from the per-layer offset ([ADR #36](docs/ADR-pixel-geometry.md)), output via Sixel.
- **FourSix Engine (own layer, developed in `four-sixel-lab`):** pixel engine with graphics commands (rect, line, blit), text, clipping, composition and dirty tracking. It knows nothing about terminals: Sixel is one output path of the host; web or a native window would be others. Game engines use it directly, without the workbench.
- **Order:** (1) ✅ done in v0.4.0: own cell backend + `Canvas` trait + `CellCanvas`, migrate widgets, remove Ratatui (#10) → (2) build and measure the engine in `four-sixel-lab` → (3) `PixelCanvas` adapter, turbo-tui windows in Sixel mode
- **Open points for Sixel mode:** pixel mouse (`DECSET 1016`) in Windows Terminal; no text selection via the terminal (own selection + OSC 52); own fonts/HiDPI; bandwidth over SSH (adaptive fallback to text mode)
- **Persistent buffer + dirty rects:** prerequisite for partial invalidation (v0.5.0, item 1) and for bandwidth-efficient Sixel over SSH.
- **Tokio host (optional feature `tokio`):** UI owner task with `select!` over crossterm `EventStream`, service channels (`mpsc` for commands/results, `watch` for latest state) and a render tick (`MissedTickBehavior::Skip`, frame cap). Many parallel producers, **one** sequential consumer: the view tree stays in a single task (three-phase dispatch is sequential; no `Send`/locks needed). Service messages feed into the existing deferred event queue. Replaces the former "Channel-Based Events" item.

### Dual UI slices (epic #39, ADRs #41 / #36)

Decisions: [ADR #41 Dual UI](docs/ADR-dual-ui.md) — display list per layer + WM state, per-layer
diffs, JSON v1, latest-frame-wins, one shared desktop, controller roles, two security stages.
[ADR #36 pixel geometry](docs/ADR-pixel-geometry.md) — cell layout + per-layer pixel offset.

- [ ] **D1 #41:** ADR Dual UI (protocol, diffs, geometry, multi-client, security) and the
      companion geometry ADR #36. ✅ decision written; architect sign-off pending.
- [ ] **D2 #42:** `RecordingCanvas` + diff protocol behind feature `web` (off by default):
      display list per layer with stable `ViewId`, full snapshot, per-layer diff, input mapping,
      latest-frame-wins. Testable without a browser.
- [ ] **D3 #43:** Web host (axum + WebSocket) and the Deno/TypeScript browser client, bidirectional
      sync, **localhost only**, token, read-only by default (`--web-control`), measurements.
- [ ] **D4 #44:** Data widgets (`LineChart`, `Sparkline`, `Gauge`, `BarChart`) with per-renderer
      output: braille/blocks in the cell canvas, vector/anti-aliased in pixel, SVG/Canvas in the
      browser. Ops carry data, not raster.
- [ ] **D5 #45:** Open-port web GUI: explicit opt-in, TLS, per-user tokens and `viewer`/
      `controller` roles, Origin check, CSP/CSRF, rate/size limits, timeouts, audit log,
      `docs/WEB-SECURITY.md`.

Dependencies: #42 after #41 and #35 (done) · #43 after #42 · #44 after #35 (browser after #43,
pixel part after #34) · #45 after #43 (and #44 for the monitoring case). #34 and #51 depend on
four-sixel-lab#58; #52 depends on four-sixel-lab#58 as well. #40 (prior art) runs in parallel and
may adjust positioning, not the protocol.

### Pixel geometry slices (ADR #36)

- [ ] **G1:** `PixelOffset` + `LayerOrigin` + `CellRect`/`PixelRect` in `foursix-look`; `ViewBase`
      offset, `Canvas::begin_layer`/`end_layer` defaults; `offset = 0` snapshots byte-identical.
- [ ] **G2:** `Window` drag/resize with offsets, `Desktop` tile/cascade/shelf force zero, WM state
      serialization gains the offset.
- [ ] **G3:** `PixelCanvas` exact geometry, `DECSET 1016` pointer mapping, look-owned
      `frame_layout` hit-testing — the pixel half of #34, after four-sixel-lab#58 / #51.
- [ ] **G4:** `RecordingCanvas`/browser layer origins and pointer conversion — geometry part of
      #42/#43.
- [ ] **G5:** four-code adoption: persist offsets, opt in per window type, verify text mode
      unchanged.

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
| AGENTS.md | `AGENTS.md` | Agent configuration, conventions, current state |
| HISTORY.md | `HISTORY.md` | Change log (append-only) |
| v0.2 Plan | `docs/PLAN-v0.2.md` | v0.2 architecture rebuild plan (completed) |
| v0.2.1 Plan | `docs/PLAN-v0.2.1.md` | v0.2.1 sprint plan (completed) |
| Dual UI ADR | `docs/ADR-dual-ui.md` | Protocol, diffs, multi-client, security (Dual UI) |
| Pixel Geometry ADR | `docs/ADR-pixel-geometry.md` | Geometry units, free pixel positioning, snap rule |
| Skin/Look ADR | `docs/ADR-skin-look.md` | Palette, look families, shared primitive vocabulary |
| HorizontalBar Design | `docs/DESIGN-horizontal-bar.md` | Unified MenuBar+StatusBar design |
| ADR-002 | `~/four-code/docs/ADR-002-turbo-tui-windowing.md` | Architecture decision record |
