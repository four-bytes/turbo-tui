<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# Ratatui -> window Manager. Wie am besten Aufbauen?

- Containerizing, Z-Index, Evnts, Dropdown Mnüs mit overflow, Resizing, Mous events performant

Kurz: Bau dir ein eigenes kleines „Window-System“ über Ratatui, das Rechtecke + Z‑Layer + Events verwaltet, und lass Ratatui nur zeichnen.

## Grundarchitektur

- Ein globaler **UiState**: `Vec<Window>` plus Fokus/Z‑Order, globale Event‑Queue.
- Jedes `Window` mit: `id`, `z_index`, `rect: Rect`, `kind` (normal, modal, dropdown), `content: Box<dyn WidgetLike>`.
- Hauptloop: `crossterm::event::read()` → in eigene Event-Struktur mappen → an Window‑Manager geben → danach `terminal.draw(|f| ui(f, &state))`.


## Z‑Index und Container

- Z‑Order simpel halten: „last wins“ – `windows` nach `z_index` sortiert rendern, bei Klick Fenster nach oben schieben.
- Für Performance lieber `u16`‑Z‑Index + `sort_unstable_by_key` statt ständig `Vec` neu allokieren.
- Container: `Layout` nur für grobe Bereiche verwenden (Main‑Pane, Sidebar, Statusbar), in jedem Bereich dann dein Window‑Manager, der seine Fenster reinrendert.[^1_1][^1_2]


## Events und Mouse performant

- Eigenes `enum UiEvent { Key(KeyEvent), Mouse(MouseEvent), Resize(cols, rows), Tick, App(AppEvent) }`.
- Direkt beim Einlesen hit‑test machen:
    - Von oben nach unten über `windows` (ab höchstem `z_index`) iterieren.
    - Erstes Fenster, dessen `rect` den Mauspunkt enthält, bekommt das Event.
- Nur bei tatsächlichen Änderungen neu zeichnen (Key, Mouse‑Down/Up, Resize). Kein ständiger `Tick`, außer du brauchst Animationen.
- Mouse‑Drag: State im Window‑Manager halten (`dragging_window_id`, `drag_origin`, `mouse_origin`), nur bei `MouseDrag` neue `rect` berechnen.


## Dropdowns und Overflow

- Dropdown nicht als „Inline‑Widget“, sondern als eigenes `Window` mit `kind = Dropdown`.
- Berechnung:
    - Basis‑Rect = Button‑Rect, Dropdown‑Rect darunter.
    - Wenn `rect.bottom() > terminal.height`, nach oben klappen; wenn `rect.right() > width`, nach links schieben.
- Overflow/Scroll:
    - Interne `offset`‑Zeile im Dropdown halten, nur sichtbaren Ausschnitt rendern.
    - Mouse‑Wheel ↑/↓ passt `offset` an, Enter/Klick auf Zeile schließt Dropdown + triggert Callback.


## Resizing und Hit‑Areas

- Fenster‑Resizing nur an Kanten/Ecken erlauben:
    - Rechte Ecke unten: `Rect` von 1x1 oder 2x2 definieren, Hit‑Test auf diese Region.
    - Bei Mouse‑Down dort: `resize_mode` aktivieren, dann bei `Drag` neue Breite/Höhe clamped rechnen (min width/height).
- Optional Resize‑Kante oben/links, aber nicht zu klein machen (2–3 Zeichen breit), sonst schwer klickbar.


## Containerizing / Komponentenmodell

- Trait à la:

```rust
trait Component {
    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderCtx);
    fn handle_event(&mut self, ev: &UiEvent, ctx: &mut EventCtx);
}
```

- `Window` hält `Box<dyn Component>`, dein Window‑Manager kennt Z‑Order/Fokus, aber UI‑Logik bleibt in Components.
- So kannst du dieselben Components in normalen Fenstern und Dropdowns verwenden.


## Praktische Tipps / Crates

- Für Event‑Abstraktion und einfache Container kannst du dir Ideen von Framework‑Experimenten holen (z.B. Stack‑Container, Buttons, Mouse‑Handling, eigene Event‑Enums).[^1_3][^1_4]
- Achte darauf, dass dein Event‑Loop single‑threaded bleibt und teure Dinge (AI‑Requests, DB‑Queries) über Channels an einen Worker gehen; UI‑Thread nur Events routen + zeichnen.

Wenn du magst, skizziere ich dir im nächsten Schritt ein konkretes `WindowManager`‑Struct mit minimalem Code‑Gerüst (Events, Z‑Order, Hit‑Test).
<span style="display:none">[^1_10][^1_5][^1_6][^1_7][^1_8][^1_9]</span>

<div align="center">⁂</div>

[^1_1]: https://docs.rs/ratatui/latest/ratatui/prelude/struct.Layout.html

[^1_2]: https://ratatui.rs/concepts/layout/

