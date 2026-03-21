# turbo-tui v0.2 — Detaillierter Architekturplan

> Status: **Genehmigt** (2026-03-21)
> Branch: `v0.2-rebuild`
> Basis: v0.1.0 committed auf `main` (172 Tests)
> Aktueller Stand: 222 Tests auf v0.2-rebuild (Steps 1-8, 10 fertig)

---

## Architektur-Entscheidungen (alle bestätigt)

| Entscheidung | Gewählt | Begründung |
|---|---|---|
| Container-Name | `Container` (nicht Group) | Modern, klar |
| Container-Struktur | Submodule: `container/mod.rs`, `dispatch.rs`, `draw.rs` | Wartbar bei ~1000 Zeilen |
| Event-Architektur | Three-Phase Dispatch + Deferred Event Queue | Deterministisch, child→parent ohne Bus |
| Component-Modell | View trait + Lifecycle hooks | Kein Reactive-Props (Rust ownership), kein Watcher |
| Dropdown/Overlay | Overlay-System in Application | Wiederverwendbar für Menus, Tooltips, Autocomplete |
| Event Coalescing | Application-Level drain+coalesce | Einfachster Ansatz, kein Event-Struct-Änderung |
| Smart Border | Frame besitzt `Option<ScrollBar>` direkt | Kein `Box<dyn View>`, kein vtable |
| Rect-Format | Ratatui `(x, y, width, height)` | Keine Konvertierung |
| Performance | dirty-flag, 16ms poll, draw-before-poll, single-threaded | Research-Ergebnis |

---

## Modul-Übersicht v0.2

```
src/
├── lib.rs                     # Crate root + prelude
│
│── Level 0: Foundation (UNVERÄNDERT aus v0.1)
├── command.rs                 # CommandId, CM_*, CommandSet
├── theme.rs                   # Theme struct, 4 Themes
│
│── Level 1: View (DONE — v0.2 updated)
├── view.rs                    # View trait, ViewBase (dirty-flag), ViewId, Event/EventKind
│                              # NEU: Lifecycle hooks (on_insert, on_remove, on_resize)
│                              # NEU: EventQueue Typ für deferred events
│
│── Level 2: Container (group.rs → container/)
├── container/
│   ├── mod.rs                 # Container struct + public API
│   ├── dispatch.rs            # Three-phase dispatch + mouse capture
│   └── draw.rs                # draw_children + intersection clipping
│
│── Level 3: Window System
├── frame.rs                   # Smart Border (Borders, Title, Close, Resize, ScrollBars)
├── window.rs                  # Frame + Interior(Container), Drag/Resize State Machine
├── desktop.rs                 # Window Manager, Background, Click-to-Front, Focus
│
│── Level 4: Application + Overlay + Dialog
├── application.rs             # Event Loop Owner, Event Coalescing, Overlay-System
├── overlay.rs                 # Overlay-Layer (über Desktop, für Dropdowns/Tooltips)
├── dialog.rs                  # Modal Window, execute/end_modal
│
│── Level 5: Widgets
├── scrollbar.rs               # ANPASSEN: bounds-aware, clip-aware
├── button.rs                  # ANPASSEN: bounds-aware, clip-aware
├── static_text.rs             # ANPASSEN: bounds-aware, clip-aware
├── menu_bar.rs                # UMBAUEN: Dropdown → Overlay statt self-draw
├── menu_box.rs                # ERWEITERN: Overflow-Erkennung (oben/unten klappen)
├── status_line.rs             # Minimal anpassen
│
│── Level 5b: Compositions
├── msgbox.rs                  # message_box(), confirm_box() mit relativen Coords
│
│── Level 6: Demo
└── examples/
    └── demo.rs                # Nutzt Application struct
```

---

## Neue Konzepte (Detail)

### 1. Deferred Event Queue

**Problem:** Ein Child-Widget (z.B. Button) will dem Parent etwas mitteilen, kennt aber den Parent nicht. Aktuell muss es `event.kind = EventKind::Command(cmd)` setzen — aber das funktioniert nur wenn der Event zurück nach oben propagiert.

**Lösung:** Views können Events in eine Queue posten. Application verarbeitet die Queue nach jedem Dispatch-Zyklus.

```rust
// Event struct Erweiterung
pub struct Event {
    pub kind: EventKind,
    pub handled: bool,
    /// Deferred events to be dispatched after the current cycle.
    pub deferred: Vec<Event>,
}

// Button Beispiel:
fn handle_event(&mut self, event: &mut Event) {
    // ... button click detected ...
    event.deferred.push(Event::command(CM_BUTTON_PRESSED));
    event.clear();
}
```

