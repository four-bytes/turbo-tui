//! Container — Container view with Z-order and three-phase event dispatch.
//!
//! `Container` is the core container for turbo-tui. It manages child views with:
//! - Relative coordinate system (children added with coords relative to container)
//! - Z-order (Vec index 0 = back, last = front)
//! - Three-phase keyboard/command dispatch (`PreProcess` → `Focused` → `PostProcess`)
//! - Reverse Z-order mouse hit-testing with mouse capture
//! - Position delta propagation on `set_bounds()`
//! - Intersection clipping on `draw()`

mod dispatch;
mod draw;

#[allow(unused_imports)]
use crate::view::{
    Event, EventKind, OwnerType, View, ViewBase, ViewId, OF_POST_PROCESS, OF_PRE_PROCESS,
    OF_SELECTABLE, SF_DRAGGING, SF_RESIZING, SF_VISIBLE,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;

/// Container view that manages child views with Z-order and three-phase event dispatch.
///
/// Children are added with **relative** coordinates (relative to this container's
/// top-left corner). `add()` converts them to absolute before inserting.
///
/// # Z-order
///
/// Children are stored in a `Vec`. Index 0 is the back (drawn first), the last
/// index is the front (drawn last, hit-tested first for mouse events).
///
/// # Event dispatch
///
/// - **Key/Command:** Three-phase: `PreProcess` → `Focused` → `PostProcess`
/// - **Mouse:** Reverse Z-order hit-test; Drag/Up captured by focused child if
///   it has `SF_DRAGGING` or `SF_RESIZING` set.
/// - **Broadcast/Resize:** All children.
pub struct Container {
    base: ViewBase,
    children: Vec<Box<dyn View>>,
    /// Index of the currently focused child, if any.
    focused: Option<usize>,
}

impl Container {
    /// Create a new empty container with the given bounds.
    ///
    /// The container starts with no children and no focus.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            children: Vec::new(),
            focused: None,
        }
    }

    /// Add a child view to this container.
    ///
    /// **CRITICAL:** `child.bounds()` must be in **relative** coordinates
    /// (relative to this container's top-left corner). This method converts them
    /// to absolute by adding `(container.x, container.y)`.
    ///
    /// Returns the child's [`ViewId`] for later lookup.
    pub fn add(&mut self, mut child: Box<dyn View>) -> ViewId {
        let id = child.id();
        let gb = self.base.bounds();
        let cb = child.bounds();
        child.set_bounds(Rect::new(gb.x + cb.x, gb.y + cb.y, cb.width, cb.height));
        let can_focus = child.can_focus();
        self.children.push(child);

        // Auto-focus the first focusable child
        if self.focused.is_none() && can_focus {
            self.focused = Some(self.children.len() - 1);
        }

        self.base.mark_dirty();
        id
    }

    /// Remove the child at `index` and return it.
    ///
    /// Adjusts the `focused` index if necessary:
    /// - If the removed child was focused, focus is cleared.
    /// - If a child after the removed one was focused, the index is decremented.
    pub fn remove(&mut self, index: usize) -> Option<Box<dyn View>> {
        if index >= self.children.len() {
            return None;
        }
        let removed = self.children.remove(index);
        // Adjust focused index
        match self.focused {
            Some(f) if f == index => self.focused = None,
            Some(f) if f > index => self.focused = Some(f - 1),
            _ => {}
        }
        self.base.mark_dirty();
        Some(removed)
    }

    /// Remove a child by its [`ViewId`].
    ///
    /// Returns the removed child, or `None` if not found.
    pub fn remove_by_id(&mut self, id: ViewId) -> Option<Box<dyn View>> {
        let index = self.children.iter().position(|c| c.id() == id)?;
        self.remove(index)
    }

    /// Return the number of children.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Return an immutable reference to the child at `index`.
    #[must_use]
    pub fn child_at(&self, index: usize) -> Option<&dyn View> {
        self.children.get(index).map(AsRef::as_ref)
    }

    /// Return a mutable reference to the child at `index`.
    pub fn child_at_mut(&mut self, index: usize) -> Option<&mut dyn View> {
        // SAFETY: We dereference through Box<dyn View> which is 'static-bounded.
        // Using a raw pointer here to break the lifetime chain is unnecessary;
        // instead we use DerefMut directly.
        if index < self.children.len() {
            Some(&mut *self.children[index])
        } else {
            None
        }
    }

    /// Find a child by [`ViewId`] and return an immutable reference.
    #[must_use]
    pub fn child_by_id(&self, id: ViewId) -> Option<&dyn View> {
        self.children
            .iter()
            .find(|c| c.id() == id)
            .map(AsRef::as_ref)
    }

    /// Find a child by [`ViewId`] and return a mutable reference.
    pub fn child_by_id_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        for i in 0..self.children.len() {
            if self.children[i].id() == id {
                return Some(&mut *self.children[i]);
            }
        }
        None
    }

    /// Return a slice of all children.
    #[must_use]
    pub fn children(&self) -> &[Box<dyn View>] {
        &self.children
    }

    /// Hit-test in reverse Z-order (front to back).
    ///
    /// Returns the index of the topmost visible child whose bounds contain
    /// the point `(col, row)`, or `None`.
    #[must_use]
    pub fn child_at_point(&self, col: u16, row: u16) -> Option<usize> {
        for i in (0..self.children.len()).rev() {
            let child = &self.children[i];
            let b = child.bounds();
            if child.state() & SF_VISIBLE != 0
                && col >= b.x
                && col < b.x + b.width
                && row >= b.y
                && row < b.y + b.height
            {
                return Some(i);
            }
        }
        None
    }

    /// Move the child at `index` to the front (last position in the Vec).
    ///
    /// If the moved child was focused, the focused index is updated.
    pub fn bring_to_front(&mut self, index: usize) {
        if index >= self.children.len() {
            return;
        }
        let last = self.children.len() - 1;
        if index == last {
            return;
        }
        // Rotate the slice so index moves to the end
        self.children[index..].rotate_left(1);
        // Adjust focused index
        if let Some(f) = self.focused {
            if f == index {
                self.focused = Some(last);
            } else if f > index {
                self.focused = Some(f - 1);
            }
        }
        self.base.mark_dirty();
    }

    /// Move the child at `index` to the back (position 0 in the Vec).
    ///
    /// If the moved child was focused, the focused index is updated.
    pub fn send_to_back(&mut self, index: usize) {
        if index == 0 || index >= self.children.len() {
            return;
        }
        self.children[..=index].rotate_right(1);
        // Adjust focused index
        if let Some(f) = self.focused {
            if f == index {
                self.focused = Some(0);
            } else if f < index {
                self.focused = Some(f + 1);
            }
        }
        self.base.mark_dirty();
    }

    /// Set focus to the child at `index`.
    ///
    /// Clears `SF_FOCUSED` on the previously focused child and sets it on
    /// the new one. Calls `on_blur()` on the old child and `on_focus()` on
    /// the new child. If `index` is out of bounds the call is a no-op.
    pub fn set_focus_to(&mut self, index: usize) {
        if index >= self.children.len() {
            return;
        }
        // Unfocus old
        if let Some(old) = self.focused {
            if old < self.children.len() {
                let st = self.children[old].state();
                self.children[old].set_state(st & !crate::view::SF_FOCUSED);
                self.children[old].on_blur();
            }
        }
        // Focus new
        let st = self.children[index].state();
        self.children[index].set_state(st | crate::view::SF_FOCUSED);
        self.children[index].on_focus();
        self.focused = Some(index);
    }

    /// Focus the child with the given [`ViewId`].
    ///
    /// No-op if the ID is not found.
    pub fn focus_by_id(&mut self, id: ViewId) {
        if let Some(index) = self.children.iter().position(|c| c.id() == id) {
            self.set_focus_to(index);
        }
    }

    /// Cycle focus forward to the next focusable child.
    ///
    /// A child is focusable if it has `OF_SELECTABLE` set and `can_focus()` returns `true`.
    /// Wraps around if necessary. No-op if no focusable children exist.
    pub fn focus_next(&mut self) {
        let n = self.children.len();
        if n == 0 {
            return;
        }
        let start = self.focused.map_or(0, |f| (f + 1) % n);
        for offset in 0..n {
            let i = (start + offset) % n;
            if self.children[i].options() & OF_SELECTABLE != 0 && self.children[i].can_focus() {
                self.set_focus_to(i);
                return;
            }
        }
    }

    /// Cycle focus backward to the previous focusable child.
    ///
    /// Wraps around if necessary. No-op if no focusable children exist.
    pub fn focus_prev(&mut self) {
        let n = self.children.len();
        if n == 0 {
            return;
        }
        let start = self.focused.map_or(n - 1, |f| (f + n - 1) % n);
        for offset in 0..n {
            let i = (start + n - offset) % n;
            if self.children[i].options() & OF_SELECTABLE != 0 && self.children[i].can_focus() {
                self.set_focus_to(i);
                return;
            }
        }
    }

    /// Return the index of the currently focused child, if any.
    #[must_use]
    pub fn focused_index(&self) -> Option<usize> {
        self.focused
    }

    /// Return the cursor position from the focused child, if any.
    ///
    /// Delegates to the focused child's `View::cursor_position()`.
    /// Returns `None` if there is no focused child or the focused child
    /// returns `None`.
    #[must_use]
    pub fn cursor_position(&self) -> Option<Position> {
        let idx = self.focused?;
        self.children.get(idx)?.cursor_position()
    }
}

