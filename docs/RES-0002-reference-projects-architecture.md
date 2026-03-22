# RES-0002: Reference Projects & Architecture Patterns

**Date:** 2026-03-22
**Context:** v0.2.1 planning — window handling, scrollbars, composability

## References

### 1. ratatui-background-process-example
- **URL:** https://github.com/ratatui/ratatui/tree/main/examples
- **Key Pattern:** Channel-based event separation
  - `tokio::sync::mpsc` channels for background → UI communication
  - Separate event types: TerminalEvent vs AppEvent vs TickEvent
  - Non-blocking event loop: `events.next().await` instead of polling
  - Progress updates via shared channel, UI redraws reactively on any event
- **Relevance for turbo-tui:** Our deferred event queue already handles async-style updates. The channel pattern could be exposed as a public API for consumers who need background task progress (e.g., four-code file operations).

### 2. gping (orf/gping)
- **URL:** https://github.com/orf/gping
- **Key Pattern:** Real-time gauge/chart widgets
  - Uses ratatui's `Sparkline` and custom chart widgets for latency visualization
  - Data buffer with ring-buffer pattern for streaming data
  - Widget receives data slice, renders proportionally to available area
- **Relevance for turbo-tui:** Gauge/progress widgets would be a natural addition. The ring-buffer data model could inform a ScrollView widget.

### 3. Ratatui Ecosystem Patterns (2025-2026)
- **The Elm Architecture (TEA):** Message → Model → View separation
- **Component trait pattern:** `Component { handle_event(), draw(), update() }`
- **Widget composition:** Nested `Widget::render()` calls, not inheritance
- **State management:** State lives in Model, widgets are stateless renderers
- **Inactive states:** Conditional styling in draw() based on state flags

## Architecture Insights

### Event Processing (for Progress/Background tasks)
- Modern pattern: `tokio::mpsc` channel per event source
- turbo-tui equivalent: Deferred Event Queue + consumer-side channels
- Application could expose `post_event(Event)` for external event sources

### Scrollbar Integration Patterns
- **Option A:** Scrollbar as Frame configuration (current approach — scrollbar sits on border)
- **Option B:** Scrollbar as Container child (standard widget approach)
- **Option C:** JSON theme groups defining control appearance by context (window.scrollbar, dialog.scrollbar)
- **Recommendation:** Keep scrollbar on Frame (Option A) for border integration, add inactive/disabled styling via theme context awareness

### Composable Widget Stack
- Ratatui widgets are purely render-time: `Widget::render(area, buf)`
- turbo-tui's View trait adds state + events — more like a component
- **Key insight:** The View trait IS the component system. Make it more ergonomic:
  - Builder pattern for common configurations
  - `FrameConfig` struct instead of individual setters
  - Widget presets (e.g., `Window::with_scrollbars()`)

### Minimized Window Visibility
- Current bug: minimized windows collapse to height=1 but are positioned at drag_limits bottom — may be off-screen or invisible
- Fix: Desktop needs a "task bar" or minimized window shelf area
- Alternative: Minimized windows tile at bottom of desktop area, always visible

---

## Update 2026-03-22: Detailed Reference Analysis

### Ratatui Component Architecture Pattern
- **Source:** https://ratatui.rs/concepts/application-patterns/component-architecture/
- **Core trait:**
  ```rust
  pub trait Component {
      fn init(&mut self) -> Result<()>;
      fn handle_events(&mut self, event: Option<Event>) -> Action;
      fn handle_key_events(&mut self, key: KeyEvent) -> Action;
      fn handle_mouse_events(&mut self, mouse: MouseEvent) -> Action;
      fn update(&mut self, action: Action) -> Action;
      fn render(&mut self, f: &mut Frame, rect: Rect);
  }
  ```
- **Key difference from turbo-tui View trait:** Returns `Action` enum from event handlers instead of modifying event in-place. Actions bubble up for parent processing.
- **Separation:** `handle_events` → `update` → `render` is a 3-step cycle vs turbo-tui's `handle_event` + `draw` 2-step.