**Application-Loop:**
```rust
loop {
    self.draw();
    let events = self.poll_and_coalesce();
    for mut event in events {
        self.dispatch(&mut event);
        // Verarbeite deferred events
        let mut max_iterations = 100; // Endlosschleifen-Schutz
        while !event.deferred.is_empty() && max_iterations > 0 {
            let deferred: Vec<Event> = event.deferred.drain(..).collect();
            for mut def in deferred {
                self.dispatch_single(&mut def);
                event.deferred.extend(def.deferred);
            }
            max_iterations -= 1;
        }
    }
}
```

**Vorteile:**
- Kein Event Bus nötig (keine Subscriptions, keine Rc<RefCell>)
- Deterministisch (deferred events werden in Reihenfolge verarbeitet)
- Rückwärtskompatibel (bestehender Code ignoriert `deferred` einfach)
- Zero-cost wenn nicht genutzt (Vec ist leer)

### 2. Lifecycle Hooks

**Neue Methoden im View trait (alle mit Default-Impl):**

```rust
pub trait View {
    // ... bestehende Methoden ...

    /// Called when the view is added to a Container.
    /// Receives the container's bounds for relative positioning.
    fn on_insert(&mut self, _parent_bounds: Rect) {}

    /// Called when the view is removed from a Container.
    fn on_remove(&mut self) {}

    /// Called when the terminal is resized.
    /// `new_size` is the new terminal size (width, height).
    fn on_resize(&mut self, _new_size: (u16, u16)) {}
}
```

**Wann aufgerufen:**
- `on_insert()`: In `Container::add()`, nach der Koordinaten-Konvertierung
- `on_remove()`: In `Container::remove()`, bevor das Child entfernt wird
- `on_resize()`: Durch Application bei `EventKind::Resize`, broadcast an alle Views

**Kein `on_update()`/`on_mounted()`** — Terminal-UI hat keinen reaktiven Lifecycle.

### 3. Overlay-System

**Problem:** MenuBar-Dropdowns, Tooltips, Autocomplete müssen über allen Windows gezeichnet werden und bekommen Mouse-Events zuerst. Aktuell zeichnet MenuBar den Dropdown selbst (hardcoded Rendering), was zu Clipping-Problemen und Z-Order-Bugs führt.

**Lösung:** Application verwaltet einen Overlay-Layer.

```rust
// overlay.rs
pub struct Overlay {
    /// The view to render (e.g., MenuBox).
    pub view: Box<dyn View>,
    /// Who opened this overlay (for closing logic).
    pub owner_id: ViewId,
    /// Close on click outside?
    pub dismiss_on_outside_click: bool,
    /// Close on Escape?
    pub dismiss_on_escape: bool,
}

pub struct OverlayManager {
    /// Stack of active overlays (last = topmost).
    overlays: Vec<Overlay>,
    screen_size: (u16, u16),
}
```

**Event-Routing mit Overlays (in Application):**
```
1. Overlay-Layer (topmost overlay gets event first)
   ↓ (if not consumed)
2. MenuBar (F10, Alt+letter)
   ↓ (if not consumed)
3. StatusLine (PreProcess — F-keys)
   ↓ (if not consumed)
4. Desktop → focused Window → three-phase dispatch
   ↓ (if not consumed)
5. Application handles unhandled commands
6. Process deferred event queue
```

**MenuBar-Änderung:**
- MenuBar zeichnet nur noch die Bar-Zeile (1 Row)
- Beim Öffnen eines Menüs: MenuBar postet deferred Event mit Overlay-Daten
- Application fängt den Command ab, erstellt Overlay mit MenuBox
- MenuBox berechnet Position mit Overflow-Erkennung

**Overflow-Erkennung:**
```rust
pub fn calculate_overlay_bounds(
    anchor: (u16, u16),       // Anker-Punkt (z.B. MenuBar Position)
    size: (u16, u16),         // Gewünschte Größe (width, height)
    screen: Rect,             // Bildschirmgröße
    preferred: DropDirection, // Down oder Up
) -> (Rect, DropDirection) {
    // Try preferred direction first
    // If no space → flip direction
    // If right overflow → shift left
}

pub enum DropDirection {
    Down,
    Up,
}
```

### 4. Event Coalescing

**Implementierung in Application::poll_and_coalesce():**

