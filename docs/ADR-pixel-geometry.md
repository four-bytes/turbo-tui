# ADR: Geometry Units and Free Pixel Positioning

- **Status:** Proposed (decision complete, architect review pending)
- **Date:** 2026-11-27
- **Issue:** #36
- **Companion:** [ADR-dual-ui.md](ADR-dual-ui.md) — the protocol and multi-client model built on this geometry
- **Related:** #34, #35, #41, #48 · four-sixel-lab#23, four-sixel-lab#58

## Context

turbo-tui v0.4 draws every view through `Canvas`; `CellCanvas` is the only implementation and its
unit is one terminal cell (`Canvas::metric()` returns `1 × 1`, canvas.rs:157–171). Window bounds,
`View` bounds, `Container` child rects and `Desktop` policies are all whole-cell `Rect`s
(`src/core/geometry.rs`). #34 adds a `PixelCanvas` with a real cell metric (for example `8 × 16`)
but keeps the **layout on the cell grid**. #36 is the next step: windows and text may sit *between*
cells. This was the original reason for the Sixel path — pixel positioning in pixel mode, cell
positioning in text mode.

Three constraints are already fixed:

1. `VISION.md` §7 leaves geometry units open; #48 decides the shared `foursix-look` geometry stays
   `u16` and `Metrics` are pixel sizes that cell mode maps onto the `1 × 1` grid. 16K
   (`15360 × 8640`) fits in `u16` on both axes.
2. #48 (decision E) makes frame geometry look-owned: `Look::frame_layout` is the single source for
   *drawing and hit-testing*, and pixel gadgets have different sizes than cell gadgets.
