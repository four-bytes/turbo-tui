//! turbo-tui interactive demo.
//!
//! Demonstrates the full turbo-tui v0.2 widget set:
//! - Application event loop
//! - Desktop with blue background
//! - Overlapping windows with drag and resize
//! - `MenuBar` with dropdown menus
//! - `StatusLine` with F-key shortcuts
//! - Buttons and labels in windows
//!
//! Controls:
//! - Alt+X: Quit
//! - F10: Activate menu bar
//! - Mouse: Click, drag, resize windows
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
    command::{CM_CLOSE, CM_OK, CM_QUIT},
    menu_bar::{Menu, MenuBar, MenuItem},
    scrollbar::ScrollBar,
    static_text::StaticText,
    status_line::{StatusItem, StatusLine, KB_ALT_X, KB_F10},
    window::Window,
};

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

    // Optionally set a theme (dark is default)
    // theme::set(theme::Theme::borland_classic());
    // theme::set(theme::Theme::modern());
    // theme::set(theme::Theme::matrix());

    let mut app = Application::new(Rect::new(0, 0, size.width, size.height));

    // Setup menu bar
    setup_menu_bar(&mut app);

    // Setup status line
    setup_status_line(&mut app);

    // Add demo windows
    add_demo_windows(&mut app);

    // Event loop
    loop {
        terminal.draw(|frame| {
            app.draw(frame);
        })?;

        if event::poll(Duration::from_millis(16))? {
            let ct_event = event::read()?;

            // Handle Alt+X quit before passing to app
            if let event::Event::Key(key) = &ct_event {
                if key.kind == event::KeyEventKind::Press
                    && key.code == KeyCode::Char('x')
                    && key.modifiers.contains(KeyModifiers::ALT)
                {
                    break;
                }
            }

            app.handle_crossterm_event(&ct_event);
        }

        if !app.is_running() {
            break;
        }
    }

    Ok(())
}

fn setup_menu_bar(app: &mut Application) {
    let bounds = Rect::new(0, 0, 80, 1); // Will be resized by Application
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
        Menu::new(
            "~W~indow",
            vec![
                MenuItem::new("~T~ile", 1020),
                MenuItem::new("~C~ascade", 1021),
                MenuItem::separator(),
                MenuItem::new("~C~lose", CM_CLOSE),
            ],
        ),
        Menu::new("~H~elp", vec![MenuItem::new("~A~bout", 1030)]),
    ];

    let menu_bar = MenuBar::new(bounds, menus);
    app.set_menu_bar(menu_bar);
}

fn setup_status_line(app: &mut Application) {
    let bounds = Rect::new(0, 23, 80, 1); // Will be resized by Application
    let items = vec![
        StatusItem::new("~Alt+X~ Quit", CM_QUIT, KB_ALT_X),
        StatusItem::new("~F10~ Menu", 1040, KB_F10),
    ];

    let status_line = StatusLine::new(bounds, items);
    app.set_status_line(status_line);
}

fn add_demo_windows(app: &mut Application) {
    // Window1: Welcome
    let mut win1 = Window::new(Rect::new(5, 3, 35, 12), "Welcome");
    let text = StaticText::new(Rect::new(1, 1, 31, 1), "turbo-tui v0.2 Demo");
    win1.add(Box::new(text));

    let text2 = StaticText::new(Rect::new(1, 3, 31, 1), "Drag title bar to move");
    win1.add(Box::new(text2));

    let text3 = StaticText::new(Rect::new(1, 4, 31, 1), "Drag corner to resize");
    win1.add(Box::new(text3));

    let ok_btn = Button::new(Rect::new(12, 7, 10, 1), "~O~K", CM_OK, true);
    win1.add(Box::new(ok_btn));

    // Add a vertical scrollbar to demonstrate the scrollbar-on-border feature
    win1.frame_mut().set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));

    app.add_window(win1);

    // Window 2: Buttons
    let mut win2 = Window::new(Rect::new(25, 6, 30, 10), "Buttons");
    let btn1 = Button::new(Rect::new(2, 1, 12, 1), "Button ~1~", 1050, false);
    win2.add(Box::new(btn1));

    let btn2 = Button::new(Rect::new(2, 3, 12, 1), "Button ~2~", 1051, false);
    win2.add(Box::new(btn2));

    let btn3 = Button::new(Rect::new(2, 5, 12, 1), "~C~lose", CM_CLOSE, false);
    win2.add(Box::new(btn3));

    app.add_window(win2);

    // Window 3: Info
    let mut win3 = Window::new(Rect::new(45, 2, 30, 8), "Info");
    let info = StaticText::new(Rect::new(1, 1, 26, 1), "Click windows to focus");
    win3.add(Box::new(info));

    let info2 = StaticText::new(Rect::new(1, 3, 26, 1), "Alt+X or menu to quit");
    win3.add(Box::new(info2));

    app.add_window(win3);
}
