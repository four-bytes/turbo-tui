//! Event dispatch for `Container`.

use super::Container;
use crate::command::{CM_DRAG_END, CM_DRAG_MOVE, CM_DRAG_START};
use crate::view::{
    Event, EventKind, OF_DROP_TARGET, OF_POST_PROCESS, OF_PRE_PROCESS, SF_DRAGGING, SF_RESIZING,
    SF_VISIBLE,
};

impl Container {
    /// Three-phase event dispatch.
    ///
    /// - **Key/Command:** `PreProcess` → `Focused` → `PostProcess`
    /// - **Mouse:** Mouse-capture (Drag/Up to focused if dragging/resizing),
    ///   then reverse Z-order hit-test.
    /// - **Broadcast/Resize:** All children.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn dispatch_event(&mut self, event: &mut Event) {
        if event.is_cleared() {
            return;
        }

        match &event.kind.clone() {
            EventKind::Key(_) | EventKind::Command(_) => {
                // Phase 1: Pre-process
                for i in 0..self.children.len() {
                    if event.is_cleared() {
                        break;
                    }
                    if self.children[i].options() & OF_PRE_PROCESS != 0 {
                        self.children[i].handle_event(event);
                    }
                }

                // Phase 2: Focused child
                if !event.is_cleared() {
                    if let Some(idx) = self.focused {
                        if idx < self.children.len() {
                            self.children[idx].handle_event(event);
                        }
                    }
                }

                // Phase 3: Post-process
                if !event.is_cleared() {
                    for i in 0..self.children.len() {
                        if event.is_cleared() {
                            break;
                        }
                        if self.children[i].options() & OF_POST_PROCESS != 0 {
                            self.children[i].handle_event(event);
                        }
                    }
                }
            }

            EventKind::Mouse(mouse) => {
                let col = mouse.column;
                let row = mouse.row;

                // Mouse capture: Drag/Up events go to focused child if it is
                // currently dragging or resizing (regardless of hit-test).
                if matches!(
                    mouse.kind,
                    crossterm::event::MouseEventKind::Drag(_)
                        | crossterm::event::MouseEventKind::Up(_)
                ) {
                    if let Some(idx) = self.focused {
                        if idx < self.children.len() {
                            let st = self.children[idx].state();
                            if st & (SF_DRAGGING | SF_RESIZING) != 0 {
                                self.children[idx].handle_event(event);
                                if event.is_cleared() {
                                    return;
                                }
                            }
                        }
                    }
                }

                // Drop target handling: on Left mouse Up, check for drop targets
                // under the cursor before normal hit-testing.
                if matches!(
                    mouse.kind,
                    crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
                ) && !event.is_cleared()
                {
                    for i in (0..self.children.len()).rev() {
                        if event.is_cleared() {
                            break;
                        }
                        let b = self.children[i].bounds();
                        if self.children[i].state() & SF_VISIBLE != 0
                            && col >= b.x
                            && col < b.x + b.width
                            && row >= b.y
                            && row < b.y + b.height
                            && self.children[i].options() & OF_DROP_TARGET != 0
                        {
                            self.children[i].handle_drop(Box::new(()));
                            event.clear();
                            return;
                        }
                    }
                }

                // MouseMoved: broadcast to ALL visible children so each can
                // update or clear its hover state based on the mouse position.
                if matches!(mouse.kind, crossterm::event::MouseEventKind::Moved) {
                    for i in (0..self.children.len()).rev() {
                        if self.children[i].state() & SF_VISIBLE != 0 {
                            self.children[i].handle_event(event);
                        }
                    }
                    return;
                }

                // Normal hit-testing: reverse Z-order (front to back)
                for i in (0..self.children.len()).rev() {
                    if event.is_cleared() {
                        break;
                    }
                    let b = self.children[i].bounds();
                    if self.children[i].state() & SF_VISIBLE != 0
                        && col >= b.x
                        && col < b.x + b.width
                        && row >= b.y
                        && row < b.y + b.height
                    {
                        self.children[i].handle_event(event);
                        break; // Only topmost gets the mouse event
                    }
                }

                // Post drag-and-drop commands as deferred events so they are
                // processed after the main dispatch cycle. This allows views to
                // react to drag state changes (e.g. drop target highlighting).
                match mouse.kind {
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        event.post(Event::command(CM_DRAG_START));
                    }
                    crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                        event.post(Event::command(CM_DRAG_MOVE));
                    }
                    crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                        event.post(Event::command(CM_DRAG_END));
                    }
                    _ => {}
                }
            }

            EventKind::Broadcast(_) | EventKind::Resize(_, _) => {
                for i in 0..self.children.len() {
                    if event.is_cleared() {
                        break;
                    }
                    self.children[i].handle_event(event);
                }
            }

            EventKind::None => {}
        }
    }
}
