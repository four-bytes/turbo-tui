# Plan — Own Cell Backend + Canvas Trait, Remove Ratatui (Epic #10)

> Status: planned · Created: 2026-11-26 · Epic: #10 · Sub-issues: #13–#21, four-code #6/#7 · Roadmap: ROADMAP.md → Future → "Rendering Backend & Runtime", step (1)

## Goal

turbo-tui renders through its own cell types and terminal backend, widgets draw via a `Canvas`
trait, and the `ratatui` dependency is gone. This unlocks:

- **Real dirty rects** — a persistent buffer instead of Ratatui's per-frame reset (`Terminal::swap_buffers` → `reset()`)
- **A `PixelCanvas` later** — the FourSix Engine (developed in `four-sixel-lab`) plugs in as a second canvas implementation (roadmap step 3)
- **No foreign core dependency** — turbo-tui only uses Ratatui as a cell buffer + diff today

## Current state (inventory, 2026-11-26)

| Ratatui item | Where | Notes |
|---|---|---|
| `Buffer` (+ `cell`, `cell_mut`, `buf[(x, y)]`, `set_string`, `set_style`, `set_char`, `symbol`, `content`, `resize`) | all `draw()` implementations; ~95 write sites, most in `menu_box.rs` (23), `scrollbar.rs`, `horizontal_bar.rs` (10 each) | `.symbol(` 82×, `.cell(` 45× (mostly tests) |
| `Rect` (+ `intersection`, `contains`, `clamp`, `is_empty`, `area`) | everywhere | Must stay API-compatible |
| `Position` | views (cursor), `Application::draw` | |
| `Style`, `Color`, `Modifier` | theme, theme_json (color roundtrip), all widgets | ~24 direct `Color::Rgb/Indexed`/`Modifier::` uses |
| `Frame` | **public API**: `Application::draw(&mut ratatui::Frame)` | Breaking change. Note: turbo-tui already has its own `frame::Frame` (window border) → name clash, new output type must not be called `Frame` |
| `Terminal`, `TestBackend` | `examples/demo.rs`, `application.rs` tests, 15 files use `Buffer::empty`/`TestBackend` in tests | |
| Widgets/Layout/Text | **not used** | |

Existing seam: `src/clip.rs` (`set_string_clipped`) is already a small drawing helper used by 7 widgets — it becomes part of `CellCanvas`.

Consumer **four-code** (`~/four-code`, path dependency) uses `Buffer`, `Rect`, `Position`, `Style`/`Color`/`Modifier`, `Paragraph::new` (11× single-line text), `Layout`/`Constraint` (only `panel.rs`), `Terminal`/`CrosstermBackend` (main loop), and Ratatui styles in `four-code-highlight`.

## Design decisions

1. **Ratatui-compatible names and signatures** for the core types (`Rect { x, y, width, height }`, `Buffer::cell_mut`, `buf[(x, y)]`, `Style::default().fg(..)`). Migration is mostly an import change, for turbo-tui and four-code.
2. **Seam first, swap second**: the core types are first introduced as re-exports of Ratatui under `turbo_tui::core` (and the prelude). All code switches its imports. Only then the implementation is replaced — no big-bang change.
3. **`View::draw(&self, canvas: &mut dyn Canvas, clip: Rect)`** — `dyn` because `View` must stay object-safe (`Box<dyn View>`).
4. **Escape hatch during migration**: `CellCanvas::buffer_mut()` lets views keep their current buffer code after the signature change; widget groups migrate one by one; the hatch is removed at the end.
5. **Units**: `CellCanvas` works in cells (metric 1 × 1). Making views *metric-aware* for pixel units is **not** part of this epic — it belongs to the `PixelCanvas` step (roadmap step 3). The trait already exposes `metric()` so that step does not change the trait again.
6. **Output type name**: `Terminal` (own) + `Buffer`; no own type named `Frame`.
7. **Keep `unicode-width`** (as Ratatui does) for wide characters in the buffer.
8. **crossterm stays** for input events and terminal commands.

## Canvas trait (draft)

```rust
pub trait Canvas {
    /// Width × height of one character in canvas units (CellCanvas: 1 × 1).
    fn metric(&self) -> (u16, u16);
    /// Drawable area in canvas units.
    fn area(&self) -> Rect;
    fn draw_text(&mut self, x: u16, y: u16, text: &str, style: Style, clip: Rect);
    fn put_char(&mut self, x: u16, y: u16, ch: char, style: Style, clip: Rect);
    fn fill(&mut self, rect: Rect, ch: char, style: Style);
    fn set_style(&mut self, rect: Rect, style: Style);
    fn set_cursor(&mut self, pos: Option<Position>);
}
```

Final method set is decided in sub-issue S4 from the actual write sites (box-drawing frames may get a dedicated `draw_frame` later, for nicer pixel rendering).

## Sub-issues

Order is strict (one branch at a time). Each is ≤ 5 steps.

