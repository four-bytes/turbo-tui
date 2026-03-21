//! Event dispatch for `Container`.

use super::Container;
use crate::view::{
    Event, EventKind, OF_POST_PROCESS, OF_PRE_PROCESS, SF_DRAGGING, SF_RESIZING, SF_VISIBLE,
};

impl Container {
    /// Three-phase event dispatch.
    ///
    /// - **Key/Command:** `PreProcess` → `Focused` → `PostProcess`
    /// - **Mouse:** Mouse-capture (Drag/Up to focused if dragging/resizing),
    ///   then reverse Z-order hit-test.
    /// - **Broadcast/Resize:** All children.
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