// ============================================================================
// View trait implementation
// ============================================================================

impl View for Container {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    /// Set the bounds of this container.
    ///
    /// If the position changed (dx or dy ≠ 0), the delta is propagated to
    /// **all** children so that they maintain their relative layout.
    fn set_bounds(&mut self, bounds: Rect) {
        let old = self.base.bounds();
        self.base.set_bounds(bounds);

        let dx = i32::from(bounds.x) - i32::from(old.x);
        let dy = i32::from(bounds.y) - i32::from(old.y);
        if dx != 0 || dy != 0 {
            for child in &mut self.children {
                let cb = child.bounds();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let nx = (i32::from(cb.x) + dx).max(0) as u16;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let ny = (i32::from(cb.y) + dy).max(0) as u16;
                child.set_bounds(Rect::new(nx, ny, cb.width, cb.height));
            }
        }
    }

    fn draw(&self, buf: &mut Buffer, clip: Rect) {
        self.draw_children(buf, clip);
    }

    fn handle_event(&mut self, event: &mut Event) {
        self.dispatch_event(event);
    }

    /// Containers can always receive focus.
    fn can_focus(&self) -> bool {
        true
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

    fn owner_type(&self) -> OwnerType {
        self.base.owner_type()
    }

    fn set_owner_type(&mut self, owner_type: OwnerType) {
        self.base.set_owner_type(owner_type);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_CLOSE, CM_OK};
    use crate::view::{
        Event, EventKind, View, ViewBase, ViewId, OF_POST_PROCESS, OF_PRE_PROCESS, OF_SELECTABLE,
        SF_DRAGGING, SF_FOCUSED, SF_VISIBLE,
    };
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    };
    use ratatui::layout::Rect;
    use std::cell::RefCell;

