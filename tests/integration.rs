//! Integration tests for cross-component workflows.
//!
//! Tests cover:
//! - Context menu overlay lifecycle (push, dismiss on Escape/outside-click)
//! - Window drag lifecycle (start on title bar, end on mouse up)
//! - Drop target routing through Container

use turbo_tui::prelude::*;
use turbo_tui::menu_bar::MenuItem;
use turbo_tui::menu_box::MenuBox;
use turbo_tui::overlay::Overlay;
use turbo_tui::view::ViewId;
use turbo_tui::window::Window;
use ratatui::layout::Rect;
use crossterm::event::{MouseButton, MouseEventKind};

/// Helper to create a mouse event at a given position.
fn mouse_at(col: u16, row: u16, kind: MouseEventKind) -> Event {
    Event::mouse(crossterm::event::MouseEvent {
        kind,
        column: col,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    })
}

// ========================================================================
// Overlay (Context Menu) Tests
// ========================================================================

/// Test: pushing a `MenuBox` overlay and dismissing it with Escape.
#[test]
fn test_overlay_dismiss_on_escape() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));

    // Create a context menu as a MenuBox overlay
    let items = vec![MenuItem::new("~T~est", 1000)];
    let bounds = MenuBox::calculate_bounds(10, 5, &items);
    let menu_box = MenuBox::new(bounds, items);

    app.overlay_manager_mut().push(Overlay {
        view: Box::new(menu_box),
        owner_id: ViewId::new(),
        dismiss_on_outside_click: true,
        dismiss_on_escape: true,
    });
    assert!(!app.overlay_manager().is_empty(), "overlay must be present after push");

    // Press Escape to dismiss
    let escape = Event::key(crossterm::event::KeyEvent {
        code: crossterm::event::KeyCode::Esc,
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    let mut ev = escape;
    app.dispatch(&mut ev);

    assert!(
        app.overlay_manager().is_empty(),
        "overlay must be dismissed on Escape"
    );
}

/// Test: clicking outside the overlay bounds dismisses it.
#[test]
fn test_overlay_dismiss_on_outside_click() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));

    // Create a MenuBox overlay at (10, 5)
    let items = vec![MenuItem::new("~T~est", 1000)];
    let bounds = MenuBox::calculate_bounds(10, 5, &items);
    let menu_box = MenuBox::new(bounds, items);

    app.overlay_manager_mut().push(Overlay {
        view: Box::new(menu_box),
        owner_id: ViewId::new(),
        dismiss_on_outside_click: true,
        dismiss_on_escape: true,
    });
    assert!(!app.overlay_manager().is_empty(), "overlay must be present after push");

    // Click outside the overlay (top-left corner, far from menu at 10,5)
    app.dispatch(&mut mouse_at(0, 0, MouseEventKind::Down(MouseButton::Left)));

    assert!(
        app.overlay_manager().is_empty(),
        "overlay must be dismissed on outside click"
    );
}

/// Test: basic push/pop sanity of overlay manager.
#[test]
fn test_overlay_can_be_pushed_and_popped() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));

    let items = vec![MenuItem::new("~T~est", 1000)];
    let bounds = MenuBox::calculate_bounds(10, 5, &items);
    let menu_box = MenuBox::new(bounds, items);

    app.overlay_manager_mut().push(Overlay {
        view: Box::new(menu_box),
        owner_id: ViewId::new(),
        dismiss_on_outside_click: true,
        dismiss_on_escape: true,
    });
    assert_eq!(app.overlay_manager().count(), 1, "must have 1 overlay");

    let popped = app.overlay_manager_mut().pop();
    assert!(popped.is_some(), "pop must return the overlay");
    assert!(
        app.overlay_manager().is_empty(),
        "must be empty after pop"
    );
}

// ========================================================================
// Window Drag Lifecycle Tests
// ========================================================================

/// Test: dragging starts when user clicks on the title bar.
#[test]
fn test_window_drag_starts_on_titlebar_mousedown() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    // Window at (10, 5, 30, 15): title bar is row 5
    let win = Window::new(Rect::new(10, 5, 30, 15), "Test Window");
    let _wid = app.add_window(win);

    // Click on title bar (col=20, row=5) — past close button at cols 11-13
    app.dispatch(&mut mouse_at(20, 5, MouseEventKind::Down(MouseButton::Left)));

    // Downcast to check drag state
    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(
        window.is_dragging(),
        "click on title bar must start dragging"
    );
}

