# ADR: Skin and Look Architecture

- **Status:** Accepted (architect review 2026-11-27)
- **Date:** 2026-11-27
- **Issue:** #48
- **Related:** #34, #35, #36, #41 · four-sixel-lab#35, four-sixel-lab#37, four-sixel-lab#51

## Context

`VISION.md` describes one Rust stack from the pixel engine to the app, rendered natively in the
terminal (text or Sixel/Kitty pixel mode) and in the browser from the same window state. The
principle "Skins are data" (VISION.md §3) requires one skin to drive the cell renderer, the pixel
renderer and the future web frontend (Dual UI #39/#41).

1. **turbo-tui already owns the semantic primitive vocabulary.** #35 (PR #49) introduced the
   neutral drawing types in `src/canvas.rs`: `Bevel`, `Direction`, `Gadget`, `GadgetState`,
   `Orientation`, `ScrollHover`, `FrameKind`, `FrameGadgets`, `FrameSpec`, `ScrollSpec`
   (canvas.rs:19–155). They are deliberately free of theme/style/cell-encoding so they can move to
   a shared crate without an API break (canvas.rs:15–17). `cell_skin.rs` holds the theme lookups,
   glyphs and `ButtonTray` geometry (cell_skin.rs:20–46).
2. **FourSix has a skin model, but it is engine-coupled.** `foursix-skin/src/lib.rs` defines `Pens`,
   `Metrics`, `AnalyzerStyle`, `TableStyle`, `Skin` and the `WORKBENCH`/`DOS` constants, but imports
   `foursix_engine::Color` (lib.rs:12) and documents that rule as temporary, to be removed by this
   ADR (lib.rs:7–10). Its `Cargo.toml` depends on `foursix-engine`.
3. **Pixel mode and pixel geometry are pending.** #34 (pixel `Canvas`) and #36 (pixel geometry)
   mean gadget/title-bar geometry is no longer a 1×1 cell. four-sixel-lab#37 (crate split) and #51
   (tracker-blue) will consume this decision.

The two binding constraints from the #35 review (recorded on #48) are the gadget-vocabulary split
and look-owned geometry — decisions D and E below.

## Decision summary

1. Create one dependency-free shared crate, working name **`foursix-look`**, owning geometry, the
   semantic primitive vocabulary, the skin data model and pure layout functions (`frame_layout`).
2. **turbo-tui re-exports** the moved vocabulary; the `Canvas` trait stays in turbo-tui.
3. **`foursix-skin` keeps the built-in skin data**, depends on `foursix-look`, and drops its
   `foursix_engine::Color` dependency in favour of the look crate's own `Rgb8`.
4. Split the gadget vocabulary: 3-state `GadgetState { Normal, Hover, Pressed }`; window focus and
   drag move onto `FrameSpec { active, dragging }`.
5. Make geometry look-owned: `Look::frame_layout(rect, &FrameSpec) -> FrameLayout`, shared by both
   drawing and hit-testing.
6. Define look families: `turbo-vision`, `dos-flat`, `workbench`, `tracker-blue`, each with a cell,
   a pixel and (later) a web implementation.
7. Palette = semantic pens (Workbench `DrawInfo`-style names) in `Rgb8`; required core pens plus
   optional/derived pens with documented fallbacks.
8. Metrics = base pixel sizes + font id + scale; cell mode maps them to the 1×1 char grid.
9. Component styles = open, namespaced, forward-compatible registry; no fixed fields in `Skin`.
10. Layout variants belong to components; a skin may only *suggest* a variant id.
11. Serialization = JSON with `schema_version`, serde behind a feature; today's `themes/*.json` is a
    legacy palette migrated by a converter/adapter, not a big-bang replacement.
12. Validation = reusable contrast check (≥ 4.5:1) in the look crate plus snapshot tests per family.

## Question 1 — Palette

**Options.** (a) Fixed `Pens` struct with exactly the seven Workbench pens. (b) Semantic core pens
plus optional/derived pens with fallbacks. (c) Free-form colour map keyed by arbitrary strings.

**Trade-offs.** (a) cannot express DOS-style per-widget pens or the tracker's extra accents; every
new widget forces a skin-type change. (c) loses the compiler-checked set and lets typos silently
fall back. (b) keeps the vocabulary typed while allowing per-look and per-component overrides.

**Decision.** (b). The core pens are the Workbench `DrawInfo`-style names proven in `foursix-skin`
(lib.rs:14–31): `background`, `text`, `shine`, `shadow`, `fill`, `fill_text`, `highlight_text`. App
pens (e.g. `title`, `frame_active`, `scrollbar_thumb`, `analyzer_bar_low`) are optional and resolve
through a documented fallback chain ending in the core pens. Colours are `Rgb8`; ANSI/Indexed
mapping is the cell renderer's job, so no engine type leaks in.

**Example pen tables** (from `foursix-skin`, carried over as data):

| Pen | Workbench | DOS |
|---|---|---|
| background | #AAAAAA | #0000AA |
| text | #000000 | #55FFFF |
| shine | #FFFFFF | #FFFFFF |
| shadow | #000000 | #000000 |
| fill | #6688BB | #55FFFF |
| fill_text | #000000 | #0000AA |
| highlight_text | #FFFFFF | #FFFFFF |

## Question 2 — Look families

**Options.** (a) One monolith skin with fixed fields per widget. (b) A small set of look families,
parameterised by data, each with cell/pixel/web impls. (c) Per-widget renderer objects with no
shared "family" concept.

**Trade-offs.** (a) cannot express the shape differences between Workbench, DOS, tracker and Turbo
Vision gadgets/frames/scrollbars. (c) maximises flexibility but loses "one skin drives every
renderer". (b) is the balance: the family defines *how* frames, titles, gadgets, buttons, fields,
lists and scrollbars are drawn, parameterised by data (bevel width/style, title-bar style, gadget
shapes, frame chars), so a new skin rarely needs new code.

**Decision.** (b). Look families are at least `turbo-vision`, `dos-flat`, `workbench`,
`tracker-blue`; each has a cell, a pixel and (later) a web implementation. The `Canvas` trait stays
in turbo-tui (bound to `Style`/rendering); the family supplies layout + paint, the canvas supplies
the surface.

**Vocabulary split (binding, decision D).** `GadgetState` becomes exactly
`{ Normal, Hover, Pressed }`; focus and drag are window state, so `FrameSpec` carries `active` and
`dragging` (already present, canvas.rs:133–135). Each look decides how gadget state × frame state is
drawn. Today's 5-variant enum (`Inactive`/`Dragging`, canvas.rs:61–72) exists only to keep cell
output byte-identical; the split and the follow-up that changes turbo-tui are recorded here.

**Look-owned geometry (binding, decision E).** `Look::frame_layout(rect, &FrameSpec) ->
FrameLayout` returns gadget rects, title area and resize grip; drawing *and* hit-testing query the
same result. `cell_skin::ButtonTray` becomes the cell implementation of the `turbo-vision`/
`dos-flat` look, not a second geometry source. In a pixel look gadgets have different
sizes/positions, so cell geometry would desynchronise the click area from the pixels (#34).

## Question 3 — Metrics and typography

**Options.** (a) Cell-only sizes. (b) Base pixel metrics + font id/scale with a documented cell
mapping.

**Trade-offs.** (a) is what exists today and cannot describe pixel mode. (b) is forward-compatible
with #34/#36 and keeps the model renderer-neutral.

**Decision.** (b). `Metrics` holds base pixel sizes (`bevel`, `border`, `title_height`, `gadget`,
`side_border`, `bottom_border`, `padding`), a font id and a scale. Cell mode maps them onto the 1×1
char grid: every pixel metric collapses to 1 cell, the title bar is 1 row, and bevel/border become
the single-cell border line. The mapping is documented once so cell and pixel families agree on the
same `frame_layout`.

## Question 4 — Component styles

**Options.** (a) Fixed fields on `Skin` per widget (`analyzer`, `table`, …). (b) Open, namespaced
registry keyed by component namespace.

**Trade-offs.** (a) re-introduces "change the skin type per widget". (b) lets `analyzer`,
`pattern-table`, `scope`, `chart`, etc. declare their namespace + defaults and lets a skin override
by key.

**Decision.** (b). `ComponentStyles` is a namespaced map. Components declare their namespace and
defaults; a skin overrides by key. No fixed fields in the skin type; unknown keys are preserved for
forward compatibility.

## Question 5 — Layout variants

**Options.** (a) Skins own layout. (b) Components own layout; skins only suggest a variant id.

**Trade-offs.** (a) couples skin and layout and breaks when a layout changes independently
(four-sixel-lab#51 `tracker-blue`). (b) keeps layout a component concern while letting a skin nudge
the variant.

**Decision.** (b). Layouts belong to components. A skin may only *suggest* a variant id; switching a
skin may switch the suggested layout, but the component remains the layout authority.

## Question 6 — Shared vocabulary and crate placement

**Options.**
1. Duplicate the primitive types and write an adapter.
2. Keep the vocabulary in turbo-tui; FourSix depends on turbo-tui.
3. Put the model in `foursix-engine`.
4. One dependency-free shared crate.

**Trade-offs / why rejected.** (1) drift and double maintenance; two vocabularies for one semantic.
(2) couples a media engine to a terminal framework; heavy, terminal-coupled dependency. (3) the
engine is pixel-specific; the skin must be engine-free and usable by the cell renderer. (4) one
neutral home for geometry + vocabulary + skin model, usable by every consumer.

**Decision (A).** (4). One dependency-free shared crate, working name **`foursix-look`** (neutral
alternative names may include `look`, `tui-look`, `foursix-appearance`). It owns: geometry
(`Rect`/`Position`/`Size`, `u16` like turbo-tui today), the semantic primitive vocabulary
(`FrameSpec`, `FrameKind`, `FrameGadgets`, `Gadget`, `GadgetState`, `Bevel`, `Direction`,
   `Orientation`, `ScrollSpec`, `ScrollHover`), the skin data model (palette/pens, metrics, look
   families, component-style registry, layout-variant hint), and pure layout functions
   (`frame_layout`). No mandatory external dependencies; `serde` only behind a feature; no
   terminal/Sixel/engine types.

   **Placement and publishing.** The crate lives in the FourSix workspace
   (`four-sixel-lab/crates/foursix-look`) until the FourSix extraction
   (four-sixel-lab#49) creates a public repository. turbo-tui consumes it via a path dependency
   during development and via a pinned git rev/tag afterwards. Because turbo-tui is a published
   crate, the vocabulary definitions stay in turbo-tui behind `pub use` re-exports until
   `foursix-look` is published; only then does the definition move, keeping the existing
   `turbo_tui::canvas::*` paths valid.

**Decision (B).** turbo-tui re-exports the moved vocabulary (no duplicate types, no adapter). The
`Canvas` trait stays in turbo-tui (it uses `Style`/rendering). `turbo_tui::core` keeps/re-exports
geometry. This is a pre-1.0 API move.

**Decision (C).** `foursix-skin` keeps the built-in skin data and depends on `foursix-look`; the
temporary `foursix_engine::Color` dependency is removed (the look crate defines `Rgb8`; cell mapping
happens in turbo-tui).

**The 65535 ceiling.** Geometry stays `u16`: 8K (7680×4320) and even 16K (15360×8640) fit in both
axes, so the shared crate needs no `u32` coordinates and turbo-tui's `Rect::new` clamping
(core/geometry.rs:72–81) is unchanged.

## Question 7 — Serialization and migration

**Options.** (a) Replace `themes/*.json` with the new schema in one step. (b) Additive JSON with
`schema_version`; treat today's themes as a legacy palette and convert/adapt in phase 1.

**Trade-offs.** (a) is a big-bang break of the shipped `json-themes` feature. (b) preserves today's
files and only adds a `look: "turbo-vision"` default plus a converter/adapter.

**Decision (K).** (b). JSON carries `schema_version`; changes within a major version are additive.
serde stays behind a feature. Today's `themes/*.json` are a colour-only subset → treated as a legacy
palette with `look: "turbo-vision"`; a converter or adapter is delivered in phase 1, not a big-bang
replacement. Browser transfer (#41/#42) reuses the same JSON because the skin is pure data.

## Question 8 — Validation

**Options.** (a) Validate ad hoc per consumer. (b) Reusable `validate()` in the look crate + snapshot
tests per family.

**Trade-offs.** (a) duplicates the 4.5:1 logic already in `foursix-skin` tests (lib.rs:305–414).
(b) makes the contrast rule a single, testable entry point.

**Decision (L).** (b). Contrast ratio ≥ 4.5:1 for every text pen against every background it can
appear on; empty/dimmed markers are exempt (the `empty` pen rule, lib.rs:110–113). A reusable
`validate()` lives in the look crate, and snapshot tests cover each look family for cell and pixel.

## Type sketches (illustrative, API shape only)

```rust
// geometry — shared, u16 like turbo-tui today
pub struct Position { pub x: u16, pub y: u16 }
pub struct Size     { pub width: u16, pub height: u16 }
pub struct Rect     { pub x: u16, pub y: u16, pub width: u16, pub height: u16 }

// semantic primitive vocabulary (final form)
pub enum GadgetState { Normal, Hover, Pressed }            // interaction state only
pub enum Gadget { Close, Zoom, Depth, Size, Arrow(Direction) }
pub enum Bevel { Raised, Recessed }
pub enum Direction { Up, Down, Left, Right }
pub enum Orientation { Vertical, Horizontal }
pub enum ScrollHover { None, Track, Thumb, Arrow }
pub enum FrameKind { Window, Dialog, Single }

pub struct FrameGadgets { pub close: Option<GadgetState>, pub minimize: Option<GadgetState>,
    pub maximize: Option<GadgetState>, pub resize_grip: Option<GadgetState> }

pub struct FrameSpec<'a> { pub title: &'a str, pub kind: FrameKind,
    pub active: bool /* focus, replaces GadgetState::Inactive */,
    pub dragging: bool /* replaces GadgetState::Dragging */, pub gadgets: FrameGadgets }

pub struct ScrollSpec { pub position: i32 /* value-min */, pub total: i32 /* max-min */,
    pub visible: i32 /* page size */, pub active: bool, pub hover: ScrollHover,
    pub dragging: bool }

// layout result — shared by drawing and hit-testing
pub struct FrameLayout {
    pub outer: Rect,
    pub title_bar: Rect,                     // full title row incl. gadgets
    pub title_text: Rect,                    // centered title, clipped by gadgets
    pub client: Rect,                        // interior
    pub gadgets: Vec<(Gadget, Rect)>,        // positioned gadget rects
    pub resize_grip: Option<Rect>,
    pub scrollbars: [Option<Rect>; 2],       // [vertical, horizontal]
}

// skin data model
pub struct Rgb8 { pub r: u8, pub g: u8, pub b: u8 }

pub struct Pens {
    pub background: Rgb8, pub text: Rgb8, pub shine: Rgb8, pub shadow: Rgb8,
    pub fill: Rgb8, pub fill_text: Rgb8, pub highlight_text: Rgb8,
    // app pens (title, frame_active, scrollbar_*, ...) with documented fallbacks
}

pub struct Metrics { pub bevel: i32, pub border: i32, pub title_height: i32, pub gadget: i32,
    pub side_border: i32, pub bottom_border: i32, pub padding: i32,
    pub font_id: Option<String>, pub scale: f32 }

pub struct LookId(pub String);                 // "turbo-vision", "dos-flat", ...
pub struct ComponentStyles { /* namespaced map; unknown keys preserved */ }

pub struct Skin { pub name: String, pub schema_version: u32, pub pens: Pens,
    pub metrics: Metrics, pub look: LookId, pub layout_hint: Option<String>,
    pub components: ComponentStyles }

// Look trait split: data (Skin) vs. rendering (per-surface families)
pub trait Look { fn frame_layout(&self, rect: Rect, spec: &FrameSpec<'_>) -> FrameLayout; }
pub trait CellLook: Look { /* draw to a cell canvas using frame_layout */ }
pub trait PixelLook: Look { /* draw to a pixel canvas using frame_layout */ }
```

## Mapping tables

### `foursix-skin` → new model

| Current (`foursix-skin/src/lib.rs`) | New model |
|---|---|
| `Pens` | `Skin.pens` (palette) |
| `Metrics` | `Skin.metrics` |
| `AnalyzerStyle` | `components["analyzer"]` |
| `TableStyle` | `components["pattern-table"]` |
| `Skin.name` | `Skin.name` |
| `WORKBENCH` | built-in `Skin`, `look: "workbench"` |
| `DOS` | built-in `Skin`, `look: "dos-flat"` |
| `foursix_engine::Color` | `Rgb8` |
| `ALL` / `by_name` / `toggle` | registry helpers in `foursix-look` or `foursix-skin` |

### turbo-tui `Theme` → palette / look keys

Palette keys (colours, `Rgb8`): `desktop_bg`, `window_frame_active`, `window_frame_inactive`,
`window_title_active`, `window_title_inactive`, `window_interior`, `window_close_button`
(+ `_inactive`, `_hover`), `window_resize_handle` (+ `_inactive`, `_hover`), `window_minimize_button`
/ `window_maximize_button` (+ `_inactive`, `_hover`), `dialog_frame`, `dialog_title`,
`dialog_interior`, `single_frame`, `menu_bar_*`, `menu_box_*`, `status_*`, `button_*`,
`static_text`, `input_line_*`, `list_box_*`, `memo_*`, `scrollbar_*` (+ `_hover`, `_inactive`),
`title_bar_bg`.

Look/data keys (non-colour): border characters (`border_tl/tr/bl/br/h/v`, `menu_border_*`,
`menu_sep_l/r`), gadget texts (`close_button_text`, `minimize_button_text`, `maximize_button_text`,
`maximize_restore_text`, `resize_grip_char`), margins and sides (`button_margin_left/right`,
`close_button_side`, `controls_side`), `desktop_char`.

State notes: the three gadget pens per control (normal/hover/inactive) collapse onto
`GadgetState × FrameSpec.active` as the look resolves them; `window_frame_dragging` becomes a look
decision for `FrameSpec.dragging`, not a separate pen.

## Consequences & migration slices

Follow-up issues (created from this ADR):

1. **Create `foursix-look`** — geometry + vocabulary + skin model + `frame_layout`, serde feature:
   four-sixel-lab#58.
2. **turbo-tui adoption** — move/re-export types; 3-state `GadgetState`; `cell_skin` implements the
   `turbo-vision` look and `frame_layout` for both drawing and hit-testing: #51.
3. **`foursix-skin`** depends on `foursix-look`; remove `foursix_engine::Color`; built-in skins as
   data: part of four-sixel-lab#37 / follow-up slices.
4. **FourSix pixel look** — implement the Canvas primitives for pixel mode: #34.
5. **Theme JSON adapter/converter** + `schema_version`: #52.
6. **Web / display-list serialization**: #41.

Gating: #34 needs slice 2 (look-owned geometry + pixel family); four-sixel-lab#37 must not settle
the skin crate before slices 1 and 3 land.

## Cross-references

- turbo-tui: #34 (pixel mode), #35 (semantic primitives), #36 (pixel geometry), #39/#41 (Dual UI),
  #48 (this ADR).
- four-sixel-lab: #35 (skin model), #37 (crate split), #51 (tracker-blue layout variant).
- Local files: `src/canvas.rs`, `src/cell_skin.rs`, `src/theme.rs`, `src/theme_json.rs`,
  `src/core/geometry.rs`, `themes/*.json`, `VISION.md`.