| # | Title | Depends on | Breaking |
|---|---|---|---|
| S1 (#13) | `[REFACTOR]` Core type seam: `turbo_tui::core` re-exports, switch all imports | #7 merged | no |
| S2 (#14) | `[REFACTOR]` Own core types: `Rect`, `Position`, `Style`/`Color`/`Modifier`, `Cell`, `Buffer` | S1 (#13) | no (same API) |
| S3 (#15) | `[FEAT]` Own terminal backend: persistent buffers, diff, crossterm output, test backend | S2 (#14) | **yes** (`Application::draw`) |
| S4 (#16) | `[REFACTOR]` `Canvas` trait + `CellCanvas`, new `View::draw` signature (with escape hatch) | S3 (#15) | **yes** (`View::draw`) |
| S5a (#17) | `[REFACTOR]` Migrate core views to `Canvas`: frame, window, desktop, container, overlay, dialog, application | S4 (#16) | no |
| S5b (#18) | `[REFACTOR]` Migrate bars and menus: horizontal_bar, menu_box, menu_bar, status_bar | S4 (#16) | no |
| S5c (#19) | `[REFACTOR]` Migrate widgets: button, static_text, input_line, listbox, scrollbar, msgbox | S4 (#16) | no |
| S5d (#20) | `[REFACTOR]` Migrate editor, memo, syntax | S4 (#16) | no |
| S6 (#21) | `[CHORE]` Remove escape hatch + `ratatui` dependency, docs, migration notes, version bump | S5a–d | — |

Consumer issues in **four-code** (separate repo):

| # | Title | After |
|---|---|---|
| F1 (four-code#6) | Switch imports to `turbo_tui` core types; replace `Paragraph::new` with `draw_text`/`set_string`; own split logic instead of `Layout` in `panel.rs`; highlight styles on turbo-tui `Style` | S1 (works against re-exports already) |
| F2 (four-code#7) | Main loop on turbo-tui `Terminal`; `View::draw` on `Canvas` for four-code views | S4 (#16) |

### S1 — Core type seam
- New module `src/core.rs`: `pub use ratatui::{buffer::{Buffer, Cell}, layout::{Position, Rect}, style::{Color, Modifier, Style}};`
- Prelude exports the core types
- Replace every `use ratatui::…` in `src/`, `tests/`, `examples/` with `crate::core` / `turbo_tui::core` (except `Terminal`/`Frame`/`TestBackend`, handled in S3)
- AC: `grep -rn "ratatui::" src tests examples` only hits `core.rs` and the S3-owned items; all tests green; no behaviour change

### S2 — Own core types
- Implement `Rect`, `Position`, `Color`, `Modifier`, `Style`, `Cell`, `Buffer` in `src/core/` with the API subset turbo-tui and four-code use (inventory above)
- Wide-character handling via `unicode-width`
- Temporary `From` conversions to Ratatui types, used only by `Application::draw` until S3
- Port/extend unit tests (buffer indexing, set_string clipping, wide chars, style merge)
- AC: core types no longer re-export Ratatui; all existing tests green unchanged

### S3 — Own terminal backend
- `Terminal<W: Write>`: front/back `Buffer`, `draw(|buf| …)`, diff → crossterm (`MoveTo`, colors, attributes, `Print`) in one buffered write, cursor show/hide/position, resize
- Persistent back buffer (no reset per frame) → `Application::draw` may redraw only the dirty rect correctly
- `TestBackend` replacement: in-memory terminal for tests
- `Application::draw(&mut ratatui::Frame)` → `Application::draw(&mut Buffer)` (or `render(&mut Terminal)`); demo + tests switched
- AC: demo runs without Ratatui's `Terminal`; test proves a dirty-rect redraw leaves the rest of the screen intact; diff writes only changed cells (test)

### S4 — Canvas trait + CellCanvas
- `Canvas` trait (see draft) + `CellCanvas<'a>` wrapping `&mut Buffer`; `clip.rs` helpers move into it
- `View::draw(&self, canvas: &mut dyn Canvas, clip: Rect)`; all views compile via `CellCanvas::buffer_mut()` escape hatch
- Container/desktop/overlay pass the canvas through
- AC: all views on the new signature, all tests green, escape-hatch uses listed in the PR

### S5a–d — Widget migration
- Per group: replace escape-hatch buffer code with `Canvas` calls; snapshot-style tests compare rendered buffers before/after (identical output)
- AC per group: no `buffer_mut()` left in the group's files; rendered output identical (tests)

### S6 — Cleanup
- Remove `buffer_mut()` escape hatch and the `ratatui` dependency
- Update `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `Cargo.toml` description/keywords ("for Ratatui" → standalone)
- `HISTORY.md` with migration notes (type mapping, `Application::draw`, `View::draw`)
- Version bump (see open decisions)
- AC: `cargo tree | grep ratatui` empty; docs updated; four-code F1/F2 merged or explicitly scheduled

## Risks

| Risk | Mitigation |
|---|---|
| Subtle rendering differences (wide chars, style merging, cursor) | Snapshot tests in S2/S5 comparing buffers before/after |
| Terminal diff output slower/buggier than Ratatui's | S3 tests on written bytes; manual check locally + SSH |
| four-code breaks between S3 and F2 | four-code pins a turbo-tui git rev/tag until F2 is done |
| Scope creep into metric-aware views | Explicitly out of scope (decision 5) |

## Decisions (2026-11-26)

1. **Version**: API break accepted; released as **v0.4.0** (partial invalidation depends on it).
2. **Module name**: `turbo_tui::core`. Inside the crate always refer to it as `crate::core::…` (a root module named `core` shadows the built-in `core` crate in root-level `use core::…`).
3. **`Application::draw` target**: `&mut Buffer` — the consumer owns the `Terminal`; a runtime that owns output comes with the Tokio host.
