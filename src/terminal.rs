//! Terminal backend with persistent front and back buffers.
//!
//! [`Terminal`] owns an output writer and renders only the cells that changed
//! since the previous frame. [`TestTerminal`] provides the same persistent
//! buffer diffing in memory so widgets and integration tests can run without a
//! real terminal.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::queue;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Print, SetAttribute, SetBackgroundColor,
    SetForegroundColor, SetUnderlineColor,
};
use crossterm::terminal::{Clear, ClearType};
use unicode_width::UnicodeWidthStr;

use crate::core::{Buffer, Cell, Color, LinkId, LinkTable, Modifier, Position, Rect};
use crate::hyperlink::{sanitized_url, DEFAULT_LINK_SCHEMES};

/// Begins an OSC 8 hyperlink with a stable id parameter: `ESC ] 8 ; id=`.
///
/// The id lets the terminal track a link region exactly instead of grouping
/// every cell that shares the same URI. Windows Terminal does the latter when
/// the id is absent, which merges unrelated links and leaves stale decorations
/// behind when a region moves.
const OSC8_OPEN_ID: &[u8] = b"\x1b]8;id=";
/// Separates the OSC 8 `id` parameter from the URI: `;`.
const OSC8_PARAM_SEPARATOR: &[u8] = b";";
/// Ends an OSC 8 control string: `ESC \`.
const OSC8_ST: &[u8] = b"\x1b\\";
/// Closes an OSC 8 hyperlink: `ESC ] 8 ; ; ESC \`.
const OSC8_CLOSE: &[u8] = b"\x1b]8;;\x1b\\";

/// Optional frame trace sink enabled by `TURBO_TUI_TRACE`.
///
/// Each block is length-prefixed raw output plus a snapshot of the expected back
/// buffer, so a captured trace can be replayed into a VT model and compared
/// frame by frame.
struct TraceFile {
    file: BufWriter<File>,
    frame: u64,
}

/// Opens the trace file named by `TURBO_TUI_TRACE`, if set and non-empty.
fn trace_from_env() -> Option<TraceFile> {
    let path = std::env::var_os("TURBO_TUI_TRACE")?;
    if path.is_empty() {
        return None;
    }
    let file = File::create(&path).ok()?;
    Some(TraceFile {
        file: BufWriter::new(file),
        frame: 0,
    })
}

/// A writer that forwards bytes to the terminal writer and records exactly the
/// bytes that were accepted.
struct TapWriter<'a, W: Write> {
    inner: &'a mut W,
    tap: &'a mut Vec<u8>,
}

