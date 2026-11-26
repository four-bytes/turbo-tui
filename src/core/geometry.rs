//! Geometry primitives: [`Position`], [`Rect`] and [`Size`].
//!
//! These types mirror the subset of Ratatui's geometry API that turbo-tui
//! uses, so the rest of the crate can keep importing them from `crate::core`.

use std::cmp::{max, min};

/// A position in the terminal coordinate system.
///
/// The position is relative to the top left corner of the terminal, with
/// `(0, 0)` being the top left. The x axis increases to the right and the y
/// axis increases downwards.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// The x coordinate (column).
    pub x: u16,
    /// The y coordinate (row).
    pub y: u16,
}

impl Position {
    /// The origin, `(0, 0)`.
    pub const ORIGIN: Self = Self::new(0, 0);

    /// Creates a new position.
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

impl From<(u16, u16)> for Position {
    fn from((x, y): (u16, u16)) -> Self {
        Self { x, y }
    }
}

impl From<Position> for (u16, u16) {
    fn from(position: Position) -> Self {
        (position.x, position.y)
    }
}

/// A rectangular area in the terminal coordinate system.
///
/// The rectangle is defined by its top-left corner (`x`, `y`) and its
/// `width` and `height`. The right and bottom coordinates are exclusive.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// The x coordinate of the top left corner.
    pub x: u16,
    /// The y coordinate of the top left corner.
    pub y: u16,
    /// The width of the rectangle.
    pub width: u16,
    /// The height of the rectangle.
    pub height: u16,
}

impl Rect {
    /// A zero sized rectangle at the origin.
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// Creates a new rectangle, clamping the width and height so the right and
    /// bottom coordinates stay within `u16`.
    #[must_use]
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        let width = x.saturating_add(width) - x;
        let height = y.saturating_add(height) - y;
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The area of the rectangle in cells.
    #[must_use]
    pub const fn area(self) -> u32 {
        (self.width as u32) * (self.height as u32)
    }

    /// Returns `true` if the rectangle has no area.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the left coordinate of the rectangle.
    #[must_use]
    pub const fn left(self) -> u16 {
        self.x
    }

    /// Returns the right coordinate of the rectangle (exclusive).
    #[must_use]
    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    /// Returns the top coordinate of the rectangle.
    #[must_use]
    pub const fn top(self) -> u16 {
        self.y
    }

    /// Returns the bottom coordinate of the rectangle (exclusive).
    #[must_use]
    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    /// Returns a new rectangle that contains both `self` and `other`.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x1 = min(self.x, other.x);
        let y1 = min(self.y, other.y);
        let x2 = max(self.right(), other.right());
        let y2 = max(self.bottom(), other.bottom());
        Self {
            x: x1,
            y: y1,
            width: x2.saturating_sub(x1),
            height: y2.saturating_sub(y1),
        }
    }

    /// Returns the intersection of `self` and `other`.
    ///
    /// If the two rectangles do not overlap, the result has no area.
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let x1 = max(self.x, other.x);
        let y1 = max(self.y, other.y);
        let x2 = min(self.right(), other.right());
        let y2 = min(self.bottom(), other.bottom());
        Self {
            x: x1,
            y: y1,
            width: x2.saturating_sub(x1),
            height: y2.saturating_sub(y1),
        }
    }

    /// Returns `true` if `position` lies inside the rectangle.
    ///
    /// The left and top borders are inclusive, the right and bottom borders
    /// are exclusive.
    #[must_use]
    pub const fn contains(self, position: Position) -> bool {
        position.x >= self.x
            && position.x < self.right()
            && position.y >= self.y
            && position.y < self.bottom()
    }

    /// Moves and shrinks this rectangle so it fits inside `other`.
    #[must_use]
    pub fn clamp(self, other: Self) -> Self {
        let width = min(self.width, other.width);
        let height = min(self.height, other.height);
        let x = self.x.clamp(other.x, other.right().saturating_sub(width));
        let y = self.y.clamp(other.y, other.bottom().saturating_sub(height));
        Self::new(x, y, width, height)
    }

    /// Returns the top-left corner as a [`Position`].
    #[must_use]
    pub const fn as_position(self) -> Position {
        Position {
            x: self.x,
            y: self.y,
        }
    }

    /// Returns the width and height as a [`Size`].
    #[must_use]
    pub const fn as_size(self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }
}

/// A width/height pair.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    /// The width in columns.
    pub width: u16,
    /// The height in rows.
    pub height: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_intersection_overlap() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(b), Rect::new(5, 5, 5, 5));
    }

    #[test]
    fn test_rect_intersection_disjoint_is_empty() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(20, 20, 5, 5);
        let result = a.intersection(b);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rect_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 15, 10, 10);
        assert_eq!(a.union(b), Rect::new(0, 0, 15, 25));
    }

    #[test]
    fn test_rect_contains_border_exclusive() {
        let rect = Rect::new(2, 3, 4, 5);
        assert!(rect.contains(Position::new(2, 3)));
        assert!(rect.contains(Position::new(5, 7)));
        assert!(!rect.contains(Position::new(6, 7)));
        assert!(!rect.contains(Position::new(2, 8)));
    }

    #[test]
    fn test_rect_clamp_moves_into_bounds() {
        let area = Rect::new(0, 0, 100, 100);
        assert_eq!(
            Rect::new(80, 80, 30, 30).clamp(area),
            Rect::new(70, 70, 30, 30)
        );
    }

    #[test]
    fn test_rect_clamp_limits_size() {
        let area = Rect::new(0, 0, 20, 10);
        assert_eq!(Rect::new(0, 0, 50, 50).clamp(area), Rect::new(0, 0, 20, 10));
    }

    #[test]
    fn test_rect_is_empty() {
        assert!(Rect::new(0, 0, 0, 5).is_empty());
        assert!(Rect::new(0, 0, 5, 0).is_empty());
        assert!(!Rect::new(0, 0, 1, 1).is_empty());
    }

    #[test]
    fn test_position_from_tuple_roundtrip() {
        let position = Position::from((3, 4));
        assert_eq!(position, Position::new(3, 4));
        assert_eq!(<(u16, u16)>::from(position), (3, 4));
    }

    #[test]
    fn test_rect_as_position_and_size() {
        let rect = Rect::new(1, 2, 3, 4);
        assert_eq!(rect.as_position(), Position::new(1, 2));
        assert_eq!(
            rect.as_size(),
            Size {
                width: 3,
                height: 4
            }
        );
    }
}
