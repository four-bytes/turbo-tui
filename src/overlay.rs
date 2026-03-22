//! Overlay system for floating views rendered above all windows.
//!
//! Overlays are used for:
//! - Menu dropdowns (opened by `MenuBar`)
//! - Tooltips
//! - Autocomplete popups
//!
//! The [`OverlayManager`] maintains a stack of overlays where the topmost
//! overlay receives events first. If it doesn't consume the event, it falls
//! through to the normal view hierarchy.

use crate::command::CM_DROPDOWN_CLOSED;
use crate::view::{Event, EventKind, View, ViewId};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// Direction a dropdown should open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropDirection {
    /// Open downward from the anchor point.
    Down,
    /// Open upward from the anchor point.
    Up,
}

/// A single overlay entry — a floating view with dismiss behavior.
pub struct Overlay {
    /// The view to render (e.g., `MenuBox`).
    pub view: Box<dyn View>,
    /// Who opened this overlay (for closing logic).
    pub owner_id: ViewId,
    /// Close this overlay on click outside?
    pub dismiss_on_outside_click: bool,
    /// Close this overlay on Escape?
    pub dismiss_on_escape: bool,
}

/// Manages a stack of floating overlays rendered above all windows.
///
/// Overlays are used for:
/// - Menu dropdowns (opened by `MenuBar`)
/// - Tooltips
/// - Autocomplete popups
///
/// The topmost overlay receives events first. If it doesn't consume the event,
/// it falls through to the next overlay, then to the normal view hierarchy.
///
/// # Event Routing
///
/// 1. Topmost overlay gets the event first
/// 2. On `MouseDown` outside overlay bounds → dismiss if `dismiss_on_outside_click`
/// 3. On `Escape` key → dismiss if `dismiss_on_escape`
/// 4. On `MouseDown` inside → deliver to overlay's view
pub struct OverlayManager {
    /// Stack of active overlays (last = topmost).
    overlays: Vec<Overlay>,
    /// Current screen size for overflow calculations.
    screen_size: (u16, u16),
}

impl OverlayManager {
    /// Create a new empty overlay manager.
    #[must_use]
    pub fn new(screen_width: u16, screen_height: u16) -> Self {
        Self {
            overlays: Vec::new(),
            screen_size: (screen_width, screen_height),
        }
    }