/// Test: dragging ends when user releases the mouse button.
#[test]
fn test_window_drag_ends_on_mouseup() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    let win = Window::new(Rect::new(10, 5, 30, 15), "Test Window");
    let _wid = app.add_window(win);

    // Start drag
    app.dispatch(&mut mouse_at(20, 5, MouseEventKind::Down(MouseButton::Left)));
    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(window.is_dragging(), "drag must have started");

    // Release mouse button
    app.dispatch(&mut mouse_at(25, 10, MouseEventKind::Up(MouseButton::Left)));

    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(
        !window.is_dragging(),
        "mouse up must end dragging"
    );
}

/// Test: clicking on the window interior (not title bar) does NOT start drag.
#[test]
fn test_window_outside_titlebar_no_drag() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    let win = Window::new(Rect::new(10, 5, 30, 15), "Test Window");
    let _wid = app.add_window(win);

    // Click on interior area (col=20, row=12 — inside the window but not title bar)
    app.dispatch(&mut mouse_at(20, 12, MouseEventKind::Down(MouseButton::Left)));

    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(
        !window.is_dragging(),
        "click on interior must NOT start dragging"
    );
}

// ========================================================================
// Drop Target Tests
// ========================================================================

/// Test: container routes drop to target with `OF_DROP_TARGET`.
#[test]
fn test_drop_routes_to_drop_target() {
    use std::any::Any;
    use std::cell::RefCell;
    use turbo_tui::view::ViewBase;
    use ratatui::buffer::Buffer;

    // A view that accepts drops
    struct DropTargetView {
        base: ViewBase,
        accepted: RefCell<bool>,
    }

    impl DropTargetView {
        fn new(bounds: Rect) -> Self {
            Self {
                base: ViewBase::with_options(bounds, OF_DROP_TARGET | OF_SELECTABLE),
                accepted: RefCell::new(false),
            }
        }
        fn was_accepted(&self) -> bool {
            *self.accepted.borrow()
        }
    }

    impl View for DropTargetView {
        fn id(&self) -> ViewId {
            self.base.id()
        }
        fn bounds(&self) -> Rect {
            self.base.bounds()
        }
        fn set_bounds(&mut self, r: Rect) {
            self.base.set_bounds(r);
        }
        fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}
        fn handle_event(&mut self, _event: &mut Event) {}
        fn handle_drop(&mut self, _payload: Box<dyn Any>) -> bool {
            *self.accepted.borrow_mut() = true;
            true
        }
        fn can_focus(&self) -> bool {
            true
        }
        fn state(&self) -> u16 {
            self.base.state()
        }
        fn set_state(&mut self, s: u16) {
            self.base.set_state(s);
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

    let mut container = Container::new(Rect::new(0, 0, 80, 24));
    let target = Box::new(DropTargetView::new(Rect::new(10, 10, 20, 10)));
    container.add(target);

    // Simulate drop on the target
    let mut event = mouse_at(15, 15, MouseEventKind::Up(MouseButton::Left));
    container.handle_event(&mut event);

    let t = container
        .child_at(0)
        .unwrap()
        .as_any()
        .downcast_ref::<DropTargetView>()
        .expect("DropTargetView must be present");
    assert!(
        t.was_accepted(),
        "drop must be routed to OF_DROP_TARGET child"
    );
}

/// Test: drag outside window ends without crash.
#[test]
fn test_drag_outside_no_drop() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));

    // Add a window at (10, 5, 30, 15)
    let win = Window::new(Rect::new(10, 5, 30, 15), "Test Window");
    let _wid = app.add_window(win);

    // Start drag on title bar
    app.dispatch(&mut mouse_at(20, 5, MouseEventKind::Down(MouseButton::Left)));
    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(window.is_dragging(), "drag must have started");

    // Move outside (drop on desktop background area)
    app.dispatch(&mut mouse_at(70, 20, MouseEventKind::Up(MouseButton::Left)));

    let win_ref = app.desktop().windows().child_at(0).unwrap();
    let window = win_ref
        .as_any()
        .downcast_ref::<Window>()
        .expect("child must be a Window");
    assert!(!window.is_dragging(), "mouse up must end dragging");

    // Application still running
    assert!(app.is_running(), "app must still be running after drag");
}
