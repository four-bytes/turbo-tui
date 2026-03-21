//! turbo-tui demo — shows Desktop, MenuBar, Windows, Buttons, StatusLine.
//!
//! Run with:  cargo run --example demo
//!
//! Keys:
//!   F10        — open/close menu bar
//!   Alt+X      — quit (Borland convention)
//!   F5         — cycle active window
//!   Mouse      — click windows to focus, drag title bars
//!   F2         — cycle theme (Dark → Modern → Borland Classic)

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use turbo_tui::command::{
    CM_CANCEL, CM_CLOSE, CM_COPY, CM_CUT, CM_NEW, CM_OK, CM_OPEN, CM_PASTE, CM_QUIT, CM_REDO,
    CM_SAVE, CM_UNDO,
};
use turbo_tui::prelude::*;
use turbo_tui::theme::{self, Theme};

// Custom command IDs that don't conflict with the library's built-ins
const CM_ABOUT: u16 = 200;
const CM_THEME_TOGGLE: u16 = 201;

// ============================================================================
// Entry point
// ============================================================================

fn main() -> io::Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

// ============================================================================
// Application loop
// ============================================================================

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let size = terminal.size()?;

    // ── Desktop ────────────────────────────────────────────────────────────
    // Row 0 = menu bar, rows 1..height-2 = desktop, row height-1 = status
    let desktop_rect = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
    let mut desktop = Desktop::new(desktop_rect);

    // ── Menu Bar ───────────────────────────────────────────────────────────
    let menu_rect = Rect::new(0, 0, size.width, 1);
    let mut menu_bar = MenuBar::new(
        menu_rect,
        vec![
            Menu::new(
                "~F~ile",
                vec![
                    MenuItem::new("~N~ew", CM_NEW),
                    MenuItem::new("~O~pen   Ctrl+O", CM_OPEN),
                    MenuItem::new("~S~ave   Ctrl+S", CM_SAVE),
                    MenuItem::separator(),
                    MenuItem::new("E~x~it   Alt+X", CM_QUIT),
                ],
            ),
            Menu::new(
                "~E~dit",
                vec![
                    MenuItem::new("~U~ndo   Ctrl+Z", CM_UNDO),
                    MenuItem::new("~R~edo   Ctrl+Y", CM_REDO),
                    MenuItem::separator(),
                    MenuItem::new("Cu~t~    Ctrl+X", CM_CUT),
                    MenuItem::new("~C~opy   Ctrl+C", CM_COPY),
                    MenuItem::new("~P~aste  Ctrl+V", CM_PASTE),
                ],
            ),
            Menu::new(
                "~V~iew",
                vec![MenuItem::new("~T~oggle Theme  F2", CM_THEME_TOGGLE)],
            ),
            Menu::new("~H~elp", vec![MenuItem::new("~A~bout", CM_ABOUT)]),
        ],
    );

    // ── Status Line ────────────────────────────────────────────────────────
    let status_rect = Rect::new(0, size.height.saturating_sub(1), size.width, 1);
    let mut status_line = StatusLine::new(
        status_rect,
        vec![
            StatusItem::new("~F1~ Help", 0, KB_F1),
            StatusItem::new("~F2~ Theme", CM_THEME_TOGGLE, KB_F2),
            StatusItem::new("~F5~ Next win", 0, KB_F5),
            StatusItem::new("~F10~ Menu", 0, KB_F10),
            StatusItem::new("~Alt+X~ Quit", 0, 0),
        ],
    );

    // ── Window 1: Editor ───────────────────────────────────────────────────
    // Position: col 4, row 2 (inside desktop); size 44×12
    let mut win1 = Window::new(Rect::new(4, 2, 44, 12), "Editor");
    win1.set_resizable(true);
    win1.set_drag_limits(desktop_rect);

    // Interior starts at (5, 3) for a window at (4, 2)
    let int1 = win1.interior_rect();
    win1.add(Box::new(StaticText::new(
        Rect::new(0, 0, int1.width, 1),
        "Welcome to turbo-tui!",
    )));
    win1.add(Box::new(StaticText::new(
        Rect::new(0, 1, int1.width, 1),
        "Drag this window by its title bar.",
    )));
    win1.add(Box::new(StaticText::new(
        Rect::new(0, 2, int1.width, 1),
        "Resize from the bottom-right corner.",
    )));
    win1.add(Box::new(StaticText::new(
        Rect::new(0, 4, int1.width, 1),
        "Press F5 to cycle windows.",
    )));
    win1.add(Box::new(StaticText::new(
        Rect::new(0, 5, int1.width, 1),
        "Press F10 to open the menu bar.",
    )));

    // Vertical scrollbar on right border (frame child, not interior child)
    // Coordinates relative to window: x = width-1 (right border), y = 1 (below title), height = height-2
    let mut scrollbar = ScrollBar::vertical(Rect::new(43, 2, 1, 9));
    scrollbar.set_params(25, 0, 100, 10, 1);
    win1.add_frame_child(Box::new(scrollbar));

    // ── Window 2: Controls ─────────────────────────────────────────────────
    // Overlapping: col 20, row 7; size 36×10
    let mut win2 = Window::new(Rect::new(20, 7, 36, 10), "Controls");
    win2.set_drag_limits(desktop_rect);

    let int2 = win2.interior_rect();
    win2.add(Box::new(StaticText::new(
        Rect::new(0, 0, int2.width, 1),
        "Click the buttons below:",
    )));

    // OK button — 10 wide, row 2
    win2.add(Box::new(Button::new(
        Rect::new(0, 2, 10, 1),
        "~O~K",
        CM_OK,
        true,
    )));
    // Cancel button — 12 wide, same row
    win2.add(Box::new(Button::new(
        Rect::new(12, 2, 12, 1),
        "~C~ancel",
        CM_CANCEL,
        false,
    )));

    win2.add(Box::new(StaticText::new(
        Rect::new(0, 4, int2.width, 1),
        "Last command: (none)",
    )));

    desktop.add_window(Box::new(win1));
    desktop.add_window(Box::new(win2));

    // ── Event loop ─────────────────────────────────────────────────────────
    let mut running = true;
    let mut last_cmd: &'static str = "(none)";
    let mut theme_index: usize = 0; // 0=dark, 1=modern, 2=borland

    while running {
        // Draw frame
        terminal.draw(|f| {
            let area = f.area();
            let buf = f.buffer_mut();

            // Desktop (background + windows) — rows 1..height-2
            let d_area = Rect::new(0, 1, area.width, area.height.saturating_sub(2));
            desktop.draw(buf, d_area);

            // Menu bar — row 0
            let m_area = Rect::new(0, 0, area.width, 1);
            menu_bar.draw(buf, m_area);

            // Status line — last row
            let s_area = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
            status_line.draw(buf, s_area);
        })?;

        // Poll for input (50 ms tick)
        if !event::poll(Duration::from_millis(16))? {
            continue;
        }

        match event::read()? {
            // ── Keyboard ───────────────────────────────────────────────────
            CEvent::Key(key) => {
                // Global: Alt+X quits (Borland convention)
                if key.code == KeyCode::Char('x')
                    && key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                {
                    running = false;
                    continue;
                }

                // F5: cycle to next window
                if key.code == KeyCode::F(5) && !menu_bar.is_active() {
                    desktop.next_window();
                    last_cmd = "F5: next window";
                    continue;
                }

                // F2: cycle theme (dark → modern → borland)
                if key.code == KeyCode::F(2) && !menu_bar.is_active() {
                    theme_index = (theme_index + 1) % 3;
                    let name = apply_theme(theme_index);
                    last_cmd = match name {
                        "Dark" => "Theme: Dark",
                        "Modern" => "Theme: Modern",
                        "Borland Classic" => "Theme: Borland Classic",
                        _ => "Theme changed",
                    };
                    continue;
                }

                let mut ev = Event::key(key);

                // Menu bar gets priority when active, for F10, or for Alt+letter hotkeys
                if menu_bar.is_active()
                    || key.code == KeyCode::F(10)
                    || key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                {
                    menu_bar.handle_event(&mut ev);
                    if let Some(cmd) = ev.command_id() {
                        handle_command(cmd, &mut running, &mut last_cmd);
                        continue;
                    }
                    if ev.is_cleared() || ev.handled {
                        continue;
                    }
                }

                // Otherwise pass to desktop
                desktop.handle_event(&mut ev);
                if let Some(cmd) = ev.command_id() {
                    if cmd == CM_CLOSE {
                        if let Some(active_id) = desktop.active_window_id() {
                            desktop.close_window(active_id);
                            last_cmd = "Window closed";
                        }
                    } else if cmd == CM_THEME_TOGGLE {
                        theme_index = (theme_index + 1) % 3;
                        let name = apply_theme(theme_index);
                        last_cmd = match name {
                            "Dark" => "Theme: Dark",
                            "Modern" => "Theme: Modern",
                            "Borland Classic" => "Theme: Borland Classic",
                            _ => "Theme changed",
                        };
                    } else {
                        handle_command(cmd, &mut running, &mut last_cmd);
                    }
                }
            }

            // ── Mouse ──────────────────────────────────────────────────────
            CEvent::Mouse(mouse) => {
                let mut ev = Event::mouse(mouse);

                // When menu is active, it gets ALL mouse events first (like Borland TV).
                // This prevents clicks on dropdown items from "falling through" to windows.
                if menu_bar.is_active() {
                    menu_bar.handle_event(&mut ev);
                    if let Some(cmd) = ev.command_id() {
                        handle_command(cmd, &mut running, &mut last_cmd);
                        continue;
                    }
                    if ev.is_cleared() || ev.handled {
                        continue;
                    }
                    // Menu didn't consume it — close menu and let it fall through to desktop
                    menu_bar.close();
                } else if mouse.row == 0 {
                    // Menu bar row — always route there even when inactive
                    menu_bar.handle_event(&mut ev);
                    if let Some(cmd) = ev.command_id() {
                        handle_command(cmd, &mut running, &mut last_cmd);
                    }
                    if ev.is_cleared() || ev.handled {
                        continue;
                    }
                }

                desktop.handle_event(&mut ev);
                if let Some(cmd) = ev.command_id() {
                    if cmd == CM_CLOSE {
                        if let Some(active_id) = desktop.active_window_id() {
                            desktop.close_window(active_id);
                            last_cmd = "Window closed";
                        }
                    } else if cmd == CM_THEME_TOGGLE {
                        theme_index = (theme_index + 1) % 3;
                        let name = apply_theme(theme_index);
                        last_cmd = match name {
                            "Dark" => "Theme: Dark",
                            "Modern" => "Theme: Modern",
                            "Borland Classic" => "Theme: Borland Classic",
                            _ => "Theme changed",
                        };
                    } else {
                        handle_command(cmd, &mut running, &mut last_cmd);
                    }
                }
            }

            // ── Resize ─────────────────────────────────────────────────────
            CEvent::Resize(w, h) => {
                desktop.set_bounds(Rect::new(0, 1, w, h.saturating_sub(2)));
                menu_bar.set_bounds(Rect::new(0, 0, w, 1));
                status_line.set_bounds(Rect::new(0, h.saturating_sub(1), w, 1));
            }

            _ => {}
        }

        // Suppress unused-variable warning — last_cmd is displayed via draw loop
        let _ = last_cmd;
    }

    Ok(())
}