[^1_3]: https://ratatui.rs/faq/

[^1_4]: https://github.com/joshka/ratatui-widgets/blob/main/README.md

[^1_5]: https://stackoverflow.com/questions/44231597/how-to-change-the-z-order-of-window-added-via-windowmanager

[^1_6]: https://github.com/radix-ui/primitives/issues/1566

[^1_7]: https://www.tiktok.com/@besperon/video/6895683166292544774

[^1_8]: https://github.com/radix-ui/primitives/discussions/796

[^1_9]: https://hexdocs.pm/ratatouille/under-the-hood.html

[^1_10]: https://www.youtube.com/watch?v=d-IHMhnCOYM


---

# Build a starter Rust app using ratatui like turbovision, TUI window manager with resizable draggable windows, Z-order, mouse events, dropdown menus with overflow, and event handling

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
anyhow = "1.0"
```

```rust
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    collections::VecDeque,
    io,
    time::{Duration, Instant},
};

// Window state
#[derive(Clone)]
struct Window {
    id: usize,
    z_index: usize,
    title: String,
    rect: ratatui::layout::Rect,
    content: String,
    dragging: Option<(u16, u16)>, // drag start position
    resizing: bool,
}

struct App {
    windows: Vec<Window>,
    next_id: usize,
    next_z: usize,
    terminal_size: ratatui::layout::Rect,
    focused: Option<usize>,
    dropdown_open: Option<usize>,
    dropdown_items: Vec<Vec<String>>,
    dropdown_pos: Option<(u16, u16)>,
    drag_window: Option<usize>,
    drag_offset: Option<(i16, i16)>,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            windows: vec![],
            next_id: 0,
            next_z: 0,
            terminal_size: ratatui::layout::Rect::new(0, 0, 100, 30),
            focused: None,
            dropdown_open: None,
            dropdown_items: vec![vec!["Option 1".to_string(), "Option 2".to_string(), "Option 3".to_string()]],
            drag_window: None,
            drag_offset: None,
            dropdown_pos: None,
        };
        app.add_window("Window 1", "Content of window 1...".to_string());
        app.add_window("Window 2", "Content of window 2...".to_string());
        app
    }

    fn add_window(&mut self, title: &str, content: String) {
        let rect = ratatui::layout::Rect::new(10, 5, 40, 15);
        self.windows.push(Window {
            id: self.next_id,
            z_index: self.next_z,
            title: title.to_string(),
            rect,
            content,
            dragging: None,
            resizing: false,
        });
        self.next_id += 1;
        self.next_z += 1;
        self.focused = Some(self.windows.len() - 1);
    }

    fn get_window_mut(&mut self, id: usize) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn get_window(&self, id: usize) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn bring_to_front(&mut self, id: usize) {
        if let Some(window) = self.get_window_mut(id) {
            window.z_index = self.next_z;
            self.next_z += 1;
            self.focused = Some(id);
        }
    }

    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        // Sort by z_index descending (highest first)
        let mut sorted: Vec<_> = self.windows.iter().enumerate().collect();
        sorted.sort_unstable_by_key(|(_, w)| std::u16::MAX - w.z_index as u16);

        for (idx, window) in sorted {
            if x >= window.rect.x && x < window.rect.x + window.rect.width
                && y >= window.rect.y && y < window.rect.y + window.rect.height
            {
                return Some(self.windows[idx].id);
            }
        }
        None
    }

    fn update_layout(&mut self, area: ratatui::layout::Rect) {
        self.terminal_size = area;
    }
}

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        
        if crossterm::event::poll(timeout)? {
            if let Event::Mouse(me) = event::read()? {
                handle_mouse_event(&mut app, me);
            }
            if let Event::Key(key) = event::read()? {
                handle_key_event(&mut app, key);
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // Tick logic here if needed
            last_tick = Instant::now();
        }

        if app.windows.is_empty() {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn handle_mouse_event(app: &mut App, me: crossterm::event::MouseEvent) {
    let (x, y) = (me.column, me.row);

    match me.kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            // Check resize handle first (bottom-right corner)
            if let Some(id) = app.hit_test(x, y) {
                if let Some(window) = app.get_window(id) {
                    let resize_area = ratatui::layout::Rect::new(
                        window.rect.x + window.rect.width.saturating_sub(3),
                        window.rect.y + window.rect.height.saturating_sub(3),
                        3, 3,
                    );
                    if resize_area.intersects(&(ratatui::layout::Rect::new(x, y, 1, 1))) {
                        // Start resize
                        if let Some(window) = app.get_window_mut(id) {
                            window.resizing = true;
                        }
                    } else {
                        // Check title bar for drag
                        let title_bar = ratatui::layout::Rect::new(
                            window.rect.x,
                            window.rect.y,
                            window.rect.width,
                            3,
                        );
                        if title_bar.intersects(&(ratatui::layout::Rect::new(x, y, 1, 1))) {
                            app.drag_window = Some(id);
                            app.drag_offset = Some((
                                (x as i16 - window.rect.x as i16),
                                (y as i16 - window.rect.y as i16)
                            ));
                        } else {
                            app.bring_to_front(id);
                        }
                    }
                }
            }
        }
        MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
            if let Some(id) = app.drag_window {
                if let Some(offset) = app.drag_offset {
                    if let Some(window) = app.get_window_mut(id) {
                        let new_x = (x as i16 - offset.0).max(0) as u16;
                        let new_y = (y as i16 - offset.1).max(0) as u16;
                        window.rect.x = new_x.min(app.terminal_size.width.saturating_sub(10));
                        window.rect.y = new_y.min(app.terminal_size.height.saturating_sub(5));
                    }
                }
            }
        }
        MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
            // End drag/resize
            app.drag_window = None;
            app.drag_offset = None;
            for window in &mut app.windows {
                window.resizing = false;
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            // Handle scroll in focused window
        }
        _ => {}
    }
}

