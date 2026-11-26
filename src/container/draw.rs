//! Drawing logic for `Container`.

use super::Container;
use crate::core::Buffer;
use crate::core::Rect;
use crate::view::SF_VISIBLE;

impl Container {
    /// Draw all visible children whose bounds intersect `clip`.
    pub(crate) fn draw_children(&self, buf: &mut Buffer, clip: Rect) {
        for child in &self.children {
            if child.state() & SF_VISIBLE != 0 {
                let cb = child.bounds();
                let intersection = cb.intersection(clip);
                if intersection.width > 0 && intersection.height > 0 {
                    child.draw(buf, intersection);
                }
            }
        }
    }
}