impl<W: Write> Write for TapWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.tap.extend_from_slice(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Writes an OSC 8 hyperlink opener carrying the link's stable id.
///
/// The emitted sequence is `ESC ] 8 ; id=<id> ; <url> ESC \`.
fn write_link_open<W: Write>(writer: &mut W, id: LinkId, url: &str) -> io::Result<()> {
    writer.write_all(OSC8_OPEN_ID)?;
    let id_value = id.get();
    write!(writer, "{id_value}")?;
    writer.write_all(OSC8_PARAM_SEPARATOR)?;
    writer.write_all(url.as_bytes())?;
    writer.write_all(OSC8_ST)
}

/// Maps a turbo-tui [`Color`] to the equivalent crossterm color.
///
/// The named colors follow a consistent ANSI mapping: turbo-tui's
/// `Red` is the normal ANSI red, which crossterm calls `DarkRed`.
fn map_color(color: Color) -> CrosstermColor {
    match color {
        Color::Reset => CrosstermColor::Reset,
        Color::Black => CrosstermColor::Black,
        Color::Red => CrosstermColor::DarkRed,
        Color::Green => CrosstermColor::DarkGreen,
        Color::Yellow => CrosstermColor::DarkYellow,
        Color::Blue => CrosstermColor::DarkBlue,
        Color::Magenta => CrosstermColor::DarkMagenta,
        Color::Cyan => CrosstermColor::DarkCyan,
        Color::Gray => CrosstermColor::Grey,
        Color::DarkGray => CrosstermColor::DarkGrey,
        Color::LightRed => CrosstermColor::Red,
        Color::LightGreen => CrosstermColor::Green,
        Color::LightYellow => CrosstermColor::Yellow,
        Color::LightBlue => CrosstermColor::Blue,
        Color::LightMagenta => CrosstermColor::Magenta,
        Color::LightCyan => CrosstermColor::Cyan,
        Color::White => CrosstermColor::White,
        Color::Rgb(r, g, b) => CrosstermColor::Rgb { r, g, b },
        Color::Indexed(index) => CrosstermColor::AnsiValue(index),
    }
}

/// Returns the row-major `(x, y)` positions of the cells that changed between
/// `front` and `back`.
///
/// Buffer diffing: cells hidden behind a preceding
/// multi-width grapheme are skipped, cells invalidated by a multi-width symbol
/// change are re-emitted, and VS16 emoji presentation sequences get explicit
/// trailing-cell clears.
fn diff_updates(front: &Buffer, back: &Buffer) -> Vec<(u16, u16)> {
    let mut updates = Vec::new();
    let mut invalidated: usize = 0;
    let mut to_skip: usize = 0;

    let area = back.area;
    for (i, (current, previous)) in back.content.iter().zip(front.content.iter()).enumerate() {
        if !current.skip && (current != previous || invalidated > 0) && to_skip == 0 {
            updates.push(position_of(area, i));

            // Some terminals do not reliably clear the trailing cell(s) of a wide
            // grapheme. Emit explicit updates for the trailing cells of emoji
            // presentation sequences (VS16 / U+FE0F); standard wide characters
            // such as CJK are cleared by the terminal itself.
            let symbol = current.symbol();
            let cell_width = symbol.width();
            let contains_vs16 = symbol.chars().any(|c| c == '\u{FE0F}');
            if cell_width > 1 && contains_vs16 {
                for k in 1..cell_width {
                    let j = i + k;
                    if j >= back.content.len() || j >= front.content.len() {
                        break;
                    }
                    let previous_trailing = &front.content[j];
                    let next_trailing = &back.content[j];
                    if !next_trailing.skip && previous_trailing != next_trailing {
                        updates.push(position_of(area, j));
                    }
                }
            }
        }

        to_skip = current.symbol().width().saturating_sub(1);
        let affected_width = current.symbol().width().max(previous.symbol().width());
        invalidated = affected_width.max(invalidated).saturating_sub(1);
    }

    updates
}

/// Maps a linear buffer index to its `(x, y)` position within `area`.
fn position_of(area: Rect, index: usize) -> (u16, u16) {
    let width = usize::from(area.width);
    if width == 0 {
        return (area.x, area.y);
    }
    let column = u16::try_from(index % width).unwrap_or_default();
    let row = u16::try_from(index / width).unwrap_or_default();
    (area.x.saturating_add(column), area.y.saturating_add(row))
}

/// Queues the escape sequences that draw a single cell.
///
/// The attribute reset comes first because crossterm's reset clears the colors
/// as well; colors and modifiers are applied afterwards.
///
/// `underline` controls whether an `UNDERLINED` modifier is emitted as SGR 4.
/// Hyperlink-aware emission passes `false` for cells that are actually wrapped
/// in OSC 8, so the terminal's own link decoration is the only underline (see
/// [`Terminal::set_hyperlinks`]).
fn queue_cell<W: Write>(writer: &mut W, cell: &Cell, underline: bool) -> io::Result<()> {
    queue!(
        writer,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(map_color(cell.fg)),
        SetBackgroundColor(map_color(cell.bg)),
        SetUnderlineColor(map_color(cell.underline_color)),
    )?;
    if cell.modifier.contains(Modifier::BOLD) {
        queue!(writer, SetAttribute(Attribute::Bold))?;
    }
    if cell.modifier.contains(Modifier::DIM) {
        queue!(writer, SetAttribute(Attribute::Dim))?;
    }
    if cell.modifier.contains(Modifier::ITALIC) {
        queue!(writer, SetAttribute(Attribute::Italic))?;
    }
    if underline && cell.modifier.contains(Modifier::UNDERLINED) {
        queue!(writer, SetAttribute(Attribute::Underlined))?;
    }
    if cell.modifier.contains(Modifier::SLOW_BLINK) {
        queue!(writer, SetAttribute(Attribute::SlowBlink))?;
    }
    if cell.modifier.contains(Modifier::RAPID_BLINK) {
        queue!(writer, SetAttribute(Attribute::RapidBlink))?;
    }
    if cell.modifier.contains(Modifier::REVERSED) {
        queue!(writer, SetAttribute(Attribute::Reverse))?;
    }
    if cell.modifier.contains(Modifier::HIDDEN) {
        queue!(writer, SetAttribute(Attribute::Hidden))?;
    }
    if cell.modifier.contains(Modifier::CROSSED_OUT) {
        queue!(writer, SetAttribute(Attribute::CrossedOut))?;
    }
    let symbol = if cell.symbol().is_empty() {
        " "
    } else {
        cell.symbol()
    };
    queue!(writer, Print(symbol))
}

/// Resolves the OSC 8 URL for a cell, if it has a link that is present in the
/// table and passes URL sanitisation.
fn resolve_cell_link(
    links: &LinkTable,
    allowed_schemes: &[String],
    cell: &Cell,
) -> Option<(LinkId, String)> {
    let id = cell.link()?;
    let link = links.get(id)?;
    let url = sanitized_url(link, allowed_schemes)?;
    Some((id, url))
}

/// Emits the cell updates between `front` and `back` to `writer`.
///
/// Shared by [`Terminal`] and [`TestTerminal`] so both exercise the same diff
/// and OSC 8 code path.
fn emit_updates<W: Write>(
    writer: &mut W,
    front: &Buffer,
    back: &Buffer,
    hyperlinks: bool,
    allowed_schemes: &[String],
) -> io::Result<()> {
    if hyperlinks {
        emit_linked_updates(writer, front, back, allowed_schemes)
    } else {
        for (x, y) in diff_updates(front, back) {
            queue!(writer, MoveTo(x, y))?;
            queue_cell(writer, &back[(x, y)], true)?;
        }
        Ok(())
    }
}

/// Emits the diff with run-aware OSC 8 hyperlink wrapping.
///
/// A run continues while updates share a row, are horizontally adjacent, share
/// the same link id, and the previous and current cells are ASCII single-width.
/// Everything else is addressed with an explicit `MoveTo`: plain cells are
/// always positioned absolutely, and non-ASCII or wide cells break the run,
/// because the terminal's display width for ambiguous-width glyphs (and VS16
/// trailing-cell updates) may differ from the buffer's width. Outside a run the
/// active link is closed *before* the cursor moves. The link is always closed
/// after the last update, so a frame never ends with an open hyperlink.
///
/// Linked cells that are actually wrapped in OSC 8 drop their SGR underline; the
/// terminal's own link decoration is the only underline. See
/// [`Terminal::set_hyperlinks`] for the policy.
fn emit_linked_updates<W: Write>(
    writer: &mut W,
    front: &Buffer,
    back: &Buffer,
    allowed_schemes: &[String],
) -> io::Result<()> {
    let mut active: Option<LinkId> = None;
    let mut prev: Option<(u16, u16)> = None;
    let mut prev_safe = false;

    for (x, y) in diff_updates(front, back) {
        let cell = &back[(x, y)];
        let resolved = resolve_cell_link(&back.links, allowed_schemes, cell);
        let target = resolved.as_ref().map(|(id, _)| *id);

        // The cursor-advance shortcut is only safe for an adjacent ASCII
        // single-width linked run. Plain cells are always addressed absolutely,
        // and any non-ASCII or wide cell breaks the run: terminals may render
        // an "ambiguous width" glyph (e.g. an em dash) wider than the buffer
        // does, and VS16 trailing-cell updates must not be treated as
        // contiguous with the wide grapheme that produced them.
        let current_safe = cell.symbol().is_ascii() && cell.symbol().width() == 1;
        let contiguous = target.is_some()
            && target == active
            && prev_safe
            && current_safe
            && prev.is_some_and(|(px, py)| py == y && px.saturating_add(1) == x);

        if !contiguous {
            if active.is_some() {
                writer.write_all(OSC8_CLOSE)?;
                active = None;
            }
            queue!(writer, MoveTo(x, y))?;
        }

        if target != active {
            if active.is_some() {
                writer.write_all(OSC8_CLOSE)?;
                active = None;
            }
            if let Some((id, url)) = &resolved {
                write_link_open(writer, *id, url)?;
                active = Some(*id);
            }
        }

        queue_cell(writer, cell, resolved.is_none())?;
        prev = Some((x, y));
        prev_safe = current_safe;
    }

    if active.is_some() {
        writer.write_all(OSC8_CLOSE)?;
    }

    Ok(())
}

/// A terminal that renders into persistent front and back buffers.
///
/// The back buffer is handed to the draw callback and keeps its contents
/// between frames, so callers that only update a small region do not have to
/// repaint the whole screen.
pub struct Terminal<W: Write> {
    writer: W,
    width: u16,
    height: u16,
    front: Buffer,
    back: Buffer,
    cursor: Option<Position>,
    cursor_visible: bool,
    hyperlinks: bool,
    allowed_schemes: Vec<String>,
    trace: Option<TraceFile>,
}

impl<W: Write> Terminal<W> {
    /// Creates a terminal with empty front and back buffers.
    ///
    /// OSC 8 hyperlink emission is off by default; enable it with
    /// [`set_hyperlinks`](Terminal::set_hyperlinks) or
    /// [`with_hyperlinks`](Terminal::with_hyperlinks) once
    /// [`HyperlinkSupport`](crate::hyperlink::HyperlinkSupport) reports support.
    ///
    /// Set `TURBO_TUI_TRACE=<file>` to write every frame's terminal bytes plus a
    /// back-buffer snapshot to that file. The file can be replayed with the
    /// `test_trace_replay_*` VT-model test.
    #[must_use]
    pub fn new(writer: W, width: u16, height: u16) -> Self {
        let area = Rect::new(0, 0, width, height);
        Self {
            writer,
            width,
            height,
            front: Buffer::empty(area),
            back: Buffer::empty(area),
            cursor: None,
            cursor_visible: true,
            hyperlinks: false,
            allowed_schemes: DEFAULT_LINK_SCHEMES
                .iter()
                .copied()
                .map(str::to_owned)
                .collect(),
            trace: trace_from_env(),
        }
    }

    /// Enables tracing to `path`, replacing any trace opened from the
    /// environment.
    ///
    /// # Errors
    ///
    /// Returns any error from creating the trace file.
    pub fn set_trace_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = File::create(path)?;
        self.trace = Some(TraceFile {
            file: BufWriter::new(file),
            frame: 0,
        });
        Ok(())
    }

    /// Disables frame tracing.
    pub fn clear_trace(&mut self) {
        self.trace = None;
    }

    /// Returns whether frame tracing is active.
    #[must_use]
    pub const fn tracing(&self) -> bool {
        self.trace.is_some()
    }

    /// Returns the terminal size as `(width, height)`.
    #[must_use]
    pub const fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Returns whether OSC 8 hyperlink emission is enabled.
    #[must_use]
    pub const fn hyperlinks(&self) -> bool {
        self.hyperlinks
    }

    /// Enables or disables OSC 8 hyperlink emission.
    ///
    /// When enabled, every emitted link is opened as
    /// `ESC ] 8 ; id=<id> ; <url> ESC \` using the buffer's stable link id, is
    /// closed before each cursor move, and is closed again at the end of every
    /// frame. The id lets terminals track a link region exactly instead of
    /// grouping all cells that share a URI.
    ///
    /// # Underline policy
    ///
    /// A cell that is actually wrapped in OSC 8 does **not** emit its
    /// `UNDERLINED` modifier as SGR 4. Terminals such as Windows Terminal draw
    /// their own hyperlink decoration; emitting SGR 4 as well produces a second
    /// line that can survive redraws as an artefact. Emitted links therefore
    /// rely on the terminal decoration plus the theme foreground colour.
    ///
    /// Links that are not emitted — hyperlinks disabled, non-URL targets, or a
    /// URL rejected by the scheme allowlist — keep the theme `link*` pens,
    /// including their SGR underline.
    ///
    /// # Positioning
    ///
    /// Plain, non-ASCII and wide cells are always positioned with an explicit
    /// cursor move; the cursor-advance shortcut is used only for adjacent ASCII
    /// single-width linked cells, so an ambiguous-width glyph (such as the em
    /// dash) rendered wider by the terminal cannot shift the following text.
    ///
    /// The consumer should pass
    /// [`Application::hyperlink_support`](crate::application::Application::hyperlink_support)'s
    /// `enabled` flag here.
    pub fn set_hyperlinks(&mut self, enabled: bool) {
        self.hyperlinks = enabled;
    }

    /// Enables or disables OSC 8 hyperlink emission (Builder Lite).
    #[must_use]
    pub fn with_hyperlinks(mut self, enabled: bool) -> Self {
        self.hyperlinks = enabled;
        self
    }

    /// Sets the schemes allowed in emitted hyperlinks.
    ///
    /// Comparison is ASCII case-insensitive. Defaults to
    /// [`DEFAULT_LINK_SCHEMES`](crate::hyperlink::DEFAULT_LINK_SCHEMES).
    pub fn set_allowed_link_schemes(&mut self, schemes: &[&str]) {
        self.allowed_schemes = schemes.iter().copied().map(str::to_owned).collect();
    }

    /// Returns the schemes allowed in emitted hyperlinks.
    #[must_use]
    pub fn allowed_link_schemes(&self) -> &[String] {
        &self.allowed_schemes
    }

    /// Returns a reference to the underlying writer.
    #[must_use]
    pub const fn writer(&self) -> &W {
        &self.writer
    }

    /// Renders one frame and flushes the changed cells to the writer.
    ///
    /// The callback receives the persistent back buffer, which is **not**
    /// cleared between frames. The returned position becomes the cursor
    /// position for this frame.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Buffer) -> Option<Position>,
    {
        let cursor = render(&mut self.back);
        self.set_cursor(cursor);
        self.flush()
    }

    /// Writes one trace block: raw emitted bytes plus the expected back buffer.
    fn trace_block(&mut self, label: &str, bytes: &[u8]) -> io::Result<()> {
        let Some(trace) = self.trace.as_mut() else {
            return Ok(());
        };
        if trace.frame == 0 {
            writeln!(trace.file, "TURBO_TUI_TRACE 1")?;
        }
        trace.frame += 1;
        let frame = trace.frame;
        let (width, height) = (self.width, self.height);
        writeln!(
            trace.file,
            "=== turbo-tui frame {frame} {label} {} bytes {width}x{height} ===",
            bytes.len()
        )?;
        trace.file.write_all(bytes)?;
        writeln!(trace.file)?;

        writeln!(trace.file, "--- back ---")?;
        for y in 0..height {
            for x in 0..width {
                if x > 0 {
                    trace.file.write_all(b"\t")?;
                }
                trace
                    .file
                    .write_all(self.back[(x, y)].symbol().as_bytes())?;
            }
            writeln!(trace.file)?;
        }

        writeln!(trace.file, "--- mods ---")?;
        for y in 0..height {
            for x in 0..width {
                if x > 0 {
                    trace.file.write_all(b" ")?;
                }
                write!(trace.file, "{:04x}", self.back[(x, y)].modifier.bits())?;
            }
            writeln!(trace.file)?;
        }

        writeln!(trace.file, "--- links ---")?;
        for y in 0..height {
            for x in 0..width {
                if x > 0 {
                    trace.file.write_all(b" ")?;
                }
                let resolved = if self.hyperlinks {
                    resolve_cell_link(&self.back.links, &self.allowed_schemes, &self.back[(x, y)])
                } else {
                    None
                };
                match resolved {
                    Some((id, _)) => write!(trace.file, "{}", id.get())?,
                    None => trace.file.write_all(b"-")?,
                }
            }
            writeln!(trace.file)?;
        }

        writeln!(trace.file, "=== end frame {frame} ===")?;
        trace.file.flush()
    }

    /// Emits the cells that differ between the back and the front buffer.
    ///
    /// When hyperlinks are enabled, linked cells are wrapped in OSC 8
    /// sequences; otherwise the output is byte-identical to a plain redraw.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        let mut frame = Vec::new();
        {
            let mut writer = TapWriter {
                inner: &mut self.writer,
                tap: &mut frame,
            };
            emit_updates(
                &mut writer,
                &self.front,
                &self.back,
                self.hyperlinks,
                &self.allowed_schemes,
            )?;
            match self.cursor {
                Some(position) => {
                    queue!(&mut writer, MoveTo(position.x, position.y))?;
                    if !self.cursor_visible {
                        queue!(&mut writer, Show)?;
                        self.cursor_visible = true;
                    }
                }
                None => {
                    if self.cursor_visible {
                        queue!(&mut writer, Hide)?;
                        self.cursor_visible = false;
                    }
                }
            }
            writer.inner.flush()?;
        }
        self.front = self.back.clone();
        self.trace_block("flush", &frame)
    }

    /// Clears the screen and resets both buffers.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn clear(&mut self) -> io::Result<()> {
        let mut frame = Vec::new();
        {
            let mut writer = TapWriter {
                inner: &mut self.writer,
                tap: &mut frame,
            };
            queue!(&mut writer, Clear(ClearType::All))?;
            writer.inner.flush()?;
        }
        let area = Rect::new(0, 0, self.width, self.height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        self.trace_block("clear", &frame)
    }

    /// Resizes both buffers and forces a full repaint on the next frame.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.width = width;
        self.height = height;
        let area = Rect::new(0, 0, width, height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        let mut frame = Vec::new();
        {
            let mut writer = TapWriter {
                inner: &mut self.writer,
                tap: &mut frame,
            };
            queue!(&mut writer, Clear(ClearType::All))?;
            writer.inner.flush()?;
        }
        self.trace_block("resize", &frame)
    }

    /// Shows the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        let mut frame = Vec::new();
        {
            let mut writer = TapWriter {
                inner: &mut self.writer,
                tap: &mut frame,
            };
            queue!(&mut writer, Show)?;
            writer.inner.flush()?;
        }
        self.cursor_visible = true;
        self.trace_block("cursor", &frame)
    }

    /// Hides the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        let mut frame = Vec::new();
        {
            let mut writer = TapWriter {
                inner: &mut self.writer,
                tap: &mut frame,
            };
            queue!(&mut writer, Hide)?;
            writer.inner.flush()?;
        }
        self.cursor_visible = false;
        self.trace_block("cursor", &frame)
    }

    /// Stores the cursor position requested by the render callback.
    const fn set_cursor(&mut self, cursor: Option<Position>) {
        self.cursor = cursor;
    }
}

