//! turbo-tui interactive demo.
//!
//! Demonstrates the full turbo-tui v0.2.3 widget set:
//! - Application event loop
//! - Desktop with blue background
//! - Overlapping windows with drag and resize
//! - `MenuBar` with dropdown menus (theme list built dynamically)
//! - `StatusBar` with F-key shortcuts
//! - Buttons and labels in windows
//! - Builder Lite pattern for window construction
//! - Window presets (editor, tool)
//! - Focus-dependent scrollbar styling (active/inactive)
//!
//! Controls:
//! - Alt+X: Quit
//! - F10: Activate menu bar
//! - Mouse: Click, drag, resize windows
//! - F2: Cycle themes
//! - Tab: Cycle focus within a window

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use std::io;
use std::time::{Duration, Instant};
use turbo_tui::canvas::CellCanvas;
use turbo_tui::core::{Link, LinkTarget, Rect};
use turbo_tui::terminal::Terminal;
use turbo_tui::{
    application::Application,
    button::Button,
    command::{CM_CASCADE, CM_CLOSE, CM_CLOSE_ALL, CM_NEXT_THEME, CM_OK, CM_QUIT, CM_TILE},
    editor::Editor,
    horizontal_bar::{BarEntry, HorizontalBar},
    link_label::LinkLabel,
    listbox::ListBox,
    menu_bar::{menu_bar_from_menus, Menu, MenuItem},
    opener::open_url,
    static_text::StaticText,
    status_bar::{KB_ALT_X, KB_F10, KB_F2},
    syntax::RustHighlighter,
    theme,
    view::OF_DROP_TARGET,
    window::Window,
};

/// Base command ID for dynamic theme selection.
/// Theme at index `i` in the sorted registry gets command `CM_THEME_BASE + i`.
const CM_THEME_BASE: u16 = 1060;

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let (width, height) = size()?;
    let mut terminal = Terminal::new(io::stdout(), width, height);

    // Run app
    let result = run_app(&mut terminal);

    // Restore terminal
    terminal.show_cursor()?;
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

    result
}

