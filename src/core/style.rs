//! Color and style types: [`Color`], [`Modifier`] and [`Style`].
//!
//! Mirrors the Ratatui API subset used by turbo-tui. [`Modifier`] is a
//! hand-rolled bitfield instead of a `bitflags` dependency.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// A terminal color.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// Resets the foreground or background color.
    #[default]
    Reset,
    /// ANSI black.
    Black,
    /// ANSI red.
    Red,
    /// ANSI green.
    Green,
    /// ANSI yellow.
    Yellow,
    /// ANSI blue.
    Blue,
    /// ANSI magenta.
    Magenta,
    /// ANSI cyan.
    Cyan,
    /// ANSI gray (the normal white of the 8-color palette).
    Gray,
    /// ANSI bright black (dark gray).
    DarkGray,
    /// ANSI bright red.
    LightRed,
    /// ANSI bright green.
    LightGreen,
    /// ANSI bright yellow.
    LightYellow,
    /// ANSI bright blue.
    LightBlue,
    /// ANSI bright magenta.
    LightMagenta,
    /// ANSI bright cyan.
    LightCyan,
    /// ANSI bright white.
    White,
    /// A 24-bit RGB color.
    Rgb(u8, u8, u8),
    /// An 8-bit 256-color palette index.
    Indexed(u8),
}

/// Text modifiers.
///
/// Modifiers are a bitfield so they can be combined with `|`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifier(u16);

impl Modifier {
    /// Bold text.
    pub const BOLD: Self = Self(0b0000_0000_0001);
    /// Dim text.
    pub const DIM: Self = Self(0b0000_0000_0010);
    /// Italic text.
    pub const ITALIC: Self = Self(0b0000_0000_0100);
    /// Underlined text.
    pub const UNDERLINED: Self = Self(0b0000_0000_1000);
    /// Slowly blinking text.
    pub const SLOW_BLINK: Self = Self(0b0000_0001_0000);
    /// Rapidly blinking text.
    pub const RAPID_BLINK: Self = Self(0b0000_0010_0000);
    /// Reverse video.
    pub const REVERSED: Self = Self(0b0000_0100_0000);
    /// Hidden text.
    pub const HIDDEN: Self = Self(0b0000_1000_0000);
    /// Crossed-out text.
    pub const CROSSED_OUT: Self = Self(0b0001_0000_0000);

    /// No modifiers.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// All defined modifiers.
    #[must_use]
    pub const fn all() -> Self {
        Self(0b0001_1111_1111)
    }

    /// Creates a modifier from raw bits, discarding unknown bits.
    #[must_use]
    pub const fn from_bits_truncate(bits: u16) -> Self {
        Self(bits & Self::all().0)
    }