    // -----------------------------------------------------------------------
    // TestView helper
    // -----------------------------------------------------------------------

    /// Minimal view for testing: records event names and supports option flags.
    struct TestView {
        base: ViewBase,
        events_received: RefCell<Vec<String>>,
    }

    impl TestView {
        fn new(bounds: Rect) -> Self {
            Self {
                base: ViewBase::new(bounds),
                events_received: RefCell::new(Vec::new()),
            }
        }

        fn with_options(bounds: Rect, options: u16) -> Self {
            Self {
                base: ViewBase::with_options(bounds, options),
                events_received: RefCell::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<String> {
            self.events_received.borrow().clone()
        }
    }

    impl View for TestView {
        fn id(&self) -> ViewId {
            self.base.id()
        }

        fn bounds(&self) -> Rect {
            self.base.bounds()
        }

        fn set_bounds(&mut self, bounds: Rect) {
            self.base.set_bounds(bounds);
        }

        fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}

        fn handle_event(&mut self, event: &mut Event) {
            let name = match &event.kind {
                EventKind::Key(_) => "key",
                EventKind::Mouse(_) => "mouse",
                EventKind::Command(_) => "command",
                EventKind::Broadcast(_) => "broadcast",
                EventKind::Resize(_, _) => "resize",
                EventKind::None => "none",
            };
            self.events_received.borrow_mut().push(name.to_owned());
            event.clear();
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

    // -----------------------------------------------------------------------
    // Helper to build a simple key event
    // -----------------------------------------------------------------------
    fn key_event(code: KeyCode) -> Event {
        Event::key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn mouse_event_at(col: u16, row: u16, kind: MouseEventKind) -> Event {
        Event::mouse(MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_container_add_converts_relative_to_absolute() {
        // Container at (10, 20), child at relative (5, 3) → absolute (15, 23)
        let mut group = Container::new(Rect::new(10, 20, 80, 40));
        let child = Box::new(TestView::new(Rect::new(5, 3, 10, 2)));
        let id = group.add(child);

        let stored = group.child_by_id(id).expect("child not found");
        assert_eq!(stored.bounds(), Rect::new(15, 23, 10, 2));
    }

    #[test]
    fn test_container_set_bounds_propagates_delta() {
        // Container at (0, 0), child at absolute (5, 3).
        // Move container to (10, 20) → child must move to (15, 23).
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        let child = Box::new(TestView::new(Rect::new(5, 3, 10, 2)));
        let id = group.add(child);

        // After add, child should be at absolute (5, 3) (group.x=0, group.y=0)
        assert_eq!(
            group.child_by_id(id).unwrap().bounds(),
            Rect::new(5, 3, 10, 2)
        );

        // Move container
        group.set_bounds(Rect::new(10, 20, 80, 40));
        assert_eq!(
            group.child_by_id(id).unwrap().bounds(),
            Rect::new(15, 23, 10, 2)
        );
    }

    #[test]
    fn test_container_set_bounds_no_delta_no_move() {
        // Resize only (no position change) must not shift children.
        let mut group = Container::new(Rect::new(5, 5, 40, 20));
        let child = Box::new(TestView::new(Rect::new(2, 2, 8, 3)));
        let id = group.add(child);
        let before = group.child_by_id(id).unwrap().bounds();

        group.set_bounds(Rect::new(5, 5, 60, 30)); // resize only
        assert_eq!(group.child_by_id(id).unwrap().bounds(), before);
    }

    #[test]
    fn test_container_add_remove() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        assert_eq!(group.child_count(), 0);

        let id0 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2))));
        assert_eq!(group.child_count(), 2);

        // Remove first child
        let removed = group.remove(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id(), id0);
        assert_eq!(group.child_count(), 1);
        assert_eq!(group.child_at(0).unwrap().id(), id1);

        // Remove by ID
        let removed2 = group.remove_by_id(id1);
        assert!(removed2.is_some());
        assert_eq!(group.child_count(), 0);

        // Out-of-bounds remove returns None
        assert!(group.remove(0).is_none());
    }

