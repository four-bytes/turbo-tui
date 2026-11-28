//! The opt-in tmux scenarios for issue #74.
//!
//! Every test is `#[ignore]`d so the default `cargo test` run stays free of
//! tmux. Run them explicitly:
//!
//! ```text
//! TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored --test-threads=1 --nocapture
//! ```

use std::collections::HashSet;
use std::time::Duration;

use crate::harness::{
    expect_known_bug, wait_until, Attr, CheckFailure, Color, Grid, Harness, Rect80,
    REACTION_TIMEOUT,
};

// ---------------------------------------------------------------------------
// 1 — drag must not leave underline streaks
// ---------------------------------------------------------------------------

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_1_drag_leaves_no_underline_streaks() {
    let Some(harness) = Harness::start() else {
        return;
    };

    assert!(
        harness.click_window_title("Control"),
        "Control window title not found"
    );
    std::thread::sleep(Duration::from_millis(150));

    let before = harness.capture();
    let before_underlines: HashSet<(u16, u16)> = before
        .cells_with_attr(Attr::Underline)
        .into_iter()
        .collect();
    let before_count = before_underlines.len();
    let (title_x, title_y) = before
        .find_text("Control")
        .expect("Control title must be on screen");
    let from = (title_x, title_y);
    let to = (title_x.saturating_sub(6), title_y);
    harness.drag(from, to, 3);
    std::thread::sleep(Duration::from_millis(200));

    let after = harness.capture();
    let after_underlines = after.cells_with_attr(Attr::Underline);
    let after_count = after_underlines.len();
    let moved = after
        .window_rect("Control")
        .expect("Control frame after drag");

    // Accept underlines that already existed at the same cell (the Links window
    // is visible next to Control and keeps its own links underlined). The
    // regression this guards is a *new* underlined cell outside the moved frame.
    let stray: Vec<(u16, u16)> = after_underlines
        .into_iter()
        .filter(|(x, y)| !moved.contains(*x, *y))
        .filter(|position| !before_underlines.contains(position))
        .collect();

    let bold = after.cells_with_attr(Attr::Bold).len();
    let reverse = after.cells_with_attr(Attr::Reverse).len();
    println!(
        "scenario 1: underline cells before={before_count} after={after_count} \
         stray_outside_moved_control={} bold={bold} reverse={reverse}",
        stray.len()
    );

    assert_eq!(
        before_count, after_count,
        "underline cell count changed after dragging Control"
    );
    assert!(
        stray.is_empty(),
        "new underlined cells outside the moved Control frame: {stray:?}"
    );
}

// ---------------------------------------------------------------------------
// 2 — scrollbars (#70): Scroll Demo vertical + horizontal
// ---------------------------------------------------------------------------

/// Acceptance test for the `Scroll Demo` frame scrollbars.
///
/// On main `84407c3` the `Scroll Demo` vertical **and** horizontal bars work
/// under tmux: an arrow click moves one step, a track click jumps a page and a
/// thumb drag moves further. #70 therefore does not reproduce for this window;
/// this test is its acceptance test — any future failure means #70 is live
/// again. The Editor frame scrollbar still fails and is covered by
/// [`scenario_2b_scrollbars_expected_fail_70`].
#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_2_scrollbars_scroll_demo() {
    let Some(harness) = Harness::start() else {
        return;
    };

    match check_scroll_demo_scrollbars(&harness) {
        Ok(evidence) => println!("PASS #70 Scroll Demo bars — {evidence}"),
        Err(reason) => panic!("#70 reproduced on Scroll Demo — {reason}"),
    }
}