```rust
fn poll_and_coalesce(&self) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();
    let mut last_mouse_move: Option<Event> = None;
    let mut last_resize: Option<Event> = None;

    // Blocking poll with 16ms timeout
    if !event::poll(Duration::from_millis(16)).unwrap_or(false) {
        return events;
    }

    // Drain all pending events (non-blocking)
    loop {
        if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
            break;
        }
        match event::read() {
            Ok(Event::Mouse(m)) => match m.kind {
                MouseEventKind::Moved => {
                    last_mouse_move = Some(Event::mouse(m)); // Keep only latest
                }
                _ => {
                    // Flush pending move before non-move event
                    if let Some(mv) = last_mouse_move.take() {
                        events.push(mv);
                    }
                    events.push(Event::mouse(m));
                }
            },
            Ok(Event::Resize(w, h)) => {
                last_resize = Some(Event::resize(w, h)); // Keep only latest
            }
            Ok(Event::Key(k)) => events.push(Event::key(k)),
            _ => {}
        }
    }

    // Append coalesced events at the end
    if let Some(mv) = last_mouse_move { events.push(mv); }
    if let Some(rs) = last_resize { events.push(rs); }
    events
}
```

**Regeln:**
- Mouse Move: Nur das letzte (alle vorherigen verworfen)
- Resize: Nur das letzte (letztes Terminal-Size zählt)
- Mouse Down/Up/Drag: SOFORT (kein Coalescing)
- Key: SOFORT (kein Coalescing)
- Pending Move VOR einem Down/Up wird geflusht (Positionsupdate)

---

## Build-Reihenfolge (10 Schritte)

### Schritt 1: Container-Submodule
**Aufwand:** ~30 min (mechanisch)
**Agent:** developer-mini (free)

Umbenennung `Group` → `Container`, Aufteilung in Submodule:

1. Erstelle `src/container/` Verzeichnis
2. `src/container/mod.rs` — Struct `Container`, public API, imports, tests
3. `src/container/dispatch.rs` — `impl Container { fn dispatch_event() }`
4. `src/container/draw.rs` — `impl Container { fn draw_children() }`
5. Lösche `src/group.rs`
6. Update `src/lib.rs`: `pub mod group;` → `pub mod container;`
7. Update prelude: `Group` → `Container`

**Tests:** Alle 21 Container-Tests + alle anderen 91 = 112 grün.
**Verifizierung:** `cargo test` + `cargo clippy -- -D warnings`

---

### Schritt 2: View trait Erweiterungen
**Aufwand:** ~45 min
**Agent:** developer-mid

Änderungen an `src/view.rs`:

1. **Deferred Event Queue:** `Event.deferred: Vec<Event>` Feld
   - `Event::new()` initialisiert mit leerem Vec
   - Neue Methode: `event.post(deferred: Event)`
2. **Lifecycle Hooks:** Drei Default-Methoden im View trait
   - `on_insert(&mut self, _parent_bounds: Rect) {}`
   - `on_remove(&mut self) {}`
   - `on_resize(&mut self, _new_size: (u16, u16)) {}`
3. **Container aufrufe:** `child.on_insert()` in add(), `child.on_remove()` in remove()

**Tests:** deferred Vec leer, post() fügt hinzu, on_insert/on_remove aufgerufen

---

### Schritt 3: Frame (Smart Border)
**Aufwand:** ~2h
**Agent:** developer-mid

Neue Datei `src/frame.rs`:

```rust
pub struct Frame {
    base: ViewBase,
    title: String,
    frame_type: FrameType,        // Window | Dialog | Single
    closeable: bool,
    resizable: bool,
    v_scrollbar: Option<ScrollBar>,
    h_scrollbar: Option<ScrollBar>,
}
```

**Frame::draw() — ein Pass:**
1. Border-Zeichen aus Theme (dark=thick, modern=rounded, borland=double)
2. Top border + Title (zentriert)
3. Close `[■]` oben-links (wenn closeable)
4. Side borders
5. Bottom border + Resize grip `⋱` (wenn resizable, Color::Cyan)
6. V-ScrollBar auf rechtem Border
7. H-ScrollBar auf unterem Border

**Frame::interior_area() → Rect** (innere Fläche abzüglich Borders + ScrollBars)

**Tests:** ~15 Tests (Border-Zeichen, Title, Close, Resize, interior_area, Click-Regionen)

---

### Schritt 4: Window
**Aufwand:** ~2h
**Agent:** developer (Sonnet) — komplexe State Machine

```rust
pub struct Window {
    base: ViewBase,
    frame: Frame,
    interior: Container,
    drag_offset: Option<(i16, i16)>,
    resize_start: Option<(u16, u16, u16, u16)>,
    min_size: (u16, u16),
    prev_bounds: Option<Rect>,   // Zoom toggle
    drag_limits: Option<Rect>,
}
```

**Drag/Resize State Machine:**
- MouseDown auf Title → SF_DRAGGING + drag_offset
- MouseDrag bei SF_DRAGGING → move_to(mouse - offset)
- MouseUp → SF_DRAGGING off
- MouseDown auf Resize-Grip → SF_RESIZING
- MouseDrag bei SF_RESIZING → resize_to(delta) mit min_size clamping
- MouseUp → SF_RESIZING off
- Zoom toggle speichert/restored prev_bounds

