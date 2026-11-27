# ADR: Dual UI — Protocol, Diffs, Multi-Client and Security

- **Status:** Proposed (decision complete, architect review pending)
- **Date:** 2026-11-27
- **Issue:** #41
- **Companion:** [ADR-pixel-geometry.md](ADR-pixel-geometry.md) — geometry units and free pixel positioning
- **Related:** #34, #35, #36, #39, #42, #43, #44, #45, #48 · four-sixel-lab#58

## Context

**Dual UI** (epic #39, concept G01) means one session, one view tree and one window manager,
rendered natively by two frontends at the same time: the terminal (text or pixel mode) and a
browser. Input from either side changes the same state; moving a window in the browser moves it in
the TUI. The frontends do **not** receive a terminal stream: views describe *what* to draw, each
renderer decides *how* (`VISION.md` §3, "native rendering per frontend").

The building blocks are decided:

- v0.4: own cell backend and the `Canvas` trait; views draw through it.
- #35: semantic primitives (`FrameSpec`, `Bevel`, `Gadget`, `ScrollSpec`) with a cell
  implementation and default methods.
- #48: skins are data; one `foursix-look` vocabulary and look family drives cell, pixel and web;
  frame geometry is look-owned.
- #36 (companion ADR): geometry is cell layout plus a per-layer pixel offset; surface space is
  cells or reference pixels depending on the renderer.
- ROADMAP: optional Tokio host — many producers, one sequential consumer that owns the view tree.

What remains is the wire: what is sent, at which granularity, how a browser at a different
resolution maps it, what happens with several clients, and how the two security stages differ.
This ADR decides those points and defines the protocol sketch implemented by #42 and #43.

## Decision summary

1. **Content = a display list per layer plus window-manager state, not the view tree.** The server
   records semantic drawing ops through `RecordingCanvas` (#42) and sends the WM state separately.
   The browser never needs widget internals to render.
2. **Frames and gadgets are server-laid-out semantic ops.** `Frame` ops carry the `FrameSpec` and
   the `FrameLayout` (from `Look::frame_layout`, #48) in base pixels. The client paints them; it
   does not re-derive gadget geometry.
3. **Diffs are per layer, whole-layer replacement.** Stable `ViewId`, a per-layer `rev`, one
   monotonic `seq` per connection, full snapshot on connect/reconnect. No per-primitive diff in v1.
4. **Latest-frame-wins backpressure.** The UI task publishes the newest frame through a `watch`;
   a slow client's encoder skips intermediate versions and resyncs with a snapshot if it falls too
   far behind. No unbounded queue.
5. **Geometry on the wire = base pixels for content, cells for the WM.** Every layer has an origin
   `cell × base_metric + offset` (see #36); `base_metric` is announced once in `welcome`. Each
   client scales base pixels to its own cell size.
6. **Text v1 is a scaled cell grid with a real web font.** Text ops carry strings and cell origins;
   the browser draws them at its own font size. No free text layout until M4.
7. **One shared desktop, sized by the terminal session.** The browser scales/letterboxes to fit;
   it does not fork the layout. Tile/cascade/shelf stay server-side cell policies.
8. **Roles: terminal is the owner; web clients are viewers by default, one controller lease at a
   time.** Viewers cannot send input (enforced on the server). A pointer gesture holds an exclusive
   interaction lock; keyboard input is ordered by arrival.
9. **Runtime: one Tokio UI task owns the view tree.** All producers feed an `mpsc`; the UI task
   dispatches sequentially and publishes frames. Encoders and WebSocket writers run separately.
10. **JSON v1 behind feature `web`.** Debuggable, browser-native, versioned (`protocol` +
    `schema_version`). Binary (CBOR/MessagePack) remains a later negotiated option. The default
    build gains no dependencies.
11. **Browser client = TypeScript built with Deno, plain DOM + Canvas 2D, bundled into the binary.**
    No Node toolchain, no CDN, no external files at runtime.
12. **Two security stages.** Local stage (#43): localhost, token, read-only default. Open-port stage
    (#45): explicit opt-in, TLS, per-user roles, origin check, CSP/CSRF, rate and size limits,
    timeouts, audit log.

## Question 1 — Protocol content

**Options.**

- **A. Serialized view/window tree.** Send the actual widget tree and let the client instantiate
  the same widgets.
- **B. Display list per layer + window-manager state (chosen).** Send recorded semantic drawing
  ops per layer and a small WM state.
- **C. Terminal raster / cell stream.** Send the rendered cells or Sixel bytes.
- **D. Abstract per-widget UI tree** (a second widget model for the web).

**Trade-offs.**

- **A** forces every widget type and its private state into a serialization contract; the browser
  would need a parallel implementation of every widget and every layout rule. It breaks the
  "one owner" principle and makes every refactor a protocol break.
- **B** keeps the view tree private. The display list is exactly what `Canvas` already receives, so
  `RecordingCanvas` is a natural second implementation. Semantic ops let the browser render real
  bevels/gadgets and let charts carry data (#44). Frames stay server-owned, so hit-testing and
  visuals cannot diverge.
- **C** is terminal streaming, an explicit non-goal (`VISION.md` §6): huge bandwidth, no native
  text/selection, no resolution independence.
- **D** duplicates the widget model and drifts from Turbo Vision semantics.

**Decision.** **B.** The protocol carries a display list per layer plus WM state. The display list
uses the #35/#48 vocabulary; a widget that has data (chart, gauge) sends the data, not rasterised
pixels (#44). The WM state carries ids, bounds, z-order, focus, minimized/maximized, modality and
the controller lease.

Why frames are not re-derived by the client:

- #48 decision E makes `frame_layout` the single source for drawing *and* hit-testing. The server
  hit-tests clicks. If the browser computed a different gadget rect, a click could visually hit the
  close gadget while the server closes a different control. Sending the layout (or, equivalently,
  the frame plus its layout rects) removes that class of bug.
- A web look implementation may still paint differently (rounded bevels, different glyphs); only
  the *geometry* is shared.

## Question 2 — Diff granularity and ordering

**Options.** (a) Per primitive with stable op ids. (b) Per layer, whole-layer replacement.
(c) Full frame every time. (d) Per window subtree inside a layer.

**Trade-offs.** (a) saves the most bytes for small edits but needs stable ids for every op, a
mutation model and careful ordering; graphical ops are cheap to resend. (b) is simple, idempotent
and matches how windows redraw (a drag redraws its layer). Typing changes only the focused text
layer. (c) wastes bandwidth for a keystroke. (d) saves less than (a) with more structural
complexity than (b).

**Decision.** (b) for v1. A `layer` message replaces the complete ops list of one layer. The WM
state is a separate, small full-state message (it is cheap and naturally idempotent). Measurement
in #43 decides whether a later per-primitive or binary encoding is worth it.

Rules:

- Each layer has a stable `ViewId` and a per-layer `rev` incremented whenever its content changes.
- Each connection has a monotonic `seq`. A client ignores a message with `seq ≤ last_seq`; it
  ignores a layer with `rev ≤ stored_rev`.
- `snapshot` on connect, on `resync_request`, and when the server detects a gap/overflow. A
  snapshot is self-contained: skin, WM state and all layers.
- `layer_remove` deletes a layer; `wm` is authoritative for which layers exist and their z-order.
- Ordering within a frame: `wm`/`layer_remove` before `layer`; a client that receives a layer for an
  unknown window may hold it until the next `wm` or drop it and rely on the next snapshot. Keep the
  rule simple: apply `wm` first, then layers.
- Idempotence: applying the same `layer` or `wm` twice is harmless.

**Backpressure.** The UI task never waits for a socket. It publishes an immutable frame (skin
version, WM state, layer map) through a `tokio::sync::watch`. Each client encoder keeps the last
state it sent, diffs against the newest watch value on its own schedule, and writes. If the client
cannot keep up, `watch` overwrites intermediate frames — latest-frame-wins. If a client's send
buffer is full or it has missed more than a threshold (for example 2 s or 120 frames), the server
drops its delta stream and queues a fresh `snapshot`. Input queues are bounded too; a client that
floods is disconnected (Stage O logs it).

## Question 3 — Geometry on the wire and multi-resolution clients

This ADR consumes the [geometry ADR](ADR-pixel-geometry.md); it does not redefine it.

- **Layout space** (WM state, bounds, tile/cascade/shelf) is **cells**.
- **Surface space** (display-list rects) is **base pixels**. `base_metric` (for example `8 × 16`) is
  announced in `welcome`; it is the cell size of the session's reference renderer, or a configured
  default for a cell-only session.
- A layer's origin is `cell × base_metric + offset`, with `offset` the normalised sub-cell offset.
- A client computes `scale = client_cell_size / base_metric` and renders the layer at
  `origin × client_cell_size + offset × client_cell_size / base_metric`. A client that cannot do
  sub-cell positioning applies the #36 snap rule to the origin.
- `RecordingCanvas` is a pixel-metric canvas: views still pass cell coordinates, and it records
  layer-local base pixels using `base_metric`. The same look function therefore returns a
  base-pixel `FrameLayout` for the protocol, whether the local terminal renders cells or pixels.
- Text ops carry cell origins and strings, so a client draws them with its own font metrics at its
  own grid; the text origin is snapped to the client's glyph grid to stay crisp.
- The browser uses a fit-to-viewport scale (letterboxed) by default, optionally a 1:1 zoom. The
  whole desktop is scaled uniformly; windows are never repositioned client-side.

**Why not per-client viewports.** A per-client desktop size would fork the layout: a window
maximized in the terminal would not be maximized in the browser, and a drag would target different
cells. The epic requires bidirectional sync of the *same* window state. Scaling the shared desktop
keeps one truth. If a future product needs independent viewports, it is a separate session, not a
second layout of this one.

**Why not pixels everywhere.** The WM and all view code are cell-based (#34 keeps layout on the
cell grid). Sending pixels everywhere would force the browser to reconstruct cell boundaries for
text and would not match tile/cascade semantics. Cells + a base metric is the same model as the
local renderers.

## Question 4 — Text in the browser

**Options.** (a) v1 on a scaled cell grid with a real web font (chosen). (b) Free text layout from
the start. (c) Render text as image runs.

**Trade-offs.** (a) is crisp on HiDPI, searchable/selectable later, cheap and consistent with the
terminal grid; it cannot do proportional typography. (b) is the M4 vision (four-sixel-lab#23) but
needs a layout engine, font metrics per client and a different protocol. (c) is a raster viewer.

**Decision.** (a). Text ops carry `text`, a layer-local cell origin and the base cell metric; the
client draws with a real monospace stack (or a bundled WOFF2 later) at its measured cell size and
`devicePixelRatio`. Selection/copy is a later hidden-DOM mirror, not part of the display list.
Proportional fonts and free text arrive with M4 and a protocol extension; the envelope is
versioned, so this is additive.

## Question 5 — Multiple clients

**Options.**

- **A. One shared desktop, all clients see the same size (chosen).** Terminal owns the size; the
  browser scales.
- **B. Per-client viewport** with its own size and scroll.
- **C. Per-client desktop**, independent window positions.

**Trade-offs.** A satisfies "move a window in the browser, it moves in the TUI". B breaks
maximize/tile and makes pointer mapping ambiguous. C is a multi-session product, not Dual UI.

**Decision.** A, with explicit roles and interaction rules:

| Aspect | Rule |
|---|---|
| Desktop size | Owned by the terminal session; fixed for the life of the session unless the terminal resizes. Browser scales to fit. |
| Late join | `welcome` + `snapshot` (skin, WM, all layers). No replay; no history. |
| Roles | `viewer` (default) and `controller`. The terminal is always a controller. At most one web controller lease at a time. |
| Role changes | `--web-control` enables the lease; `controller` message broadcasts the holder; a viewer's input is rejected server-side and audited. |
| Concurrent pointer | One exclusive `interaction_id` (gesture) at a time. A second controller's `pointer_down` is ignored until `pointer_up`, timeout or disconnect. |
| Keyboard | Serialized by the UI task in arrival order (last-wins at the state level); only controllers may send. |
| Terminal priority | Terminal input is always accepted. The web lease can be revoked by the terminal; the client shows the indicator. |
| Disconnect mid-gesture | Socket close or a 2 s stale-heartbeat timeout releases the lock and cancels the gesture. |
| Limits | Max clients, per-client outbound buffer, idle timeout, message-rate/size caps (Stage O enforces the security side). |

A visible controller indicator in both frontends is required: the user must know who can move the
windows. The server is the enforcement point; hiding the button in the client is not security.

## Question 6 — Runtime

**Options.** (a) Synchronous loop with a web thread. (b) Tokio host with one owner task and
producer channels (chosen). (c) Shared `Arc<Mutex<Application>>` from every task.

**Trade-offs.** (a) blocks on sockets or needs ad-hoc locking. (c) turns the sequential three-phase
dispatch into a lock contention problem and can reorder events. (b) matches the existing dispatch
model: many producers, one consumer.

**Decision.** (b), the roadmap Tokio host:

- `UiTask` owns `Application` and the view tree. It receives `Input` from an `mpsc`, runs the
  existing three-phase dispatch, and draws once per frame.
- Producers: crossterm `EventStream`, one WebSocket reader per client, timers, service channels.
  All of them send into the same input channel; no producer touches the view tree.
- A render tick uses `MissedTickBehavior::Skip` and a frame cap (start at 60 Hz; measure). Input is
  coalesced within a tick (mouse moves keep the latest position).
- Output: the UI task publishes an immutable frame through `watch`. A local terminal sink and the
  per-client encoders consume it. Encoders diff per layer and write to their sockets on their own
  tasks, so a slow client cannot stall the UI.
- `web` implies the Tokio/axum dependencies; the default build stays dependency-free. A sync
  terminal-only app keeps working without Tokio.

The three-phase dispatch stays sequential; no `Send` bound is forced onto `View`, and no lock is
added around the view tree.

## Question 7 — Serialization and versioning

**Options.** (a) JSON v1 only. (b) Binary (CBOR/MessagePack) from the start. (c) JSON now, binary
later behind a negotiated capability.

**Trade-offs.** (a) is debuggable, inspectable in the browser and matches the skin JSON from #48;
it is larger. (b) is compact but opaque, needs tooling and delays the first working Dual UI.
(c) gets v1 shipped and keeps the door open.

**Decision.** (c): JSON v1, binary later only by negotiation.

- `protocol: 1` is the message-shape version; `schema_version` covers payload/skin schema. Both are
  announced in `welcome`; incompatible major versions are refused with a clear error.
- Changes within a major version are additive: clients ignore unknown message/op types they did not
  advertise, and the server ignores unknown client capabilities.
- `serde` and `serde_json` live behind feature `web`; `cargo build` without features adds no
  dependency (AC of #42).
- The skin is the #48 JSON, sent once in `snapshot` (and on change); the browser uses it as data.
- #43 measures bytes per interaction; if JSON is the bottleneck, a binary encoding is a new
  negotiated capability, not a breaking change.

## Question 8 — Security model

Two stages, deliberately separated. Stage L is for a developer on the same machine; Stage O is
remote access to a live session.

| Control | Stage L — local (#43) | Stage O — open port (#45) |
|---|---|---|
| Bind | `127.0.0.1` only, hard-coded default | `--web-listen <addr>` **and** `--web-allow-remote`; loud warning; never from config alone |
| Transport | Loopback; no TLS required | TLS (rustls) with cert/key; refuse non-loopback without TLS unless `--web-insecure` (reverse-proxy mode, documented) |
| Authentication | Bearer token, random at startup or `--web-token`; constant-time compare; token printed once, never in a URL | Per-user tokens/passwords, hashed at rest if stored; optional OIDC/reverse-proxy header hook documented |
| Roles | Read-only viewer by default; `--web-control` enables the controller lease | `viewer` / `controller` per token; read-only remains the default; server rejects viewer input |
| Origin | WebSocket `Origin` must match the local origin; reject foreign origins | Origin allowlist; reject missing/foreign origins; CSRF protection for HTTP actions |
| Browser policy | Token in the `Authorization` header (not a query string); no third-party content | Strict CSP, no inline scripts, secure cookies (`HttpOnly`, `Secure`, `SameSite=Strict`) or bearer token |
| Abuse | Message size cap, basic rate limit, max clients | Auth and message rate limits, size caps, idle timeout, max clients, bounded queues with latest-frame-wins |
| Audit | Connect/disconnect and auth failure to stderr | Connect/disconnect, auth failures, role, client address; documented log format |
| Docs | SSH-tunnel recipe | `docs/WEB-SECURITY.md`: threat model, deployment options (direct TLS, reverse proxy, tunnel), hardening checklist |

Notes:

- **Localhost is not a trust boundary.** Other local users and browsers can reach a loopback port;
  the token and the Origin check are required even in Stage L. A token in the URL leaks through
  history and logs, so it goes in the upgrade header.
- **The server enforces roles.** A viewer's `input` message is dropped and audited; a client-side
  disabled button is convenience only.
- **Never unbounded.** Rate limits, size caps and latest-frame-wins are part of the security model
  (memory/CPU exhaustion), not only performance.
- Stage O is a separate issue (#45) with its own code review and threat-model review; this ADR only
  fixes the target.

## Question 9 — Browser client stack

**Options.** (a) TypeScript built with Deno, bundled (chosen). (b) Plain JavaScript. (c) A
framework (React/Svelte) plus a Node toolchain. (d) WebAssembly UI.

**Trade-offs.** (a) gives types for the protocol, matches the house rule (Deno, never Node), and
keeps the bundle small. (b) avoids a toolchain but loses protocol types. (c) adds a Node/npm
toolchain and a large bundle for a windowing canvas. (d) duplicates the Rust renderer and is a much
larger project.

**Decision.** (a):

- Source in `web/` (`client.ts`, renderer, protocol types); `deno task build` (esbuild run through
  Deno, for example `deno run npm:esbuild`, or `deno bundle` where available) produces one
  `client.js` plus a small CSS file, committed under `web/dist/`. No Node/npm toolchain is used.
- Rust embeds them with `include_bytes!`; the **Rust build never requires Deno**, and the binary
  serves no external files at runtime. Maintainers rebuild the client only when it changes;
  `deno fmt --check`, `deno lint`, `deno check` and protocol-fixture round-trips are part of the
  client gate.
- Rendering: plain DOM shell + Canvas 2D for the display list, `devicePixelRatio` aware. No
  framework, no CDN, no inline script (CSP-friendly). Text uses a real monospace font stack; a
  bundled WOFF2 can be added later without a protocol change.
- Protocol types are hand-mirrored in TypeScript from the Rust definitions and tested against
  committed JSON fixtures; codegen is optional later.
- A hidden DOM text mirror for selection/accessibility is a follow-up, not v1.

## Protocol sketch (v1)

Transport: WebSocket, JSON text frames, subprotocol `turbo-tui.v1`. Authentication happens during
the HTTP upgrade (`Authorization: Bearer <token>`), not in a message. The server sends `welcome` and
`snapshot` immediately after the upgrade.

### Envelope and handshake

```json
{ "protocol": 1, "seq": 42, "type": "layer", "...": "..." }
```

```json
// server -> client
{
  "type": "welcome",
  "protocol": 1,
  "schema_version": 1,
  "session": "8f3c...",
  "role": "viewer",              // "viewer" | "controller"
  "capabilities": ["json", "ops.v1"],
  "renderer": "cell",            // "cell" | "pixel"
  "base_metric": { "width": 8, "height": 16 },
  "desktop": { "cols": 120, "rows": 40 },
  "controller": null              // client id or null
}

// server -> client: full state on connect / resync
{
  "type": "snapshot",
  "seq": 1,
  "skin": { "schema_version": 1, "name": "workbench", "look": "workbench", "...": "..." },
  "wm": { "windows": [ /* WindowState */ ], "focus": 17, "controller": null },
  "layers": [ { "id": 17, "rev": 4, "origin": { "cell": {"x": 10, "y": 4}, "offset": {"x": 3, "y": 0} }, "ops": [ /* Op */ ] } ]
}
```

### Server → client messages

| Type | Fields | Meaning |
|---|---|---|
| `wm` | `windows`, `focus`, `controller` | Full WM state; authoritative for z-order, bounds, focus, modality, role |
| `layer` | `id`, `rev`, `origin`, `ops` | Full replacement of one layer |
| `layer_remove` | `id` | Layer disappeared |
| `skin` | `skin` | Skin changed (rare); `snapshot` otherwise |
| `cursor` | `pos` (base px), `shape`, `visible` | Terminal/editor cursor |
| `controller` | `holder`, `since` | Controller lease changed |
| `resync` | `reason` | Server dropped deltas; a `snapshot` follows |
| `error` | `code`, `message`, `fatal` | Protocol/auth/role error |
| `pong` | `t` | Liveness |

`WindowState`:

```json
{
  "id": 17,
  "title": "Editor — main.rs",
  "bounds": { "x": 10, "y": 4, "width": 60, "height": 20 },  // cells
  "offset": { "x": 3, "y": 0 },                              // base pixels, 0..base_metric
  "z": 3,
  "state": "normal",          // "normal" | "minimized" | "maximized"
  "focused": true,
  "modal": false,
  "parent": null
}
```

`origin` in a `layer` is redundant with the window's `bounds + offset` but is sent per layer so a
layer can be applied without cross-referencing the WM. The server keeps them consistent; the WM
wins on conflict and triggers a resync.

### Ops (layer-local coordinates)

```json
Op =
  | { "op": "text",   "cell": {"x": 1, "y": 1}, "text": "Hello", "pen": "text", "attrs": [] }
  | { "op": "fill",   "rect": {"x": 0, "y": 0, "width": 80, "height": 16}, "pen": "fill" }
  | { "op": "frame",  "spec": { "title": "Editor", "kind": "window", "active": true, "dragging": false, "gadgets": { "...": "..." } }, "layout": { /* FrameLayout in base px */ } }
  | { "op": "bevel",  "rect": {"x": 2, "y": 2, "width": 40, "height": 12}, "bevel": "raised" }
  | { "op": "gadget", "rect": {"x": 70, "y": 0, "width": 16, "height": 16}, "gadget": "close", "state": "normal" }
  | { "op": "scrollbar", "rect": {"x": 78, "y": 16, "width": 16, "height": 120}, "orientation": "vertical", "spec": { "...": "..." } }
  | { "op": "chart",  "rect": {"x": 0, "y": 0, "width": 320, "height": 160}, "spec": { "...": "..." } }   // #44
  | { "op": "link",   "rect": {"x": 0, "y": 0, "width": 40, "height": 16}, "target": "https://…" }        // #54
```

Shape rects are in **base pixels**, layer-local. `text` uses a layer-local **cell** origin so the
client can snap it to its own glyph grid. `frame` carries the server-computed `FrameLayout`
(title bar, title text, client area, gadget rects, resize grip, scrollbar rects) in base pixels;
the client paints, it does not recompute. Unknown ops are ignored by a client that did not
advertise them; a required unknown op causes `error`.

### Client → server messages

| Type | Fields | Meaning |
|---|---|---|
| `client_info` | `name`, `viewport`, `dpr` | Diagnostics and limits; not layout authority |
| `input` | `seq`, `events` | Normalized input events (below) |
| `resize` | `viewport` | Client window changed; only affects local scaling |
| `resync_request` | `since_seq` | Client lost state |
| `ack` | `seq` | Optional flow-control hint |
| `ping` | `t` | Liveness |

Input events:

```json
{ "kind": "pointer_down", "interaction": "c-8f3e", "pointer": { "cell": {"x": 12, "y": 5}, "offset": {"x": 3, "y": 0} }, "button": "left", "modifiers": [] }
{ "kind": "pointer_move", "interaction": "c-8f3e", "pointer": { "cell": {"x": 13, "y": 5}, "offset": {"x": 7, "y": 2} }, "buttons": ["left"] }
{ "kind": "pointer_up",   "interaction": "c-8f3e", "pointer": { "cell": {"x": 13, "y": 5}, "offset": {"x": 7, "y": 2} }, "button": "left" }
{ "kind": "wheel", "pointer": { "...": "..." }, "delta": { "x": 0, "y": 3 } }
{ "kind": "key", "code": "Char('a')", "text": "a", "modifiers": ["ctrl"] }
{ "kind": "paste", "text": "…" }
```

Pointer positions are **session coordinates** (`cell + offset`), computed by the client from its
viewport transform. The server converts them to surface space for hit-testing (#36). `interaction`
identifies a gesture and holds the exclusive pointer lock. Events are ordered within a message;
`seq` is per client. The server maps `key`/`text` onto the existing crossterm/`Event` vocabulary.

## Consequences and slices

- **#42 (D2)** implements `RecordingCanvas`, the protocol types, the layer diff, latest-frame-wins
  and the input mapping, plus round-trip tests. It needs #35 (done) and this ADR.
- **#43 (D3)** implements the axum/WebSocket host, the Deno/TypeScript client and Stage L security;
  it produces the first end-to-end Dual UI demo and the byte/latency measurements.
- **#44 (D4)** adds chart/gauge primitives whose ops carry data, so the browser renders SVG/Canvas
  at its own resolution; text and pixel renderers keep their braille/vector implementations.
- **#45 (D5)** implements Stage O: TLS, roles, origin/CSRF/CSP, rate and size limits, timeouts,
  audit, `docs/WEB-SECURITY.md`.
- **#36** supplies the geometry; its slices (layer origin, snap, pointer conversion) land before or
  with #42/#43.

Risks and mitigations:

| Risk | Mitigation |
|---|---|
| Protocol drift between Rust and TypeScript | Versioned envelope, committed JSON fixtures, round-trip tests on both sides |
| Bandwidth too high for JSON | Per-layer diffs, latest-frame-wins, measure in #43; binary is a negotiated capability |
| Visual mismatch between terminal and browser | One semantic vocabulary (#35/#48), server-side frame layout, golden/snapshot tests per renderer |
| Slow client stalls the UI | `watch` latest-frame-wins, bounded input queues, encoder per client, resync threshold |
| Security regression in Stage O | Explicit flags, server-side role checks, tests on #45's AC, separate security review and threat model |
| Concurrent input corrupts state | Single UI task, exclusive interaction lock, roles, disconnect timeout |

## Non-goals

- Streaming the terminal into the browser (raster or cell stream).
- Per-client desktops or independent layouts.
- Free/proportional text layout in v1 (M4).
- A second widget model for the web.
- Remote access without TLS/authentication (Stage O).

## Cross-references

- [ADR-pixel-geometry.md](ADR-pixel-geometry.md) — cell layout, per-layer pixel offset, snap rule,
  pointer conversion.
- [ADR-skin-look.md](ADR-skin-look.md) — skin as data, look families, `frame_layout`, serialization.
- Epic #39 and slices #42, #43, #44, #45; primitives #35; pixel canvas #34; geometry #36.
- `VISION.md` §1–§7; `ROADMAP.md` → "Rendering Backend & Runtime".
- Local files: `src/canvas.rs`, `src/application.rs`, `src/container/dispatch.rs`, `Cargo.toml`,
  `docs/PLAN-backend-canvas.md`.