fn check_scroll_demo_scrollbars(harness: &Harness) -> Result<String, String> {
    // The Scroll Demo interior is 28 columns wide, so the full first line is
    // clipped. Match a prefix that fits.
    const FIRST: &str = "Scroll with mouse wheel";
    const KNOWN: [&str; 4] = [
        "Scroll with mouse wheel",
        "Drag scrollbar thumb",
        "Click arrows for single-line",
        "Click track for page scroll",
    ];

    if !harness.click_window_title("Scroll Demo") {
        return Err("Scroll Demo title not found".to_owned());
    }
    if !harness.wait_for_text(FIRST, REACTION_TIMEOUT) {
        return Err(format!("first content line not visible: {FIRST:?}"));
    }
    std::thread::sleep(Duration::from_millis(150));

    let grid = harness.capture();
    let rect = grid
        .window_rect("Scroll Demo")
        .ok_or_else(|| "Scroll Demo frame not found".to_owned())?;
    let (_, content_row) = grid
        .find_text(FIRST)
        .ok_or_else(|| format!("first content line missing: {FIRST:?}"))?;
    let scrollbar = rect.right();
    let down_arrow = rect.bottom().saturating_sub(1);

    // Vertical down arrow: one line.
    harness.click(scrollbar, down_arrow);
    std::thread::sleep(Duration::from_millis(150));
    let after_arrow = harness.capture();
    let arrow_row = after_arrow.row_text(content_row);
    let arrow_index = line_index(&arrow_row, &KNOWN);
    if arrow_index != 1 {
        return Err(format!(
            "vertical down arrow did not advance exactly one line; row {content_row} = {arrow_row:?}"
        ));
    }

    // Vertical track click: at least a page (more than one line).
    let mid = rect.y + rect.height / 2;
    harness.click(scrollbar, mid);
    std::thread::sleep(Duration::from_millis(150));
    let after_track = harness.capture();
    let track_row = after_track.row_text(content_row);
    let track_index = line_index(&track_row, &KNOWN);
    if track_index < arrow_index + 2 {
        return Err(format!(
            "vertical track click did not page the content; row {content_row} = {track_row:?}"
        ));
    }

    // Vertical thumb drag: three rows down must scroll further.
    let thumb_y = (rect.y + 1..rect.bottom())
        .find(|&y| {
            after_track
                .cell(scrollbar, y)
                .is_some_and(|cell| cell.symbol == "█")
        })
        .unwrap_or(mid);
    harness.drag(
        (scrollbar, thumb_y),
        (scrollbar, thumb_y.saturating_add(3)),
        3,
    );
    std::thread::sleep(Duration::from_millis(200));
    let after_drag = harness.capture();
    let drag_row = after_drag.row_text(content_row);
    let drag_index = line_index(&drag_row, &KNOWN);
    if drag_index <= track_index {
        return Err(format!(
            "vertical thumb drag did not scroll further; row {content_row} = {drag_row:?}"
        ));
    }

    let horizontal = check_horizontal_scrollbar(harness, rect, content_row, &after_drag)?;

    Ok(format!(
        "vertical arrow→{arrow_row:?}, track→{track_row:?}, drag→{drag_row:?}; {horizontal}"
    ))
}

