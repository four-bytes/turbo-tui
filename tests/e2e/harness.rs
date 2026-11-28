//! tmux-based harness for the turbo-tui demo.
//!
//! The harness starts an isolated tmux server with its own socket and scratch
//! directory, runs the freshly built demo in an exactly 80×24 pane, and parses
//! `tmux capture-pane -e` into a cell grid so scenarios can look for text,
//! window frames and text attributes.
//!
//! Every helper is crate-local to the `e2e` integration test; the public API is
//! exactly what `scenarios.rs` needs.
//!
//! This file casts between `usize`/`u16`/`i32` when converting grid geometry
//! and SGR coordinates. The grid is a bounded terminal (80×24) and every cast
//! is guarded by the grid dimensions, so the two cast lints are allowed with
//! that justification instead of scattering casts.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthChar;

/// How long startup may take before the harness gives up.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a reaction to an input event may take.
pub const REACTION_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Colours, cells and grids
// ---------------------------------------------------------------------------

/// A colour as reported by tmux `capture-pane -e`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Terminal default (SGR 39/49 or a reset).
    Default,
    /// ANSI palette index (0–255).
    Ansi(u8),
    /// 24-bit RGB colour.
    Rgb(u8, u8, u8),
}

/// One rendered terminal cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The grapheme or symbol drawn in the cell. A continuation cell of a
    /// double-width character stores an empty string.
    pub symbol: String,
    /// Foreground colour.
    pub fg: Color,
    /// Background colour.
    pub bg: Color,
    /// SGR 4 active.
    pub underline: bool,
    /// SGR 7 active.
    pub reverse: bool,
    /// SGR 1 active.
    pub bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            symbol: " ".to_owned(),
            fg: Color::Default,
            bg: Color::Default,
            underline: false,
            reverse: false,
            bold: false,
        }
    }
}

/// A cell attribute that can be searched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    /// SGR 4.
    Underline,
    /// SGR 7.
    Reverse,
    /// SGR 1.
    Bold,
}

/// An outer window rectangle in 0-based screen cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect80 {
    /// Left column.
    pub x: u16,
    /// Top row.
    pub y: u16,
    /// Width in cells.
    pub width: u16,
    /// Height in cells.
    pub height: u16,
}

impl Rect80 {
    /// Rightmost column.
    #[must_use]
    pub const fn right(self) -> u16 {
        self.x + self.width - 1
    }

    /// Bottommost row.
    #[must_use]
    pub const fn bottom(self) -> u16 {
        self.y + self.height - 1
    }

    /// Whether `(x, y)` lies inside the rectangle.
    #[must_use]
    pub const fn contains(self, x: u16, y: u16) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// Shrink by one cell on each side, clamped to the grid.
    #[must_use]
    pub fn inner(self, width: u16, height: u16) -> Self {
        let x = self.x.saturating_add(1);
        let y = self.y.saturating_add(1);
        let right = self.right().saturating_sub(1).min(width.saturating_sub(1));
        let bottom = self
            .bottom()
            .saturating_sub(1)
            .min(height.saturating_sub(1));
        Self {
            x,
            y,
            width: right.saturating_sub(x).saturating_add(1),
            height: bottom.saturating_sub(y).saturating_add(1),
        }
    }
}

/// A parsed pane as a row-major grid of cells.
#[derive(Clone, Debug)]
pub struct Grid {
    /// Pane width in cells.
    pub width: u16,
    /// Pane height in cells.
    pub height: u16,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(width: u16, height: u16) -> Self {
        let len = usize::from(width) * usize::from(height);
        Self {
            width,
            height,
            cells: vec![Cell::default(); len],
        }
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(usize::from(y) * usize::from(self.width) + usize::from(x))
    }