fn run_app(terminal: &mut Terminal<io::Stdout>) -> io::Result<()> {
    let (width, height) = terminal.size();

    // ── Theme initialization ───────────────────────────────────────────
    // 1. Register built-in Turbo Vision theme
    theme::init_builtin();
    // 2. Load JSON themes from themes/ directory (relative to crate root, not CWD)
    let themes_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("themes");
    let themes_dir = themes_dir.as_path();
    if themes_dir.exists() {
        let report =
            theme::load_themes_from_dir(themes_dir).expect("Failed to read themes/ directory");
        if let Some(summary) = report.error_summary() {
            panic!("Theme loading errors:\n{summary}");
        }
    }
    // 3. Set initial theme (prefer "Dark" from JSON, fall back to "Turbo Vision")
    if !theme::set_by_name("Dark") {
        let _ = theme::set_by_name("Turbo Vision");
    }

    // Snapshot theme names for menu building and command handling
    let theme_names = theme::registered_names();

    let mut app = Application::new(Rect::new(0, 0, width, height));
    terminal.set_hyperlinks(app.hyperlink_support().enabled);

    // Setup menu bar (needs theme_names for dynamic theme submenu)
    setup_menu_bar(&mut app, &theme_names);

    // Setup status bar
    setup_status_bar(&mut app);

    // Add demo windows
    add_demo_windows(&mut app);

    // ── Event loop ─────────────────────────────────────────────────────
    // Draw once before entering the loop so the initial frame is visible.
    terminal.draw(|buf| app.draw(&mut CellCanvas::new(buf)))?;

    // Target frame budget (~60 fps). When a burst of events is pending, the
    // whole queue is drained and dispatched as one batch, then a single frame
    // is drawn; sleeping the remaining budget keeps the frame rate at ~60 fps
    // so a fast drag neither trails the cursor nor hammers the terminal.
    let frame_budget = Duration::from_millis(16);

    loop {
        // Block for the first event (also the idle poll interval).
        if event::poll(frame_budget)? {
            let frame_start = Instant::now();

            // Collect every already-queued event without blocking, so a burst
            // of mouse motion becomes one batch and one draw.
            let mut events = vec![event::read()?];
            while event::poll(Duration::ZERO)? {
                events.push(event::read()?);
            }

            // Handle Alt+X quit, F2 theme cycle and terminal resizes before
            // dispatching.
            let mut quit = false;
            for ct_event in &events {
                match ct_event {
                    event::Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                        if key.code == KeyCode::Char('x')
                            && key.modifiers.contains(KeyModifiers::ALT)
                        {
                            quit = true;
                        }

                        // F2 → cycle theme
                        if key.code == KeyCode::F(2) {
                            let _ = theme::cycle_next_registered();
                        }
                    }
                    event::Event::Resize(w, h) => terminal.resize(*w, *h)?,
                    _ => {}
                }
            }

            if quit {
                break;
            }

            // Coalesces consecutive Moved/Drag events, dispatches the batch.
            app.handle_crossterm_events(&events);

            // Reflect the hovered link in the status bar and open activated
            // links through the platform handler.
            app.set_status_hint(app.hovered_link().map(ToString::to_string));
            if let Some(target) = app.take_open_link() {
                match target {
                    LinkTarget::Url(url) => {
                        if let Err(e) = open_url(&url) {
                            app.set_status_hint(Some(format!("open failed: {e}")));
                        }
                    }
                    other => app.set_status_hint(Some(format!("internal link: {other}"))),
                }
            }

            // Handle commands from menu
            if let Some(cmd) = app.take_unhandled_command() {
                if cmd == CM_NEXT_THEME {
                    // StatusBar "F2 Theme" click → cycle theme
                    let _ = theme::cycle_next_registered();
                } else if cmd >= CM_THEME_BASE {
                    // Dynamic theme selection from Window menu
                    let idx = (cmd - CM_THEME_BASE) as usize;
                    if let Some(name) = theme_names.get(idx) {
                        let _ = theme::set_by_name(name);
                    }
                }
            }

            // Draw exactly once for the whole batch.
            terminal.draw(|buf| app.draw(&mut CellCanvas::new(buf)))?;

            // Enforce the frame cap: sleep whatever is left of the budget.
            if let Some(remaining) = frame_budget.checked_sub(frame_start.elapsed()) {
                std::thread::sleep(remaining);
            }
        }

        if !app.is_running() {
            break;
        }
    }

    Ok(())
}

fn setup_menu_bar(app: &mut Application, theme_names: &[String]) {
    let bounds = Rect::new(0, 0, 80, 1); // Will be resized by Application

    // Build theme menu items dynamically from registered theme names
    let mut window_items = vec![
        MenuItem::new("~T~ile", CM_TILE),
        MenuItem::new("~C~ascade", CM_CASCADE),
        MenuItem::separator(),
        MenuItem::new("~C~lose", CM_CLOSE),
        MenuItem::new("Close ~A~ll", CM_CLOSE_ALL),
    ];

    if !theme_names.is_empty() {
        window_items.push(MenuItem::separator());
        for (i, name) in theme_names.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let cmd = CM_THEME_BASE + i as u16;
            window_items.push(MenuItem::new(&format!("Theme: {name}"), cmd));
        }
    }

    let menus = vec![
        Menu::new(
            "~F~ile",
            vec![
                MenuItem::new("~N~ew", 1001),
                MenuItem::new("~O~pen", 1002),
                MenuItem::separator(),
                MenuItem::new("E~x~it", CM_QUIT),
            ],
        ),
        Menu::new(
            "~E~dit",
            vec![
                MenuItem::new("~C~ut", 1010),
                MenuItem::new("C~o~py", 1011),
                MenuItem::new("~P~aste", 1012),
            ],
        ),
        Menu::new("~W~indow", window_items),
        Menu::new("~H~elp", vec![MenuItem::new("~A~bout", 1030)]),
    ];

    let menu_bar = menu_bar_from_menus(bounds, menus);
    app.set_menu_bar(menu_bar);
}

