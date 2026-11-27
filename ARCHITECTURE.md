# turbo-tui — Architecture Overview

> Last updated: 2026-03-22

## System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│ Consumer Application (e.g., four-code)                       │
│                                                              │
│  terminal.draw(|buf| app.draw(&mut CellCanvas::new(buf)));   │
│  app.handle_crossterm_event(&event);                         │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ turbo-tui::Application                                       │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ MenuBar (row 0) — HorizontalBar with DropDirection::Down│   │
│  ├──────────────────────────────────────────────────────┤   │
│  │                                                      │   │
│  │  Desktop (rows 1..n-1) — Window Manager              │   │
│  │    ├── Container (Z-ordered children)                │   │
│  │    │   ├── Window 1 (back)                           │   │
│  │    │   │   ├── Frame (border + scrollbars)           │   │
│  │    │   │   └── Container (interior children)         │   │
│  │    │   │       ├── Button                            │   │
│  │    │   │       ├── StaticText                        │   │
│  │    │   │       └── ...                               │   │
│  │    │   ├── Window 2                                  │   │
│  │    │   └── Window N (front/focused)                  │   │
│  │    └── [Task Shelf: minimized windows] (future)      │   │
│  │                                                      │   │
│  ├──────────────────────────────────────────────────────┤   │
│  │ StatusLine (last row) — HorizontalBar, DropDir::Up   │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  OverlayManager (above everything — dropdowns, tooltips)     │
└──────────────────────────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ core/terminal/canvas (rendering)                             │
│  Canvas → Buffer → Terminal → crossterm                      │
└─────────────────────────────────────────────────────────────┘
```

## Module Dependency Graph

```
Level 0: command.rs, theme.rs          ← Foundation (no internal deps)
   ↓
Level 1: view.rs                       ← View trait, Event system
   ↓
Level 2: container/                    ← Container (uses View)
   ↓
Level 3: frame.rs, window.rs, desktop.rs  ← Window system (uses Container, View)
   ↓
Level 4: application.rs, overlay.rs, dialog.rs  ← Orchestration (uses all above)
   ↓
Level 5: scrollbar.rs, button.rs, static_text.rs,  ← Leaf widgets
         horizontal_bar.rs, menu_bar.rs, menu_box.rs,
         status_line.rs, msgbox.rs
```

## Event Dispatch Chain

```
crossterm::event::read()
        │
        ▼
Application::handle_crossterm_event()
        │
        ├─1→ OverlayManager (topmost overlay first)
        │         └─ if consumed → stop
        │
        ├─2→ MenuBar (F10, Alt+letter, arrow keys)
        │         └─ if consumed → stop
        │
        ├─3→ StatusLine (OF_PRE_PROCESS — F-keys)
        │         └─ if consumed → stop
        │
        ├─4→ Desktop → Container (three-phase dispatch)
        │         ├─ Phase 1: PreProcess (OF_PRE_PROCESS children)
        │         ├─ Phase 2: Focused child
        │         └─ Phase 3: PostProcess (OF_POST_PROCESS children)
        │
        ├─5→ Application handles unhandled commands
        │         └─ CM_QUIT, CM_CLOSE, CM_ZOOM, CM_MINIMIZE, etc.
        │
        └─6→ Process deferred event queue
                  └─ Events posted by children via event.post()
```

## Mouse Event Routing

```
MouseDown/MouseUp:
  Desktop → Container.child_at_point(col, row) → reverse Z-order hit-test
    → If SF_DRAGGING/SF_RESIZING on focused child: captured (no hit-test)
    → Otherwise: topmost hit child gets the event

MouseMoved:
  Window → Frame.update_hover() + Frame.update_scrollbar_hover()
       → Interior Container (for child hover tracking)

MouseDrag:
  If window is dragging → Window.continue_drag()
  If window is resizing → Window.continue_resize()
  Else → Frame.handle_scrollbar_click() or interior