/// An in-memory terminal used by tests instead of a real backend.
///
/// It keeps persistent front and back buffers and performs the same cell-level
/// diff (and OSC 8 emission) as [`Terminal`]. Emitted escape bytes are recorded
/// in [`TestTerminal::output`] instead of being written to a real terminal, so
/// tests can assert on the exact sequence.
pub struct TestTerminal {
    width: u16,
    height: u16,
    front: Buffer,
    back: Buffer,
    cursor: Option<Position>,
    changed: usize,
    output: Vec<u8>,
    hyperlinks: bool,
    allowed_schemes: Vec<String>,
}

impl TestTerminal {
    /// Creates a test terminal with empty front and back buffers.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let area = Rect::new(0, 0, width, height);
        Self {
            width,
            height,
            front: Buffer::empty(area),
            back: Buffer::empty(area),
            cursor: None,
            changed: 0,
            output: Vec::new(),
            hyperlinks: false,
            allowed_schemes: DEFAULT_LINK_SCHEMES
                .iter()
                .copied()
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Enables or disables OSC 8 hyperlink emission.
    pub fn set_hyperlinks(&mut self, enabled: bool) {
        self.hyperlinks = enabled;
    }

    /// Enables or disables OSC 8 hyperlink emission (Builder Lite).
    #[must_use]
    pub fn with_hyperlinks(mut self, enabled: bool) -> Self {
        self.hyperlinks = enabled;
        self
    }

    /// Returns the escape bytes emitted by all draws so far.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Discards the recorded output without touching the buffers.
    pub fn clear_output(&mut self) {
        self.output.clear();
    }

    /// Renders one frame into the back buffer and records the emitted bytes.
    ///
    /// # Panics
    ///
    /// Panics only if writing to the in-memory `Vec` fails, which cannot happen
    /// in practice.
    pub fn draw<F>(&mut self, render: F)
    where
        F: FnOnce(&mut Buffer) -> Option<Position>,
    {
        let cursor = render(&mut self.back);
        let updates = diff_updates(&self.front, &self.back);
        self.changed = updates.len();
        emit_updates(
            &mut self.output,
            &self.front,
            &self.back,
            self.hyperlinks,
            &self.allowed_schemes,
        )
        .expect("writing to an in-memory Vec cannot fail");
        self.front = self.back.clone();
        self.cursor = cursor;
    }

    /// Returns the rendered screen (the front buffer).
    #[must_use]
    pub const fn buffer(&self) -> &Buffer {
        &self.front
    }

    /// Returns the cursor position stored by the last [`TestTerminal::draw`].
    #[must_use]
    pub const fn cursor(&self) -> Option<Position> {
        self.cursor
    }

    /// Returns the number of cells that changed in the last frame.
    #[must_use]
    pub const fn last_changed_cells(&self) -> usize {
        self.changed
    }

    /// Recreates both buffers at the new size, discarding their contents.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let area = Rect::new(0, 0, width, height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        self.changed = 0;
        self.output.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_first_draw_writes_all_nonblank_cells() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "AB", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains('A'));
        assert!(output.contains('B'));
    }

    #[test]
    fn test_terminal_identical_redraw_writes_nothing() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        let render = |buffer: &mut Buffer| {
            buffer.set_string(0, 0, "AB", crate::core::Style::default());
            None
        };
        terminal.draw(render).expect("first draw succeeds");
        let after_first = terminal.writer().len();

        terminal.draw(render).expect("second draw succeeds");
        assert_eq!(terminal.writer().len(), after_first);
    }

    #[test]
    fn test_terminal_changed_cell_writes_only_that_cell() {
        let mut terminal = Terminal::new(Vec::new(), 6, 2);
        terminal
            .draw(|buffer| {
                for y in 0..2 {
                    buffer.set_string(0, y, "AAAAAA", crate::core::Style::default());
                }
                None
            })
            .expect("first draw succeeds");
        let full_frame_len = terminal.writer().len();

        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "Z", crate::core::Style::default());
                None
            })
            .expect("second draw succeeds");

        let delta = &terminal.writer()[full_frame_len..];
        let delta = String::from_utf8_lossy(delta);
        assert!(delta.contains('Z'));
        assert!(!delta.contains('A'), "unchanged cells were re-emitted");
        assert!(
            delta.len() < full_frame_len,
            "dirty frame should be smaller than a full frame"
        );
    }

    #[test]
    fn test_test_terminal_dirty_redraw_leaves_rest_intact() {
        let mut terminal = TestTerminal::new(12, 3);
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "HELLO", crate::core::Style::default());
            buffer.set_string(5, 1, "X", crate::core::Style::default());
            None
        });

        terminal.draw(|buffer| {
            buffer.set_string(1, 0, "A", crate::core::Style::default());
            None
        });

        let buffer = terminal.buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "H");
        assert_eq!(buffer[(1, 0)].symbol(), "A");
        assert_eq!(buffer[(2, 0)].symbol(), "L");
        assert_eq!(buffer[(5, 1)].symbol(), "X", "untouched cell was lost");
        assert_eq!(buffer[(11, 2)].symbol(), " ");
    }

    #[test]
    fn test_test_terminal_changed_cell_count() {
        let mut terminal = TestTerminal::new(4, 1);
        let render = |buffer: &mut Buffer| {
            buffer.set_string(0, 0, "AB", crate::core::Style::default());
            None
        };
        terminal.draw(render);
        assert_eq!(terminal.last_changed_cells(), 2);

        terminal.draw(render);
        assert_eq!(terminal.last_changed_cells(), 0);

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "X", crate::core::Style::default());
            None
        });
        assert_eq!(terminal.last_changed_cells(), 1);
    }

    #[test]
    fn test_terminal_resize_clears() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "AB", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");
        let before = terminal.writer().len();

        terminal.resize(6, 2).expect("resize succeeds");

        assert_eq!(terminal.size(), (6, 2));
        assert_eq!(terminal.front.content.len(), 12);
        assert_eq!(terminal.back.content.len(), 12);
        assert!(terminal
            .front
            .content
            .iter()
            .all(|cell| *cell == Cell::default()));
        assert!(terminal
            .back
            .content
            .iter()
            .all(|cell| *cell == Cell::default()));
        let written = &terminal.writer()[before..];
        assert!(
            written.windows(4).any(|window| window == b"\x1b[2J"),
            "resize should queue a full clear"
        );
    }

    #[test]
    fn test_cursor_position_is_tracked() {
        let mut terminal = TestTerminal::new(10, 10);
        terminal.draw(|_buffer| Some(Position::new(3, 4)));
        assert_eq!(terminal.cursor(), Some(Position::new(3, 4)));

        terminal.draw(|_buffer| None);
        assert_eq!(terminal.cursor(), None);
    }

    #[test]
    fn test_wide_char_diff() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "世", crate::core::Style::default());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(
            output.matches('世').count(),
            1,
            "wide grapheme must be emitted exactly once"
        );
    }

    #[test]
    fn test_wide_char_does_not_print_continuation() {
        let mut terminal = TestTerminal::new(4, 1);
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "ab", crate::core::Style::default());
            None
        });
        assert_eq!(terminal.last_changed_cells(), 2);

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "世", crate::core::Style::default());
            None
        });
        assert_eq!(
            terminal.last_changed_cells(),
            1,
            "the continuation cell must not be counted or printed"
        );

        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "ab", crate::core::Style::default());
            None
        });
        assert_eq!(
            terminal.last_changed_cells(),
            2,
            "the trailing cell of the previous wide char must be re-emitted"
        );
    }

    #[test]
    fn test_wide_char_over_previous_content_terminal() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "ab", crate::core::Style::default());
                None
            })
            .expect("first draw succeeds");
        let after_first = terminal.writer().len();

        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "世", crate::core::Style::default());
                None
            })
            .expect("second draw succeeds");

        let delta = &terminal.writer()[after_first..];
        let delta = String::from_utf8_lossy(delta);
        assert!(delta.contains('世'), "wide grapheme was not printed");
        assert_eq!(
            delta.matches(' ').count(),
            0,
            "the continuation column must not be printed as a space: {delta:?}"
        );
    }

    // -------------------------------------------------------------------------
    // OSC 8 hyperlinks
    // -------------------------------------------------------------------------

    use crate::core::{Link, LinkTarget, Modifier, Style};

    fn url_link(url: &str) -> Link {
        Link::new(LinkTarget::Url(url.to_owned()))
    }

    /// Counts complete OSC 8 close sequences (`ESC]8;;ESC\`).
    fn close_count(output: &str) -> usize {
        output.matches("\x1b]8;;\x1b\\").count()
    }

    /// Counts OSC 8 openers that carry a stable `id` parameter (`ESC]8;id=`).
    fn open_count(output: &str) -> usize {
        output.matches("\x1b]8;id=").count()
    }

    /// Returns the offset of the OSC 8 opener for `url`.
    fn open_pos(output: &str, url: &str) -> usize {
        output
            .find(&format!(";{url}\x1b\\"))
            .unwrap_or_else(|| panic!("open sequence for {url} not found"))
    }

    /// Scans emitted bytes and enforces the OSC 8 hygiene contract:
    ///
    /// - every cursor move (`CUP`/`MoveTo`, final byte `H`) happens with no
    ///   hyperlink open,
    /// - the frame ends with no hyperlink open.
    fn assert_osc8_hygiene(frame: &[u8]) {
        let mut index = 0;
        let mut open = false;
        while index < frame.len() {
            if frame[index..].starts_with(OSC8_CLOSE) {
                assert!(open, "OSC 8 close without an open hyperlink");
                open = false;
                index += OSC8_CLOSE.len();
            } else if frame[index..].starts_with(OSC8_OPEN_ID) {
                let rest = &frame[index + OSC8_OPEN_ID.len()..];
                let end = rest
                    .windows(2)
                    .position(|window| window == b"\x1b\\")
                    .expect("unterminated OSC 8 opener");
                open = true;
                index += OSC8_OPEN_ID.len() + end + 2;
            } else if frame[index..].starts_with(b"\x1b[") {
                let mut end = index + 2;
                while end < frame.len() && !(0x40..=0x7e).contains(&frame[end]) {
                    end += 1;
                }
                assert!(end < frame.len(), "unterminated CSI sequence");
                if frame[end] == b'H' {
                    assert!(!open, "MoveTo while a hyperlink is open");
                }
                index = end + 1;
            } else {
                index += 1;
            }
        }
        assert!(!open, "frame ended with an open hyperlink");
    }

    #[test]
    fn test_terminal_hyperlinks_off_by_default() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                buffer[(1, 0)].set_symbol("B").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(open_count(&output), 0, "hyperlinks must be off");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
    }

    #[test]
    fn test_terminal_hyperlinks_adjacent_same_link_one_open_one_close() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                buffer[(1, 0)].set_symbol("B").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(close_count(&output), 1, "exactly one close");
        assert_eq!(open_count(&output), 1, "one open sequence");
        assert!(output.contains("https://example.com"));
        let open = open_pos(&output, "https://example.com");
        let a = output.find('A').expect("first linked cell");
        let b = output.find('B').expect("second linked cell");
        let close = output.find("\x1b]8;;\x1b\\").expect("close sequence");
        assert!(
            open < a && a < b && b < close,
            "the run must open before the text and close after it"
        );
    }

    #[test]
    fn test_terminal_hyperlinks_adjacent_different_links_close_and_reopen() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let a = buffer.links.insert(&url_link("https://example.com/a"));
                let b = buffer.links.insert(&url_link("https://example.com/b"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(a));
                buffer[(1, 0)].set_symbol("B").set_link(Some(b));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(close_count(&output), 2, "each link is closed");
        assert_eq!(open_count(&output), 2, "two open sequences");
        assert!(output.contains("https://example.com/a"));
        assert!(output.contains("https://example.com/b"));
        let open_a = open_pos(&output, "https://example.com/a");
        let a = output.find('A').expect("first cell");
        let close = output.find("\x1b]8;;\x1b\\").expect("first close");
        let open_b = open_pos(&output, "https://example.com/b");
        let b = output.find('B').expect("second cell");
        assert!(
            open_a < a && a < close && close < open_b && open_b < b,
            "the first link must close before the second opens"
        );
    }

    #[test]
    fn test_terminal_hyperlinks_single_cell_run() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(1, 0)].set_symbol("X").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(close_count(&output), 1);
        assert_eq!(open_count(&output), 1);
        let open = open_pos(&output, "https://example.com");
        let x = output.find('X').expect("linked cell");
        let close = output.find("\x1b]8;;\x1b\\").expect("close sequence");
        assert!(open < x && x < close);
    }

    #[test]
    fn test_terminal_hyperlinks_row_wrap_reopens() {
        let mut terminal = Terminal::new(Vec::new(), 3, 2).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(2, 0)].set_symbol("A").set_link(Some(id));
                buffer[(0, 1)].set_symbol("B").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(
            close_count(&output),
            2,
            "a line break is not a contiguous run"
        );
        assert_eq!(open_count(&output), 2, "close then reopen across the row");
        let a = output.find('A').expect("first cell");
        let b = output.find('B').expect("second cell");
        assert!(a < b, "row-major order is preserved");
    }

    #[test]
    fn test_terminal_hyperlinks_unlinked_cell_closes_and_reopens() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                buffer[(1, 0)].set_symbol("B");
                buffer[(2, 0)].set_symbol("C").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(close_count(&output), 2, "close before and after the gap");
        assert_eq!(open_count(&output), 2, "reopened around the gap");
        let first_open = open_pos(&output, "https://example.com");
        let a = output.find('A').expect("first cell");
        let first_close = output.find("\x1b]8;;\x1b\\").expect("close before the gap");
        let b = output.find('B').expect("unlinked cell");
        let second_open = open_pos(&output[b..], "https://example.com") + b;
        let c = output.find('C').expect("re-linked cell");
        assert!(
            first_open < a
                && a < first_close
                && first_close < b
                && b < second_open
                && second_open < c,
            "the link must close before the unlinked cell and reopen after it"
        );
    }

    #[test]
    fn test_terminal_hyperlinks_disallowed_scheme_emits_nothing() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("javascript:alert(1)"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(open_count(&output), 0, "disallowed scheme is skipped");
        assert!(output.contains('A'), "text still renders");
    }

    #[test]
    fn test_terminal_hyperlinks_strip_control_chars() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer
                    .links
                    .insert(&url_link("https://example.com/a\u{9b}b\x07c"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains("https://example.com/abc"));
        assert!(!output.contains('\u{9b}'), "C1 control must be stripped");
        assert!(!output.contains('\x07'), "BEL must be stripped");
    }

    #[test]
    fn test_terminal_hyperlinks_file_scheme_allowed() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("file:///tmp/notes.txt"));
                buffer[(0, 0)].set_symbol("F").set_link(Some(id));
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains("file:///tmp/notes.txt"));
    }

    #[test]
    fn test_terminal_hyperlinks_scheme_allowlist() {
        // http is no longer allowed once the list is replaced with ftp.
        let mut restricted = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        restricted.set_allowed_link_schemes(&["ftp"]);
        assert_eq!(restricted.allowed_link_schemes(), &["ftp".to_owned()]);
        restricted
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("http://example.com"));
                buffer[(0, 0)].set_symbol("H").set_link(Some(id));
                None
            })
            .expect("draw succeeds");
        let output = String::from_utf8_lossy(restricted.writer());
        assert_eq!(open_count(&output), 0, "http must be rejected");
        assert!(output.contains('H'));

        // The same allowlist permits ftp.
        let mut allowed = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        allowed.set_allowed_link_schemes(&["ftp"]);
        allowed
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("ftp://ftp.example.com/file"));
                buffer[(0, 0)].set_symbol("F").set_link(Some(id));
                None
            })
            .expect("draw succeeds");
        let output = String::from_utf8_lossy(allowed.writer());
        assert!(output.contains("ftp://ftp.example.com/file"));
    }

    #[test]
    fn test_terminal_hyperlinks_unresolved_link_id_emits_nothing() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                // Drop the table entry while the cell keeps its id.
                buffer.links.clear();
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(
            open_count(&output),
            0,
            "a cell whose link id is missing from the table emits no OSC 8"
        );
        assert!(output.contains('A'), "text still renders");
    }

    #[test]
    fn test_test_terminal_hyperlinks_open_carries_stable_id() {
        let mut terminal = TestTerminal::new(8, 1).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let id = buffer.links.insert(&url_link("https://example.com"));
            buffer[(0, 0)].set_symbol("A").set_link(Some(id));
            buffer[(2, 0)].set_symbol("B").set_link(Some(id));
            None
        });

        let output = String::from_utf8_lossy(terminal.output());
        assert_eq!(open_count(&output), 2, "a gap reopens the link");
        assert_eq!(
            output.matches("id=0;https://example.com").count(),
            2,
            "the same link keeps the same id across runs"
        );
        assert!(
            !output.contains("\x1b]8;;http"),
            "no opener without an id: {output:?}"
        );
    }

    #[test]
    fn test_test_terminal_hyperlinks_same_uri_different_links_get_distinct_ids() {
        let mut terminal = TestTerminal::new(4, 1).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let plain = buffer.links.insert(&url_link("https://example.com"));
            let titled = buffer
                .links
                .insert(&url_link("https://example.com").with_title("Example"));
            buffer[(0, 0)].set_symbol("A").set_link(Some(plain));
            buffer[(1, 0)].set_symbol("B").set_link(Some(titled));
            None
        });

        let output = String::from_utf8_lossy(terminal.output());
        assert!(output.contains("id=0;https://example.com"));
        assert!(output.contains("id=1;https://example.com"));
        assert_eq!(
            open_count(&output),
            2,
            "distinct ids must not merge into one URI-based region"
        );
    }

    #[test]
    fn test_test_terminal_osc8_close_precedes_every_cursor_move_and_frame_end() {
        let mut terminal = TestTerminal::new(6, 2).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let id = buffer.links.insert(&url_link("https://example.com"));
            buffer[(0, 0)].set_symbol("A").set_link(Some(id));
            buffer[(2, 0)].set_symbol("B").set_link(Some(id));
            buffer[(0, 1)].set_symbol("C").set_link(Some(id));
            None
        });

        assert_osc8_hygiene(terminal.output());
    }

    #[test]
    fn test_terminal_hyperlinks_cursor_move_happens_after_close() {
        let mut terminal = Terminal::new(Vec::new(), 4, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                Some(Position::new(3, 0))
            })
            .expect("draw succeeds");

        assert_osc8_hygiene(terminal.writer());
    }

    #[test]
    fn test_test_terminal_cell_that_loses_link_is_rewritten_outside_hyperlink() {
        let mut terminal = TestTerminal::new(4, 1).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let id = buffer.links.insert(&url_link("https://example.com"));
            buffer[(0, 0)].set_symbol("A").set_link(Some(id));
            None
        });
        terminal.clear_output();

        terminal.draw(|buffer| {
            // Same symbol, but the plain write clears the link.
            buffer[(0, 0)].set_symbol("A");
            None
        });

        let delta = String::from_utf8_lossy(terminal.output()).into_owned();
        assert!(
            delta.contains('A'),
            "the cell that lost its link must be rewritten: {delta:?}"
        );
        assert_eq!(
            open_count(&delta),
            0,
            "the rewrite is outside any hyperlink"
        );
        assert_osc8_hygiene(delta.as_bytes());
    }

    fn underlined() -> Style {
        Style::default().add_modifier(Modifier::UNDERLINED)
    }

    #[test]
    fn test_terminal_linked_cell_drops_sgr_underline_when_osc8_is_emitted() {
        let mut terminal = Terminal::new(Vec::new(), 2, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)]
                    .set_symbol("A")
                    .set_link(Some(id))
                    .set_style(underlined());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains("\x1b]8;id=0;https://example.com"));
        assert!(
            !output.contains("\x1b[4m"),
            "the OSC 8 link must not also emit SGR 4: {output:?}"
        );
    }

    #[test]
    fn test_terminal_linked_cell_keeps_sgr_underline_when_hyperlinks_are_off() {
        let mut terminal = Terminal::new(Vec::new(), 2, 1);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)]
                    .set_symbol("A")
                    .set_link(Some(id))
                    .set_style(underlined());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(open_count(&output), 0);
        assert!(
            output.contains("\x1b[4m"),
            "without OSC 8 the theme underline must be drawn: {output:?}"
        );
    }

    #[test]
    fn test_terminal_unlinked_underlined_cell_keeps_sgr_underline_with_hyperlinks_on() {
        let mut terminal = Terminal::new(Vec::new(), 2, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                buffer[(0, 0)].set_symbol("U").set_style(underlined());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert!(
            output.contains("\x1b[4m"),
            "plain underlined text is not a hyperlink: {output:?}"
        );
    }

    #[test]
    fn test_terminal_link_without_emittable_url_keeps_sgr_underline() {
        let mut terminal = Terminal::new(Vec::new(), 2, 1).with_hyperlinks(true);
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&Link::new(LinkTarget::FilePos {
                    path: "src/lib.rs".to_owned(),
                    line: 1,
                    col: None,
                }));
                buffer[(0, 0)]
                    .set_symbol("F")
                    .set_link(Some(id))
                    .set_style(underlined());
                None
            })
            .expect("draw succeeds");

        let output = String::from_utf8_lossy(terminal.writer());
        assert_eq!(open_count(&output), 0, "non-URL targets emit no OSC 8");
        assert!(
            output.contains("\x1b[4m"),
            "non-emittable links still rely on the theme underline: {output:?}"
        );
    }

    // -- VT screen model ----------------------------------------------------

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct VtLink {
        id: Option<String>,
        url: String,
    }

    /// Minimal VT screen used to check that the emitted bytes, not just the
    /// front buffer, contain the cells the back buffer expects.
    struct VtScreen {
        width: u16,
        height: u16,
        cells: Vec<String>,
        mods: Vec<Modifier>,
        links: Vec<Option<VtLink>>,
        x: u16,
        y: u16,
        modifier: Modifier,
        link: Option<VtLink>,
        /// Simulate a terminal that renders the ambiguous-width em dash as two
        /// cells (Windows Terminal does this in some configurations).
        em_dash_double_width: bool,
    }

    impl VtScreen {
        fn new(width: u16, height: u16) -> Self {
            Self {
                width,
                height,
                cells: vec![" ".to_owned(); usize::from(width) * usize::from(height)],
                mods: vec![Modifier::empty(); usize::from(width) * usize::from(height)],
                links: vec![None; usize::from(width) * usize::from(height)],
                x: 0,
                y: 0,
                modifier: Modifier::empty(),
                link: None,
                em_dash_double_width: false,
            }
        }

        fn with_em_dash_double_width(mut self) -> Self {
            self.em_dash_double_width = true;
            self
        }

        fn apply(&mut self, bytes: &[u8]) {
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == 0x1b {
                    self.apply_escape(bytes, &mut i);
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != 0x1b {
                        i += 1;
                    }
                    let text = std::str::from_utf8(&bytes[start..i])
                        .expect("emitted text must be valid UTF-8");
                    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
                    {
                        self.put(grapheme);
                    }
                }
            }
        }

        fn apply_escape(&mut self, bytes: &[u8], i: &mut usize) {
            *i += 1;
            match bytes.get(*i) {
                Some(b']') => {
                    // OSC: capture the body and require ST for OSC 8.
                    *i += 1;
                    let start = *i;
                    let mut end = *i;
                    let mut st = false;
                    while *i < bytes.len() {
                        if bytes[*i] == 0x07 {
                            end = *i;
                            *i += 1;
                            break;
                        }
                        if bytes[*i] == 0x1b && bytes.get(*i + 1) == Some(&b'\\') {
                            end = *i;
                            st = true;
                            *i += 2;
                            break;
                        }
                        *i += 1;
                    }
                    self.apply_osc(&bytes[start..end], st);
                }
                Some(b'[') => {
                    // CSI: parameters/intermediates until a final byte.
                    *i += 1;
                    let start = *i;
                    while *i < bytes.len() && !(0x40..=0x7e).contains(&bytes[*i]) {
                        *i += 1;
                    }
                    if *i >= bytes.len() {
                        return;
                    }
                    let final_byte = bytes[*i];
                    let parameters = &bytes[start..*i];
                    *i += 1;
                    match final_byte {
                        b'H' | b'f' => self.move_to(parameters),
                        b'm' => self.apply_sgr(parameters),
                        b'J' => self.clear_screen(),
                        _ => {}
                    }
                }
                Some(_) => *i += 1,
                None => {}
            }
        }

        fn apply_osc(&mut self, body: &[u8], st: bool) {
            if !body.starts_with(b"8;") {
                return;
            }
            assert!(st, "OSC 8 must be terminated with ST");
            let rest = &body[2..];
            let Some(separator) = rest.iter().position(|byte| *byte == b';') else {
                panic!("OSC 8 is missing the URI separator");
            };
            let params = &rest[..separator];
            let uri = &rest[separator + 1..];
            if uri.is_empty() {
                self.link = None;
                return;
            }
            let params = std::str::from_utf8(params).expect("OSC 8 params must be UTF-8");
            let id = params
                .split(';')
                .find_map(|param| param.strip_prefix("id="))
                .map(str::to_owned);
            self.link = Some(VtLink {
                id,
                url: std::str::from_utf8(uri)
                    .expect("OSC 8 URI must be UTF-8")
                    .to_owned(),
            });
        }

        fn apply_sgr(&mut self, parameters: &[u8]) {
            let text = std::str::from_utf8(parameters).unwrap_or("");
            let codes: Vec<u16> = if text.is_empty() {
                vec![0]
            } else {
                text.split(';')
                    .map(|part| part.parse::<u16>().unwrap_or(0))
                    .collect()
            };
            let mut index = 0;
            while index < codes.len() {
                match codes[index] {
                    0 => self.modifier = Modifier::empty(),
                    1 => self.modifier.insert(Modifier::BOLD),
                    2 => self.modifier.insert(Modifier::DIM),
                    3 => self.modifier.insert(Modifier::ITALIC),
                    4 => self.modifier.insert(Modifier::UNDERLINED),
                    5 => self.modifier.insert(Modifier::SLOW_BLINK),
                    6 => self.modifier.insert(Modifier::RAPID_BLINK),
                    7 => self.modifier.insert(Modifier::REVERSED),
                    8 => self.modifier.insert(Modifier::HIDDEN),
                    9 => self.modifier.insert(Modifier::CROSSED_OUT),
                    22 => self.modifier.remove(Modifier::BOLD | Modifier::DIM),
                    23 => self.modifier.remove(Modifier::ITALIC),
                    24 => self.modifier.remove(Modifier::UNDERLINED),
                    25 => self
                        .modifier
                        .remove(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK),
                    27 => self.modifier.remove(Modifier::REVERSED),
                    28 => self.modifier.remove(Modifier::HIDDEN),
                    29 => self.modifier.remove(Modifier::CROSSED_OUT),
                    38 | 48 | 58 => match codes.get(index + 1) {
                        Some(5) => index += 2,
                        Some(2) => index += 4,
                        _ => {}
                    },
                    _ => {}
                }
                index += 1;
            }
        }

        fn move_to(&mut self, parameters: &[u8]) {
            let text = std::str::from_utf8(parameters).unwrap_or("");
            let mut parts = text.split(';');
            let row = parts
                .next()
                .and_then(|part| part.parse::<u16>().ok())
                .unwrap_or(1);
            let column = parts
                .next()
                .and_then(|part| part.parse::<u16>().ok())
                .unwrap_or(1);
            self.y = row.saturating_sub(1).min(self.height.saturating_sub(1));
            self.x = column.saturating_sub(1).min(self.width.saturating_sub(1));
        }

        fn clear_screen(&mut self) {
            self.cells.fill(" ".to_owned());
            self.mods.fill(Modifier::empty());
            self.links.fill(None);
        }

        fn put(&mut self, grapheme: &str) {
            if grapheme.contains(char::is_control) {
                return;
            }
            let width = if self.em_dash_double_width && grapheme == "—" {
                2
            } else {
                u16::try_from(unicode_width::UnicodeWidthStr::width(grapheme)).unwrap_or(1)
            };
            if width == 0 {
                return;
            }
            if self.x < self.width && self.y < self.height {
                let index = usize::from(self.y) * usize::from(self.width) + usize::from(self.x);
                self.cells[index] = grapheme.to_owned();
                self.mods[index] = self.modifier;
                self.links[index] = self.link.clone();
                for offset in 1..width {
                    let trailing = index + usize::from(offset);
                    if trailing < self.cells.len() {
                        self.cells[trailing] = " ".to_owned();
                        self.mods[trailing] = Modifier::empty();
                        self.links[trailing] = None;
                    }
                }
            }
            self.x = self.x.saturating_add(width);
        }
    }

    fn screen_for(terminal: &TestTerminal) -> VtScreen {
        let mut screen = VtScreen::new(terminal.width, terminal.height);
        screen.apply(terminal.output());
        screen
    }

    fn expected_vt_link(buffer: &Buffer, x: u16, y: u16, schemes: &[String]) -> Option<VtLink> {
        let id = buffer[(x, y)].link()?;
        let link = buffer.links.get(id)?;
        let url = crate::hyperlink::sanitized_url(link, schemes)?;
        Some(VtLink {
            id: Some(id.get().to_string()),
            url,
        })
    }

    fn assert_screen_matches(terminal: &TestTerminal, screen: &VtScreen) {
        let buffer = terminal.buffer();
        assert_eq!(buffer.area.width, screen.width);
        assert_eq!(buffer.area.height, screen.height);
        let schemes: Vec<String> = DEFAULT_LINK_SCHEMES
            .iter()
            .map(|scheme| (*scheme).to_owned())
            .collect();
        for y in 0..screen.height {
            for x in 0..screen.width {
                let index = usize::from(y) * usize::from(screen.width) + usize::from(x);
                assert_eq!(
                    screen.cells[index],
                    buffer[(x, y)].symbol(),
                    "emitted symbol differs at ({x},{y})"
                );
                let mut expected_modifier = buffer[(x, y)].modifier;
                let expected_link = if terminal.hyperlinks {
                    expected_vt_link(buffer, x, y, &schemes)
                } else {
                    None
                };
                if expected_link.is_some() {
                    // OSC 8 links intentionally drop SGR 4; the terminal draws
                    // its own decoration instead.
                    expected_modifier.remove(Modifier::UNDERLINED);
                }
                assert_eq!(
                    screen.mods[index], expected_modifier,
                    "emitted modifier differs at ({x},{y})"
                );
                assert_eq!(
                    screen.links[index], expected_link,
                    "emitted OSC 8 link differs at ({x},{y})"
                );
            }
        }
    }

    fn draw_plain(terminal: &mut TestTerminal, text: &str) {
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, text, Style::default());
            None
        });
    }

    fn draw_linked(terminal: &mut TestTerminal, text: &str) {
        terminal.draw(|buffer| {
            let id = buffer.links.insert(&url_link("https://example.com"));
            let area = buffer.area;
            crate::clip::set_string_linked(buffer, 0, 0, text, Style::default(), id, area);
            None
        });
    }

    #[test]
    fn test_vt_model_shorter_plain_text_over_longer() {
        let mut terminal = TestTerminal::new(10, 1).with_hyperlinks(true);
        draw_plain(&mut terminal, "ABCDEFGHIJ");
        draw_plain(&mut terminal, "AB");
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_link_over_plain_text() {
        let mut terminal = TestTerminal::new(10, 1).with_hyperlinks(true);
        draw_plain(&mut terminal, "ABCDEFGHIJ");
        draw_linked(&mut terminal, "AB");
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_plain_text_over_link() {
        let mut terminal = TestTerminal::new(10, 1).with_hyperlinks(true);
        draw_linked(&mut terminal, "ABCDEFGHIJ");
        draw_plain(&mut terminal, "AB");
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_wide_characters_over_plain_text() {
        let mut terminal = TestTerminal::new(10, 2).with_hyperlinks(true);
        draw_plain(&mut terminal, "A—B世C");
        draw_plain(&mut terminal, "A—B");
        draw_plain(&mut terminal, "A世B");
        draw_plain(&mut terminal, "AB");
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_wide_linked_run() {
        let mut terminal = TestTerminal::new(10, 1).with_hyperlinks(true);
        terminal.draw(|buffer| {
            buffer.set_string(0, 0, "A世B", Style::default());
            let id = buffer.links.insert(&url_link("https://example.com"));
            buffer[(0, 0)].set_link(Some(id));
            buffer[(3, 0)].set_link(Some(id));
            None
        });
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_links_window_matches_back_buffer() {
        use crate::application::Application;
        use crate::canvas::CellCanvas;
        use crate::core::Rect;
        use crate::hyperlink::HyperlinkMode;
        use crate::link_label::LinkLabel;
        use crate::static_text::StaticText;
        use crate::window::Window;

        let mut app = Application::new(Rect::new(0, 0, 60, 15)).with_hyperlinks(HyperlinkMode::On);
        let mut window = Window::new(Rect::new(2, 2, 46, 10), "Links");
        let url = "https://github.com/four-bytes/turbo-tui";
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 42, 1),
            url,
            Link::new(LinkTarget::Url(url.to_owned())),
        )));
        let issue = LinkTarget::IssueRef {
            repo: Some("four-bytes/turbo-tui".to_owned()),
            number: 54,
        };
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 2, 42, 1),
            "Issue four-bytes/turbo-tui#54",
            Link::new(issue),
        )));
        let file = LinkTarget::FilePos {
            path: "src/lib.rs".to_owned(),
            line: 1,
            col: None,
        };
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 3, 42, 1),
            "File src/lib.rs:1",
            Link::new(file),
        )));
        window.add(Box::new(StaticText::new(
            Rect::new(1, 5, 42, 1),
            "OSC 8: on — Windows Terminal",
        )));
        app.add_window(window);

        let mut terminal = TestTerminal::new(60, 15).with_hyperlinks(true);
        terminal.draw(|buffer| app.draw(&mut CellCanvas::new(buffer)));

        let buffer = terminal.buffer();
        let mut url_row = None;
        let mut support_row = None;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            if row.contains(url) {
                url_row = Some(y);
            }
            if row.contains("OSC 8:") {
                support_row = Some(y);
            }
        }
        assert!(
            url_row.is_some(),
            "the full URL must render contiguously, not be overwritten by sibling labels"
        );
        assert!(support_row.is_some(), "the support line must render");
        assert_ne!(
            url_row, support_row,
            "LinkLabel rows must not collapse onto the same row"
        );

        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_links_window_after_hover_and_drag() {
        use crate::application::Application;
        use crate::canvas::CellCanvas;
        use crate::core::Rect;
        use crate::hyperlink::HyperlinkMode;
        use crate::link_label::LinkLabel;
        use crate::static_text::StaticText;
        use crate::window::Window;
        use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        fn mouse(x: u16, y: u16, kind: MouseEventKind) -> Event {
            Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            })
        }

        let mut app = Application::new(Rect::new(0, 0, 60, 15)).with_hyperlinks(HyperlinkMode::On);
        let mut window = Window::new(Rect::new(2, 2, 46, 10), "Links");
        let url = "https://github.com/four-bytes/turbo-tui";
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 42, 1),
            url,
            Link::new(LinkTarget::Url(url.to_owned())),
        )));
        window.add(Box::new(StaticText::new(
            Rect::new(1, 5, 42, 1),
            "OSC 8: on — Windows Terminal",
        )));
        app.add_window(window);

        let mut terminal = TestTerminal::new(60, 15).with_hyperlinks(true);
        let frame = |app: &mut Application, terminal: &mut TestTerminal| {
            terminal.draw(|buffer| app.draw(&mut CellCanvas::new(buffer)));
            assert_screen_matches(terminal, &screen_for(terminal));
        };

        frame(&mut app, &mut terminal);

        // Hover the URL, then leave it again.
        app.handle_crossterm_events(&[mouse(5, 4, MouseEventKind::Moved)]);
        frame(&mut app, &mut terminal);
        app.handle_crossterm_events(&[mouse(0, 0, MouseEventKind::Moved)]);
        frame(&mut app, &mut terminal);

        // Drag the Links window by its title bar (old area must be cleared).
        app.handle_crossterm_events(&[
            mouse(4, 2, MouseEventKind::Down(MouseButton::Left)),
            mouse(9, 5, MouseEventKind::Drag(MouseButton::Left)),
            mouse(9, 5, MouseEventKind::Up(MouseButton::Left)),
        ]);
        frame(&mut app, &mut terminal);
    }

    #[test]
    fn test_vt_model_clipped_wide_characters() {
        use crate::canvas::{Canvas, CellCanvas};

        let mut terminal = TestTerminal::new(10, 2).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let area = buffer.area;
            let mut canvas = CellCanvas::new(buffer);
            canvas.draw_text(0, 0, "A世B—C", Style::default(), area);
            None
        });
        terminal.draw(|buffer| {
            let area = buffer.area;
            let mut canvas = CellCanvas::new(buffer);
            canvas.draw_text(0, 0, "A世B", Style::default(), area);
            None
        });
        assert_screen_matches(&terminal, &screen_for(&terminal));
    }

    #[test]
    fn test_vt_model_em_dash_rendered_double_width() {
        use crate::canvas::{Canvas, CellCanvas};

        let mut terminal = TestTerminal::new(40, 1).with_hyperlinks(true);
        terminal.draw(|buffer| {
            let area = buffer.area;
            let mut canvas = CellCanvas::new(buffer);
            canvas.draw_text(0, 0, "OSC 8: on — Windows Terminal", Style::default(), area);
            None
        });

        let mut screen = VtScreen::new(40, 1).with_em_dash_double_width();
        screen.apply(terminal.output());
        assert_screen_matches(&terminal, &screen);
    }

    // -- Trace replay -------------------------------------------------------

    struct TraceFrame {
        label: String,
        width: u16,
        height: u16,
        bytes: Vec<u8>,
        back: Vec<String>,
        mods: Vec<u16>,
        links: Vec<Option<String>>,
    }

    fn read_line<'a>(data: &'a [u8], position: &mut usize) -> Option<&'a str> {
        if *position >= data.len() {
            return None;
        }
        let end = data[*position..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| *position + offset)?;
        let line = std::str::from_utf8(&data[*position..end]).ok()?;
        *position = end + 1;
        Some(line)
    }

    fn parse_trace(data: &[u8]) -> Vec<TraceFrame> {
        let mut position = 0;
        assert_eq!(
            read_line(data, &mut position),
            Some("TURBO_TUI_TRACE 1"),
            "trace magic"
        );
        let mut frames = Vec::new();
        while position < data.len() {
            let header = read_line(data, &mut position).expect("frame header");
            if header.is_empty() {
                continue;
            }
            let tokens: Vec<&str> = header.split_whitespace().collect();
            assert_eq!(tokens[0], "===");
            assert_eq!(tokens[1], "turbo-tui");
            assert_eq!(tokens[2], "frame");
            let label = tokens[4].to_owned();
            let length: usize = tokens[5].parse().expect("frame byte length");
            let mut dimensions = tokens[7].split('x');
            let width: u16 = dimensions.next().expect("width").parse().expect("width");
            let height: u16 = dimensions.next().expect("height").parse().expect("height");

            let bytes = data[position..position + length].to_vec();
            position += length;
            assert_eq!(data.get(position), Some(&b'\n'), "frame byte terminator");
            position += 1;

            assert_eq!(read_line(data, &mut position), Some("--- back ---"));
            let mut back = Vec::with_capacity(usize::from(width) * usize::from(height));
            for _ in 0..height {
                let line = read_line(data, &mut position).expect("back row");
                let mut cells: Vec<&str> = line.split('\t').collect();
                while cells.len() < usize::from(width) {
                    cells.push(" ");
                }
                back.extend(
                    cells
                        .into_iter()
                        .take(usize::from(width))
                        .map(str::to_owned),
                );
            }

            assert_eq!(read_line(data, &mut position), Some("--- mods ---"));
            let mut mods = Vec::with_capacity(back.len());
            for _ in 0..height {
                let line = read_line(data, &mut position).expect("modifier row");
                let mut values: Vec<&str> = line.split_whitespace().collect();
                while values.len() < usize::from(width) {
                    values.push("0000");
                }
                for value in values.into_iter().take(usize::from(width)) {
                    mods.push(u16::from_str_radix(value, 16).expect("modifier bits"));
                }
            }

            assert_eq!(read_line(data, &mut position), Some("--- links ---"));
            let mut links = Vec::with_capacity(back.len());
            for _ in 0..height {
                let line = read_line(data, &mut position).expect("link row");
                let mut values: Vec<&str> = line.split_whitespace().collect();
                while values.len() < usize::from(width) {
                    values.push("-");
                }
                for value in values.into_iter().take(usize::from(width)) {
                    links.push((value != "-").then(|| value.to_owned()));
                }
            }

            let end = read_line(data, &mut position).expect("frame end");
            assert!(end.starts_with("=== end frame"), "frame end marker");

            frames.push(TraceFrame {
                label,
                width,
                height,
                bytes,
                back,
                mods,
                links,
            });
        }
        frames
    }

    fn assert_screen_matches_snapshot(screen: &VtScreen, frame: &TraceFrame) {
        assert_eq!(screen.width, frame.width);
        assert_eq!(screen.height, frame.height);
        for index in 0..frame.back.len() {
            assert_eq!(
                screen.cells[index], frame.back[index],
                "symbol differs in frame {} ({}) at cell {index}",
                frame.label, frame.label
            );
            assert_eq!(
                screen.mods[index].bits(),
                frame.mods[index]
                    & if frame.links[index].is_some() {
                        !Modifier::UNDERLINED.bits()
                    } else {
                        u16::MAX
                    },
                "modifier differs in frame {} at cell {index}",
                frame.label
            );
            assert_eq!(
                screen.links[index]
                    .as_ref()
                    .and_then(|link| link.id.clone()),
                frame.links[index],
                "link id differs in frame {} at cell {index}",
                frame.label
            );
        }
    }

    fn temporary_trace_path(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "turbo-tui-trace-{}-{tag}-{nanos}.bin",
            std::process::id()
        ))
    }

    fn replay_trace(data: &[u8]) -> usize {
        let frames = parse_trace(data);
        assert!(!frames.is_empty(), "trace contains at least one frame");
        let mut screen = VtScreen::new(frames[0].width, frames[0].height);
        for frame in &frames {
            if screen.width != frame.width || screen.height != frame.height {
                screen = VtScreen::new(frame.width, frame.height);
            }
            screen.apply(&frame.bytes);
            assert_screen_matches_snapshot(&screen, frame);
        }
        frames.len()
    }

    #[test]
    fn test_trace_replay_matches_back_buffer_snapshots() {
        let path = temporary_trace_path("replay");
        let mut terminal = Terminal::new(Vec::new(), 40, 3).with_hyperlinks(true);
        terminal.set_trace_file(&path).expect("trace file");

        terminal
            .draw(|buffer| {
                buffer.set_string(0, 0, "OSC 8: on — Windows Terminal", Style::default());
                Some(Position::new(0, 1))
            })
            .expect("draw plain");
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com/a"));
                crate::clip::set_string_linked(
                    buffer,
                    0,
                    1,
                    "ABCDEFGHIJ",
                    Style::default(),
                    id,
                    buffer.area,
                );
                None
            })
            .expect("draw link");
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 1, "AB", Style::default());
                None
            })
            .expect("draw shorter plain");
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 2, "A世B—C", Style::default());
                None
            })
            .expect("draw wide");
        terminal
            .draw(|buffer| {
                buffer.set_string(0, 2, "A世B", Style::default());
                None
            })
            .expect("draw shorter wide");

        let trace = std::fs::read(&path).expect("read trace");
        let frames = replay_trace(&trace);
        assert_eq!(frames, 5, "one frame per draw");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_trace_replay_hyperlinks_off_has_no_link_ids() {
        let path = temporary_trace_path("off");
        let mut terminal = Terminal::new(Vec::new(), 20, 1);
        terminal.set_trace_file(&path).expect("trace file");
        terminal
            .draw(|buffer| {
                let id = buffer.links.insert(&url_link("https://example.com"));
                buffer[(0, 0)].set_symbol("A").set_link(Some(id));
                None
            })
            .expect("draw");

        let trace = std::fs::read(&path).expect("read trace");
        let frames = parse_trace(&trace);
        assert_eq!(frames.len(), 1);
        assert!(
            frames[0].links.iter().all(Option::is_none),
            "OSC 8 off means the snapshot must not claim a link"
        );
        assert_eq!(replay_trace(&trace), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_trace_fixture_replay_if_present() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        for name in ["wt-trace-frame1.bin", "wt-trace.bin"] {
            let path = base.join(name);
            let Ok(data) = std::fs::read(&path) else {
                continue;
            };
            let frames = replay_trace(&data);
            eprintln!("replayed {frames} captured WT trace frames from {name}");
        }
    }

    #[test]
    fn test_captured_links_frame_is_no_longer_collapsed() {
        use crate::application::Application;
        use crate::core::Rect;
        use crate::hyperlink::HyperlinkMode;
        use crate::link_label::LinkLabel;
        use crate::static_text::StaticText;
        use crate::window::Window;

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("wt-trace-frame1.bin");
        let data = std::fs::read(path).expect("captured Links frame fixture");
        let frames = parse_trace(&data);
        let frame = frames.first().expect("fixture has a frame");
        let width = usize::from(frame.width);
        let broken_row = frame
            .back
            .chunks(width)
            .map(<[String]>::concat)
            .find(|row| row.contains("OSC 8:") && row.contains("turbo-tui"))
            .expect("fixture contains the collapsed Links row");
        let broken_inner = broken_row.trim_matches(|c| c == '┃' || c == ' ').to_owned();
        assert!(
            broken_inner.contains("ono-tui") || broken_inner.contains("o-tui"),
            "fixture row must show the overwritten URL tail: {broken_inner:?}"
        );

        let url = "https://github.com/four-bytes/turbo-tui";
        let mut app = Application::new(Rect::new(0, 0, 183, 52)).with_hyperlinks(HyperlinkMode::On);
        let mut window = Window::new(Rect::new(18, 3, 44, 10), "Links");
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 40, 1),
            url,
            Link::new(LinkTarget::Url(url.to_owned())),
        )));
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 2, 40, 1),
            "Issue four-bytes/turbo-tui#54",
            Link::new(LinkTarget::IssueRef {
                repo: Some("four-bytes/turbo-tui".to_owned()),
                number: 54,
            }),
        )));
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 3, 40, 1),
            "File src/lib.rs:1",
            Link::new(LinkTarget::FilePos {
                path: "src/lib.rs".to_owned(),
                line: 1,
                col: None,
            }),
        )));
        window.add(Box::new(StaticText::new(
            Rect::new(1, 5, 40, 1),
            "OSC 8: on — forced on",
        )));
        app.add_window(window);

        let mut terminal = TestTerminal::new(183, 52).with_hyperlinks(true);
        terminal.draw(|buffer| app.draw(&mut crate::canvas::CellCanvas::new(buffer)));

        let buffer = terminal.buffer();
        let mut url_row = None;
        let mut support_row = None;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            assert!(
                !row.contains(&broken_inner),
                "the captured collapsed row reappeared at y={y}: {row:?}"
            );
            if row.contains(url) {
                url_row = Some(y);
            }
            if row.contains("OSC 8: on — forced on") {
                support_row = Some(y);
                for x in 0..buffer.area.width {
                    assert!(
                        !buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
                        "the static support line inherited a link underline at ({x},{y})"
                    );
                }
            }
        }
        assert!(
            url_row.is_some(),
            "the fixed Links window must render the full URL contiguously"
        );
        assert!(
            support_row.is_some(),
            "the fixed Links window must render the support line"
        );
        assert_ne!(
            url_row, support_row,
            "the URL and support line must not share a row"
        );
    }
}