    /// Returns the cell at `(x, y)`, if it lies inside the grid.
    #[must_use]
    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).map(|index| &self.cells[index])
    }

    fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        self.index(x, y).map(|index| &mut self.cells[index])
    }

    /// Concatenates the symbols of one row. Continuation cells are skipped, so
    /// double-width characters appear once.
    #[must_use]
    pub fn row_text(&self, y: u16) -> String {
        if y >= self.height {
            return String::new();
        }
        let start = usize::from(y) * usize::from(self.width);
        let end = start + usize::from(self.width);
        self.cells[start..end]
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect()
    }

    /// Finds the first occurrence of `needle` (row-major) and returns the
    /// 0-based top-left cell.
    #[must_use]
    pub fn find_text(&self, needle: &str) -> Option<(u16, u16)> {
        if needle.is_empty() {
            return None;
        }
        for y in 0..self.height {
            let row = self.row_text(y);
            let Some(byte_pos) = row.find(needle) else {
                continue;
            };
            let char_pos = row[..byte_pos].chars().count();
            // Map the character offset back to the cell column: continuation
            // cells contribute no characters.
            let mut seen = 0_usize;
            let mut column = 0_u16;
            for x in 0..self.width {
                let symbol = self.cell(x, y).map_or("", |cell| cell.symbol.as_str());
                if symbol.is_empty() {
                    continue;
                }
                if seen == char_pos {
                    column = x;
                    break;
                }
                seen += symbol.chars().count();
                column = x;
            }
            return Some((column, y));
        }
        None
    }

    /// All cells carrying `attr`, in row-major order.
    #[must_use]
    pub fn cells_with_attr(&self, attr: Attr) -> Vec<(u16, u16)> {
        let mut found = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let Some(cell) = self.cell(x, y) else {
                    continue;
                };
                let matches = match attr {
                    Attr::Underline => cell.underline,
                    Attr::Reverse => cell.reverse,
                    Attr::Bold => cell.bold,
                };
                if matches {
                    found.push((x, y));
                }
            }
        }
        found
    }

    /// Background colour at `(x, y)`; `Color::Default` outside the grid.
    #[must_use]
    pub fn bg_at(&self, x: u16, y: u16) -> Color {
        self.cell(x, y).map_or(Color::Default, |cell| cell.bg)
    }

    /// Finds a window frame by its title: scans the title row for the nearest
    /// frame corners, then scans the left border down for the bottom corner.
    ///
    /// If the bottom border is clipped (the demo's editor extends behind the
    /// status bar) the rectangle is clamped to the last pane row.
    #[must_use]
    pub fn window_rect(&self, title: &str) -> Option<Rect80> {
        let (title_x, title_y) = self.find_text(title)?;
        let y = title_y;
        let is_top_left = |c: char| matches!(c, '┌' | '╔' | '┏' | '╭');
        let is_top_right = |c: char| matches!(c, '┐' | '╗' | '┓' | '╮');
        let char_at = |x: u16| {
            self.cell(x, y)
                .and_then(|cell| cell.symbol.chars().next())
                .unwrap_or(' ')
        };
        let left = (0..=title_x).rev().find(|&x| is_top_left(char_at(x)))?;
        let right = (title_x..self.width).find(|&x| is_top_right(char_at(x)))?;
        if right <= left {
            return None;
        }
        let bottom = self
            .find_bottom(left, y)
            .or_else(|| self.find_bottom(right, y));
        let bottom = bottom.unwrap_or_else(|| self.height.saturating_sub(1));
        if bottom < y {
            return None;
        }
        Some(Rect80 {
            x: left,
            y,
            width: right - left + 1,
            height: bottom - y + 1,
        })
    }

    fn find_bottom(&self, x: u16, top: u16) -> Option<u16> {
        let is_bottom_left = |c: char| matches!(c, '└' | '╚' | '┗' | '╰');
        let is_bottom_right = |c: char| matches!(c, '┘' | '╝' | '┛' | '╯');
        for y in (top + 1)..self.height {
            let Some(cell) = self.cell(x, y) else {
                break;
            };
            if let Some(c) = cell.symbol.chars().next() {
                if is_bottom_left(c) || is_bottom_right(c) {
                    return Some(y);
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Capture parser
// ---------------------------------------------------------------------------

/// Mutable SGR state while scanning a capture.
#[derive(Clone, Copy, Debug)]
struct Pen {
    fg: Color,
    bg: Color,
    underline: bool,
    reverse: bool,
    bold: bool,
}

impl Default for Pen {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            underline: false,
            reverse: false,
            bold: false,
        }
    }
}

fn apply_sgr(pen: &mut Pen, params: &str) {
    let values: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split(';')
            .map(|part| part.parse::<u16>().unwrap_or(0))
            .collect()
    };

    let mut index = 0;
    while index < values.len() {
        let value = values[index];
        match value {
            0 => *pen = Pen::default(),
            1 => pen.bold = true,
            22 => pen.bold = false,
            4 => pen.underline = true,
            24 => pen.underline = false,
            7 => pen.reverse = true,
            27 => pen.reverse = false,
            30..=37 => pen.fg = Color::Ansi((value - 30) as u8),
            39 => pen.fg = Color::Default,
            40..=47 => pen.bg = Color::Ansi((value - 40) as u8),
            49 => pen.bg = Color::Default,
            90..=97 => pen.fg = Color::Ansi((value - 90 + 8) as u8),
            100..=107 => pen.bg = Color::Ansi((value - 100 + 8) as u8),
            38 | 48 => {
                let extended = values.get(index + 1).copied();
                match extended {
                    Some(5) => {
                        if let Some(&slot) = values.get(index + 2) {
                            let color = Color::Ansi(slot as u8);
                            if value == 38 {
                                pen.fg = color;
                            } else {
                                pen.bg = color;
                            }
                        }
                        index += 2;
                    }
                    Some(2) => {
                        let r = values.get(index + 2).copied().unwrap_or(0) as u8;
                        let g = values.get(index + 3).copied().unwrap_or(0) as u8;
                        let b = values.get(index + 4).copied().unwrap_or(0) as u8;
                        let color = Color::Rgb(r, g, b);
                        if value == 38 {
                            pen.fg = color;
                        } else {
                            pen.bg = color;
                        }
                        index += 4;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        index += 1;
    }
}

fn place_char(grid: &mut Grid, pen: Pen, x: &mut usize, y: usize, ch: char) {
    let width = ch.width().unwrap_or(0);
    if width == 0 {
        // Combining mark: append to the previous non-continuation cell.
        if *x > 0 {
            let mut x = *x - 1;
            while x > 0
                && grid
                    .cell(x as u16, y as u16)
                    .is_some_and(|c| c.symbol.is_empty())
            {
                x -= 1;
            }
            if let Some(cell) = grid.cell_mut(x as u16, y as u16) {
                cell.symbol.push(ch);
            }
        }
        return;
    }
    if let Some(cell) = grid.cell_mut(*x as u16, y as u16) {
        *cell = Cell {
            symbol: ch.to_string(),
            fg: pen.fg,
            bg: pen.bg,
            underline: pen.underline,
            reverse: pen.reverse,
            bold: pen.bold,
        };
    }
    if width == 2 && *x + 1 < usize::from(grid.width) {
        if let Some(cell) = grid.cell_mut((*x + 1) as u16, y as u16) {
            *cell = Cell {
                symbol: String::new(),
                fg: pen.fg,
                bg: pen.bg,
                underline: pen.underline,
                reverse: pen.reverse,
                bold: pen.bold,
            };
        }
        *x += 2;
    } else {
        *x += 1;
    }
}

fn parse_capture(raw: &str, width: u16, height: u16) -> Grid {
    let chars: Vec<char> = raw.chars().collect();
    let mut grid = Grid::new(width, height);
    let mut pen = Pen::default();
    let mut x = 0_usize;
    let mut y = 0_usize;
    let mut i = 0_usize;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\x1b' {
            match chars.get(i + 1).copied() {
                Some('[') => {
                    let mut j = i + 2;
                    while j < chars.len() && !('\x40'..='\x7e').contains(&chars[j]) {
                        j += 1;
                    }
                    if j < chars.len() {
                        let params: String = chars[i + 2..j].iter().collect();
                        if chars[j] == 'm' {
                            apply_sgr(&mut pen, &params);
                        }
                        i = j + 1;
                    } else {
                        i = chars.len();
                    }
                }
                Some(']') => {
                    // OSC: skip until BEL or ST (ESC \).
                    let mut j = i + 2;
                    while j < chars.len() {
                        if chars[j] == '\x07' {
                            j += 1;
                            break;
                        }
                        if chars[j] == '\x1b' && chars.get(j + 1) == Some(&'\\') {
                            j += 2;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                }
                Some('(' | ')') => i += 3,
                Some(_) => i += 2,
                None => i += 1,
            }
            continue;
        }

        match ch {
            '\n' => {
                y += 1;
                x = 0;
            }
            '\r' => {}
            '\t' => x = (x / 8 + 1) * 8,
            c if c.is_control() => {}
            c => {
                if y < usize::from(height) {
                    place_char(&mut grid, pen, &mut x, y, c);
                }
            }
        }
        i += 1;
    }

    grid
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

static COUNTER: AtomicU64 = AtomicU64::new(0);
static BUILD_DEMO: Once = Once::new();

/// Polls `poll` until it returns `true` or `timeout` elapses.
pub fn wait_until(timeout: Duration, mut poll: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if poll() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// One running tmux server plus everything the tests need to talk to it.
pub struct Harness {
    socket: PathBuf,
    scratch: PathBuf,
    session_path: String,
}

impl Harness {
    /// Starts an isolated tmux session with the demo, or returns `None` when the
    /// opt-in flag is absent.
    ///
    /// Panics (never silently skips) when `TURBO_TUI_E2E=1` but tmux is missing
    /// or the demo cannot be built, because a requested run must not be a no-op.
    pub fn start() -> Option<Self> {
        if std::env::var("TURBO_TUI_E2E").as_deref() != Ok("1") {
            eprintln!("SKIP tmux e2e: set TURBO_TUI_E2E=1 to run");
            return None;
        }
        assert!(
            Command::new("tmux").arg("-V").output().is_ok(),
            "TURBO_TUI_E2E=1 but tmux is not installed"
        );
        build_demo();

        let scratch = unique_scratch();
        let bin = scratch.join("bin");
        std::fs::create_dir_all(&bin).expect("create e2e bin dir");
        let opener = bin.join("xdg-open");
        std::fs::write(
            &opener,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$TURBO_TUI_OPEN_LOG\"\nexit 0\n",
        )
        .expect("write xdg-open stub");
        make_executable(&opener);

        let session_path = format!("{}:/usr/bin:/bin", bin.display());
        let socket = scratch.join("tmux.sock");
        let conf = scratch.join("tmux.conf");
        std::fs::write(
            &conf,
            "set -g default-terminal \"xterm-256color\"\n\
             set -g status off\n\
             set -g mouse off\n\
             set -g escape-time 0\n\
             set -g history-limit 0\n",
        )
        .expect("write tmux.conf");

        let open_log = scratch.join("opened.log");
        let demo = demo_binary();
        let mut command = Command::new("tmux");
        command
            .arg("-S")
            .arg(&socket)
            .env("PATH", &session_path)
            // On WSL hosts `open_url` would otherwise pick `wslview` and the
            // test's `xdg-open` stub would never run. The harness is a Linux
            // tmux test, so hide the WSL markers from the server it starts.
            .env_remove("WSL_DISTRO_NAME")
            .env_remove("WSL_INTEROP")
            .arg("-f")
            .arg(&conf)
            .arg("new-session")
            .arg("-d")
            .arg("-x")
            .arg("80")
            .arg("-y")
            .arg("24")
            .arg("-s")
            .arg("e2e");
        for (key, value) in [
            ("PATH", session_path.as_str()),
            ("TERM", "xterm-256color"),
            ("LANG", "C.UTF-8"),
            ("LC_ALL", "C.UTF-8"),
            ("TURBO_TUI_OPEN_LOG", open_log.to_str().unwrap_or("")),
            ("TURBO_TUI_HYPERLINKS", "off"),
        ] {
            command.arg("-e").arg(format!("{key}={value}"));
        }
        command.arg(&demo);
        let output = command.output().expect("start tmux session");
        assert!(
            output.status.success(),
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let harness = Self {
            socket,
            scratch,
            session_path,
        };

        // The app draws once before its event loop, so a rendered menu bar is a
        // reliable readiness marker.
        let ready = harness.wait_for_text("File", STARTUP_TIMEOUT);
        assert!(ready, "demo did not render inside tmux");

        let size = harness.size();
        assert_eq!(
            size,
            (80, 24),
            "tmux pane must be exactly 80x24, got {size:?}"
        );

        Some(harness)
    }

    /// Pane dimensions `(width, height)`.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        let raw = self.tmux_stdout(&["display", "-p", "-t", "e2e", "#{pane_width}x#{pane_height}"]);
        let mut parts = raw.trim().split('x');
        let width = parts
            .next()
            .and_then(|value| value.parse().ok())
            .expect("pane width");
        let height = parts
            .next()
            .and_then(|value| value.parse().ok())
            .expect("pane height");
        (width, height)
    }

    /// Captures the pane with SGR attributes and parses it into a [`Grid`].
    #[must_use]
    pub fn capture(&self) -> Grid {
        let raw = self.tmux_stdout(&["capture-pane", "-e", "-p", "-t", "e2e"]);
        let (width, height) = self.size();
        parse_capture(&raw, width, height)
    }

    fn capture_plain(&self) -> String {
        self.tmux_stdout(&["capture-pane", "-p", "-t", "e2e"])
    }

    /// Waits until the plain capture contains `needle`.
    pub fn wait_for_text(&self, needle: &str, timeout: Duration) -> bool {
        wait_until(timeout, || self.contains_text(needle))
    }

    /// Whether the current plain capture contains `needle`.
    #[must_use]
    pub fn contains_text(&self, needle: &str) -> bool {
        self.capture_plain().contains(needle)
    }

    /// Sends a tmux key name such as `F2`, `Escape`, `PageDown` or `Tab`.
    pub fn send_key(&self, name: &str) {
        self.tmux_stdout(&["send-keys", "-t", "e2e", name]);
        std::thread::sleep(Duration::from_millis(20));
    }

    /// Sends literal text to the pane.
    pub fn send_literal(&self, text: &str) {
        self.tmux_stdout(&["send-keys", "-l", "-t", "e2e", text]);
        std::thread::sleep(Duration::from_millis(20));
    }

    /// Presses the left mouse button at the 0-based screen cell `(x, y)`.
    pub fn mouse_down(&self, x: u16, y: u16) {
        self.send_mouse(0, x, y, false);
    }

    /// Releases the left mouse button at `(x, y)`.
    pub fn mouse_up(&self, x: u16, y: u16) {
        self.send_mouse(0, x, y, true);
    }

    /// A left click (press and release) at `(x, y)`.
    pub fn click(&self, x: u16, y: u16) {
        self.mouse_down(x, y);
        self.mouse_up(x, y);
    }

    /// Moves the pointer without a button, which drives hover state.
    pub fn mouse_move(&self, x: u16, y: u16) {
        self.send_mouse(35, x, y, false);
    }

    /// Drags with the left button held from `from` to `to` in `steps` motions.
    pub fn drag(&self, from: (u16, u16), to: (u16, u16), steps: u16) {
        self.mouse_down(from.0, from.1);
        let steps = steps.max(1);
        for step in 1..=steps {
            let dx = i32::from(to.0) - i32::from(from.0);
            let dy = i32::from(to.1) - i32::from(from.1);
            let x = i32::from(from.0) + dx * i32::from(step) / i32::from(steps);
            let y = i32::from(from.1) + dy * i32::from(step) / i32::from(steps);
            let x = x.clamp(0, i32::from(u16::MAX));
            let y = y.clamp(0, i32::from(u16::MAX));
            self.send_mouse(32, x as u16, y as u16, false);
            std::thread::sleep(Duration::from_millis(10));
        }
        self.mouse_up(to.0, to.1);
    }

    /// One mouse-wheel notch upwards at `(x, y)`.
    pub fn wheel_up(&self, x: u16, y: u16) {
        self.send_mouse(64, x, y, false);
    }

    /// One mouse-wheel notch downwards at `(x, y)`.
    pub fn wheel_down(&self, x: u16, y: u16) {
        self.send_mouse(65, x, y, false);
    }

    /// Clicks the first occurrence of `needle` in the current capture.
    ///
    /// Returns `false` when the text is not on screen.
    pub fn click_text(&self, needle: &str) -> bool {
        let Some((x, y)) = self.capture().find_text(needle) else {
            return false;
        };
        self.click(x, y);
        true
    }

    /// Clicks a window title, raising and focusing its window.
    pub fn click_window_title(&self, title: &str) -> bool {
        self.click_text(title)
    }

    /// Reads the hardware cursor via `display -p`, normalising tmux's
    /// one-past-the-right-edge `cursor_x == width` to the last column.
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        let raw = self.tmux_stdout(&[
            "display",
            "-p",
            "-t",
            "e2e",
            "#{cursor_x},#{cursor_y},#{cursor_flag}",
        ]);
        let mut parts = raw.trim().split(',');
        let x: u16 = parts
            .next()
            .and_then(|value| value.parse().ok())
            .expect("cursor_x");
        let y: u16 = parts
            .next()
            .and_then(|value| value.parse().ok())
            .expect("cursor_y");
        let flag = parts.next().unwrap_or("0");
        let (width, height) = self.size();
        Cursor {
            x: x.min(width.saturating_sub(1)),
            y: y.min(height.saturating_sub(1)),
            visible: flag != "0",
        }
    }

    /// Path of the log written by the `xdg-open` stub.
    #[must_use]
    pub fn open_log(&self) -> PathBuf {
        self.scratch.join("opened.log")
    }

    fn send_mouse(&self, button: u8, x: u16, y: u16, release: bool) {
        let final_byte = if release { 'm' } else { 'M' };
        let x_plus = u32::from(x) + 1;
        let y_plus = u32::from(y) + 1;
        let sequence = format!("\x1b[<{button};{x_plus};{y_plus}{final_byte}");
        self.send_literal(&sequence);
    }

    fn tmux_stdout(&self, args: &[&str]) -> String {
        let output = self.tmux(args);
        assert!(
            output.status.success(),
            "tmux {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .env("PATH", &self.session_path)
            .args(args)
            .output()
            .expect("run tmux")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .arg("kill-server")
            .output();
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// Hardware cursor state as reported by tmux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// Column, clamped to the last pane column.
    pub x: u16,
    /// Row, clamped to the last pane row.
    pub y: u16,
    /// `true` when tmux reports the cursor visible.
    pub visible: bool,
}

/// Outcome of a known-bug check.
#[derive(Debug)]
pub enum CheckFailure {
    /// The known bug reproduced — the expected outcome while it stays open.
    Reproduced(String),
    /// The check could not run (missing anchor, window not raised, menu entry
    /// not selectable). Panics instead of counting as a reproduction.
    Setup(String),
}

/// Runs `check` and treats a [`CheckFailure::Reproduced`] as the expected
/// reproduction of `ticket`.
///
/// The suite stays green while the bug exists and fails loudly with promotion
/// instructions once the correct behaviour starts passing. A
/// [`CheckFailure::Setup`] panics: a check that could not run must never pass
/// silently as a reproduction.
pub fn expect_known_bug(ticket: &str, check: impl FnOnce() -> Result<(), CheckFailure>) {
    match check() {
        Err(CheckFailure::Reproduced(reason)) => {
            eprintln!("EXPECTED-FAIL {ticket}: reproduced — {reason}");
        }
        Err(CheckFailure::Setup(reason)) => {
            panic!("{ticket} check could not run (setup failure): {reason}");
        }
        Ok(()) => panic!(
            "{ticket} no longer reproduces — remove expect_known_bug and assert the correct behaviour"
        ),
    }
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

fn unique_scratch() -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("turbo-tui-e2e-{}-{counter}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create e2e scratch dir");
    dir
}

fn build_demo() {
    BUILD_DEMO.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["build", "--example", "demo", "--quiet"])
            .status()
            .expect("run cargo build --example demo");
        assert!(status.success(), "cargo build --example demo failed");
    });
}

fn demo_binary() -> PathBuf {
    // The integration test binary lives in `target/<profile>/deps`, so walking
    // up two parents yields the profile directory. This honours a custom target
    // dir as well as a target triple (`target/<triple>/<profile>`) because the
    // layout below `deps` is fixed.
    let profile_dir = std::env::current_exe()
        .expect("resolve current test executable")
        .parent()
        .and_then(Path::parent)
        .expect("test executable lives in target/<profile>/deps")
        .to_path_buf();
    profile_dir.join("examples").join("demo")
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .expect("stat xdg-open stub")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("chmod xdg-open stub");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
