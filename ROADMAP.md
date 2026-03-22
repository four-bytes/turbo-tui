# turbo-tui — Roadmap

> Last updated: 2026-03-22

## Version History

| Version | Status | Tests | Key Features |
|---------|--------|-------|-------------|
| v0.1.0 | ✅ Released | 172 | Full widget library, 7 known bugs |
| v0.2.0-dev | ✅ Complete | 280 | Architecture rebuild: Container, Frame, Window, Desktop, Overlay, Application, Dialog, HorizontalBar, MsgBox, JSON themes |
| v0.2.1 | ✅ Released | 321 | Scrollbar fixes, window handling, Builder Lite, task shelf, lifecycle hooks, title centering |
| v0.2.2-dev | 🔧 In Progress | — | MenuBar → Overlay dropdown refactor |

---

## v0.2.1 — Window Handling + Composability (RELEASED)

**Plan:** [`docs/PLAN-v0.2.1.md`](docs/PLAN-v0.2.1.md)
**Branch:** `v0.2-rebuild`

### Completed ✅
- [x] Scrollbar inactive styling (3 theme fields, `set_active()`, focus propagation)
- [x] Scrollbar hover fix (Frame routes mouse events to border scrollbars)
- [x] Reference analysis (Ratatui patterns, TachyonFX, tui-rs demo, gping)
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

## v0.2.2 — MenuBar → Overlay Dropdown Refactor (CURRENT)

**Plan:** [`docs/PLAN-v0.2.2.md`](docs/PLAN-v0.2.2.md)
**Branch:** `v0.2-rebuild`

### Phases
- [ ] **Phase 1:** MenuBox enhancement — command emission on confirm, Left/Right navigation
- [ ] **Phase 2:** HorizontalBar simplification — remove self-draw (~170 lines), post deferred events
- [ ] **Phase 3:** Application orchestration — intercept CM_OPEN_DROPDOWN, create MenuBox overlays
- [ ] **Phase 4:** OverlayManager — dismiss callback (on_dismiss command)
- [ ] **Phase 5:** Demo + integration tests

### Goal
Menu dropdowns render via OverlayManager above all windows, eliminating clip-area limitations. Removes ~200 lines of duplicate drawing/event code from HorizontalBar.

---

## v0.2.3 — Integration + External Events (PLANNED)

- [ ] **F4:** TachyonFX integration point (`Application::draw()` + optional `EffectManager`)
- [ ] **F5:** Channel-based external events (`tokio::mpsc` for background → UI)
- [ ] **F6:** `Application::post_event()` public API
- [ ] **F7:** Widget validation framework

---

## v0.3.0 — Advanced Widgets (FUTURE)

- [ ] **F9:** Tree widget (hierarchical list/tree view)
- [ ] Drag-and-drop between windows
- [ ] Multi-document interface (MDI) patterns
- [ ] Clipboard integration (copy/paste between views)

---

## Architecture Principles (PERMANENT — guide all versions)

These were established after reviewing Ratatui's official patterns (2026-03-22). See [`docs/RES-0002-reference-projects-architecture.md`](docs/RES-0002-reference-projects-architecture.md) for full analysis.

1. **View trait stays unified** — state + events + render in one trait. NOT separate Widget + EventHandler. turbo-tui is a component framework, not a widget library. Ref: [Ratatui Component Architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/)

2. **Builder Lite for construction** — `self`-consuming methods returning `Self`. No separate Builder struct. Ref: [Ratatui Builder Lite](https://ratatui.rs/concepts/builder-lite-pattern/)

3. **Deferred events over Action returns** — Keep deferred event queue. Action enum doesn't support three-phase dispatch.

4. **Frame owns scrollbars** — `Option<ScrollBar>` on Frame, not Container children. Scrollbars sit on the border.

5. **Post-render effects = future** — TachyonFX-style transforms. Not yet, but design must not prevent it. Ref: [TachyonFX](https://github.com/junkdog/tachyonfx)

6. **Centralized catch + three-phase dispatch** — Approach 2 from Ratatui's event handling docs. Ref: [Event Handling](https://ratatui.rs/concepts/event-handling/)

---

## Reference Projects

| Project | URL | Relevance |
|---------|-----|-----------|
| Ratatui Component Architecture | https://ratatui.rs/concepts/application-patterns/component-architecture/ | Component trait — turbo-tui's View is equivalent |
| Ratatui Builder Lite | https://ratatui.rs/concepts/builder-lite-pattern/ | Self-consuming fluent API pattern |
| Ratatui Event Handling | https://ratatui.rs/concepts/event-handling/ | 3 event patterns — we use approach 2 |
| Ratatui Widgets | https://ratatui.rs/concepts/widgets/ | Widget/StatefulWidget/WidgetRef traits |
| TachyonFX | https://github.com/junkdog/tachyonfx | Post-render effects, animation integration |
| tui-rs demo | https://github.com/fdehau/tui-rs/tree/master/examples/demo | Dense dashboard, gauge/chart patterns |
| gping | https://github.com/orf/gping | Real-time gauge, ring-buffer data model |
| turbo-vision-4-rust | https://github.com/aovestdipaperino/turbo-vision-4-rust | Original Borland TV patterns (MIT) |

---

## Key Documentation

| Document | Path | Purpose |
|----------|------|---------|
| CLAUDE.md | `CLAUDE.md` | Agent configuration, conventions, current state |
| HISTORY.md | `HISTORY.md` | Change log (append-only) |
| v0.2 Plan | `docs/PLAN-v0.2.md` | v0.2 architecture rebuild plan (completed) |
| v0.2.1 Plan | `docs/PLAN-v0.2.1.md` | Current progression plan with reference findings |
| Research | `docs/RES-0002-reference-projects-architecture.md` | Reference projects analysis |
| HorizontalBar Design | `docs/DESIGN-horizontal-bar.md` | Unified MenuBar+StatusLine design |
| ADR-002 | `~/four-code/docs/ADR-002-turbo-tui-windowing.md` | Architecture decision record |