    /// Returns the raw bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Returns `true` if all flags in `other` are set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Inserts the flags of `other`.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes the flags of `other`.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Returns the union of both modifiers.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns the flags in `self` that are not in `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Returns `true` if no flags are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Modifier {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for Modifier {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

impl BitAnd for Modifier {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Modifier {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for Modifier {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::from_bits_truncate(!self.0)
    }
}

/// A cell style: colors plus modifiers.
///
/// Styles are incremental: applying several styles to a cell merges them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    /// The foreground color.
    pub fg: Option<Color>,
    /// The background color.
    pub bg: Option<Color>,
    /// The underline color.
    pub underline_color: Option<Color>,
    /// Modifiers to add.
    pub add_modifier: Modifier,
    /// Modifiers to remove.
    pub sub_modifier: Modifier,
}

impl Style {
    /// Creates a style with no properties set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            underline_color: None,
            add_modifier: Modifier::empty(),
            sub_modifier: Modifier::empty(),
        }
    }

    /// Creates a style that resets all properties.
    #[must_use]
    pub const fn reset() -> Self {
        Self {
            fg: Some(Color::Reset),
            bg: Some(Color::Reset),
            underline_color: Some(Color::Reset),
            add_modifier: Modifier::empty(),
            sub_modifier: Modifier::all(),
        }
    }

    /// Sets the foreground color.
    #[must_use]
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Sets the background color.
    #[must_use]
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Sets the underline color.
    #[must_use]
    pub const fn underline_color(mut self, color: Color) -> Self {
        self.underline_color = Some(color);
        self
    }

    /// Adds modifiers.
    #[must_use]
    pub const fn add_modifier(mut self, modifier: Modifier) -> Self {
        self.sub_modifier = self.sub_modifier.difference(modifier);
        self.add_modifier = self.add_modifier.union(modifier);
        self
    }

    /// Removes modifiers.
    #[must_use]
    pub const fn remove_modifier(mut self, modifier: Modifier) -> Self {
        self.add_modifier = self.add_modifier.difference(modifier);
        self.sub_modifier = self.sub_modifier.union(modifier);
        self
    }

    /// Removes modifiers (alias for [`Style::remove_modifier`]).
    #[must_use]
    pub const fn sub_modifier(self, modifier: Modifier) -> Self {
        self.remove_modifier(modifier)
    }

    /// Returns `true` if the given modifier is active.
    #[must_use]
    pub const fn has_modifier(self, modifier: Modifier) -> bool {
        self.add_modifier.contains(modifier) && !self.sub_modifier.contains(modifier)
    }

    /// Merges `other` into `self`, with `other` taking precedence.
    #[must_use]
    pub fn patch<S: Into<Self>>(mut self, other: S) -> Self {
        let other = other.into();
        self.fg = other.fg.or(self.fg);
        self.bg = other.bg.or(self.bg);
        self.underline_color = other.underline_color.or(self.underline_color);
        self.add_modifier.remove(other.sub_modifier);
        self.add_modifier.insert(other.add_modifier);
        self.sub_modifier.remove(other.add_modifier);
        self.sub_modifier.insert(other.sub_modifier);
        self
    }
}

impl From<Color> for Style {
    fn from(color: Color) -> Self {
        Self::new().fg(color)
    }
}

impl From<Modifier> for Style {
    fn from(modifier: Modifier) -> Self {
        Self::new().add_modifier(modifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_patch_overrides_colors() {
        let base = Style::default().fg(Color::Blue).bg(Color::Black);
        let patch = Style::default().fg(Color::Red);
        let merged = base.patch(patch);
        assert_eq!(merged.fg, Some(Color::Red));
        assert_eq!(merged.bg, Some(Color::Black));
    }

    #[test]
    fn test_style_patch_adds_modifiers() {
        let merged = Style::default()
            .add_modifier(Modifier::BOLD)
            .patch(Style::default().add_modifier(Modifier::ITALIC));
        assert_eq!(merged.add_modifier, Modifier::BOLD | Modifier::ITALIC);
        assert_eq!(merged.sub_modifier, Modifier::empty());
    }

    #[test]
    fn test_style_patch_subtracts_modifiers() {
        let merged = Style::default()
            .add_modifier(Modifier::BOLD | Modifier::ITALIC)
            .patch(Style::default().remove_modifier(Modifier::ITALIC));
        assert_eq!(merged.add_modifier, Modifier::BOLD);
        assert!(merged.sub_modifier.contains(Modifier::ITALIC));
        assert!(!merged.has_modifier(Modifier::ITALIC));
    }

    #[test]
    fn test_style_add_modifier_cancels_sub() {
        let style = Style::default()
            .remove_modifier(Modifier::BOLD)
            .add_modifier(Modifier::BOLD);
        assert_eq!(style.add_modifier, Modifier::BOLD);
        assert_eq!(style.sub_modifier, Modifier::empty());
    }

    #[test]
    fn test_modifier_bitops() {
        let combined = Modifier::BOLD | Modifier::UNDERLINED;
        assert!(combined.contains(Modifier::BOLD));
        assert!(combined.contains(Modifier::UNDERLINED));
        assert!(!combined.contains(Modifier::ITALIC));
        assert_eq!((combined & Modifier::BOLD), Modifier::BOLD);
        assert_eq!(!Modifier::empty(), Modifier::all());
        assert_eq!(combined.bits(), 0b0000_0000_1001);
    }

    #[test]
    fn test_style_reset() {
        let reset = Style::reset();
        assert_eq!(reset.fg, Some(Color::Reset));
        assert_eq!(reset.bg, Some(Color::Reset));
        assert_eq!(reset.sub_modifier, Modifier::all());
    }
}