fn setup_status_bar(app: &mut Application) {
    let bounds = Rect::new(0, 23, 80, 1); // Will be resized by Application

    let entries = vec![
        BarEntry::Action {
            label: "~Alt+X~ Quit".into(),
            command: CM_QUIT,
            key_code: KB_ALT_X,
        },
        BarEntry::Action {
            label: "~F2~ Theme".into(),
            command: CM_NEXT_THEME,
            key_code: KB_F2,
        },
        BarEntry::Action {
            label: "~F10~ Menu".into(),
            command: 1040,
            key_code: KB_F10,
        },
        BarEntry::Dropdown {
            label: "~I~nfo".into(),
            items: vec![
                MenuItem::new("~A~bout", 1030),
                MenuItem::separator(),
                MenuItem::new("~V~ersion", 1031),
            ],
            key_code: 0,
        },
    ];

    let status_bar = HorizontalBar::status_bar(bounds, entries);
    app.set_status_bar(status_bar);
}

fn add_demo_windows(app: &mut Application) {
    // Window 1: Welcome — uses Window::editor() preset (vertical scrollbar, min 20×8)
    let mut win1 = Window::editor(Rect::new(5, 3, 35, 12), "Welcome");
    let text = StaticText::new(Rect::new(1, 1, 31, 1), "turbo-tui v0.2.3 Demo");
    win1.add(Box::new(text));

    let text2 = StaticText::new(Rect::new(1, 3, 31, 1), "Drag title bar to move");
    win1.add(Box::new(text2));

    let text3 = StaticText::new(Rect::new(1, 4, 31, 1), "Drag corner to resize");
    win1.add(Box::new(text3));

    let text4 = StaticText::new(Rect::new(1, 6, 31, 1), "Focus changes scrollbar style");
    win1.add(Box::new(text4));

    let text5 = StaticText::new(Rect::new(1, 7, 31, 1), "Mouse wheel scrolls content");
    win1.add(Box::new(text5));

    let ok_btn = Button::new(Rect::new(12, 9, 10, 1), "~O~K", CM_OK, true);
    win1.add(Box::new(ok_btn));

    app.add_window(win1);

    // Window 2: Buttons — uses Builder Lite chain
    let mut win2 = Window::new(Rect::new(25, 6, 30, 10), "Buttons").with_min_size(15, 6);
    let btn1 = Button::new(Rect::new(2, 1, 12, 1), "Button ~1~", 1050, false);
    win2.add(Box::new(btn1));

    let btn2 = Button::new(Rect::new(2, 3, 12, 1), "Button ~2~", 1051, false);
    win2.add(Box::new(btn2));

    let btn3 = Button::new(Rect::new(2, 5, 12, 1), "~C~lose", CM_CLOSE, false);
    win2.add(Box::new(btn3));

    app.add_window(win2);

    // Window 3: Scroll Demo — both scrollbars, content larger than window
    // Demonstrates viewport scrolling: mouse wheel, scrollbar click/drag
    let mut win3 = Window::new(Rect::new(45, 2, 30, 12), "Scroll Demo")
        .with_scrollbars(true, true)
        .with_min_size(20, 8)
        .with_content_size(60, 30);

    // Add many lines of content that extend beyond the visible area
    win3.add(Box::new(StaticText::new(
        Rect::new(1, 1, 55, 1),
        "Scroll with mouse wheel or click scrollbar",
    )));
    win3.add(Box::new(StaticText::new(
        Rect::new(1, 2, 55, 1),
        "Drag scrollbar thumb to scroll quickly",
    )));
    win3.add(Box::new(StaticText::new(
        Rect::new(1, 3, 55, 1),
        "Click arrows for single-line scroll",
    )));
    win3.add(Box::new(StaticText::new(
        Rect::new(1, 4, 55, 1),
        "Click track for page scroll",
    )));
    for i in 5..28 {
        win3.add(Box::new(StaticText::new(
            Rect::new(1, i, 55, 1),
            &format!("Line {i:>2} — Scrollable content extends beyond window"),
        )));
    }

    app.add_window(win3);

    // Window 4: Tool — uses Window::tool() preset
    let mut win4 = Window::tool(Rect::new(10, 8, 20, 8), "Tool Panel");
    let tool_info = StaticText::new(Rect::new(1, 1, 16, 1), "Tool preset window");
    win4.add(Box::new(tool_info));

    app.add_window(win4);
    // Window 5: ListBox demo — selectable list with scrollbar
    let list_items = vec![
        "Apple".into(),
        "Banana".into(),
        "Cherry".into(),
        "Date".into(),
        "Elderberry".into(),
        "Fig".into(),
        "Grape".into(),
        "Honeydew".into(),
        "Kiwi".into(),
        "Lemon".into(),
        "Mango".into(),
        "Nectarine".into(),
        "Orange".into(),
        "Papaya".into(),
        "Quince".into(),
    ];
    let mut win5 = Window::new(Rect::new(35, 10, 22, 12), "ListBox Demo").with_min_size(18, 6);
    let list_label = StaticText::new(Rect::new(1, 0, 18, 1), "Fruits:");
    win5.add(Box::new(list_label));
    let list_box = ListBox::new(Rect::new(1, 1, 18, 8))
        .with_items(list_items)
        .with_selected(0);
    win5.add(Box::new(list_box));
    app.add_window(win5);

    // Window 6: Drop target ListBox — accepts dropped items via drag-and-drop
    let drop_items = vec![
        "Drop Items Here".to_string(),
        "Left-drag from any window".to_string(),
    ];
    let mut win6 = Window::new(Rect::new(5, 17, 24, 10), "Drop Target").with_min_size(18, 6);
    let drop_label = StaticText::new(Rect::new(1, 0, 20, 1), "Drag items onto list:");
    win6.add(Box::new(drop_label));
    let mut drop_list = ListBox::new(Rect::new(1, 1, 20, 6)).with_items(drop_items);
    drop_list.set_options(drop_list.get_options() | OF_DROP_TARGET);
    win6.add(Box::new(drop_list));

    // Window 7 is added before window 6 to preserve the original z-order.
    add_editor_window(app);
    app.add_window(win6);

    // Window 8: Links — interactive LinkLabels
    add_links_window(app);
}