    /// Push a new overlay onto the stack (becomes topmost).
    pub fn push(&mut self, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    /// Pop the topmost overlay.
    pub fn pop(&mut self) -> Option<Overlay> {
        self.overlays.pop()
    }

    /// Pop all overlays owned by the given `ViewId`.
    pub fn pop_by_owner(&mut self, owner_id: ViewId) {
        self.overlays.retain(|o| o.owner_id != owner_id);
    }

    /// Pop all overlays (clear the stack).
    pub fn clear(&mut self) {
        self.overlays.clear();
    }

    /// Return the number of active overlays.
    #[must_use]
    pub fn count(&self) -> usize {
        self.overlays.len()
    }

    /// Check if any overlays are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    /// Check if a specific owner has an active overlay.
    #[must_use]
    pub fn has_overlay_for(&self, owner_id: ViewId) -> bool {
        self.overlays.iter().any(|o| o.owner_id == owner_id)
    }

    /// Iterate over all active overlays (bottom to top).
    ///
    /// Used by `Application` to inspect overlay contents (e.g., read
    /// `MenuBox::navigate_direction()` before dismissing).
    pub fn overlays_iter(&self) -> impl Iterator<Item = &Overlay> {
        self.overlays.iter()
    }

    /// Update screen size (for overflow calculations).
    pub fn set_screen_size(&mut self, width: u16, height: u16) {
        self.screen_size = (width, height);
    }

    /// Get the current screen size.
    #[must_use]
    pub fn screen_size(&self) -> (u16, u16) {
        self.screen_size
    }

    /// Draw all overlays (bottom to top).
    pub fn draw(&self, buf: &mut Buffer, clip: Rect) {
        for overlay in &self.overlays {
            overlay.view.draw(buf, clip);
        }
    }

    /// Handle an event.
    ///
    /// Routes to the topmost overlay first. Returns `true` if any overlay consumed the event.
    ///
    /// **Dismiss logic:**
    /// - `Escape` key: dismiss topmost overlay if `dismiss_on_escape` is set
    /// - `MouseDown` outside all overlays: dismiss topmost overlay if `dismiss_on_outside_click` is set
    /// - `MouseDown` inside an overlay: deliver to that overlay's view
    pub fn handle_event(&mut self, event: &mut Event) -> bool {
        if self.overlays.is_empty() || event.is_cleared() {
            return false;
        }

        match &event.kind.clone() {
            EventKind::Key(key) => self.handle_key_event(event, key),

            EventKind::Mouse(mouse) => self.handle_mouse_event(event, *mouse),

            // Commands and broadcasts — forward to topmost overlay
            EventKind::Command(_) | EventKind::Broadcast(_) => {
                if let Some(top) = self.overlays.last_mut() {
                    top.view.handle_event(event);
                    return event.is_cleared();
                }
                false
            }

            _ => false,
        }
    }

    /// Handle a keyboard event.
    fn handle_key_event(&mut self, event: &mut Event, key: &KeyEvent) -> bool {
        // Escape dismisses topmost overlay with dismiss_on_escape.
        // Posts CM_DROPDOWN_CLOSED so the owning bar can reset its state.
        if key.code == KeyCode::Esc {
            if let Some(top) = self.overlays.last() {
                if top.dismiss_on_escape {
                    self.overlays.pop();
                    event.post(Event::command(CM_DROPDOWN_CLOSED));
                    event.clear();
                    return true;
                }
            }
        }

        // Forward key events to topmost overlay
        if let Some(top) = self.overlays.last_mut() {
            top.view.handle_event(event);
            return event.is_cleared();
        }
        false
    }

    /// Handle a mouse event.
    fn handle_mouse_event(&mut self, event: &mut Event, mouse: MouseEvent) -> bool {
        let col = mouse.column;
        let row = mouse.row;

        // Check if click is inside any overlay (top to bottom)
        for i in (0..self.overlays.len()).rev() {
            let b = self.overlays[i].view.bounds();
            if col >= b.x && col < b.x + b.width && row >= b.y && row < b.y + b.height {
                // Hit — deliver to this overlay
                self.overlays[i].view.handle_event(event);
                return true; // Overlay consumed (even if not handled)
            }
        }

        // Click outside all overlays
        if matches!(mouse.kind, MouseEventKind::Down(_)) {
            if let Some(top) = self.overlays.last() {
                if top.dismiss_on_outside_click {
                    self.overlays.pop();
                    // Notify owning bar that dropdown was dismissed
                    event.post(Event::command(CM_DROPDOWN_CLOSED));
                    // Don't clear the event — let it fall through to windows
                    return true; // We handled the dismiss
                }
            }
        }

        false
    }
}

/// Calculate overlay position with overflow detection.
///
/// Given an anchor point and desired size, determines where to place
/// the overlay so it fits on screen. Tries the preferred direction first,
/// then flips if needed. Also shifts horizontally if it would overflow the right edge.
///
/// Returns the calculated `Rect` and actual `DropDirection` used.
#[must_use]
pub fn calculate_overlay_bounds(
    anchor: (u16, u16),
    size: (u16, u16),
    screen: Rect,
    preferred: DropDirection,
) -> (Rect, DropDirection) {
    let (anchor_x, anchor_y) = anchor;
    let (width, height) = size;

    // Try preferred direction
    let (y, direction) = match preferred {
        DropDirection::Down => {
            let y = anchor_y.saturating_add(1);
            if y.saturating_add(height) <= screen.y.saturating_add(screen.height) {
                (y, DropDirection::Down)
            } else {
                // Try up
                let up_y = anchor_y.saturating_sub(height);
                if up_y >= screen.y {
                    (up_y, DropDirection::Up)
                } else {
                    // Fall back to down, will be clipped
                    (y, DropDirection::Down)
                }
            }
        }
        DropDirection::Up => {
            let up_y = anchor_y.saturating_sub(height);
            if up_y >= screen.y {
                (up_y, DropDirection::Up)
            } else {
                // Try down
                let y = anchor_y.saturating_add(1);
                if y.saturating_add(height) <= screen.y.saturating_add(screen.height) {
                    (y, DropDirection::Down)
                } else {
                    (up_y, DropDirection::Up)
                }
            }
        }
    };

    // Horizontal: shift left if right overflow
    let x = if anchor_x.saturating_add(width) > screen.x.saturating_add(screen.width) {
        screen.x.saturating_add(screen.width).saturating_sub(width)
    } else {
        anchor_x
    };

    (Rect::new(x, y, width, height), direction)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::ViewBase;
    use crossterm::event::MouseButton;
    use std::any::Any;

    /// Test overlay view that tracks received events.
    struct TestOverlayView {
        base: ViewBase,
        received_events: std::cell::RefCell<Vec<String>>,
    }

    impl TestOverlayView {
        fn new(bounds: Rect) -> Self {
            Self {
                base: ViewBase::new(bounds),
                received_events: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl View for TestOverlayView {
        fn id(&self) -> ViewId {
            self.base.id()
        }

        fn bounds(&self) -> Rect {
            self.base.bounds()
        }

        fn set_bounds(&mut self, bounds: Rect) {
            self.base.set_bounds(bounds);
        }

        fn draw(&self, _buf: &mut Buffer, _clip: Rect) {
            // Test implementation - draw nothing
        }

        fn handle_event(&mut self, event: &mut Event) {
            let desc = match &event.kind {
                EventKind::Key(k) => format!("Key({:?})", k.code),
                EventKind::Mouse(m) => format!("Mouse({:?})", m.kind),
                EventKind::Command(c) => format!("Command({c})"),
                EventKind::Broadcast(c) => format!("Broadcast({c})"),
                EventKind::Resize(w, h) => format!("Resize({w},{h})"),
                EventKind::None => "None".to_string(),
            };
            self.received_events.borrow_mut().push(desc);
            event.clear();
        }

        fn state(&self) -> u16 {
            self.base.state()
        }

        fn set_state(&mut self, state: u16) {
            self.base.set_state(state);
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    // Helper to create a key event
    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    // Helper to create a mouse event
    fn mouse_down(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row: row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_overlay_manager_new() {
        let manager = OverlayManager::new(80, 24);
        assert_eq!(manager.count(), 0);
        assert!(manager.is_empty());
        assert_eq!(manager.screen_size(), (80, 24));
    }

    #[test]
    fn test_overlay_push_pop() {
        let mut manager = OverlayManager::new(80, 24);
        let view1 = TestOverlayView::new(Rect::new(0, 0, 10, 5));
        let owner1 = ViewId::new();
        let overlay1 = Overlay {
            view: Box::new(view1),
            owner_id: owner1,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };

        manager.push(overlay1);
        assert_eq!(manager.count(), 1);
        assert!(!manager.is_empty());

        let view2 = TestOverlayView::new(Rect::new(5, 5, 10, 5));
        let owner2 = ViewId::new();
        let overlay2 = Overlay {
            view: Box::new(view2),
            owner_id: owner2,
            dismiss_on_outside_click: true,
            dismiss_on_escape: false,
        };

        manager.push(overlay2);
        assert_eq!(manager.count(), 2);

        let popped = manager.pop();
        assert!(popped.is_some());
        assert_eq!(manager.count(), 1);
        assert_eq!(popped.unwrap().owner_id, owner2);

        let popped = manager.pop();
        assert!(popped.is_some());
        assert_eq!(manager.count(), 0);

        let popped = manager.pop();
        assert!(popped.is_none());
    }

    #[test]
    fn test_overlay_pop_by_owner() {
        let mut manager = OverlayManager::new(80, 24);

        let owner_a = ViewId::new();
        let owner_b = ViewId::new();
        let owner_c = ViewId::new();

        // Push three overlays: A, B, C (C is topmost)
        let overlay_a = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner_a,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        let overlay_b = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(5, 5, 10, 5))),
            owner_id: owner_b,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        let overlay_c = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(10, 10, 10, 5))),
            owner_id: owner_c,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };

        manager.push(overlay_a);
        manager.push(overlay_b);
        manager.push(overlay_c);

        assert_eq!(manager.count(), 3);

        // Remove all overlays for owner_b
        manager.pop_by_owner(owner_b);
        assert_eq!(manager.count(), 2);
        assert!(manager.has_overlay_for(owner_a));
        assert!(!manager.has_overlay_for(owner_b));
        assert!(manager.has_overlay_for(owner_c));

        // Remove remaining for owner_a
        manager.pop_by_owner(owner_a);
        assert_eq!(manager.count(), 1);
        assert!(!manager.has_overlay_for(owner_a));

        // Remove remaining for owner_c
        manager.pop_by_owner(owner_c);
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_overlay_clear() {
        let mut manager = OverlayManager::new(80, 24);

        let owner = ViewId::new();
        let overlay1 = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        let overlay2 = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(5, 5, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };

        manager.push(overlay1);
        manager.push(overlay2);
        assert_eq!(manager.count(), 2);

        manager.clear();
        assert_eq!(manager.count(), 0);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_overlay_has_overlay_for() {
        let mut manager = OverlayManager::new(80, 24);
        let owner1 = ViewId::new();
        let owner2 = ViewId::new();

        assert!(!manager.has_overlay_for(owner1));
        assert!(!manager.has_overlay_for(owner2));

        let overlay1 = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner1,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        manager.push(overlay1);

        assert!(manager.has_overlay_for(owner1));
        assert!(!manager.has_overlay_for(owner2));
    }

    #[test]
    fn test_overlay_dismiss_on_escape() {
        let mut manager = OverlayManager::new(80, 24);
        let owner = ViewId::new();

        // Push overlay with dismiss_on_escape = true
        let overlay = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        manager.push(overlay);
        assert_eq!(manager.count(), 1);

        // Send Escape key
        let mut event = Event::key(key_event(KeyCode::Esc));
        let handled = manager.handle_event(&mut event);
        assert!(handled);
        assert!(event.is_cleared());
        assert_eq!(manager.count(), 0); // Overlay was dismissed
    }

    #[test]
    fn test_overlay_no_dismiss_on_escape_when_disabled() {
        let mut manager = OverlayManager::new(80, 24);
        let owner = ViewId::new();

        // Push overlay with dismiss_on_escape = false
        let overlay = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: false,
        };
        manager.push(overlay);
        assert_eq!(manager.count(), 1);

        // Send Escape key
        let mut event = Event::key(key_event(KeyCode::Esc));
        let handled = manager.handle_event(&mut event);
        assert!(handled); // Still handled (forwarded to overlay view)
        assert_eq!(manager.count(), 1); // Overlay was NOT dismissed
    }

    #[test]
    fn test_overlay_dismiss_on_outside_click() {
        let mut manager = OverlayManager::new(80, 24);
        let owner = ViewId::new();

        // Push overlay at (10, 10) with size (10, 5)
        let overlay = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(10, 10, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        manager.push(overlay);
        assert_eq!(manager.count(), 1);

        // Click outside the overlay
        let mut event = Event::mouse(mouse_down(0, 0)); // Click at (0, 0) — outside
        let handled = manager.handle_event(&mut event);
        assert!(handled);
        assert!(!event.is_cleared()); // Event NOT cleared — should fall through to windows
        assert_eq!(manager.count(), 0); // Overlay was dismissed
    }

    #[test]
    fn test_overlay_click_inside_delivers() {
        let mut manager = OverlayManager::new(80, 24);
        let owner = ViewId::new();

        // Push overlay at (10, 10) with size (10, 5)
        let overlay = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(10, 10, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        manager.push(overlay);
        assert_eq!(manager.count(), 1);

        // Click inside the overlay (at x=12, y=12 which is inside 10..20, 10..15)
        let mut event = Event::mouse(mouse_down(12, 12));
        let handled = manager.handle_event(&mut event);
        assert!(handled);
        assert_eq!(manager.count(), 1); // Overlay NOT dismissed
    }

    #[test]
    fn test_overlay_key_forwarded_to_topmost() {
        let mut manager = OverlayManager::new(80, 24);

        let owner1 = ViewId::new();
        let owner2 = ViewId::new();

        // Push first overlay
        let overlay1 = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner1,
            dismiss_on_outside_click: true,
            dismiss_on_escape: false,
        };
        manager.push(overlay1);

        // Push second overlay (topmost)
        let overlay2 = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(5, 5, 10, 5))),
            owner_id: owner2,
            dismiss_on_outside_click: true,
            dismiss_on_escape: false,
        };
        manager.push(overlay2);

        // Send a character key
        let mut event = Event::key(key_event(KeyCode::Char('a')));
        let handled = manager.handle_event(&mut event);
        assert!(handled);

        // Get the topmost view and check it received the event
        let top_view = manager.overlays.last().unwrap();
        let received = top_view
            .view
            .as_any()
            .downcast_ref::<TestOverlayView>()
            .unwrap()
            .received_events
            .borrow();
        assert!(received.iter().any(|e| e.contains("Char('a')")));
    }

    #[test]
    fn test_calculate_overlay_bounds_down() {
        // Preferred Down, enough space below
        let anchor = (10u16, 5u16);
        let size = (20u16, 10u16);
        let screen = Rect::new(0, 0, 80, 25);
        let (rect, direction) = calculate_overlay_bounds(anchor, size, screen, DropDirection::Down);

        assert_eq!(direction, DropDirection::Down);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 6); // anchor_y + 1
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_calculate_overlay_bounds_flip() {
        // Preferred Down, but not enough space below -> flip to Up
        let anchor = (10u16, 20u16); // Near bottom
        let size = (20u16, 15u16); // Needs 15 rows
        let screen = Rect::new(0, 0, 80, 25); // Screen height = 25
        let (rect, direction) = calculate_overlay_bounds(anchor, size, screen, DropDirection::Down);

        // anchor_y + 1 = 21, + 15 = 36 > 25, so flip to Up
        assert_eq!(direction, DropDirection::Up);
        assert_eq!(rect.y, 5); // anchor_y - height = 20 - 15 = 5
    }

    #[test]
    fn test_calculate_overlay_bounds_right_overflow() {
        // Would overflow right edge -> shift left
        let anchor = (70u16, 5u16); // Close to right
        let size = (20u16, 10u16); // Width 20
        let screen = Rect::new(0, 0, 80, 25); // Screen width = 80
        let (rect, direction) = calculate_overlay_bounds(anchor, size, screen, DropDirection::Down);

        // anchor_x + width = 70 + 20 = 90 > 80, so shift left
        assert_eq!(direction, DropDirection::Down);
        assert_eq!(rect.x, 60); // 80 - 20 = 60
        assert_eq!(rect.width, 20);
    }

    #[test]
    fn test_calculate_overlay_bounds_up_preferred() {
        // Preferred Up with enough space above
        let anchor = (10u16, 20u16);
        let size = (20u16, 10u16);
        let screen = Rect::new(0, 0, 80, 25);
        let (rect, direction) = calculate_overlay_bounds(anchor, size, screen, DropDirection::Up);

        assert_eq!(direction, DropDirection::Up);
        assert_eq!(rect.y, 10); // anchor_y - height = 20 - 10
    }

    #[test]
    fn test_overlay_set_screen_size() {
        let mut manager = OverlayManager::new(80, 24);
        assert_eq!(manager.screen_size(), (80, 24));

        manager.set_screen_size(120, 40);
        assert_eq!(manager.screen_size(), (120, 40));
    }

    #[test]
    fn test_overlay_command_forwarded() {
        let mut manager = OverlayManager::new(80, 24);
        let owner = ViewId::new();

        let overlay = Overlay {
            view: Box::new(TestOverlayView::new(Rect::new(0, 0, 10, 5))),
            owner_id: owner,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        };
        manager.push(overlay);

        // Send a command event
        let mut event = Event::command(42);
        let handled = manager.handle_event(&mut event);
        assert!(handled);
        assert!(event.is_cleared());

        // Check the overlay received it
        let top_view = manager.overlays.last().unwrap();
        let received = top_view
            .view
            .as_any()
            .downcast_ref::<TestOverlayView>()
            .unwrap()
            .received_events
            .borrow();
        assert!(received.iter().any(|e| e.contains("Command(42)")));
    }

    #[test]
    fn test_overlay_empty_no_crash() {
        let mut manager = OverlayManager::new(80, 24);

        // Sending events to empty manager should not crash
        let mut event = Event::key(key_event(KeyCode::Char('a')));
        assert!(!manager.handle_event(&mut event));

        let mut event = Event::mouse(mouse_down(10, 10));
        assert!(!manager.handle_event(&mut event));

        let mut event = Event::command(42);
        assert!(!manager.handle_event(&mut event));
    }

    #[test]
    fn test_outside_click_dismiss_posts_dropdown_closed() {
        let mut mgr = OverlayManager::new(80, 24);
        let view = TestOverlayView::new(Rect::new(10, 10, 20, 5));
        let id = view.id();
        mgr.push(Overlay {
            view: Box::new(view),
            owner_id: id,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        });
        assert_eq!(mgr.count(), 1);

        // Click outside the overlay bounds
        let mut event = Event::mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        mgr.handle_event(&mut event);

        // Overlay should be dismissed
        assert_eq!(mgr.count(), 0, "outside click must dismiss overlay");
        // Should have posted CM_DROPDOWN_CLOSED
        assert!(
            event
                .deferred
                .iter()
                .any(|e| matches!(e.kind, EventKind::Command(cmd) if cmd == CM_DROPDOWN_CLOSED)),
            "outside-click dismiss must post CM_DROPDOWN_CLOSED"
        );
        // Event should NOT be cleared (falls through to windows)
        assert!(
            !event.is_cleared(),
            "event must not be cleared after outside-click dismiss"
        );
    }
}
