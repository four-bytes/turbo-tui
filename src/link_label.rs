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
/// Hovered or focused:  underlined and bold (theme `link_hover`)
/// Visited:             underlined (theme `link_visited`)
/// Plain:               theme `link`
/// ```
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
        self.base.set_bounds(bounds);
    }

    fn draw(&self, canvas: &mut dyn Canvas, clip: Rect) {
        let style = theme::with_current(|t| {
            if self.hovered || self.base.state() & SF_FOCUSED != 0 {
                t.link_hover
            } else if self.visited {
                t.link_visited
            } else {
                t.link
            }
        });

        let bounds = self.base.bounds();
        canvas.draw_text_link(bounds.x, bounds.y, &self.text, style, &self.link, clip);
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
    use crate::core::LinkTarget;
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
        assert!(has_link_activate(&event, &label.link().target));
    }

    #[test]
    fn test_link_label_enter_when_focused_activates() {
        let mut label = label();
        label.set_state(label.state() | SF_FOCUSED);
        let mut event = Event::key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        label.handle_event(&mut event);

        assert!(label.is_visited());
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
}
