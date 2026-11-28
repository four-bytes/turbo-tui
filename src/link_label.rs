//! `LinkLabel` — interactive inline link.
//!
//! A `LinkLabel` renders its text as a link and reports hover and activation
//! events to the [`Application`](crate::application::Application):
//!
//! - Moving the mouse over the label posts [`EventKind::LinkHover`] with the
//!   link target.
//! - Ctrl+click (or Enter/Space while focused) marks the link visited and posts
//!   [`EventKind::LinkActivate`].
//!
//! The application turns a hover into a status hint and an activation into
//! `CM_OPEN_LINK` + [`Application::take_open_link`](crate::application::Application::take_open_link).
//!
//! # Hover and overlapping windows
//!
//! The desktop broadcasts every `Moved` event to all visible windows, so a
//! label geometrically under the pointer updates even when another window is
//! drawn on top of it. Hover state therefore follows mouse geometry, not
//! z-order: over an overlap the topmost window does not win, and an obscured
//! label can still render as hovered. Keep interactive links out of overlapping
//! regions if that matters.

use crate::canvas::Canvas;
use crate::core::{Link, Rect};
use crate::theme;
use crate::view::{Event, EventKind, View, ViewBase, ViewId, OF_SELECTABLE, SF_FOCUSED};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::any::Any;

/// Interactive inline link.
///
/// # Visual Style
///
/// ```text
/// Hovered:  underlined and bold (theme `link_hover`)
/// Visited:  underlined (theme `link_visited`)
/// Plain:    underlined (theme `link`)
/// Focused:  the plain/visited pen; focus alone does not use the hover pen
/// ```
///
/// Focus is an activation state (Enter/Space), not a hover state. Keeping the
/// bold hover pen on a focused link made the underline look stuck after the
/// pointer left it.
pub struct LinkLabel {
    /// Embedded base providing id, bounds, state and options.
    base: ViewBase,
    /// Text drawn for the link.
    text: String,
    /// The link target attached to the drawn cells.
    link: Link,
    /// Whether the mouse currently hovers the label.
    hovered: bool,
    /// Whether the link has been activated at least once.
    visited: bool,
    /// Modifier required for a click activation (default: Ctrl).
    activate_modifier: KeyModifiers,
}

impl LinkLabel {
    /// Creates a new link label.
    ///
    /// The label is selectable by default, so it can receive focus and be
    /// activated with Enter or Space.
    #[must_use]
    pub fn new(bounds: Rect, text: impl Into<String>, link: Link) -> Self {
        Self {
            base: ViewBase::with_options(bounds, OF_SELECTABLE),
            text: text.into(),
            link,
            hovered: false,
            visited: false,
            activate_modifier: KeyModifiers::CONTROL,
        }
    }

    /// Sets the modifier required to activate the link with a click.
    ///
    /// The default is [`KeyModifiers::CONTROL`]. Pass
    /// [`KeyModifiers::NONE`] to activate on a plain left click.
    #[must_use]
    pub fn with_activate_modifier(mut self, modifiers: KeyModifiers) -> Self {
        self.activate_modifier = modifiers;
        self
    }

    /// Returns the link this label points at.
    #[must_use]
    pub const fn link(&self) -> &Link {
        &self.link
    }

    /// Returns `true` while the mouse hovers the label.
    #[must_use]
    pub const fn is_hovered(&self) -> bool {
        self.hovered
    }

    /// Returns `true` once the link has been activated.
    #[must_use]
    pub const fn is_visited(&self) -> bool {
        self.visited
    }

    /// Sets the visited flag.
    pub fn set_visited(&mut self, visited: bool) {
        self.visited = visited;
    }

    /// Returns `true` when `(column, row)` lies inside the label bounds.
    fn contains(&self, column: u16, row: u16) -> bool {
        let b = self.base.bounds();
        column >= b.x && column < b.x + b.width && row >= b.y && row < b.y + b.height
    }

    /// Marks the link visited and posts the activation event.
    fn activate(&mut self, event: &mut Event) {
        self.visited = true;
        event.post(Event::link_activate(self.link.target.clone()));
        event.clear();
    }
}

