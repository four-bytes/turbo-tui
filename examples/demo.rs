//! turbo-tui interactive demo.
//!
//! Demonstrates the full turbo-tui v0.2.1 widget set:
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
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::io;
use std::time::Duration;
use turbo_tui::{
    application::Application,
    button::Button,
    command::{CM_CLOSE, CM_NEXT_THEME, CM_OK, CM_QUIT},
    horizontal_bar::{BarEntry, HorizontalBar},
    menu_bar::{menu_bar_from_menus, Menu, MenuItem},
    static_text::StaticText,
    status_bar::{KB_ALT_X, KB_F10, KB_F2},
    theme,
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
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let size = terminal.size()?;

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

    let mut app = Application::new(Rect::new(0, 0, size.width, size.height));

    // Setup menu bar (needs theme_names for dynamic theme submenu)
    setup_menu_bar(&mut app, &theme_names);

    // Setup status bar
    setup_status_bar(&mut app);

    // Add demo windows
    add_demo_windows(&mut app);

    // ── Event loop ─────────────────────────────────────────────────────
    // Draw once before entering the loop so the initial frame is visible.
    terminal.draw(|frame| {
        app.draw(frame);
    })?;

    loop {
        if event::poll(Duration::from_millis(16))? {
            let ct_event = event::read()?;

            // Handle Alt+X quit before passing to app
            if let event::Event::Key(key) = &ct_event {
                if key.kind == event::KeyEventKind::Press {
                    if key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::ALT) {
                        break;
                    }

                    // F2 → cycle theme
                    if key.code == KeyCode::F(2) {
                        let _ = theme::cycle_next_registered();
                    }
                }
            }

            app.handle_crossterm_event(&ct_event);

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

            // Redraw only after processing an event
            terminal.draw(|frame| {
                app.draw(frame);
            })?;
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
        MenuItem::new("~T~ile", 1020),
        MenuItem::new("~C~ascade", 1021),
        MenuItem::separator(),
        MenuItem::new("~C~lose", CM_CLOSE),
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
    let text = StaticText::new(Rect::new(1, 1, 31, 1), "turbo-tui v0.2.1 Demo");
    win1.add(Box::new(text));

    let text2 = StaticText::new(Rect::new(1, 3, 31, 1), "Drag title bar to move");
    win1.add(Box::new(text2));

    let text3 = StaticText::new(Rect::new(1, 4, 31, 1), "Drag corner to resize");
    win1.add(Box::new(text3));

    let text4 = StaticText::new(Rect::new(1, 6, 31, 1), "Focus changes scrollbar style");
    win1.add(Box::new(text4));

    let ok_btn = Button::new(Rect::new(12, 8, 10, 1), "~O~K", CM_OK, true);
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

    // Window 3: Scroll Demo — both scrollbars via Builder Lite
    let mut win3 = Window::new(Rect::new(45, 2, 30, 12), "Scroll Demo")
        .with_scrollbars(true, true)
        .with_min_size(20, 8);
    let info = StaticText::new(Rect::new(1, 1, 24, 1), "Both scrollbars active");
    win3.add(Box::new(info));

    let info2 = StaticText::new(Rect::new(1, 3, 24, 1), "Click to focus — watch");
    win3.add(Box::new(info2));

    let info3 = StaticText::new(Rect::new(1, 4, 24, 1), "scrollbar style change");
    win3.add(Box::new(info3));

    app.add_window(win3);

    // Window 4: Tool — uses Window::tool() preset
    let mut win4 = Window::tool(Rect::new(10, 8, 20, 8), "Tool Panel");
    let tool_info = StaticText::new(Rect::new(1, 1, 16, 1), "Tool preset window");
    win4.add(Box::new(tool_info));

    app.add_window(win4);
}