fn handle_key_event(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        crossterm::event::KeyCode::Char('q') => {
            if key.modifiers == crossterm::event::KeyModifiers::CONTROL {
                app.windows.clear();
            }
        }
        crossterm::event::KeyCode::Char('n') => {
            app.add_window("New Window", "New content...".to_string());
        }
        crossterm::event::KeyCode::Tab => {
            // Cycle focus
            if let Some(focused) = app.focused {
                let next = (focused + 1) % app.windows.len();
                app.bring_to_front(app.windows[next].id);
            }
        }
        _ => {}
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    // Update terminal size
    app.update_layout(f.area());

    // Sort windows by z_index (highest first)
    let mut sorted_windows: Vec<_> = app.windows.iter().collect();
    sorted_windows.sort_unstable_by_key(|w| std::u16::MAX - w.z_index as u16);

    // Render background windows first
    for window in &sorted_windows {
        draw_window(f, window, false);
    }

    // Render topmost window with focus styling
    if let Some(top_window) = sorted_windows.first() {
        draw_window(f, top_window, true);
    }

    // Dropdown handling
    if let Some(dropdown_id) = app.dropdown_open {
        if let Some(pos) = app.dropdown_pos {
            draw_dropdown(f, dropdown_id, pos);
        }
    }
}

fn draw_window(f: &mut Frame, window: &Window, focused: bool) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", window.title),
            Style::default()
                .fg(if focused { Color::Yellow } else { Color::White })
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let content = Paragraph::new(window.content.as_str())
        .block(block)
        .wrap(Wrap::default());

    f.render_widget(content, window.rect);

    // Resize handle
    if window.resizing || true {
        let handle_rect = ratatui::layout::Rect::new(
            window.rect.x + window.rect.width.saturating_sub(3),
            window.rect.y + window.rect.height.saturating_sub(3),
            3, 3,
        );
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
            handle_rect,
        );
    }
}

fn draw_dropdown(f: &mut Frame, _window_id: usize, pos: (u16, u16)) {
    let dropdown_rect = ratatui::layout::Rect::new(pos.0, pos.1 + 3, 20, 10);
    let items = vec![
        Span::raw("Option 1"),
        Span::raw("Option 2"),
        Span::raw("Option 3"),
    ];
    let dropdown = Paragraph::new(Line::from(items))
        .block(
            Block::default()
                .title("Dropdown")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
    f.render_widget(dropdown, dropdown_rect);
}
```

**Controls:**

- Drag titlebar to move windows
- Click bottom-right corner to resize
- `Ctrl+q`: Quit
- `n`: New window
- `Tab`: Cycle focus
- Mouse events fully supported

Starter für TurboVision-ähnliches Window Manager mit Z-Order, Drag/Resize, Mouse Handling![^2_2][^2_3]
<span style="display:none">[^2_1][^2_10][^2_4][^2_5][^2_6][^2_7][^2_8][^2_9]</span>

<div align="center">⁂</div>

[^2_1]: https://ratatui.rs/examples/apps/demo/

[^2_2]: https://stackoverflow.com/questions/78263467/detecting-mouse-click-events-on-blocks-when-using-ratatui

[^2_3]: https://docs.rs/tui-vision/latest/tui_vision/

[^2_4]: https://www.youtube.com/watch?v=awX7DUp-r14

[^2_5]: https://github.com/radix-ui/primitives/issues/1242

[^2_6]: https://lib.rs/crates/tui-vision

[^2_7]: https://blog.orhun.dev/ratatui-0-21-0/

[^2_8]: https://ratatui.rs/concepts/event-handling/

[^2_9]: https://github.com/ratatui/awesome-ratatui

[^2_10]: https://crates.io/crates/ratatui-toolkit/0.1.4