// ============================================================================
// Theme cycling
// ============================================================================

/// Theme names for status display.
const THEME_NAMES: [&str; 3] = ["Dark", "Modern", "Borland Classic"];

/// Apply theme by index and return its name.
fn apply_theme(index: usize) -> &'static str {
    match index % 3 {
        0 => {
            theme::set(Theme::dark());
            THEME_NAMES[0]
        }
        1 => {
            theme::set(Theme::modern());
            THEME_NAMES[1]
        }
        2 => {
            theme::set(Theme::borland_classic());
            THEME_NAMES[2]
        }
        _ => unreachable!(),
    }
}

// ============================================================================
// Command dispatch
// ============================================================================

fn handle_command(cmd: u16, running: &mut bool, last_cmd: &mut &'static str) {
    match cmd {
        CM_QUIT => {
            *running = false;
        }
        CM_OK => {
            *last_cmd = "OK pressed";
        }
        CM_CANCEL => {
            *last_cmd = "Cancel pressed";
        }
        CM_NEW => {
            *last_cmd = "File > New";
        }
        CM_OPEN => {
            *last_cmd = "File > Open";
        }
        CM_SAVE => {
            *last_cmd = "File > Save";
        }
        CM_UNDO => {
            *last_cmd = "Edit > Undo";
        }
        CM_REDO => {
            *last_cmd = "Edit > Redo";
        }
        CM_CUT => {
            *last_cmd = "Edit > Cut";
        }
        CM_COPY => {
            *last_cmd = "Edit > Copy";
        }
        CM_PASTE => {
            *last_cmd = "Edit > Paste";
        }
        CM_ABOUT => {
            *last_cmd = "Help > About";
        }
        CM_CLOSE => {
            // Close handled at event loop level — just label it
            *last_cmd = "Close window";
        }
        CM_THEME_TOGGLE => {
            // Theme toggle handled at event loop level — just label it
            *last_cmd = "Toggle Theme";
        }
        _ => {}
    }
}
