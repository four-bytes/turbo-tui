//! Group — Container view with Z-order and three-phase event dispatch.
//!
//! `Group` is a container that holds child views with Z-order management
//! and Borland Turbo Vision's three-phase event dispatch pattern.

use crate::view::{
    Event, EventKind, OwnerType, View, ViewBase, ViewId, OF_POST_PROCESS, OF_PRE_PROCESS,
    OF_SELECTABLE, OF_TOP_SELECT, SF_VISIBLE,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;

/// Container view with Z-order and three-phase event dispatch.
///
/// A `Group` holds child views and manages:
/// - **Z-order**: Children are drawn back-to-front, mouse events hit front-to-back
/// - **Focus**: Tab/Shift-Tab cycling through focusable children
/// - **Three-phase dispatch**: Pre-process → Focused → Post-process for keyboard/command events
///
/// # Example
///
/// ```ignore
/// let mut group = Group::new(Rect::new(0, 0, 80, 24));
/// group.add(Box::new(Button::new("OK")));
/// group.add(Box::new(Button::new("Cancel")));
/// group.set_focus_to(0);  // Focus first button
/// ```
pub struct Group {
    /// Embedded base providing `ViewId`, bounds, state, options.
    base: ViewBase,
    /// Child views — Vec order = draw order (0=back, last=front).
    children: Vec<Box<dyn View>>,
    /// Index of focused child (None = no focus).
    focused: Option<usize>,
}

impl Group {
    /// Create a new group with the given bounds.
    ///
    /// The group starts with no children and no focused child.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: ViewBase::new(bounds),
            children: Vec::new(),
            focused: None,
        }
    }

    /// Add a child view and return its `ViewId`.
    ///
    /// The child is added at the front (highest Z-order).
    pub fn add(&mut self, child: Box<dyn View>) -> ViewId {
        let id = child.id();
        self.children.push(child);
        id
    }

    /// Remove a child by index.
    ///
    /// Returns the removed child, or `None` if index is out of bounds.
    /// Adjusts the focused index if necessary.
    pub fn remove(&mut self, index: usize) -> Option<Box<dyn View>> {
        if index >= self.children.len() {
            return None;
        }

        let removed = self.children.remove(index);

        // Adjust focused index
        if let Some(focused_idx) = self.focused {
            if focused_idx == index {
                // Removed the focused child
                self.focused = None;
            } else if focused_idx > index {
                // Focused child shifted left
                self.focused = Some(focused_idx - 1);
            }
        }

        Some(removed)
    }

    /// Remove a child by `ViewId`.
    ///
    /// Returns the removed child, or `None` if not found.
    pub fn remove_by_id(&mut self, id: ViewId) -> Option<Box<dyn View>> {
        let index = self.children.iter().position(|c| c.id() == id)?;
        self.remove(index)
    }

    /// Get the number of children.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Get a child by index.
    #[must_use]
    pub fn child_at(&self, index: usize) -> Option<&dyn View> {
        self.children.get(index).map(std::convert::AsRef::as_ref)
    }

    /// Get a child by index (mutable).
    pub fn child_at_mut(&mut self, index: usize) -> Option<&mut Box<dyn View>> {
        self.children.get_mut(index)
    }

    /// Find a child by its `ViewId`.
    #[must_use]
    pub fn child_by_id(&self, id: ViewId) -> Option<&dyn View> {
        self.children
            .iter()
            .find(|c| c.id() == id)
            .map(std::convert::AsRef::as_ref)
    }

    /// Find a child by its `ViewId` (mutable).
    pub fn child_by_id_mut(&mut self, id: ViewId) -> Option<&mut Box<dyn View>> {
        self.children.iter_mut().find(|c| c.id() == id)
    }

    /// Get all children as a slice.
    #[must_use]
    pub fn children(&self) -> &[Box<dyn View>] {
        &self.children
    }

    /// Get mutable access to all children.
    pub fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    /// Move a child to the front (highest Z-order, last in Vec).
    ///
    /// Does nothing if index is out of bounds.
    pub fn bring_to_front(&mut self, index: usize) {
        if index >= self.children.len() || index == self.children.len() - 1 {
            return;
        }

        let child = self.children.remove(index);
        self.children.push(child);

        // Adjust focused index
        if let Some(focused_idx) = self.focused {
            if focused_idx == index {
                // The focused child was moved to front
                self.focused = Some(self.children.len() - 1);
            } else if focused_idx > index {
                // A child before the focused one was moved to front
                self.focused = Some(focused_idx - 1);
            }
            // else: focused_idx < index, no change needed
        }
    }

    /// Move a child to the back (lowest Z-order, first in Vec).
    ///
    /// Does nothing if index is out of bounds.
    pub fn send_to_back(&mut self, index: usize) {
        if index == 0 || index >= self.children.len() {
            return;
        }

        let child = self.children.remove(index);
        self.children.insert(0, child);

        // Adjust focused index
        if let Some(focused_idx) = self.focused {
            if focused_idx == index {
                // The focused child was moved to back
                self.focused = Some(0);
            } else if focused_idx < index {
                // A child after the focused one was moved to back
                self.focused = Some(focused_idx + 1);
            }
            // else: focused_idx > index, no change needed
        }
    }

    /// Get the index of the focused child.
    #[must_use]
    pub fn focused_index(&self) -> Option<usize> {
        self.focused
    }

    /// Focus a child by index, unfocusing the previous focused child.
    ///
    /// Returns `false` if index is out of bounds.
    pub fn set_focus_to(&mut self, index: usize) -> bool {
        if index >= self.children.len() {
            return false;
        }

        // Check OF_TOP_SELECT before any borrowing
        let needs_bring_to_front = (self.children[index].options() & OF_TOP_SELECT) != 0;
        let child_id = self.children[index].id();

        // Unfocus previous
        if let Some(old_idx) = self.focused {
            if let Some(child) = self.children.get_mut(old_idx) {
                child.set_focused(false);
            }
        }

        // Focus new
        if let Some(child) = self.children.get_mut(index) {
            child.set_focused(true);
        }
        self.focused = Some(index);

        // Handle OF_TOP_SELECT - bring to front if needed
        if needs_bring_to_front {
            self.bring_to_front(index);
            // After bring_to_front, find the child again by ID
            if let Some(new_idx) = self.children.iter().position(|c| c.id() == child_id) {
                self.focused = Some(new_idx);
            }
        }

        true
    }

    /// Focus a child by its `ViewId`.
    ///
    /// Returns `true` if the child was found and focused.
    pub fn focus_by_id(&mut self, id: ViewId) -> bool {
        match self.children.iter().position(|c| c.id() == id) {
            Some(index) => {
                self.set_focus_to(index);
                true
            }
            None => false,
        }
    }

    /// Move focus to the next focusable child (Tab key behavior).
    ///
    /// Cycles around if at the end.
    pub fn focus_next(&mut self) {
        if self.children.is_empty() {
            return;
        }

        let start = self
            .focused
            .map_or(0, |idx| (idx + 1) % self.children.len());
        let mut current = start;

        loop {
            if self.is_child_focusable(current) {
                self.set_focus_to(current);
                return;
            }

            current = (current + 1) % self.children.len();
            if current == start {
                // No focusable child found, clear focus
                self.clear_focus();
                return;
            }
        }
    }

    /// Move focus to the previous focusable child (Shift-Tab behavior).
    ///
    /// Cycles around if at the beginning.
    pub fn focus_prev(&mut self) {
        if self.children.is_empty() {
            return;
        }

        let len = self.children.len();
        let start = self
            .focused
            .map_or(len - 1, |idx| if idx == 0 { len - 1 } else { idx - 1 });
        let mut current = start;

        loop {
            if self.is_child_focusable(current) {
                self.set_focus_to(current);
                return;
            }

            if current == 0 {
                current = len - 1;
            } else {
                current -= 1;
            }

            if current == start {
                // No focusable child found, clear focus
                self.clear_focus();
                return;
            }
        }
    }

    /// Check if a child is focusable.
    fn is_child_focusable(&self, index: usize) -> bool {
        if let Some(child) = self.children.get(index) {
            let state = child.state();
            let options = child.options();
            (state & SF_VISIBLE != 0) && (options & OF_SELECTABLE != 0 || child.can_focus())
        } else {
            false
        }
    }

    /// Clear focus from all children.
    pub fn clear_focus(&mut self) {
        if let Some(focused_idx) = self.focused {
            if let Some(child) = self.children.get_mut(focused_idx) {
                child.set_focused(false);
            }
        }
        self.focused = None;
    }

    /// Find the child at a screen point (reverse Z-order, front to back).
    ///
    /// Returns the index of the top-most visible child containing the point,
    /// or `None` if no child contains the point.
    #[must_use]
    pub fn child_at_point(&self, x: u16, y: u16) -> Option<usize> {
        for i in (0..self.children.len()).rev() {
            let child = &self.children[i];
            let bounds = child.bounds();
            if (child.state() & SF_VISIBLE) != 0
                && x >= bounds.x
                && x < bounds.x + bounds.width
                && y >= bounds.y
                && y < bounds.y + bounds.height
            {
                return Some(i);
            }
        }
        None
    }

    /// Draw all visible children in Z-order (back to front).
    fn draw_children(&self, buf: &mut Buffer, area: Rect) {
        for child in &self.children {
            if child.state() & SF_VISIBLE != 0 {
                child.draw(buf, area);
            }
        }
    }

    /// Dispatch events using the three-phase pattern.
    ///
    /// For keyboard/command events:
    /// 1. Pre-process phase: children with `OF_PRE_PROCESS`
    /// 2. Focused child
    /// 3. Post-process phase: children with `OF_POST_PROCESS`
    ///
    /// For mouse events: reverse Z-order hit-test (front to back).
    ///
    /// For broadcast/resize events: all children.
    fn dispatch_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind {
            EventKind::Key(_) | EventKind::Command(_) => {
                // Phase 1: Pre-process
                for child in &mut self.children {
                    if event.is_cleared() {
                        break;
                    }
                    if child.options() & OF_PRE_PROCESS != 0 {
                        child.handle_event(event);
                    }
                }

                // Phase 2: Focused child
                if !event.is_cleared() {
                    if let Some(focused_idx) = self.focused {
                        if let Some(child) = self.children.get_mut(focused_idx) {
                            child.handle_event(event);
                        }
                    }
                }

                // Phase 3: Post-process
                if !event.is_cleared() {
                    for child in &mut self.children {
                        if event.is_cleared() {
                            break;
                        }
                        if child.options() & OF_POST_PROCESS != 0 {
                            child.handle_event(event);
                        }
                    }
                }
            }

            EventKind::Mouse(mouse) => {
                let col = mouse.column;
                let row = mouse.row;

                // Reverse Z-order (front to back)
                for i in (0..self.children.len()).rev() {
                    if event.is_cleared() {
                        break;
                    }
                    let child = &self.children[i];
                    let bounds = child.bounds();
                    if (child.state() & SF_VISIBLE) != 0
                        && col >= bounds.x
                        && col < bounds.x + bounds.width
                        && row >= bounds.y
                        && row < bounds.y + bounds.height
                    {
                        self.children[i].handle_event(event);
                        break; // Only top-most view gets mouse events
                    }
                }
            }

            EventKind::Broadcast(_) | EventKind::Resize(_, _) => {
                // All children get broadcast/resize events
                for child in &mut self.children {
                    if event.is_cleared() {
                        break;
                    }
                    child.handle_event(event);
                }
            }

            EventKind::None => {}
        }
    }
}