fn check_horizontal_scrollbar(
    harness: &Harness,
    rect: Rect80,
    content_row: u16,
    grid_before: &Grid,
) -> Result<String, String> {
    let hbar_row = rect.bottom();
    let h_left = rect.x + 1;
    let h_right = rect.right().saturating_sub(1);
    let h_before = row_slice(grid_before, content_row, h_left, h_right);

    // Right arrow: a one-step left shift.
    harness.click(h_right, hbar_row);
    std::thread::sleep(Duration::from_millis(180));
    let after_h_arrow = harness.capture();
    let h_arrow = row_slice(&after_h_arrow, content_row, h_left, h_right);
    if h_arrow == h_before {
        return Err(format!(
            "horizontal right arrow did not shift the content; before={h_before:?} after={h_arrow:?}"
        ));
    }
    let arrow_shift = horizontal_shift(&h_before, &h_arrow).ok_or_else(|| {
        format!("horizontal arrow was not a left shift; before={h_before:?} after={h_arrow:?}")
    })?;
    let arrow_thumb = find_horizontal_thumb(&after_h_arrow, hbar_row, h_left, h_right);

    // Track click: one page.
    let h_mid = u16::midpoint(h_left, h_right);
    harness.click(h_mid, hbar_row);
    std::thread::sleep(Duration::from_millis(180));
    let after_h_track = harness.capture();
    let h_track = row_slice(&after_h_track, content_row, h_left, h_right);
    if h_track == h_arrow {
        return Err(format!(
            "horizontal track click did not page the content; row={h_track:?}"
        ));
    }
    let track_shift = horizontal_shift(&h_arrow, &h_track);
    let track_thumb = find_horizontal_thumb(&after_h_track, hbar_row, h_left, h_right);
    let thumb_jump = matches!(
        (arrow_thumb, track_thumb),
        (Some(before), Some(after)) if after >= before.saturating_add(2)
    );
    // A page can be wider than the visible span, leaving no common text; that
    // case is already proven by the thumb jump. A matched shift must exceed a
    // single arrow step.
    let text_page = track_shift.is_some_and(|shift| shift >= arrow_shift + 2);
    if !(thumb_jump || text_page) {
        return Err(format!(
            "horizontal track click did not page; arrow_shift={arrow_shift}, \
             track_shift={track_shift:?}, thumb={arrow_thumb:?}->{track_thumb:?}, row={h_track:?}"
        ));
    }

    // Thumb drag: further movement to the right.
    let thumb_x = track_thumb.unwrap_or(h_mid);
    harness.drag(
        (thumb_x, hbar_row),
        (thumb_x.saturating_add(3), hbar_row),
        3,
    );
    std::thread::sleep(Duration::from_millis(200));
    let after_h_drag = harness.capture();
    let h_drag = row_slice(&after_h_drag, content_row, h_left, h_right);
    if h_drag == h_track {
        return Err(format!(
            "horizontal thumb drag did not move further; row={h_drag:?}"
        ));
    }
    let drag_shift = horizontal_shift(&h_track, &h_drag);
    let drag_thumb = find_horizontal_thumb(&after_h_drag, hbar_row, h_left, h_right);
    let thumb_moved = matches!(
        (track_thumb, drag_thumb),
        (Some(before), Some(after)) if after > before
    );
    if !(thumb_moved || drag_shift.is_some()) {
        return Err(format!(
            "horizontal thumb drag did not move further; \
             thumb={track_thumb:?}->{drag_thumb:?}, row={h_drag:?}"
        ));
    }

    Ok(format!(
        "horizontal arrow shift={arrow_shift} ({h_arrow:?}), \
         track shift={track_shift:?} thumb={arrow_thumb:?}->{track_thumb:?} ({h_track:?}), \
         drag shift={drag_shift:?} thumb={track_thumb:?}->{drag_thumb:?} ({h_drag:?})"
    ))
}

fn find_horizontal_thumb(grid: &Grid, row: u16, left: u16, right: u16) -> Option<u16> {
    (left..=right).find(|&x| grid.cell(x, row).is_some_and(|cell| cell.symbol == "█"))
}

/// Concatenates the symbols of `y` between `x0` and `x1` (inclusive).
fn row_slice(grid: &Grid, y: u16, x0: u16, x1: u16) -> String {
    let mut text = String::new();
    for x in x0..=x1 {
        if let Some(cell) = grid.cell(x, y) {
            if !cell.symbol.is_empty() {
                text.push_str(&cell.symbol);
            }
        }
    }
    text
}

/// Returns the number of columns `after` is shifted left relative to `before`,
/// if `after` is a clean suffix-of-`before` window (the horizontal-scroll case).
fn horizontal_shift(before: &str, after: &str) -> Option<usize> {
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let width = old.len().min(new.len());
    if width < 2 {
        return None;
    }
    (1..width).find(|&shift| {
        old[shift..width]
            .iter()
            .zip(new[..width - shift].iter())
            .all(|(a, b)| a == b)
    })
}