```

## Key Data Structures

### ViewBase (embedded by all widgets)
```rust
struct ViewBase {
    id: ViewId,          // Unique, atomic counter
    bounds: Rect,        // Absolute screen coordinates
    state: u16,          // SF_VISIBLE | SF_FOCUSED | SF_DRAGGING | ...
    options: u16,        // OF_SELECTABLE | OF_PRE_PROCESS | ...
    dirty: bool,         // Needs redraw
    owner_type: OwnerType,  // Window | Dialog | None
    end_state: CommandId,   // For modal dialogs
}
```

### Event (passed through hierarchy)
```rust
struct Event {
    kind: EventKind,     // Key | Mouse | Command | Broadcast | Resize | None
    handled: bool,       // Set by consumer
    deferred: Vec<Event>, // Posted for later dispatch
}
```

### Container (manages children)
```rust
struct Container {
    base: ViewBase,
    children: Vec<Box<dyn View>>,  // Z-order: 0=back, last=front
    focused: Option<usize>,        // Index of focused child
}
```

### Window (Frame + Interior)
```rust
struct Window {
    base: ViewBase,
    frame: Frame,           // Border decoration + optional scrollbars
    interior: Container,    // Child widgets
    min_size: (u16, u16),
    drag_limits: Option<Rect>,
    pre_zoom_bounds: Option<Rect>,
    pre_minimize_bounds: Option<Rect>,
}
```

## Theme Architecture

```
Thread-local: CURRENT_THEME (RefCell<Theme>)
Thread-local: THEME_REGISTRY (RefCell<BTreeMap<String, Theme>>)
Thread-local: CURRENT_THEME_NAME (RefCell<String>)

Access:   theme::with_current(|t| { ... })      ← cheap borrow
Set:      theme::set(theme) / theme::set_by_name("Dark")
Register: theme::register("name", theme)
Load:     theme::load_themes_from_dir(path)      ← JSON files (feature-gated)
```

### Theme Style Rules
- **Every style MUST include both fg AND bg** — prevents background bleed-through
- Validated by `test_turbo_vision_has_bg_on_all_styles` test
- Active vs inactive styles for: frame, close button, resize handle, scrollbar, minimize/maximize buttons

## Rendering Pipeline

```
Application::draw(canvas)
  │
  ├── MenuBar.draw(canvas, clip)        ← Row 0
  │
  ├── Desktop.draw(canvas, clip)
  │     ├── draw_background()           ← Fill with theme desktop_bg
  │     └── Container.draw(canvas, clip) ← Z-order: back to front
  │           └── for each child:
  │                 Window.draw(canvas, clip)
  │                   ├── fill_interior()    ← Prevent bleed-through
  │                   ├── Frame.draw()       ← Borders + scrollbars
  │                   └── Container.draw()   ← Interior children
  │
  ├── StatusLine.draw(canvas, clip)     ← Last row
  │
  └── OverlayManager.draw(canvas)      ← On top of everything
```

All rendering goes through turbo-tui's own `core::Buffer` via the `Canvas` trait. `terminal::Terminal` keeps persistent front/back buffers and writes the cell diff to the terminal via crossterm — views never write to the terminal directly.

### Canvas Primitives

The `Canvas` trait exposes two layers of drawing:

- **Low-level primitives** — `draw_text`, `put_char`, `put_symbol`, `fill`, `set_style`, `set_cursor`.
- **Semantic primitives** — `draw_frame`, `draw_bevel`, `draw_gadget`, `draw_scrollbar`, driven by resolved specs (`FrameSpec`, `GadgetSpec`, `ScrollSpec`) plus `BorderChars`, `Bevel`, `Gadget`, `GadgetState`, `Direction` and `ScrollHover`.

Semantic primitives have cell-based default implementations that reproduce the historical character output byte-for-byte, so `CellCanvas` needs no overrides. Pixel/sub-cell canvases ignore the `*_style` and `chars` fallback fields and render the semantic information instead (`draw_bevel`, for example, is a no-op on cell canvases because they encode elevation in styles and text). Views keep all hit-testing, geometry and theme resolution; the specs only carry the resolved result.


## Future: TachyonFX Integration Point

```
Application::draw(canvas)
  ├── (existing render pipeline)
  └── optional: EffectManager.process_effects(elapsed, buf, area)
       └── Post-render buffer transforms (fade, slide, color shift)
```

Effects modify already-rendered cells. They don't re-trigger widget logic. This keeps the effect system decoupled from the View trait.