    #[test]
    fn test_container_remove_adjusts_focus() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2))));
        group.add(Box::new(TestView::new(Rect::new(0, 6, 10, 2))));

        // Focus index 1
        group.focused = Some(1);

        // Remove focused child → focus cleared
        group.remove(1);
        assert_eq!(group.focused, None);

        // Reset: 2 children, focus index 1 (the last one)
        group.add(Box::new(TestView::new(Rect::new(0, 9, 10, 2))));
        group.focused = Some(2);

        // Remove child at index 0 (before focused) → focus shifts to 1
        group.remove(0);
        assert_eq!(group.focused, Some(1));

        // Remove child after focused → focus unchanged
        group.focused = Some(0);
        group.add(Box::new(TestView::new(Rect::new(0, 12, 10, 2))));
        group.focused = Some(0);
        group.remove(1); // remove child after focused
        assert_eq!(group.focused, Some(0));
    }

    #[test]
    fn test_container_bring_to_front() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        let id0 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 6, 10, 2))));

        // Bring index 0 to front → order becomes [id1, id2, id0]
        group.bring_to_front(0);
        assert_eq!(group.child_at(0).unwrap().id(), id1);
        assert_eq!(group.child_at(1).unwrap().id(), id2);
        assert_eq!(group.child_at(2).unwrap().id(), id0);

        // bring_to_front on already-front is no-op
        group.bring_to_front(2);
        assert_eq!(group.child_at(2).unwrap().id(), id0);
    }

    #[test]
    fn test_container_bring_to_front_updates_focus() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2)))); // idx 0
        group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2)))); // idx 1
        group.add(Box::new(TestView::new(Rect::new(0, 6, 10, 2)))); // idx 2

        // Focus idx 0, bring it to front → it moves to idx 2, focus must follow
        group.focused = Some(0);
        group.bring_to_front(0);
        assert_eq!(group.focused, Some(2));

        // Focus idx 2 (currently the brought-up one), bring idx 1 to front
        // Focused child was at 2 which > the moved index (1) → focus shifts to 1
        group.focused = Some(2);
        group.bring_to_front(1);
        assert_eq!(group.focused, Some(1));
    }

    #[test]
    fn test_container_send_to_back() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        let id0 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 6, 10, 2))));

        // Send index 2 to back → order becomes [id2, id0, id1]
        group.send_to_back(2);
        assert_eq!(group.child_at(0).unwrap().id(), id2);
        assert_eq!(group.child_at(1).unwrap().id(), id0);
        assert_eq!(group.child_at(2).unwrap().id(), id1);

        // send_to_back on already-back is no-op
        group.send_to_back(0);
        assert_eq!(group.child_at(0).unwrap().id(), id2);
    }

    #[test]
    fn test_container_send_to_back_updates_focus() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2)))); // idx 0
        group.add(Box::new(TestView::new(Rect::new(0, 3, 10, 2)))); // idx 1
        group.add(Box::new(TestView::new(Rect::new(0, 6, 10, 2)))); // idx 2

        // Focus idx 2, send it to back → moves to idx 0, focus must follow
        group.focused = Some(2);
        group.send_to_back(2);
        assert_eq!(group.focused, Some(0));

        // Focus idx 0 (the sent-to-back one), send idx 2 to back
        // Focused (0) < moved index (2) → focus shifts to 1
        group.focused = Some(0);
        group.send_to_back(2);
        assert_eq!(group.focused, Some(1));
    }

    #[test]
    fn test_container_child_at_point() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Back view: covers (0,0)..(10,5)
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        // Front view: covers (2,1)..(7,4) — overlaps with back
        group.add(Box::new(TestView::new(Rect::new(2, 1, 5, 3))));

        // Point inside front view → should return index 1 (front)
        assert_eq!(group.child_at_point(3, 2), Some(1));

        // Point inside back view only (outside front) → index 0
        assert_eq!(group.child_at_point(0, 0), Some(0));

        // Point outside all children → None
        assert_eq!(group.child_at_point(20, 20), None);
    }

    #[test]
    fn test_container_child_at_point_invisible_skipped() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5)))); // idx 0, visible
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5)))); // idx 1, will be hidden

        // Hide top child
        let st = group.child_at(1).unwrap().state();
        group.child_at_mut(1).unwrap().set_state(st & !SF_VISIBLE);

        // Hit should fall through to idx 0
        assert_eq!(group.child_at_point(5, 2), Some(0));
    }

    #[test]
    fn test_container_focus_next_prev() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Non-selectable child (idx 0) — does not trigger auto-focus
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        // Selectable children
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
        )));
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 6, 10, 2),
            OF_SELECTABLE,
        )));

        // Adding the first selectable child (idx 1) auto-focused it.
        assert_eq!(group.focused, Some(1));

        // focus_next from idx 1 → idx 2
        group.focus_next();
        assert_eq!(group.focused, Some(2));
        assert_ne!(group.child_at(2).unwrap().state() & SF_FOCUSED, 0);

        // focus_next wraps around → idx 1
        group.focus_next();
        assert_eq!(group.focused, Some(1));

        // focus_prev from 1 → wraps to idx 2
        group.focus_prev();
        assert_eq!(group.focused, Some(2));

        // focus_prev → idx 1
        group.focus_prev();
        assert_eq!(group.focused, Some(1));
    }

    #[test]
    fn test_container_focus_clears_old_sf_focused() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 0, 10, 2),
            OF_SELECTABLE,
        )));
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
        )));

        group.set_focus_to(0);
        assert_ne!(group.child_at(0).unwrap().state() & SF_FOCUSED, 0);

        group.set_focus_to(1);
        // Old child must no longer be focused
        assert_eq!(group.child_at(0).unwrap().state() & SF_FOCUSED, 0);
        assert_ne!(group.child_at(1).unwrap().state() & SF_FOCUSED, 0);
    }

    // -----------------------------------------------------------------------
    // Event dispatch tests
    // -----------------------------------------------------------------------

    /// A non-consuming TestView that records events but does NOT clear them.
    struct RecordingView {
        base: ViewBase,
        log: RefCell<Vec<String>>,
    }

    impl RecordingView {
        fn new(bounds: Rect, options: u16, name: &str) -> Self {
            let base = ViewBase::with_options(bounds, options);
            // Store a marker so we can identify phase in the log
            let _ = name; // used only for test setup clarity
            Self {
                base,
                log: RefCell::new(Vec::new()),
            }
        }

        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl View for RecordingView {
        fn id(&self) -> ViewId {
            self.base.id()
        }
        fn bounds(&self) -> Rect {
            self.base.bounds()
        }
        fn set_bounds(&mut self, b: Rect) {
            self.base.set_bounds(b);
        }
        fn draw(&self, _buf: &mut Buffer, _clip: Rect) {}
        fn handle_event(&mut self, event: &mut Event) {
            let name = match &event.kind {
                EventKind::Key(_) => "key",
                EventKind::Command(_) => "cmd",
                EventKind::Broadcast(_) => "broadcast",
                EventKind::Resize(_, _) => "resize",
                EventKind::Mouse(_) => "mouse",
                EventKind::None => "none",
            };
            self.log.borrow_mut().push(name.to_owned());
            // Do NOT clear — let event pass through to next phase
        }
        fn can_focus(&self) -> bool {
            self.base.options() & OF_SELECTABLE != 0
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

    #[test]
    fn test_container_three_phase_dispatch() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // pre-process child (idx 0)
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 0, 10, 2),
            OF_PRE_PROCESS,
            "pre",
        )));
        // normal/focused child (idx 1)
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
            "focused",
        )));
        // post-process child (idx 2)
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 6, 10, 2),
            OF_POST_PROCESS,
            "post",
        )));

        group.set_focus_to(1);

        let mut event = key_event(KeyCode::Char('a'));
        group.handle_event(&mut event);

        // Pre-process (idx 0)
        let pre = group
            .child_at(0)
            .unwrap()
            .as_any()
            .downcast_ref::<RecordingView>()
            .unwrap();
        assert_eq!(pre.log(), vec!["key"], "pre-process must receive key event");

        // Focused (idx 1)
        let focused = group
            .child_at(1)
            .unwrap()
            .as_any()
            .downcast_ref::<RecordingView>()
            .unwrap();
        assert_eq!(focused.log(), vec!["key"], "focused must receive key event");

        // Post-process (idx 2)
        let post = group
            .child_at(2)
            .unwrap()
            .as_any()
            .downcast_ref::<RecordingView>()
            .unwrap();
        assert_eq!(
            post.log(),
            vec!["key"],
            "post-process must receive key event"
        );
    }

    #[test]
    fn test_container_pre_process_can_consume_event() {
        // If pre-process clears the event, focused and post-process must NOT receive it.
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Consuming pre-process child (TestView clears events)
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 0, 10, 2),
            OF_PRE_PROCESS,
        )));
        // Focused child
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
            "focused",
        )));

        group.set_focus_to(1);

        let mut event = key_event(KeyCode::Char('x'));
        group.handle_event(&mut event);

        // Focused must not have received the event
        let focused = group
            .child_at(1)
            .unwrap()
            .as_any()
            .downcast_ref::<RecordingView>()
            .unwrap();
        assert!(
            focused.log().is_empty(),
            "focused must not receive consumed event"
        );
    }

    #[test]
    fn test_container_mouse_reverse_z() {
        // Two overlapping views. Front view (idx 1) should receive the mouse event.
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        group.add(Box::new(TestView::new(Rect::new(0, 0, 20, 10)))); // back, idx 0
        group.add(Box::new(TestView::new(Rect::new(0, 0, 20, 10)))); // front, idx 1

        let mut event = mouse_event_at(5, 5, MouseEventKind::Down(MouseButton::Left));
        group.handle_event(&mut event);

        // Front view consumed the event; back view must not have received it
        let front = group
            .child_at(1)
            .unwrap()
            .as_any()
            .downcast_ref::<TestView>()
            .unwrap();
        let back = group
            .child_at(0)
            .unwrap()
            .as_any()
            .downcast_ref::<TestView>()
            .unwrap();
        assert_eq!(front.events(), vec!["mouse"]);
        assert!(back.events().is_empty());
    }

    #[test]
    fn test_container_broadcast_all() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Use recording views so we can check without consuming
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 0, 10, 2),
            0,
            "c0",
        )));
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 3, 10, 2),
            0,
            "c1",
        )));
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 6, 10, 2),
            0,
            "c2",
        )));

        let mut event = Event::broadcast(CM_CLOSE);
        group.handle_event(&mut event);

        for i in 0..3 {
            let rv = group
                .child_at(i)
                .unwrap()
                .as_any()
                .downcast_ref::<RecordingView>()
                .unwrap();
            assert_eq!(
                rv.log(),
                vec!["broadcast"],
                "child {i} must receive broadcast"
            );
        }
    }

    #[test]
    fn test_container_mouse_capture() {
        // Focused child with SF_DRAGGING set should receive Drag events
        // even if the mouse is outside its bounds.
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Focused/dragging child occupies only (0,0)..(10,5)
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 0, 10, 5),
            OF_SELECTABLE,
        )));

        group.set_focus_to(0);

        // Mark focused child as dragging
        let st = group.child_at(0).unwrap().state();
        group.child_at_mut(0).unwrap().set_state(st | SF_DRAGGING);

        // Drag event way outside the child's bounds
        let mut event = mouse_event_at(50, 30, MouseEventKind::Drag(MouseButton::Left));
        group.handle_event(&mut event);

        let dragged = group
            .child_at(0)
            .unwrap()
            .as_any()
            .downcast_ref::<TestView>()
            .unwrap();
        assert_eq!(
            dragged.events(),
            vec!["mouse"],
            "dragging child must capture drag event"
        );
    }

    #[test]
    fn test_container_resize_event_all_children() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        group.add(Box::new(RecordingView::new(
            Rect::new(0, 0, 10, 2),
            0,
            "c0",
        )));
        group.add(Box::new(RecordingView::new(
            Rect::new(0, 3, 10, 2),
            0,
            "c1",
        )));

        let mut event = Event::resize(100, 50);
        group.handle_event(&mut event);

        for i in 0..2 {
            let rv = group
                .child_at(i)
                .unwrap()
                .as_any()
                .downcast_ref::<RecordingView>()
                .unwrap();
            assert_eq!(rv.log(), vec!["resize"], "child {i} must receive resize");
        }
    }

    #[test]
    fn test_container_command_dispatch_to_focused() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        group.add(Box::new(TestView::with_options(
            Rect::new(0, 0, 10, 2),
            OF_SELECTABLE,
        )));
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
        )));

        group.set_focus_to(1);

        let mut event = Event::command(CM_OK);
        group.handle_event(&mut event);

        let focused = group
            .child_at(1)
            .unwrap()
            .as_any()
            .downcast_ref::<TestView>()
            .unwrap();
        let other = group
            .child_at(0)
            .unwrap()
            .as_any()
            .downcast_ref::<TestView>()
            .unwrap();
        assert_eq!(focused.events(), vec!["command"]);
        assert!(other.events().is_empty());
    }

    #[test]
    fn test_container_child_count_and_accessors() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));
        assert_eq!(group.child_count(), 0);
        assert!(group.child_at(0).is_none());
        assert!(group.child_at_mut(0).is_none());

        group.add(Box::new(TestView::new(Rect::new(0, 0, 5, 2))));
        assert_eq!(group.child_count(), 1);
        assert!(group.child_at(0).is_some());
    }

    #[test]
    fn test_container_add_auto_focuses_first_focusable_child() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // First child: focusable (OF_SELECTABLE → can_focus() == true)
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 0, 10, 2),
            OF_SELECTABLE,
        )));
        assert_eq!(
            group.focused_index(),
            Some(0),
            "first focusable child must become focused"
        );

        // Second child: also focusable — focus must stay on the first
        group.add(Box::new(TestView::with_options(
            Rect::new(0, 3, 10, 2),
            OF_SELECTABLE,
        )));
        assert_eq!(
            group.focused_index(),
            Some(0),
            "focus must not move away from first child when second is added"
        );
    }

    #[test]
    fn test_container_add_non_focusable_child_does_not_set_focus() {
        let mut group = Container::new(Rect::new(0, 0, 80, 40));

        // Non-focusable child (no OF_SELECTABLE → can_focus() == false)
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 2))));
        assert_eq!(
            group.focused_index(),
            None,
            "non-focusable child must not set focused"
        );
    }

    #[test]
    fn test_set_focus_calls_lifecycle_hooks() {
        // This test verifies that set_focus_to() calls on_blur/on_focus.
        // Since the default View implementations are no-ops, we just verify
        // the focus state changes work correctly with the hooks in place.
        let mut group = Container::new(Rect::new(0, 0, 80, 25));
        let child0 = Box::new(crate::static_text::StaticText::new(
            Rect::new(0, 0, 10, 1),
            "A",
        ));
        let child1 = Box::new(crate::static_text::StaticText::new(
            Rect::new(0, 1, 10, 1),
            "B",
        ));
        group.add(child0);
        group.add(child1);

        group.set_focus_to(0);
        assert_ne!(group.child_at(0).unwrap().state() & SF_FOCUSED, 0);

        group.set_focus_to(1);
        assert_eq!(group.child_at(0).unwrap().state() & SF_FOCUSED, 0);
        assert_ne!(group.child_at(1).unwrap().state() & SF_FOCUSED, 0);
    }
}