// ---------------------------------------------------------------------------
// 2b — Editor vertical scrollbar (#70, reproduces)
// ---------------------------------------------------------------------------

/// On main `84407c3` #70 reproduces for the Editor vertical frame scrollbar:
/// clicking the down arrow leaves the first visible code line unchanged, even
/// though the Editor accepts typed input and page keys scroll it. Expected-fail
/// until the frame scrollbar drives the self-scrolling Editor.
#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_2b_scrollbars_expected_fail_70() {
    let Some(harness) = Harness::start() else {
        return;
    };

    expect_known_bug("#70", || check_editor_scrollbar(&harness));
}

fn check_editor_scrollbar(harness: &Harness) -> Result<(), CheckFailure> {
    const ANCHOR: &str = "//! turbo-tui";
    const TITLE: &str = "Editor — demo.rs";

    if !harness.click_text(ANCHOR) {
        return Err(CheckFailure::Setup(format!(
            "editor anchor not found: {ANCHOR:?}"
        )));
    }
    if !wait_until(REACTION_TIMEOUT, || {
        harness.capture().window_rect(TITLE).is_some()
    }) {
        return Err(CheckFailure::Setup(
            "editor frame did not become visible".to_owned(),
        ));
    }
    std::thread::sleep(Duration::from_millis(150));

    let before = harness.capture();
    let outer = before
        .window_rect(TITLE)
        .ok_or_else(|| CheckFailure::Setup("editor frame not found".to_owned()))?;
    let interior = outer.inner(before.width, before.height);
    let first_row = interior.y;
    let first_before = row_slice(&before, first_row, interior.x, interior.right());
    let second_before = row_slice(
        &before,
        first_row.saturating_add(1),
        interior.x,
        interior.right(),
    );

    let arrow_x = outer.right();
    let arrow_y = outer.bottom().saturating_sub(1);
    harness.click(arrow_x, arrow_y);
    std::thread::sleep(Duration::from_millis(250));
    let after = harness.capture();
    let first_after = row_slice(&after, first_row, interior.x, interior.right());

    if first_after == first_before {
        return Err(CheckFailure::Reproduced(format!(
            "editor down arrow did not scroll; row {first_row} (arrow at {arrow_x},{arrow_y}):              {first_before:?} -> {first_after:?}"
        )));
    }
    if first_before != second_before && first_after != second_before {
        return Err(CheckFailure::Reproduced(format!(
            "editor down arrow did not scroll exactly one line; row {first_row}:              before={first_before:?}, expected={second_before:?}, got={first_after:?}"
        )));
    }

    Ok(())
}

