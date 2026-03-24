# turbo-tui — Roadmap

> Last updated: 2026-03-22

## Version History

| Version | Status | Tests | Key Features |
|---------|--------|-------|-------------|
| v0.1.0 | ✅ Released | 172 | Full widget library, 7 known bugs |
| v0.2.0-dev | ✅ Complete | 280 | Architecture rebuild: Container, Frame, Window, Desktop, Overlay, Application, Dialog, HorizontalBar, MsgBox, JSON themes |
| v0.2.1 | ✅ Released | 321 | Scrollbar fixes, window handling, Builder Lite, task shelf, lifecycle hooks, title centering |
| v0.2.2 | ✅ Released | 335 | MenuBar → Overlay dropdown refactor, minimized window tray fix |

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
- [x] **Phase 7:** JSON theme files update

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

## v0.2.3 — Scrollbar Fix + Integration (PLANNED)

- [ ] **F3:** Scrollbar thumb positioning fix — mouse click maps to middle of positions; only area between arrow buttons counts for thumb calculation
- [ ] **F4:** TachyonFX integration point (`Application::draw()` + optional `EffectManager`)
- [ ] **F5:** Channel-based external events (`tokio::mpsc` for background → UI)
- [ ] **F6:** `Application::post_event()` public API
- [ ] **F7:** Widget validation framework
- [ ] Once grabbed, the thumb movement should follow the mouse even if outside of scrollbar until the button is released

---

## v0.3.0 — Advanced Widgets + Invalidation (FUTURE)

- [ ] **F8:** Partial invalidation system — dirty-region tracking for partial window redraws instead of full-screen repaint (reduces SSH bandwidth for remote usage)
- [ ] **F9:** Tree widget (hierarchical list/tree view)
- [ ] Drag-and-drop between windows
- [ ] Multi-document interface (MDI) patterns
- [ ] Clipboard integration (copy/paste between views)
- [ ] **Community Controls pattern** — documented extension pattern for community-contributed View implementations (guide + example)

---

## Architecture Principles (PERMANENT — guide all versions)

1. **View trait stays unified** — state + events + render in one trait. NOT separate Widget + EventHandler. turbo-tui is a component framework, not a widget library.
2. **Builder Lite for construction** — `self`-consuming methods returning `Self`. No separate Builder struct.
3. **Deferred events over Action returns** — Keep deferred event queue. Action enum doesn't support three-phase dispatch.
4. **Frame owns scrollbars** — `Option<ScrollBar>` on Frame, not Container children. Scrollbars sit on the border.
5. **Post-render effects = future** — TachyonFX-style transforms. Not yet, but design must not prevent it.
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