impl View for LinkLabel {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        // Moving or resizing the label (window drag/move) means the pointer is
        // no longer necessarily over it. Clear the hover so the old `link_hover`
        // pen cannot survive the move.
        if bounds != self.base.bounds() {
            self.hovered = false;
        }
        self.base.set_bounds(bounds);
    }

    fn draw(&self, canvas: &mut dyn Canvas, clip: Rect) {
        let bounds = self.base.bounds();
        if self.text.is_empty() || bounds.width == 0 || bounds.height == 0 {
            return;
        }

        let style = theme::with_current(|t| {
            // Focus is not hover: a focused but non-hovered label must not keep
            // the bold `link_hover` pen after the pointer leaves it.
            if self.hovered {
                t.link_hover
            } else if self.visited {
                t.link_visited
            } else {
                t.link
            }
        });

        canvas.draw_text_link(bounds.x, bounds.y, &self.text, style, &self.link, clip);
    }

    fn on_blur(&mut self) {
        self.hovered = false;
    }

    fn handle_event(&mut self, event: &mut Event) {
        match event.kind.clone() {
            EventKind::Mouse(mouse) => {
                let inside = self.contains(mouse.column, mouse.row);
                match mouse.kind {
                    MouseEventKind::Moved => {
                        self.hovered = inside;
                        if inside {
                            event.post(Event::link_hover(Some(self.link.target.clone())));
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if inside && mouse.modifiers.contains(self.activate_modifier) {
                            self.activate(event);
                        }
                    }
                    _ => {}
                }
            }
            EventKind::Key(key) => {
                if self.base.state() & SF_FOCUSED != 0
                    && matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
                {
                    self.activate(event);
                }
            }
            _ => {}
        }
    }

    fn can_focus(&self) -> bool {
        self.base.options() & OF_SELECTABLE != 0
    }

    fn state(&self) -> u16 {
        self.base.state()
    }

    fn set_state(&mut self, state: u16) {
        self.base.set_state(state);
    }

    fn options(&self) -> u16 {
        self.base.options()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CellCanvas;
    use crate::core::{Buffer, Cell, LinkTarget, Position, Style};
    use crate::theme;
    use crate::window::Window;
    use crossterm::event::{KeyEvent, MouseEvent};

    fn url_link() -> Link {
        Link::new(LinkTarget::Url("https://example.com".to_owned()))
    }

    fn label() -> LinkLabel {
        LinkLabel::new(Rect::new(2, 3, 10, 1), "example", url_link())
    }

    const fn mouse(
        column: u16,
        row: u16,
        kind: MouseEventKind,
        modifiers: KeyModifiers,
    ) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers,
        }
    }

    fn has_link_hover(event: &Event, expected: &LinkTarget) -> bool {
        event.deferred.iter().any(|deferred| {
            matches!(&deferred.kind, EventKind::LinkHover(Some(target)) if target == expected)
        })
    }

    fn has_link_activate(event: &Event, expected: &LinkTarget) -> bool {
        event.deferred.iter().any(|deferred| {
            matches!(&deferred.kind, EventKind::LinkActivate(target) if target == expected)
        })
    }

    /// Asserts that a drawn cell carries the colours and modifiers of `pen`.
    ///
    /// The cell's `underline_color` is always `Some` (default `Reset`) while
    /// the theme pen may leave it `None`, so only the meaningful parts are
    /// compared.
    fn assert_pen_matches(cell: &Cell, pen: Style) {
        let style = cell.style();
        assert_eq!(style.fg, pen.fg, "foreground");
        assert_eq!(style.bg, pen.bg, "background");
        assert_eq!(style.add_modifier, pen.add_modifier, "modifiers");
    }

    /// Returns `true` when `cell` carries the colours and modifiers of `pen`.
    fn cell_matches_pen(cell: &Cell, pen: Style) -> bool {
        let style = cell.style();
        style.fg == pen.fg && style.bg == pen.bg && style.add_modifier == pen.add_modifier
    }

    /// Returns `true` when any cell in `buf` still carries `pen`.
    fn buffer_has_pen(buf: &Buffer, pen: Style) -> bool {
        buf.content().iter().any(|cell| cell_matches_pen(cell, pen))
    }

    #[test]
    fn test_link_label_new_is_selectable_and_plain() {
        let label = label();
        assert_eq!(label.options() & OF_SELECTABLE, OF_SELECTABLE);
        assert!(label.can_focus());
        assert!(!label.is_hovered());
        assert!(!label.is_visited());
        assert_eq!(label.link(), &url_link());
    }

    #[test]
    fn test_link_label_move_inside_sets_hover_and_posts_event() {
        let mut label = label();
        let mut event = Event::mouse(mouse(5, 3, MouseEventKind::Moved, KeyModifiers::NONE));

        label.handle_event(&mut event);

        assert!(label.is_hovered());
        assert!(has_link_hover(&event, &label.link().target));
    }

    #[test]
    fn test_link_label_move_outside_clears_hover_without_posting() {
        let mut label = label();
        label.handle_event(&mut Event::mouse(mouse(
            5,
            3,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));

        let mut event = Event::mouse(mouse(50, 20, MouseEventKind::Moved, KeyModifiers::NONE));
        label.handle_event(&mut event);

        assert!(!label.is_hovered());
        assert!(event.deferred.is_empty());
    }

    #[test]
    fn test_link_label_ctrl_click_activates_and_clears() {
        let mut label = label();
        let mut event = Event::mouse(mouse(
            5,
            3,
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::CONTROL,
        ));

        label.handle_event(&mut event);

        assert!(label.is_visited());
        assert_eq!(event.deferred.len(), 1, "exactly one activation event");
        assert!(has_link_activate(&event, &label.link().target));
        assert!(event.is_cleared());
    }

    #[test]
    fn test_link_label_click_without_modifier_does_not_activate() {
        let mut label = label();
        let mut event = Event::mouse(mouse(
            5,
            3,
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::NONE,
        ));

        label.handle_event(&mut event);

        assert!(!label.is_visited());
        assert!(event.deferred.is_empty());
        assert!(!event.is_cleared());
    }

    #[test]
    fn test_link_label_click_with_custom_modifier_activates() {
        let mut label = label().with_activate_modifier(KeyModifiers::ALT);
        let mut event = Event::mouse(mouse(
            5,
            3,
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::ALT,
        ));

        label.handle_event(&mut event);

        assert!(label.is_visited());
        assert_eq!(event.deferred.len(), 1, "exactly one activation event");
        assert!(has_link_activate(&event, &label.link().target));
    }

    #[test]
    fn test_link_label_enter_when_focused_activates() {
        let mut label = label();
        label.set_state(label.state() | SF_FOCUSED);
        let mut event = Event::key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        label.handle_event(&mut event);

        assert!(label.is_visited());
        assert_eq!(event.deferred.len(), 1, "exactly one activation event");
        assert!(has_link_activate(&event, &label.link().target));
        assert!(event.is_cleared());
    }

    #[test]
    fn test_link_label_enter_when_not_focused_does_nothing() {
        let mut label = label();
        let mut event = Event::key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        label.handle_event(&mut event);

        assert!(!label.is_visited());
        assert!(event.deferred.is_empty());
    }

    #[test]
    fn test_link_label_set_visited() {
        let mut label = label();
        assert!(!label.is_visited());
        label.set_visited(true);
        assert!(label.is_visited());
        label.set_visited(false);
        assert!(!label.is_visited());
    }

    #[test]
    fn test_link_label_draw_uses_link_pens_and_tags_cells() {
        let clip = Rect::new(0, 0, 10, 2);

        let mut label = LinkLabel::new(Rect::new(1, 0, 5, 1), "abc", url_link());
        let expected_plain = theme::with_current(|t| t.link);
        let mut buf = Buffer::empty(clip);
        label.draw(&mut CellCanvas::new(&mut buf), clip);
        let cell = buf.cell(Position::new(1, 0)).expect("cell");
        assert_pen_matches(cell, expected_plain);
        assert!(cell.link().is_some(), "drawn cell must carry a link id");

        label.handle_event(&mut Event::mouse(mouse(
            2,
            0,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        assert!(label.is_hovered());
        let expected_hover = theme::with_current(|t| t.link_hover);
        let mut buf = Buffer::empty(clip);
        label.draw(&mut CellCanvas::new(&mut buf), clip);
        assert_pen_matches(buf.cell(Position::new(1, 0)).expect("cell"), expected_hover);

        let mut visited = LinkLabel::new(Rect::new(1, 0, 5, 1), "abc", url_link());
        visited.set_visited(true);
        let expected_visited = theme::with_current(|t| t.link_visited);
        let mut buf = Buffer::empty(clip);
        visited.draw(&mut CellCanvas::new(&mut buf), clip);
        assert_pen_matches(
            buf.cell(Position::new(1, 0)).expect("cell"),
            expected_visited,
        );

        let mut focused = LinkLabel::new(Rect::new(1, 0, 5, 1), "abc", url_link());
        focused.set_state(SF_FOCUSED);
        let mut buf = Buffer::empty(clip);
        focused.draw(&mut CellCanvas::new(&mut buf), clip);
        assert_pen_matches(buf.cell(Position::new(1, 0)).expect("cell"), expected_plain);
    }

    #[test]
    fn test_link_label_zero_size_draw_draws_nothing() {
        let label = LinkLabel::new(Rect::new(0, 0, 0, 0), "abc", url_link());
        let clip = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(clip);

        label.draw(&mut CellCanvas::new(&mut buf), clip);

        assert_eq!(buf.cell(Position::new(0, 0)).expect("cell").symbol(), " ");
    }

    #[test]
    fn test_link_label_draw_outside_clip_draws_nothing() {
        let label = LinkLabel::new(Rect::new(20, 20, 5, 1), "abc", url_link());
        let clip = Rect::new(0, 0, 10, 2);
        let mut buf = Buffer::empty(clip);

        label.draw(&mut CellCanvas::new(&mut buf), clip);

        assert_eq!(buf.cell(Position::new(0, 0)).expect("cell").symbol(), " ");
        assert!(buf.cell(Position::new(20, 20)).is_none());
    }

    #[test]
    fn test_link_label_hover_clears_when_pointer_leaves_window() {
        let mut window = Window::new(Rect::new(0, 0, 20, 8), "W");
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 10, 1),
            "example",
            url_link(),
        )));
        // Interior origin is (1, 1), so the label sits at absolute (2, 2).

        window.handle_event(&mut Event::mouse(mouse(
            2,
            2,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        {
            let child = window.interior().child_at(0).expect("label");
            let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
            assert!(label.is_hovered(), "label hovered inside the window");
        }

        window.handle_event(&mut Event::mouse(mouse(
            60,
            20,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        {
            let child = window.interior().child_at(0).expect("label");
            let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
            assert!(
                !label.is_hovered(),
                "label must clear hover once the pointer leaves the window"
            );
        }
    }

    #[test]
    fn test_link_label_move_away_leaves_no_hover_pen_in_buffer() {
        let clip = Rect::new(0, 0, 20, 4);
        let mut label = LinkLabel::new(Rect::new(1, 1, 10, 1), "example", url_link());
        let hover_pen = theme::with_current(|t| t.link_hover);

        label.handle_event(&mut Event::mouse(mouse(
            5,
            1,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        let mut buf = Buffer::empty(clip);
        label.draw(&mut CellCanvas::new(&mut buf), clip);
        assert!(
            buffer_has_pen(&buf, hover_pen),
            "hovering must paint the link_hover pen"
        );

        label.handle_event(&mut Event::mouse(mouse(
            19,
            3,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        assert!(!label.is_hovered());

        let mut buf = Buffer::empty(clip);
        label.draw(&mut CellCanvas::new(&mut buf), clip);
        assert!(
            !buffer_has_pen(&buf, hover_pen),
            "the hover pen must not survive after the pointer leaves"
        );
    }

    #[test]
    fn test_link_label_focus_change_clears_hover_pen() {
        let mut window = Window::new(Rect::new(0, 0, 20, 8), "W");
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 10, 1),
            "first",
            url_link(),
        )));
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 3, 10, 1),
            "second",
            url_link(),
        )));

        // Hover the first (auto-focused) label at absolute (2, 2).
        window.handle_event(&mut Event::mouse(mouse(
            2,
            2,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        {
            let child = window.interior().child_at(0).expect("label");
            let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
            assert!(label.is_hovered());
        }

        // Focus moves to the second label: the first must drop its hover.
        window.interior_mut().set_focus_to(1);
        let child = window.interior().child_at(0).expect("label");
        let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
        assert!(
            !label.is_hovered(),
            "focus change must clear the previous hover"
        );

        let clip = Rect::new(0, 0, 20, 8);
        let mut buf = Buffer::empty(clip);
        window.draw(&mut CellCanvas::new(&mut buf), clip);
        let hover_pen = theme::with_current(|t| t.link_hover);
        assert!(
            !buffer_has_pen(&buf, hover_pen),
            "no cell may keep the hover pen after the focus change"
        );
    }

    #[test]
    fn test_link_label_window_move_clears_hover_pen() {
        let mut window = Window::new(Rect::new(0, 0, 20, 8), "W");
        window.add(Box::new(LinkLabel::new(
            Rect::new(1, 1, 10, 1),
            "example",
            url_link(),
        )));

        window.handle_event(&mut Event::mouse(mouse(
            2,
            2,
            MouseEventKind::Moved,
            KeyModifiers::NONE,
        )));
        {
            let child = window.interior().child_at(0).expect("label");
            let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
            assert!(label.is_hovered());
        }

        // Dragging/moving the window repositions the label; hover must clear.
        window.set_bounds(Rect::new(10, 10, 20, 8));
        let child = window.interior().child_at(0).expect("label");
        let label = child.as_any().downcast_ref::<LinkLabel>().expect("label");
        assert!(
            !label.is_hovered(),
            "moving the window must clear the label hover"
        );

        let clip = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(clip);
        window.draw(&mut CellCanvas::new(&mut buf), clip);
        let hover_pen = theme::with_current(|t| t.link_hover);
        assert!(
            !buffer_has_pen(&buf, hover_pen),
            "no cell may keep the hover pen after the window moves"
        );
    }
}