**Tests:** ~15 Tests

---

### Schritt 5: Desktop (Window Manager)
**Aufwand:** ~1.5h
**Agent:** developer-mid

```rust
pub struct Desktop {
    base: ViewBase,
    windows: Container,
    background_style: Style,  // Borland Blue
}
```

**API:** add_window, close_window, click_to_front, tile, cascade, next_window
**Tests:** ~12 Tests

---

### Schritt 6: Overlay-System
**Aufwand:** ~1.5h
**Agent:** developer-mid

```rust
pub struct OverlayManager {
    overlays: Vec<Overlay>,
    screen_size: (u16, u16),
}
```

**API:** push, pop, pop_by_owner, draw, handle_event
**Overflow:** `calculate_overlay_bounds()` utility
**Tests:** ~10 Tests

---

### Schritt 7: Application (Event Loop)
**Aufwand:** ~2h
**Agent:** developer (Sonnet) — zentrale Orchestrierung

```rust
pub struct Application {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    desktop: Desktop,
    menu_bar: MenuBar,
    status_line: StatusLine,
    overlay_manager: OverlayManager,
    running: bool,
}
```

**Loop:** draw → poll_and_coalesce → dispatch (Overlay → MenuBar → StatusLine → Desktop) → deferred queue
**Tests:** ~8 Tests (Coalescing-Logik, Event-Routing)

---

### Schritt 8: Dialog (Modal)
**Aufwand:** ~1h
**Agent:** developer-mid

Dialog = Window mit FrameType::Dialog + modal execute loop.
Commands < 1000 schließen Dialog. Escape → CM_CANCEL, Enter → CM_OK.
**Tests:** ~8 Tests

---

### Schritt 9: Widget-Anpassungen
**Aufwand:** ~2h
**Agent:** developer-mini (einfache) + developer-mid (MenuBar)

**9a:** scrollbar, button, static_text — bounds-aware, lifecycle noop
**9b:** MenuBar Umbau — nur Bar-Zeile zeichnen, Dropdown als Overlay
**9c:** MenuBox Overflow — calculate_bounds_with_overflow()
**9d:** StatusLine — minimal
**Tests:** Bestehende + neue für Overflow

---

### Schritt 10: MsgBox + Demo
**Aufwand:** ~1.5h
**Agent:** developer-mid

**msgbox.rs:** Dialog-Factories mit relativen Coords
**demo.rs:** Nutzt Application struct, kein manuelles Event-Routing
**Tests:** `cargo run --example demo` (manuell)

---

## Abhängigkeits-Graph

```
Schritt 1 (Container) ─────┐
                            ├→ 3 (Frame) → 4 (Window) → 5 (Desktop)
Schritt 2 (View trait) ─────┘                                  │
                                                               ↓
                            6 (Overlay) → 7 (Application) → 8 (Dialog)
                                                │
                                                ├→ 9 (Widgets, MenuBar Umbau)
                                                └→ 10 (MsgBox + Demo)
```

**Kritischer Pfad:** 1 → 3 → 4 → 5 → 7 → 9b → 10

**Parallelisierbar:**
- Schritt 1 + 2 gleichzeitig
- Schritt 6 nach Schritt 2
- Schritt 8 nach Schritt 4
- Schritt 9a jederzeit nach Schritt 2

---

## Geschätzte Aufwände

| Schritt | Beschreibung | Agent | Aufwand |
|---|---|---|---|
| 1 | Container-Submodule | developer-mini | ~30 min |
| 2 | View trait Erweiterungen | developer-mid | ~45 min |
| 3 | Frame (Smart Border) | developer-mid | ~2h |
| 4 | Window (Drag/Resize SM) | developer (Sonnet) | ~2h |
| 5 | Desktop (Window Manager) | developer-mid | ~1.5h |
| 6 | Overlay-System | developer-mid | ~1.5h |
| 7 | Application (Event Loop) | developer (Sonnet) | ~2h |
| 8 | Dialog (Modal) | developer-mid | ~1h |
| 9 | Widget-Anpassungen | mini + mid | ~2h |
| 10 | MsgBox + Demo | developer-mid | ~1.5h |
| **Gesamt** | | | **~15h** |

---

## Offene Themen (spätere Versionen)

- **Submenu-Kaskaden:** Verschachtelte Overlays → v0.3
- **Drag & Drop zwischen Windows** → v0.3+
- **Custom Widgets (Tabs, Trees, Lists)** → v0.3+
- **Select + Copy + Paste** → four-code Integration
- **Alt+P Search Dialog** → four-code Integration
- **Sidebar Special Panel** → four-code Integration
