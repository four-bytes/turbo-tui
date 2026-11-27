# Vision

> Status: draft, 2026-11-27. Statements about novelty are provisional until the prior-art research (#40) is done.

## 1. In one paragraph

One Rust stack, from the pixel engine up to the applications. An app built on it runs natively in the terminal, in text mode or in pixel mode via Sixel/Kitty, and can show the same session in a browser at the same time. Both sides share one window state, and input on either side reaches the same session. The result is a single small binary that you copy to a server via SSH: there it works as a TUI, and if you want, it also serves a web UI. Each frontend renders at its own resolution. A chart is braille in the terminal and a vector graphic in the browser, not a scaled-up picture of the terminal.

## 2. The stack

| Layer | Project | Role |
|---|---|---|
| Engine | **FourSix** (incubated in `four-sixel-lab`, to become a public repo: four-bytes/four-sixel-lab#49) | Pixel surface, dirty rects, Sixel/Kitty output, text fallback, skins, UI widgets, media components (FLI, MOD) |
| Framework | **turbo-tui** | Turbo Vision-style windowing, own cell backend, `Canvas` (cell / pixel / recording), Dual UI |
| Apps | **four-code** (IDE), later **four-office** | Consumers and showcases |

## 3. Principles

These follow from decisions that are already made:

- **Native rendering per frontend, no streaming.** Views describe *what* to draw (semantic primitives, data for charts). Each renderer decides *how*. Data stays separate from presentation.
- **One view tree, one owner.** Many producers (terminal input, web clients, timers, background tasks) feed one consumer that owns the view tree (Tokio host).
- **Automatic degradation.** The pixel backend falls back from Kitty to Sixel to text. In a remote session, audio is muted. A missing capability must never produce a broken screen.
- **Skins are data.** One skin drives every renderer: pixel bevels, cell box-drawing and the browser.
- **SSH first.** Only dirty regions are sent, and a slow consumer gets the latest frame instead of a growing queue.
- **Secure by default.** The web frontend listens on localhost only and needs a token. A browser client starts as a viewer; input and window control need an explicitly granted controller role. An open port requires explicit opt-in, TLS and authentication.
- **Lean by default.** There is no `unsafe`. Pixel, web and media support are opt-in cargo features; the default build gains no dependencies from them.

## 4. Target groups and use cases

- **TUI developers:** a windowing framework that offers a pixel mode and a web frontend on top of the same views.
- **DevOps and server tools:** a monitoring or admin binary for databases, Kubernetes or system load. You use it as a TUI over SSH, and the same session is available as a dashboard in the browser.
- **Vibe coding and AI-assisted development:** four-code runs with its agents on a remote dev box. You watch and steer the same session from the browser, also on a phone. There are separate viewer and controller roles, and live charts for tokens, cost and test status.
- **Office, long term:** a word processor with pixel/WYSIWYG rendering that you use in the terminal and in the browser (four-office).

## 5. Milestones

| Milestone | Outcome | Issues |
|---|---|---|
| **M1** | four-code in pixel mode with the Workbench look | #34, #35, #37 · four-bytes/four-code#10, four-bytes/four-code#11 · four-bytes/four-sixel-lab#37, four-bytes/four-sixel-lab#49 |
| **M2** | Dual UI locally: four-code in terminal and browser at the same time, windows synced in both directions | #39 (#41, #42, #43) |
| **M3** | Data widgets and the open-port web GUI; covers the DevOps and vibe-coding use cases | #44, #45 |
| **M4** | Proportional fonts and free pixel positioning; groundwork for four-office | #36 · four-bytes/four-sixel-lab#23 |

Licensing is settled before the next release: turbo-tui, so far released under MIT, moves to Apache-2.0 with v0.5.0 (#38; v0.4.0 and earlier stay MIT). FourSix gets its own public Apache-2.0 repo before turbo-tui depends on it (four-bytes/four-sixel-lab#49).

## 6. Non-goals for now

- Replacing full GUI toolkits or building native desktop or mobile apps
- Streaming a terminal into the browser as pixels or text (that already exists)
- XM/IT tracker support
- Publishing crates before licensing and the FourSix extraction are done

## 7. Open questions

- Geometry units and free pixel positioning: ADR #36
- Dual UI protocol, multi-client model, security: ADR #41
- What already exists, and what is actually new: prior-art research #40
