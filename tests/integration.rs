//! Integration tests for cross-component workflows.
//!

use turbo_tui::prelude::*;
use turbo_tui::menu_bar::MenuItem;
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
// Context Menu Tests
// ========================================================================

/// Test: right-click opens context menu at last mouse position.
#[test]
fn test_context_menu_opens_on_right_click() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    app.set_context_menu_items(vec![
        MenuItem::new("~T~est", 1000),
        MenuItem::separator(),
        MenuItem::new("~Q~uit", 1),
    ]);

    // Simulate right-click at (10, 10) which posts CM_CONTEXT_MENU
    let mut event = mouse_at(10, 10, MouseEventKind::Down(MouseButton::Right));
    app.dispatch(&mut event);

    // Overlay must be present with the MenuBox
    assert!(!app.overlay_manager().is_empty(), "context menu overlay must open on right-click");
}

/// Test: Escape dismisses context menu via overlay manager.
#[test]
fn test_context_menu_dismiss_on_escape() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    app.set_context_menu_items(vec![MenuItem::new("~T~est", 1000)]);

    // Open context menu
    app.dispatch(&mut mouse_at(10, 10, MouseEventKind::Down(MouseButton::Right)));
    assert!(!app.overlay_manager().is_empty());

    // Press Escape to dismiss
    use crossterm::event::{KeyCode, KeyEventKind};
    let escape = Event::key(crossterm::event::KeyEvent {
        code: KeyCode::Esc,
        modifiers: crossterm::event::KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    let mut ev = escape;
    app.dispatch(&mut ev);

    // Overlay must be dismissed by OverlayManager directly
    assert!(app.overlay_manager().is_empty(), "context menu must dismiss on Escape");
}

/// Test: clicking outside context menu dismisses it.
#[test]
fn test_context_menu_dismiss_on_outside_click() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    app.set_context_menu_items(vec![MenuItem::new("~T~est", 1000)]);

    // Open context menu at (10, 10)
    app.dispatch(&mut mouse_at(10, 10, MouseEventKind::Down(MouseButton::Right)));
    assert!(!app.overlay_manager().is_empty());

    // Click outside the menu (top-left corner, far from menu at 10,10)
    app.dispatch(&mut mouse_at(0, 0, MouseEventKind::Down(MouseButton::Left)));

    // Overlay must be dismissed
    assert!(app.overlay_manager().is_empty(), "context menu must dismiss on outside click");
}

// ========================================================================
// Drag-and-Drop Tests
// ========================================================================

/// Test: drag starts and ends correctly.
#[test]
fn test_drag_lifecycle() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));
    assert!(!app.is_dragging());

    // Simulate left mouse down at (5, 5)
    app.dispatch(&mut mouse_at(5, 5, MouseEventKind::Down(MouseButton::Left)));
    assert!(app.is_dragging(), "left click must start drag");
    assert!(app.drag_origin().is_some());

    // Simulate drag move
    app.dispatch(&mut mouse_at(10, 10, MouseEventKind::Drag(MouseButton::Left)));
    assert!(app.is_dragging());

    // Simulate drop (left mouse up)
    app.dispatch(&mut mouse_at(15, 15, MouseEventKind::Up(MouseButton::Left)));
    assert!(!app.is_dragging(), "drop must end drag");
    assert!(app.drag_origin().is_none());
}

/// Test: container routes drop to target with OF_DROP_TARGET.
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
        fn id(&self) -> ViewId { self.base.id() }
        fn bounds(&self) -> Rect { self.base.bounds() }
        fn set_bounds(&mut self, r: Rect) { self.base.set_bounds(r); }
        fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}
        fn handle_event(&mut self, _event: &mut Event) {}
        fn handle_drop(&mut self, _payload: Box<dyn Any>) -> bool {
            *self.accepted.borrow_mut() = true;
            true
        }
        fn can_focus(&self) -> bool { true }
        fn state(&self) -> u16 { self.base.state() }
        fn set_state(&mut self, s: u16) { self.base.set_state(s); }
        fn options(&self) -> u16 { self.base.options() }
        fn owner_type(&self) -> OwnerType { OwnerType::None }
        fn set_owner_type(&mut self, _o: OwnerType) {}
        fn end_state(&self) -> CommandId { 0 }
        fn set_end_state(&mut self, _c: CommandId) {}
        fn valid(&mut self, _cmd: CommandId) -> bool { true }
        fn as_any(&self) -> &dyn Any { self }
        fn as_any_mut(&mut self) -> &mut dyn Any { self }
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
    assert!(t.was_accepted(), "drop must be routed to OF_DROP_TARGET child");
}

/// Test: drag outside window ends without drop.
#[test]
fn test_drag_outside_no_drop() {
    let mut app = Application::new(Rect::new(0, 0, 80, 24));

    // Start drag inside
    app.dispatch(&mut mouse_at(5, 5, MouseEventKind::Down(MouseButton::Left)));
    assert!(app.is_dragging());

    // Move outside any window (drop on desktop background)
    app.dispatch(&mut mouse_at(70, 20, MouseEventKind::Up(MouseButton::Left)));

    // Drag ends but no drop target exists at that location
    assert!(!app.is_dragging());
    // Application still running
    assert!(app.is_running());
}