impl View for Group {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, buf: &mut Buffer, area: Rect) {
        self.draw_children(buf, area);
    }

    fn handle_event(&mut self, event: &mut Event) {
        self.dispatch_event(event);
    }

    fn can_focus(&self) -> bool {
        true // Groups are focusable containers
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

    fn end_state(&self) -> u16 {
        self.base.end_state()
    }

    fn set_end_state(&mut self, cmd: u16) {
        self.base.set_end_state(cmd);
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
    use crate::command::CM_OK;
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Helper test view that tracks received events.
    struct TestView {
        base: ViewBase,
        events_received: Rc<RefCell<Vec<String>>>,
    }

    impl TestView {
        fn new(bounds: Rect) -> Self {
            Self {
                base: ViewBase::with_options(bounds, OF_SELECTABLE),
                events_received: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn with_options(bounds: Rect, options: u16) -> Self {
            Self {
                base: ViewBase::with_options(bounds, options),
                events_received: Rc::new(RefCell::new(Vec::new())),
            }
        }

        #[allow(dead_code)]
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

        fn draw(&self, _buf: &mut Buffer, _area: Rect) {}

        fn handle_event(&mut self, event: &mut Event) {
            let label = match &event.kind {
                EventKind::Key(_) => "key".to_string(),
                EventKind::Command(cmd) => format!("cmd:{cmd}"),
                EventKind::Mouse(_) => "mouse".to_string(),
                EventKind::Broadcast(cmd) => format!("broadcast:{cmd}"),
                EventKind::Resize(_, _) => "resize".to_string(),
                EventKind::None => "none".to_string(),
            };
            self.events_received.borrow_mut().push(label);
            event.handled = true;
        }

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

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn test_group_add_children() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id3 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        assert_eq!(group.child_count(), 3);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert!(group.child_by_id(id1).is_some());
        assert!(group.child_by_id(id2).is_some());
        assert!(group.child_by_id(id3).is_some());
    }

    #[test]
    fn test_group_remove_by_index() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        assert_eq!(group.child_count(), 3);

        let removed = group.remove(1);
        assert!(removed.is_some());
        assert_eq!(group.child_count(), 2);

        // Try to remove out of bounds
        let none = group.remove(10);
        assert!(none.is_none());
        assert_eq!(group.child_count(), 2);
    }

    #[test]
    fn test_group_remove_by_id() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id3 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        assert_eq!(group.child_count(), 3);

        // Remove middle by ID
        let removed = group.remove_by_id(id2);
        assert!(removed.is_some());
        assert_eq!(group.child_count(), 2);

        // Verify correct one was removed
        assert!(group.child_by_id(id1).is_some());
        assert!(group.child_by_id(id3).is_some());

        // Remove non-existent
        let none = group.remove_by_id(id2);
        assert!(none.is_none());
    }

    #[test]
    fn test_group_bring_to_front() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id3 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        // Initial order: [id1, id2, id3]
        assert_eq!(group.children()[0].id(), id1);
        assert_eq!(group.children()[1].id(), id2);
        assert_eq!(group.children()[2].id(), id3);

        // Bring id1 to front
        group.bring_to_front(0);

        // New order: [id2, id3, id1]
        assert_eq!(group.children()[0].id(), id2);
        assert_eq!(group.children()[1].id(), id3);
        assert_eq!(group.children()[2].id(), id1);

        // Bring non-existent index - should be no-op
        group.bring_to_front(100);
        assert_eq!(group.child_count(), 3);
    }

    #[test]
    fn test_group_send_to_back() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        let id1 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id2 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        let id3 = group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        // Initial order: [id1, id2, id3]
        // Send id3 to back
        group.send_to_back(2);

        // New order: [id3, id1, id2]
        assert_eq!(group.children()[0].id(), id3);
        assert_eq!(group.children()[1].id(), id1);
        assert_eq!(group.children()[2].id(), id2);

        // Send non-existent index - should be no-op
        group.send_to_back(100);
        assert_eq!(group.child_count(), 3);
    }

    #[test]
    fn test_group_focus_next() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        // Focus first
        group.focus_next();
        assert_eq!(group.focused_index(), Some(0));

        // Tab to next
        group.focus_next();
        assert_eq!(group.focused_index(), Some(1));

        // Tab to next
        group.focus_next();
        assert_eq!(group.focused_index(), Some(2));

        // Tab again wraps around
        group.focus_next();
        assert_eq!(group.focused_index(), Some(0));
    }

    #[test]
    fn test_group_focus_prev() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));
        group.add(Box::new(TestView::new(Rect::new(0, 0, 10, 5))));

        // Focus from end (no current focus, starts from last)
        group.focus_prev();
        assert_eq!(group.focused_index(), Some(2));

        // Shift-Tab to previous
        group.focus_prev();
        assert_eq!(group.focused_index(), Some(1));

        // Shift-Tab to previous
        group.focus_prev();
        assert_eq!(group.focused_index(), Some(0));

        // Shift-Tab again wraps around
        group.focus_prev();
        assert_eq!(group.focused_index(), Some(2));
    }

    #[test]
    fn test_group_child_at_point() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        // Back view (larger, at back position)
        let child1 = Box::new(TestView::new(Rect::new(0, 0, 20, 10)));
        let id1 = child1.id();
        group.add(child1);

        // Front view (smaller, overlaps)
        let child2 = Box::new(TestView::new(Rect::new(5, 5, 10, 5)));
        let id2 = child2.id();
        group.add(child2);

        // Point in back view only (not overlapped)
        let hit = group.child_at_point(2, 2);
        assert_eq!(hit, Some(0)); // child1 is at index 0
        let view_id = group.child_at(0).unwrap().id();
        assert_eq!(view_id, id1);

        // Point in overlap area (should hit front view)
        let hit = group.child_at_point(10, 7);
        assert_eq!(hit, Some(1)); // child2 is at index 1
        let view_id = group.child_at(1).unwrap().id();
        assert_eq!(view_id, id2);

        // Point outside all views
        let hit = group.child_at_point(50, 50);
        assert!(hit.is_none());
    }

    #[test]
    fn test_group_three_phase_dispatch() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        // Pre-process view (status line)
        let pre_view = TestView::with_options(Rect::new(0, 0, 80, 1), OF_PRE_PROCESS);
        let pre_events = pre_view.events_received.clone();
        group.add(Box::new(pre_view));

        // Regular view (gets focus)
        let regular_view = TestView::new(Rect::new(0, 1, 80, 22));
        let regular_events = regular_view.events_received.clone();
        let regular_id = regular_view.id();
        group.add(Box::new(regular_view));

        // Post-process view (help handler)
        let post_view = TestView::with_options(Rect::new(0, 23, 80, 1), OF_POST_PROCESS);
        let post_events = post_view.events_received.clone();
        group.add(Box::new(post_view));

        // Set focus to regular view
        group.focus_by_id(regular_id);

        // Dispatch a command event
        let mut event = Event::command(CM_OK);
        group.handle_event(&mut event);

        // Verify order: pre-process → focused → post-process
        // Pre-process should have received it
        let pre_evts = pre_events.borrow();
        assert_eq!(pre_evts.len(), 1);
        drop(pre_evts);

        // Focused view should have received it (after pre-process)
        let regular_evts = regular_events.borrow();
        assert_eq!(regular_evts.len(), 1);
        drop(regular_evts);

        // Post-process should also receive it
        let post_evts = post_events.borrow();
        assert_eq!(post_evts.len(), 1);
    }

    #[test]
    fn test_group_mouse_reverse_z() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        // Back view
        let back = TestView::new(Rect::new(0, 0, 20, 10));
        let back_events = back.events_received.clone();
        group.add(Box::new(back));

        // Front view (overlapping)
        let front = TestView::new(Rect::new(5, 5, 10, 5));
        let front_events = front.events_received.clone();
        group.add(Box::new(front));

        // Click in overlap area (should hit front only)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 10,
            row: 7,
            modifiers: KeyModifiers::empty(),
        };
        let mut event = Event::mouse(mouse_event);
        group.handle_event(&mut event);

        // Front should have received the event
        let front_evts = front_events.borrow();
        assert_eq!(front_evts.len(), 1);
        assert_eq!(front_evts[0], "mouse");
        drop(front_evts);

        // Back should NOT have received it
        let back_evts = back_events.borrow();
        assert!(back_evts.is_empty());
    }

    #[test]
    fn test_group_broadcast_all() {
        let mut group = Group::new(Rect::new(0, 0, 80, 24));

        let view1 = TestView::new(Rect::new(0, 0, 10, 5));
        let events1 = view1.events_received.clone();
        group.add(Box::new(view1));

        let view2 = TestView::new(Rect::new(0, 0, 10, 5));
        let events2 = view2.events_received.clone();
        group.add(Box::new(view2));

        let view3 = TestView::new(Rect::new(0, 0, 10, 5));
        let events3 = view3.events_received.clone();
        group.add(Box::new(view3));

        // Dispatch a broadcast
        let mut event = Event::broadcast(CM_OK);
        group.handle_event(&mut event);

        // All children should have received it
        let evts1 = events1.borrow();
        let evts2 = events2.borrow();
        let evts3 = events3.borrow();
        assert_eq!(evts1.len(), 1);
        assert_eq!(evts2.len(), 1);
        assert_eq!(evts3.len(), 1);
        assert_eq!(evts1[0], "broadcast:10");
        assert_eq!(evts2[0], "broadcast:10");
        assert_eq!(evts3[0], "broadcast:10");
    }
}