fn line_index(row: &str, known: &[&str]) -> usize {
    if let Some(position) = row.find("Line ") {
        let digits: String = row[position + "Line ".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(number) = digits.trim().parse::<usize>() {
            return 100 + number;
        }
    }
    known
        .iter()
        .position(|needle| row.contains(needle))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 3 — editor black holes (#73)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_3_editor_no_black_holes_expected_fail_73() {
    let Some(harness) = Harness::start() else {
        return;
    };

    // The black holes are not `Color::Default` in the Dark theme: scrolled
    // cells fall back to the window background `Rgb(30, 30, 30)` instead of the
    // editor's `Ansi(4)`. The check therefore compares every interior cell with
    // the majority background, not only against `Color::Default`.
    expect_known_bug("73", || check_editor_background(&harness).map(|_| ()));
}

fn check_editor_background(harness: &Harness) -> Result<String, CheckFailure> {
    const ANCHOR: &str = "//! turbo-tui";
    const TITLE: &str = "Editor — demo.rs";

    if !harness.click_text(ANCHOR) {
        return Err(CheckFailure::Setup(format!(
            "editor anchor not found: {ANCHOR:?}"
        )));
    }
    let raised = wait_until(REACTION_TIMEOUT, || {
        harness
            .capture()
            .window_rect(TITLE)
            .is_some_and(|rect| rect.y < 10)
    });
    if !raised {
        return Err(CheckFailure::Setup(
            "editor frame did not become visible".to_owned(),
        ));
    }
    // A second click places the caret and focuses the editor child.
    harness.click_text(ANCHOR);
    std::thread::sleep(Duration::from_millis(120));

    let before = harness.capture();
    let outer = before
        .window_rect(TITLE)
        .ok_or_else(|| CheckFailure::Setup("editor frame not found".to_owned()))?;
    let mut interior = outer.inner(before.width, before.height);
    // The vertical scrollbar sits on the right border; its track styling bleeds
    // one cell into the interior. Exclude that column from the background check
    // so a legitimate scrollbar colour is not reported as a hole.
    let has_vertical_scrollbar = (outer.y..=outer.bottom()).any(|y| {
        before
            .cell(outer.right(), y)
            .is_some_and(|cell| matches!(cell.symbol.as_str(), "▲" | "▼" | "█" | "░"))
    });
    if has_vertical_scrollbar && interior.width > 1 {
        interior.width -= 1;
    }
    let majority = majority_bg(&before, interior);
    let reference = before.row_text(interior.y);

    let centre_x = interior.x + interior.width / 2;
    let centre_y = interior.y + interior.height / 2;
    for _ in 0..3 {
        harness.wheel_down(centre_x, centre_y);
        std::thread::sleep(Duration::from_millis(30));
    }
    harness.wheel_up(centre_x, centre_y);
    for _ in 0..8 {
        harness.send_key("PageDown");
    }
    harness.send_key("PPage");
    std::thread::sleep(Duration::from_millis(200));

    let after = harness.capture();
    let scrolled = after.row_text(interior.y) != reference;
    let holes: Vec<(u16, u16, Color)> = interior_cells(interior)
        .filter(|(x, y)| after.bg_at(*x, *y) != majority)
        .map(|(x, y)| (x, y, after.bg_at(x, y)))
        .collect();

    if let Some((x, y, color)) = holes.first() {
        let default_cells = holes
            .iter()
            .filter(|(_, _, cell)| *cell == Color::Default)
            .count();
        return Err(CheckFailure::Reproduced(format!(
            "{} interior cells differ from {majority:?} after scrolling \
             ({default_cells} default-bg holes, scrolled={scrolled}), e.g. ({x},{y}) = {color:?}",
            holes.len()
        )));
    }

    Ok(format!(
        "majority={majority:?}, scrolled={scrolled}, mismatched_bg_cells=0"
    ))
}

fn majority_bg(grid: &Grid, rect: Rect80) -> Color {
    let mut counts: Vec<(Color, usize)> = Vec::new();
    for (x, y) in interior_cells(rect) {
        let color = grid.bg_at(x, y);
        if let Some(entry) = counts.iter_mut().find(|(known, _)| *known == color) {
            entry.1 += 1;
        } else {
            counts.push((color, 1));
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(Color::Default, |(color, _)| color)
}

fn interior_cells(rect: Rect80) -> impl Iterator<Item = (u16, u16)> {
    let right = rect.right();
    let bottom = rect.bottom();
    (rect.y..=bottom).flat_map(move |y| (rect.x..=right).map(move |x| (x, y)))
}

// ---------------------------------------------------------------------------
// 4 — cursor visibility per theme (#71)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_4_cursor_visible_per_theme_expected_fail_71() {
    let Some(harness) = Harness::start() else {
        return;
    };

    let themes = discover_themes(&harness);
    assert!(
        !themes.is_empty(),
        "no `Theme: ` entries found in the Window menu"
    );
    println!("scenario 4: discovered themes {themes:?}");

    expect_known_bug("71", || check_cursor_per_theme(&harness, &themes));
}

fn discover_themes(harness: &Harness) -> Vec<String> {
    if !harness.click_text("Window") {
        return Vec::new();
    }
    if !harness.wait_for_text("Theme: ", REACTION_TIMEOUT) {
        return Vec::new();
    }
    let grid = harness.capture();
    let mut themes = Vec::new();
    for y in 0..grid.height {
        let text = grid.row_text(y);
        let Some(position) = text.find("Theme: ") else {
            continue;
        };
        // The dropdown row continues with the pane content to its right, so
        // stop at the dropdown's right border instead of trimming the whole
        // terminal line.
        let rest = &text[position + "Theme: ".len()..];
        let name = rest.split('│').next().unwrap_or(rest).trim().to_owned();
        if !name.is_empty() && !themes.contains(&name) {
            themes.push(name);
        }
    }
    harness.send_key("Escape");
    std::thread::sleep(Duration::from_millis(100));
    themes
}

fn check_cursor_per_theme(harness: &Harness, themes: &[String]) -> Result<(), CheckFailure> {
    const TITLE: &str = "Editor — demo.rs";
    const ANCHOR: &str = "//! turbo-tui";

    // Bring the editor to the front once so it stays visible while themes are
    // switched through the menu, and remember its interior: selecting a theme
    // does not move windows, so the frame glyphs changing between themes must
    // not invalidate the geometry we measure against.
    let _ = harness.click_text(ANCHOR);
    std::thread::sleep(Duration::from_millis(150));
    let initial = harness.capture();
    let interior = initial
        .window_rect(TITLE)
        .ok_or_else(|| {
            CheckFailure::Setup("editor frame not visible before theme switch".to_owned())
        })?
        .inner(initial.width, initial.height);

    let mut failures = Vec::new();
    for theme in themes {
        if !select_theme(harness, theme) {
            return Err(CheckFailure::Setup(format!(
                "{theme}: menu entry could not be selected"
            )));
        }
        if !harness.click_text(ANCHOR) {
            return Err(CheckFailure::Setup(format!(
                "{theme}: editor anchor not visible"
            )));
        }
        std::thread::sleep(Duration::from_millis(120));

        let cursor = harness.cursor();
        let grid = harness.capture();
        // tmux reports the cursor on a 0-based cell convention that may be one
        // cell off at the pane edge, so allow ±1 on each axis.
        let inside = cursor.x >= interior.x.saturating_sub(1)
            && cursor.x <= interior.right().saturating_add(1)
            && cursor.y >= interior.y.saturating_sub(1)
            && cursor.y <= interior.bottom().saturating_add(1);
        let distinguishable = grid
            .cell(cursor.x, cursor.y)
            .is_some_and(|cell| cell.reverse || cell.fg != cell.bg);

        if !(cursor.visible && inside && distinguishable) {
            let cell = grid
                .cell(cursor.x, cursor.y)
                .map_or_else(|| "off-grid".to_owned(), |cell| format!("{cell:?}"));
            failures.push(format!(
                "{theme}: cursor=({}, {}) visible={} inside={inside} distinguishable={distinguishable} cell={cell}",
                cursor.x, cursor.y, cursor.visible
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(CheckFailure::Reproduced(format!(
            "{} theme(s): {}",
            failures.len(),
            failures.join("; ")
        )))
    }
}

fn select_theme(harness: &Harness, theme: &str) -> bool {
    if !harness.click_text("Window") {
        return false;
    }
    if !harness.wait_for_text("Theme: ", REACTION_TIMEOUT) {
        return false;
    }
    let needle = format!("Theme: {theme}");
    if !harness.click_text(&needle) {
        harness.send_key("Escape");
        return false;
    }
    // The dropdown closes once the command is dispatched.
    wait_until(REACTION_TIMEOUT, || !harness.contains_text("Theme: "))
}

// ---------------------------------------------------------------------------
// 5 — clicking a link reports and opens it
// ---------------------------------------------------------------------------

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_5_link_click_reports_opened() {
    const URL: &str = "https://github.com/four-bytes/turbo-tui";
    let Some(harness) = Harness::start() else {
        return;
    };

    // Raise the Links window first: on an unfocused window the first press only
    // establishes focus instead of activating the label.
    assert!(
        harness.click_window_title("Links"),
        "Links window title not found"
    );
    std::thread::sleep(Duration::from_millis(150));

    let grid = harness.capture();
    let (x, y) = grid.find_text(URL).expect("Links URL must be visible");
    harness.mouse_move(x, y);
    std::thread::sleep(Duration::from_millis(120));
    // Link activation happens on the press. Release after a short pause: when
    // press and release arrive in one event batch the deferred activation is
    // lost, so this deliberately splits them.
    harness.mouse_down(x, y);

    // The demo reports the opener result in the status bar while the press is
    // handled; releasing the button clears the hover hint again, so read the
    // status before the release.
    let expected = format!("opened {URL}");
    let status_seen = harness.wait_for_text(&expected, REACTION_TIMEOUT);
    let status = {
        let grid = harness.capture();
        grid.row_text(grid.height.saturating_sub(1))
    };
    harness.mouse_up(x, y);
    assert!(
        status_seen,
        "status bar did not report {expected:?}; last row = {status:?}"
    );

    let log = harness.open_log();
    let logged = wait_until(REACTION_TIMEOUT, || {
        std::fs::read_to_string(&log).is_ok_and(|contents| contents.contains(URL))
    });
    assert!(
        logged,
        "xdg-open stub did not log the URL to {}",
        log.display()
    );

    println!("scenario 5: status line = {status:?}");
}

// ---------------------------------------------------------------------------
// 6 — menu opens by mouse and F10, theme switching actually recolours
// ---------------------------------------------------------------------------

/// Representative background colours that change with the active theme.
fn theme_signature(grid: &Grid) -> Vec<Color> {
    [(0_u16, 0_u16), (0, 23), (79, 22)]
        .iter()
        .map(|&(x, y)| grid.bg_at(x, y))
        .collect()
}

/// Text drawn at `(x, y)` for `width` cells, trimmed.
fn text_at(grid: &Grid, x: u16, y: u16, width: u16) -> String {
    grid.row_text(y)
        .chars()
        .skip(usize::from(x))
        .take(usize::from(width))
        .collect::<String>()
        .trim()
        .to_owned()
}

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_6_menu_switches_theme_by_mouse_and_keyboard() {
    let Some(harness) = Harness::start() else {
        return;
    };

    let themes = discover_themes(&harness);
    assert!(
        themes.len() >= 3,
        "need at least three themes in the Window menu, got {themes:?}"
    );
    let before = theme_signature(&harness.capture());

    // Mouse: open Window and pick the second theme.
    assert!(
        harness.click_text("Window"),
        "menu bar entry 'Window' not found"
    );
    assert!(
        harness.wait_for_text("Theme: ", REACTION_TIMEOUT),
        "Window dropdown did not open by mouse"
    );
    let mouse_theme = themes[1].clone();
    assert!(
        harness.click_text(&format!("Theme: {mouse_theme}")),
        "menu item 'Theme: {mouse_theme}' not clickable"
    );
    assert!(
        wait_until(REACTION_TIMEOUT, || !harness.contains_text("Theme: ")),
        "dropdown did not close after the mouse click"
    );
    std::thread::sleep(Duration::from_millis(150));
    let after_mouse = theme_signature(&harness.capture());
    assert_ne!(
        before, after_mouse,
        "mouse theme switch to {mouse_theme} did not change the colours"
    );

    // Keyboard: F10 -> Window -> Down into the list -> theme entry -> Enter.
    harness.send_key("F10");
    std::thread::sleep(Duration::from_millis(120));
    harness.send_key("Right");
    std::thread::sleep(Duration::from_millis(80));
    harness.send_key("Right");
    std::thread::sleep(Duration::from_millis(80));
    harness.send_key("Down");
    std::thread::sleep(Duration::from_millis(120));
    let keyboard_index = themes
        .iter()
        .position(|theme| theme == &themes[2])
        .expect("third theme must exist");
    // Tile, Cascade, Close, Close All, then the theme rows; separators are
    // skipped by the menu selection.
    for _ in 0..(5 + keyboard_index) {
        harness.send_key("Down");
        std::thread::sleep(Duration::from_millis(40));
    }
    harness.send_key("Enter");
    std::thread::sleep(Duration::from_millis(200));
    let after_keyboard = theme_signature(&harness.capture());
    assert_ne!(
        after_mouse, after_keyboard,
        "keyboard theme switch to {} did not change the colours",
        themes[2]
    );

    // F2 must cycle the theme as well.
    harness.send_key("F2");
    std::thread::sleep(Duration::from_millis(200));
    let after_f2 = theme_signature(&harness.capture());
    assert_ne!(
        after_keyboard, after_f2,
        "F2 did not change the theme colours"
    );

    println!(
        "scenario 6: themes={themes:?} mouse={after_mouse:?} keyboard={after_keyboard:?} f2={after_f2:?}"
    );
}

// ---------------------------------------------------------------------------
// 7 — ListBox scrolls by wheel, page keys and its embedded scrollbar
// ---------------------------------------------------------------------------

#[test]
#[ignore = "opt-in: TURBO_TUI_E2E=1 cargo test --test e2e -- --ignored"]
fn scenario_7_listbox_scrolls_by_wheel_keys_and_scrollbar() {
    let Some(harness) = Harness::start() else {
        return;
    };

    // Tile: the ListBox Demo window becomes visible, but its interior is only a
    // few rows high. The window must clamp the ListBox to that visible viewport,
    // otherwise wheel steps select rows that are clipped away (#77).
    assert!(
        harness.click_text("Window"),
        "menu bar entry 'Window' not found"
    );
    assert!(
        harness.wait_for_text("Tile", REACTION_TIMEOUT),
        "Window dropdown did not open"
    );
    assert!(harness.click_text("Tile"), "menu item 'Tile' not found");
    std::thread::sleep(Duration::from_millis(300));

    let grid = harness.capture();
    let (bx, by) = grid
        .find_text("Apple")
        .expect("ListBox first item not visible after Tile");
    harness.click(bx + 2, by);
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(
        text_at(&harness.capture(), bx, by, 10),
        "Apple",
        "ListBox must start at the first item"
    );

    // Wheel: one row per event; four events reach the bottom of the viewport.
    for _ in 0..4 {
        harness.wheel_down(bx + 2, by + 1);
        std::thread::sleep(Duration::from_millis(60));
    }
    let after_wheel = text_at(&harness.capture(), bx, by, 10);
    assert_eq!(
        after_wheel, "Banana",
        "wheel did not scroll the ListBox viewport"
    );

    // Keys: PageDown moves a whole viewport.
    harness.send_key("PageDown");
    std::thread::sleep(Duration::from_millis(120));
    let after_page = text_at(&harness.capture(), bx, by, 10);
    assert_eq!(
        after_page, "Fig",
        "PageDown did not scroll the ListBox viewport"
    );

    // Scrollbar: a track click below the thumb pages down.
    let sb_col = bx + 17;
    harness.click(sb_col, by + 2);
    std::thread::sleep(Duration::from_millis(150));
    let after_track = text_at(&harness.capture(), bx, by, 10);
    assert_eq!(
        after_track, "Lemon",
        "scrollbar track click did not page the ListBox"
    );

    // Scrollbar: the down arrow steps the selection and keeps it visible.
    harness.click(sb_col, by + 3);
    std::thread::sleep(Duration::from_millis(150));
    let after_arrow = text_at(&harness.capture(), bx, by, 10);
    assert_eq!(
        after_arrow, "Mango",
        "scrollbar down arrow did not scroll the ListBox"
    );

    println!(
        "scenario 7: list top after wheel={after_wheel:?} page={after_page:?} \
         track={after_track:?} arrow={after_arrow:?}"
    );
}