/// Adds the interactive link demo window.
fn add_links_window(app: &mut Application) {
    let support = app.hyperlink_support();
    let support_line = format!(
        "OSC 8: {} — {}",
        if support.enabled { "on" } else { "off" },
        support.reason
    );

    let mut win8 = Window::new(Rect::new(18, 3, 44, 10), "Links").with_min_size(30, 7);

    let url_text = "https://github.com/four-bytes/turbo-tui";
    win8.add(Box::new(LinkLabel::new(
        Rect::new(1, 1, 40, 1),
        url_text,
        Link::new(LinkTarget::Url(url_text.to_owned())),
    )));

    let issue_target = LinkTarget::IssueRef {
        repo: Some("four-bytes/turbo-tui".to_owned()),
        number: 54,
    };
    let issue_text = format!("Issue {issue_target}");
    win8.add(Box::new(LinkLabel::new(
        Rect::new(1, 2, 40, 1),
        issue_text,
        Link::new(issue_target),
    )));

    let file_target = LinkTarget::FilePos {
        path: "src/lib.rs".to_owned(),
        line: 1,
        col: None,
    };
    let file_text = format!("File {file_target}");
    win8.add(Box::new(LinkLabel::new(
        Rect::new(1, 3, 40, 1),
        file_text,
        Link::new(file_target),
    )));

    win8.add(Box::new(StaticText::new(
        Rect::new(1, 5, 40, 1),
        &support_line,
    )));

    app.add_window(win8);
}

/// Adds the editor demo window (window 7).
fn add_editor_window(app: &mut Application) {
    // Window 7: Editor — text editor with Rust syntax highlighting
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let demo_path = manifest_dir.join("examples").join("demo.rs");
    let mut editor = Editor::new(Rect::new(0, 0, 55, 16))
        .with_file(demo_path.to_str().unwrap_or(""))
        .unwrap_or_else(|_| Editor::new(Rect::new(0, 0, 55, 16)));
    editor.set_highlighter(Box::new(RustHighlighter));

    let mut win7 = Window::new(Rect::new(1, 5, 58, 18), "Editor — demo.rs")
        .with_scrollbars(true, true)
        .with_min_size(30, 8)
        .with_content_size(55, 16);
    win7.add(Box::new(editor));
    app.add_window(win7);
}