### Ratatui Builder Lite Pattern
- **Source:** https://ratatui.rs/concepts/builder-lite-pattern/
- **Pattern:** Consume `self`, return `Self` — fluent chaining without separate Builder struct
- **Example:** `Paragraph::new("text").block(Block::bordered()).centered()`
- **Key for turbo-tui:** This is the pattern we should adopt for FrameConfig and Window builder — NOT a separate `WindowBuilder` struct, but `Window::new().scrollbars(true).min_size(20, 8)`

### Ratatui Event Handling Patterns
- **Source:** https://ratatui.rs/concepts/event-handling/
- **Three approaches:**
  1. Centralized (single match) — simple, doesn't scale
  2. Centralized catch + message passing — match in main, delegate via messages
  3. Distributed event loops — each module owns its event loop
- **turbo-tui alignment:** We use approach 2 (centralized catch in Application, three-phase dispatch to views). This is correct for a windowing framework.

### Ratatui Widget Traits
- **Source:** https://ratatui.rs/concepts/widgets/
- **Key traits:**
  - `Widget` — `fn render(self, area: Rect, buf: &mut Buffer)` (consumes self)
  - `StatefulWidget` — adds `State` type, `fn render(self, area, buf, state: &mut State)`
  - `WidgetRef` — `fn render_ref(&self, area, buf)` (renders by reference, stores between frames)
- **Critical insight:** Ratatui widgets are STATELESS renderers. State lives in `App`/`Model`. turbo-tui's `View` trait combines state + rendering + events. This is intentional — turbo-tui is a component framework, not a widget library.
- **`Box<dyn WidgetRef>`** — collections of heterogeneous widgets. turbo-tui already does this with `Box<dyn View>`.

### TachyonFX Effects Architecture
- **Source:** https://github.com/junkdog/tachyonfx
- **Architecture:** Post-render buffer transformation (after widgets render, effects modify cells)
- **Effect composition:** `fx::parallel()`, `fx::sequence()`, spatial patterns (radial, diagonal, sweep)
- **Stateful effects:** `EffectTimer` tracks elapsed time, interpolation progress, completion
- **EffectManager:** Tracks effects by ID, allows replacement/cancellation
- **Key for turbo-tui:** Window transitions (minimize/restore animations) could use similar pattern — post-render cell transforms rather than animation baked into widget logic.

### TUI-RS Demo UI Pattern (from tui-rs, pre-ratatui fork)
- **Source:** https://github.com/fdehau/tui-rs/tree/master/examples/demo
- **UI Structure:** Tab-based dashboard with gauges, charts, sparklines, lists, tables
- **App State:** Single `App` struct owns all data (selected tab, gauge values, chart data)
- **Widget Composition:** Layout::default().constraints() → split into regions → render widget per region
- **Event Loop:** crossterm poll → state update → full redraw
- **Visual impact:** Dense information display achieved through careful constraint-based layout, not complex widget internals.

### Synthesis: What turbo-tui Should Adopt

| Pattern | Source | Apply to turbo-tui |
|---------|--------|-------------------|
| Builder Lite | Ratatui docs | Window/FrameConfig: consume self, return Self |
| Component trait | Ratatui template | Add `update() -> Action` step to View lifecycle |
| Action enum | Component arch | Replace deferred event queue with typed Action returns |
| Post-render effects | TachyonFX | Optional integration point for window animations |
| StatefulWidget | Ratatui | Consider splitting View into StatefulWidget + EventHandler |
| WidgetRef | Ratatui 0.26+ | Already done via `Box<dyn View>` |
| Centralized + dispatch | Event handling | Already implemented correctly |

### Architecture Decision: Keep View Trait Unified
After reviewing all patterns, the turbo-tui `View` trait (state + events + render in one trait) is the RIGHT choice for a windowing framework. Ratatui's stateless Widget pattern is designed for data-display apps, not interactive component hierarchies. The Borland TV pattern of owned state + three-phase dispatch IS the component architecture that Ratatui's docs describe, just with a different API shape.

**What to adopt:** Builder Lite pattern for construction ergonomics. Lifecycle hooks (on_focus/on_blur) for cleaner state management. NOT: splitting View into separate traits.
