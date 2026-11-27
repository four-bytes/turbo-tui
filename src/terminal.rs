//! Terminal backend with persistent front and back buffers.
//!
//! [`Terminal`] owns an output writer and renders only the cells that changed
//! since the previous frame. [`TestTerminal`] provides the same persistent
//! buffer diffing in memory so widgets and integration tests can run without a
//! real terminal.

use std::io::{self, Write};

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
/// A run continues while updates share a row, are horizontally adjacent and
/// resolve to the same link id. Outside a run the active link is closed *before*
/// the cursor moves. The link is always closed after the last update, so a frame
/// never ends with an open hyperlink.
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

    for (x, y) in diff_updates(front, back) {
        let cell = &back[(x, y)];
        let resolved = resolve_cell_link(&back.links, allowed_schemes, cell);
        let target = resolved.as_ref().map(|(id, _)| *id);

        let contiguous =
            prev.is_some_and(|(px, py)| py == y && px.saturating_add(1) == x) && target == active;

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
}

impl<W: Write> Terminal<W> {
    /// Creates a terminal with empty front and back buffers.
    ///
    /// OSC 8 hyperlink emission is off by default; enable it with
    /// [`set_hyperlinks`](Terminal::set_hyperlinks) or
    /// [`with_hyperlinks`](Terminal::with_hyperlinks) once
    /// [`HyperlinkSupport`](crate::hyperlink::HyperlinkSupport) reports support.
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
        }
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

    /// Emits the cells that differ between the back and the front buffer.
    ///
    /// When hyperlinks are enabled, linked cells are wrapped in OSC 8
    /// sequences; otherwise the output is byte-identical to a plain redraw.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        emit_updates(
            &mut self.writer,
            &self.front,
            &self.back,
            self.hyperlinks,
            &self.allowed_schemes,
        )?;
        self.front = self.back.clone();

        match self.cursor {
            Some(position) => {
                queue!(self.writer, MoveTo(position.x, position.y))?;
                if !self.cursor_visible {
                    queue!(self.writer, Show)?;
                    self.cursor_visible = true;
                }
            }
            None => {
                if self.cursor_visible {
                    queue!(self.writer, Hide)?;
                    self.cursor_visible = false;
                }
            }
        }

        self.writer.flush()
    }

    /// Clears the screen and resets both buffers.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn clear(&mut self) -> io::Result<()> {
        queue!(self.writer, Clear(ClearType::All))?;
        let area = Rect::new(0, 0, self.width, self.height);
        self.front = Buffer::empty(area);
        self.back = Buffer::empty(area);
        self.writer.flush()
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
        queue!(self.writer, Clear(ClearType::All))?;
        self.writer.flush()
    }

    /// Shows the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Show)?;
        self.cursor_visible = true;
        self.writer.flush()
    }

    /// Hides the cursor.
    ///
    /// # Errors
    ///
    /// Returns any error produced while writing to the underlying writer.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Hide)?;
        self.cursor_visible = false;
        self.writer.flush()
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
}