3. The Dual UI protocol (#41) must transport positions to a browser at a different resolution
   without forking the window state.

The question is therefore not "cells or pixels" but **which space each value lives in, and where
the conversion happens**.

## Decision summary

1. **Two named spaces, one `Rect` type.** *Layout space* is the cell grid (whole cells, `CellRect`):
   `View`/`Container` bounds, `Desktop` tile/cascade/shelf, window-manager state, input routing.
   *Surface space* is the active canvas unit (`PixelRect`): cells for `CellCanvas`, reference pixels
   for `PixelCanvas`, base pixels for the recording/web canvas. Existing `Rect` stays the plain
   `u16` type; new pixel-aware APIs use `CellRect`/`PixelRect` newtypes so the spaces cannot be
   mixed silently.
2. **Free positioning is a per-layer pixel offset, not a new coordinate system for every view.**
   `PixelOffset { x, y }` (reference pixels, normalised to `0 .. metric`) refines the origin of a
   top-level layer only. It is stored in `ViewBase`, `Window` state and the WM/protocol state.
3. **Layer origin:** `surface_origin = cell_origin × metric + offset`. `Canvas` gains a default
   `begin_layer(ViewId, LayerOrigin)` / `end_layer()` hook. `CellCanvas` ignores the offset,
   `PixelCanvas` applies it exactly, `RecordingCanvas` records it for the browser.
4. **Only the layer origin is sub-cell.** Nested views stay cell-based and inherit the layer
   origin. Frame/background/widget geometry moves freely; glyphs never do.
5. **Text is always on the glyph grid inside its layer.** A sub-cell window position does not
   produce sub-cell text. Text starts at the nearest glyph origin after the layer transform, so it
   stays crisp in every renderer.
6. **Snap rule (cell targets):** normalise the offset into `0 .. metric`, then
   `cell' = cell + round_half_up(offset / metric)`; the whole-cell size is unchanged and the offset
   is dropped. Cell mode never renders half cells and never grows a window.
7. **Hit-testing uses surface space.** Input is converted at the edge: terminal cell events → cell
   coordinates, `DECSET 1016` pixel events → reference pixels, browser pointer → cell + offset.
   `Look::frame_layout` (surface in, surface out) is used for both drawing and hit-testing.
8. **Clipping and dirty regions are surface rects.** A cell canvas clips to the snapped cell rect;
   a pixel canvas clips exactly. With `offset = 0` the result is byte-identical to today.
9. **Tiling is a cell policy.** `Desktop::tile`, `cascade` and the minimized shelf force
   `offset = 0`; free positioning applies to floating windows only. Cell-mode drags set the offset
   to zero; pixel-mode drags accumulate in pixels.
10. **Migration is additive and opt-in.** Cell-only code keeps compiling and behaving unchanged.
    The break is limited to new state fields and the pixel-mode semantics; `Rect` keeps its meaning.

The rest of this ADR records the options and why the alternatives were rejected, then the effects
on each type and the implementation slices.

## Question 1 — Geometry units

**Options.**

- **A. Cell layout + optional per-view pixel offset (chosen).** `Rect` stays whole cells; a
  `PixelOffset` per layer refines the origin. Conversion happens at the canvas boundary.
- **B. Unit-agnostic `Rect<Unit>` / typed generic geometry everywhere.** `Rect<Cell>`,
  `Rect<Pixel>`, phantom units on every view and container.
- **C. All-pixel coordinates.** Every `Rect` value is a pixel; cell mode is the special case
  `metric = 1 × 1`.
- **D. A fixed logical sub-cell grid.** One logical unit is e.g. `1/8 × 1/16` cell; all values are
  in logical units and every renderer quantises.

**Trade-offs.**

- **A** keeps the invariant #34 relies on — layout is a cell grid — and makes text mode and today's
  API exact. It needs one extra field and a layer hook, but no widget becomes metric-aware. The
  cost is that two spaces exist and must be documented and typed.
- **B** is the cleanest type-theoretic model, but it turns every signature, container, theme
  geometry helper and consumer (`four-code`) into a generic API. The window manager still needs a
  cell policy for tile/cascade/shelf, so the generics buy little at this stage.
- **C** changes the meaning of every existing literal (`Rect::new(0, 0, 80, 24)` would become
  80 × 24 pixels), breaks every consumer and still requires a cell grid for text. It also makes
  `CellCanvas` fake a metric it does not have.
- **D** is equivalent to A but changes the meaning of all numbers and introduces a unit that no
  frontend natively uses. It is the right model for a mature layout engine, not for a pre-1.0
  framework whose cell API is already shipped.

**Decision.** **A.** Layout is cells; surface is canvas units; the pixel offset is the only
sub-cell refinement. This matches #48's "metrics in pixels, cell mode maps to `1 × 1`" and keeps
`Rect`'s existing semantics.

### Why not move to `Rect<Unit>` now

The architecture already has a clean seam (`Canvas`). Making every `View` generic would push the
unit through the object-safe `View` trait (`Box<dyn View>`), the prelude and every widget — a large
pre-1.0 break with no user-visible benefit before proportional fonts (M4) exist. The newtypes
`CellRect`/`PixelRect` give most of the safety at the boundaries where mixing is actually possible
(canvas, look, protocol) without generics. Generic units remain a possible 1.0 cleanup.

## Question 2 — Representation of the sub-cell offset

**Options.** (a) An absolute pixel coordinate (`x: u32`). (b) A fraction of a cell
(`0.0 .. 1.0`, `f32`). (c) A normalised pixel offset in reference pixels (`0 .. metric`, `u16`).

**Trade-offs.** (a) duplicates the cell position and lets the two drift; normalising on every
change is error-prone. (b) depends on the renderer's metric at layout time, breaks `Eq`/`Hash` and
makes protocol values non-deterministic across clients. (c) is deterministic, `u16`, independent of
the client's font size, and composes with the cell origin into one canonical value.

**Decision.** (c). `PixelOffset { x: u16, y: u16 }`, invariant `0 ≤ x < metric.width`,
`0 ≤ y < metric.height`. Merging a drag delta uses the canonical total:
`total = cell × metric + offset + delta`; then `cell = total / metric`, `offset = total % metric`.
The offset is expressed in the **session's reference pixels**: the cell size of the active pixel
renderer, or a configured `base_metric` (default `8 × 16`) when only a cell renderer is active.
A cell-only session cannot produce offsets of its own, but it can receive them from the browser;
the canonical state keeps them and the terminal snaps them.

## Question 3 — Text at sub-cell positions

**Options.** (a) Let glyphs follow the sub-cell offset (free text positioning). (b) Keep glyphs on
the font grid inside the layer; only the layer origin moves. (c) Render text as pixel runs in pixel
mode.

**Trade-offs.** (a) produces blurry or resampled glyphs, breaks text selection/copy and forces
per-glyph subpixel placement in every renderer. (c) loses real font rendering, search and
selection, and makes the browser a raster viewer. (b) keeps glyphs crisp, keeps text runs
grid-based for selection, and still allows a window to sit at any pixel position.

**Decision.** (b). A window can be positioned at `cell + (dx, dy)` pixels; its title, labels and
editor text start at the nearest glyph origin *inside* that window. The frame, bevels, background
and gadgets move freely; text does not. Proportional/free text layout is explicitly an M4 concern
(`VISION.md` §5, four-sixel-lab#23), not part of this decision.

## Question 4 — Snap rule (mapping back to cells)

**Options.** (a) Floor to the containing cell. (b) Round to the nearest cell. (c) Round the origin
and ceil the size.

**Trade-offs.** (a) shifts a window up to one full cell to the left/up, which is visibly wrong when
the browser has positioned it. (c) avoids clipping but makes a window grow by a cell during a
one-pixel drag, causing layout jitter and surprising tile/shelf results. (b) is stable and
symmetric; a sub-cell sliver that falls into the neighbouring cell is not rendered in text mode,
which is the correct degradation.

**Decision.** (b), with `round_half_up`, applied only to the **origin**; the cell size is
unchanged. The normalised offset is dropped after snapping. Concretely:

```
fn snap(origin_cell: u16, offset_px: u16, cell_px: u16) -> u16 {
    origin_cell + (offset_px + cell_px / 2) / cell_px   // integer, half up
}
```

The same rule is used by `CellCanvas`, by a terminal frontend in a Dual UI session and by any
client that cannot render sub-cell positions. Tile, cascade and the minimized shelf set
`offset = 0` before applying their policy. A pixel renderer never snaps; it uses the exact origin.
An optional grid snap (`snap_to_cells` on `Window`/`Application`) can be offered later for users
who want free positioning but tidy edges; it is not required by this ADR.

## Question 5 — Clipping and dirty regions

**Options.** (a) Keep clipping in cells and ignore the offset. (b) Clip in surface space and derive
the cell clip by snapping. (c) Keep both a cell clip and a pixel clip everywhere.

**Trade-offs.** (a) lets a pixel window paint outside its snapped cell bounds or leaves stale
pixels. (b) is one canonical space; the cell clip is a cheap projection. (c) duplicates state and
invites divergence.

**Decision.** (b). The layer's surface rect is the clip for pixel rendering; the cell clip is the
snapped rect (`snap` per axis, same size). Dirty tracking stores the exact surface rect for a pixel
sink and the union of the old and new snapped rects for a cell sink. With `offset = 0`, snapping is
the identity and the existing partial-invalidation logic (ROADMAP v0.5.0, item 1) is unaffected.

## Question 6 — Mouse and hit-testing

**Options.** (a) Keep cell events everywhere; map pixel events by division. (b) Convert every
frontend's pointer into canonical `cell + offset` and hit-test in surface space through
`Look::frame_layout`.

**Trade-offs.** (a) loses sub-cell precision and cannot hit pixel-sized gadgets. (b) matches the
single geometry source from #48 and lets every frontend share one hit-test path; the conversion is
small and testable.

**Decision.** (b). `Pointer { cell: Position, offset: PixelOffset }` is the canonical form.

| Source | Event | Conversion |
|---|---|---|
| Terminal, text | crossterm cell event | `cell`, `offset = 0` |
| Terminal, pixel | `DECSET 1016` pixels | `cell = px / metric`, `offset = px % metric` |
| Browser | session-space pointer (see #41) | already `cell + offset` |

Hit-testing converts the pointer to the layer's surface rect using the **active renderer's** metric,
then asks the look for `frame_layout`. A cell-mode session uses the cell look (`metric = 1 × 1`), so
the same pointer rounds to the same cell as today. A pixel session uses the pixel look and hits
pixel-sized gadgets exactly. No view duplicates gadget geometry.

**Relation to #48.** #48 fixes the look trait, the families and `frame_layout` as the single
geometry source; this ADR only fixes the coordinate space of that function: it takes the layer's
surface rect plus `Metrics` and returns surface rects. The #48 sketch can be read as the cell case
(`metric = 1 × 1`). `foursix-look` owns the types; turbo-tui consumes them.

## Question 7 — Effect on the existing types

### `View` / `ViewBase`

- `bounds()` stays a whole-cell `Rect` (`CellRect` in new APIs). Existing views are untouched.
- `ViewBase` gains `pixel_offset: PixelOffset` (default `ZERO`) and accessors
  `pixel_offset()` / `set_pixel_offset()`.
- `View::draw(&self, canvas, clip)` is unchanged. Views draw in their local cell coordinates; the
  container supplies the layer transform.
- Optional builder: `with_pixel_offset(x, y)` for `Window` and other layer roots.

### `Container`

- Child rects remain cells; z-order remains Vec order.
- `draw_children` wraps each child that is a layer root with `begin_layer(child.id, origin)` /
  `end_layer()`. Nested containers do not create new origins.
- Mouse dispatch converts the pointer once, at the container entry, and hit-tests children in
  surface space. Overlap and z-order semantics do not change.

### `Window` / `Frame`

- Drag/resize keeps the cell bounds as the layout value and the offset as the sub-cell refinement.
  In cell mode a drag updates only the cell bounds and forces `offset = 0`. In pixel mode it
  accumulates a pixel delta, normalises the offset, and updates the cell bounds when it crosses a
  cell boundary.
- Minimum size stays in cells.
- Frame geometry (gadget rects, title area, resize grip) comes from
  `Look::frame_layout(surface_rect, spec, metrics)` — no cell assumptions in `Window` or `Frame`
  (this is #48 decision E, consumed here).
- A drag that ends in pixel mode keeps the exact offset. There is no implicit snap on release.

### `Desktop`

- `tile`, `cascade`, `effective_area` and the minimized shelf are cell policies; they set
  `offset = 0`.
- Click-to-focus and window-manager state use cell bounds plus offset; the z-order list is
  unchanged.
- Free positioning is a property of floating windows, not of the desktop layout.

### `Overlay`

- An overlay anchored to a window inherits that window's layer origin so menus stay attached at
  sub-cell positions. Overlay *content* (menu rows, tooltips) stays cell-based.

### `Canvas`

```rust
pub struct PixelOffset { pub x: u16, pub y: u16 }   // 0 .. metric
pub struct LayerOrigin { pub cell: Position, pub offset: PixelOffset }

pub trait Canvas {
    fn metric(&self) -> (u16, u16) { (1, 1) }
    fn supports_subcell(&self) -> bool { false }
    fn begin_layer(&mut self, _id: ViewId, _origin: LayerOrigin) {}
    fn end_layer(&mut self) {}
    // existing drawing methods unchanged
}
```

- `CellCanvas`: `supports_subcell() == false`; `begin_layer` is a no-op, so a cell renderer draws
  at `cell` and effectively snaps.
- `PixelCanvas`: `supports_subcell() == true`; converts `cell × metric + offset` and draws exactly.
- `RecordingCanvas` (#42): records the layer origin and per-layer display list; no rasterisation.
- Third-party canvases keep compiling: every new method has a default.

## Question 8 — Migration and API break

**Additive for existing code.**

- `Rect`, `Position`, `Size`, the `View` trait and `View::draw` do not change.
- Cell-only applications compile unchanged and render byte-identically (`offset = 0`).
- New fields default to zero; new `Canvas` methods have defaults; new `Window` methods are
  additive.

**What breaks semantically.**

- `Window`/WM state and the protocol gain a pixel offset. Code that persists window positions
  stores one more field (default `0`).
- Pixel mode interprets positions sub-cell; code that assumed rounding to whole cells must use the
  snap helper.
- The `GadgetState` split and look-owned `frame_layout` are #48/#51 work, not this ADR; #36 depends
  on them only for pixel hit-testing.

**four-code migration.**

1. Cell mode: no change; keep using `Rect`.
2. Persisted layouts: add `offset: { x: 0, y: 0 }` on load and write it on save.
3. Pixel mode (after #34): use `Window::with_pixel_offset` only where free positioning is wanted;
   panels and tiled areas keep cell policy.
4. Browser input: no change in four-code; the runtime converts pointers.

No other consumer is expected to break; `Rect`'s meaning is preserved.

## Implementation slices (follow-up issues to be filed)

1. **`PixelOffset` + layer origin plumbing (additive).** Add `PixelOffset`, `LayerOrigin`,
   `CellRect`/`PixelRect` aliases/newtypes in `foursix-look`; `ViewBase` field and accessors;
   `Canvas::begin_layer`/`end_layer` defaults; `Container::draw_children` emits origins. Tests:
   with `offset = 0` every existing snapshot is byte-identical.
2. **Window-manager and desktop policy.** `Window` drag/resize stores offset; `Desktop::tile`,
   `cascade` and shelf force zero; WM state serialization gains the offset. Tests: tile/cascade
   ignore offsets, free drag normalises across cell boundaries.
3. **PixelCanvas exact geometry and input.** Implement offsets and `supports_subcell` in
   `PixelCanvas`; `DECSET 1016` mapping; `Look::frame_layout` hit-testing in surface space. This
   is the pixel half of #34 and needs four-sixel-lab#58 / #51 vocabulary.
4. **Recording/browser geometry.** `RecordingCanvas` records layer origins; the browser scales the
   session surface and converts pointers back to `cell + offset`. This is the geometry part of
   #42/#43.
5. **four-code adoption.** Persist offsets, opt in per window type, verify text mode unchanged.

A four-sixel-lab spike measuring sub-cell drag is useful for slice 3 (encode cost, mouse sync), but
the decision above does not depend on a number. It should be filed as a lab spike, not block this
ADR.

## Consequences

- Positive: the cell API and all text-mode behaviour stay exact; pixel mode gets true free
  positioning; the Dual UI protocol has one canonical geometry; the look crate remains the only
  source of frame/gadget geometry.
- Negative: two spaces must be documented and respected; a sub-cell sliver is visibly lost in cell
  mode; every frontend needs a pointer conversion.
- Risks: a widget mixing `CellRect` and `PixelRect` (mitigated by newtypes and cell-mode
  snapshots); offset drift in persisted state (mitigated by the normalised representation);
  hit-testing divergence (mitigated by one `frame_layout` path and cross-renderer tests).

## Cross-references

- [ADR-dual-ui.md](ADR-dual-ui.md) — protocol layers, browser scaling, multi-client.
- [ADR-skin-look.md](ADR-skin-look.md) — shared `foursix-look` geometry, `Metrics` in pixels,
  look-owned `frame_layout`, cell mapping to `1 × 1`.
- Issues: #34 (PixelCanvas), #35 (semantic primitives), #41 (Dual UI), #48 (look ADR).
- four-sixel-lab: #23 (proportional fonts, M4), #58 (create `foursix-look`).
- Local files: `src/canvas.rs`, `src/core/geometry.rs`, `src/view.rs`, `src/window.rs`,
  `src/desktop.rs`, `src/container/draw.rs`, `VISION.md`, `ROADMAP.md`.
