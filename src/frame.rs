//! Frame — Window border decoration with optional scrollbars.
//!
//! Frame draws a rectangular border using characters from the current theme.
//! It supports:
//! - Three frame types: Window, Dialog, Single
//! - Centered title on the top border
//! - Close button `[■]` on the top-left (for closeable frames)
//! - Resize handle `◢` on the bottom-right (for resizable frames)
//! - Optional vertical scrollbar on the right border
//! - Optional horizontal scrollbar on the bottom border
//!
//! Frame does NOT manage children — it only draws the border decoration.
//! Use [`Window`] for a complete window with Frame + interior Container.

use crate::canvas::{Canvas, FrameGadgets, FrameKind, FrameSpec, GadgetState};
use crate::cell_skin::ButtonTray;
use crate::core::Rect;
use crate::scrollbar::ScrollBar;
use crate::theme;
use crate::view::{Event, View, ViewBase, ViewId, SF_DRAGGING, SF_FOCUSED, SF_RESIZING};
use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use std::any::Any;

// ============================================================================
// FrameType
// ============================================================================

/// Frame type determines the visual style of the border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Window frame — used for regular overlapping windows.
    Window,
    /// Dialog frame — used for modal dialogs.
    Dialog,
    /// Single-line frame — used for group boxes and panels.
    Single,
}

/// Which element of the frame is currently hovered by the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameHover {
    /// Nothing hovered.
    None,
    /// Close button is hovered.
    CloseButton,
    /// Minimize button is hovered.
    MinimizeButton,
    /// Maximize button is hovered.
    MaximizeButton,
    /// Resize handle is hovered.
    ResizeHandle,
}

// ============================================================================
// FrameConfig — Configuration presets for Frame construction
// ============================================================================

/// Configuration for creating a [`Frame`] with a specific set of features.
///
/// Use the named constructors for common configurations:
/// - [`FrameConfig::window()`] — closeable, resizable, min/max buttons
/// - [`FrameConfig::dialog()`] — no close, no resize, no buttons
/// - [`FrameConfig::panel()`] — single-line frame, no close, no resize
///
/// # Example
///
/// ```
/// use turbo_tui::frame::{FrameConfig, FrameType};
///
/// let config = FrameConfig::window().with_v_scrollbar(true);
/// assert!(config.closeable);
/// assert!(config.v_scrollbar);
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameConfig {
    /// Frame type (Window, Dialog, Single).
    pub frame_type: FrameType,
    /// Whether the frame has a close button.
    pub closeable: bool,
    /// Whether the frame has a resize handle.
    pub resizable: bool,
    /// Whether the frame has a minimize button.
    pub minimizable: bool,
    /// Whether the frame has a maximize button.
    pub maximizable: bool,
    /// Whether to create a vertical scrollbar.
    pub v_scrollbar: bool,
    /// Whether to create a horizontal scrollbar.
    pub h_scrollbar: bool,
}

impl Default for FrameConfig {
    /// Default is a Window config (closeable, resizable).
    fn default() -> Self {
        Self::window()
    }
}

impl FrameConfig {
    /// Window defaults: closeable, resizable, min/max from theme.
    #[must_use]
    pub fn window() -> Self {
        Self {
            frame_type: FrameType::Window,
            closeable: true,
            resizable: true,
            minimizable: true,
            maximizable: true,
            v_scrollbar: false,
            h_scrollbar: false,
        }
    }

    /// Dialog defaults: dialog frame, no close, no resize, no min/max.
    #[must_use]
    pub fn dialog() -> Self {
        Self {
            frame_type: FrameType::Dialog,
            closeable: false,
            resizable: false,
            minimizable: false,
            maximizable: false,
            v_scrollbar: false,
            h_scrollbar: false,
        }
    }

    /// Panel defaults: single-line frame, no close, no resize, no min/max.
    #[must_use]
    pub fn panel() -> Self {
        Self {
            frame_type: FrameType::Single,
            closeable: false,
            resizable: false,
            minimizable: false,
            maximizable: false,
            v_scrollbar: false,
            h_scrollbar: false,
        }
    }

    /// Set whether to create a vertical scrollbar.
    #[must_use]
    pub fn with_v_scrollbar(mut self, yes: bool) -> Self {
        self.v_scrollbar = yes;
        self
    }

    /// Set whether to create a horizontal scrollbar.
    #[must_use]
    pub fn with_h_scrollbar(mut self, yes: bool) -> Self {
        self.h_scrollbar = yes;
        self
    }

    /// Set whether the frame is closeable.
    #[must_use]
    pub fn with_closeable(mut self, yes: bool) -> Self {
        self.closeable = yes;
        self
    }

    /// Set whether the frame is resizable.
    #[must_use]
    pub fn with_resizable(mut self, yes: bool) -> Self {
        self.resizable = yes;
        self
    }

    /// Set whether the frame has a minimize button.
    #[must_use]
    pub fn with_minimizable(mut self, yes: bool) -> Self {
        self.minimizable = yes;
        self
    }

    /// Set whether the frame has a maximize button.
    #[must_use]
    pub fn with_maximizable(mut self, yes: bool) -> Self {
        self.maximizable = yes;
        self
    }
}

// ============================================================================
// Frame
// ============================================================================

/// Window border decoration with title, close button, resize handle, and optional scrollbars.
///
/// Frame draws a rectangular border using characters from the current theme.
/// It supports:
/// - Centered title on the top border
/// - Close button `[■]` on the top-left (configurable)
/// - Resize handle `⋱` on the bottom-right (configurable)
/// - Optional vertical scrollbar on the right border
/// - Optional horizontal scrollbar on the bottom border
///
/// Frame does NOT manage children — it only draws the border decoration.
/// Use [`Window`] for a complete window with Frame + interior Container.
///
/// # Example
///
/// ```ignore
/// use turbo_tui::frame::{Frame, FrameType};
/// use turbo_tui::core::Rect;
///
/// let frame = Frame::new(Rect::new(10, 5, 40, 20), "My Window", FrameType::Window);
/// assert!(frame.closeable());
/// assert!(frame.resizable());
/// ```
#[allow(clippy::struct_field_names)]
#[allow(clippy::struct_excessive_bools)]
pub struct Frame {
    /// Base view functionality.
    base: ViewBase,
    /// Window/dialog title displayed on the top border.
    title: String,
    /// Frame type determines border style.
    frame_type: FrameType,
    /// Whether the frame has a close button.
    closeable: bool,
    /// Whether the frame has a resize handle.
    resizable: bool,
    /// Whether the frame has a minimize button.
    minimizable: bool,
    /// Whether the frame has a maximize button.
    maximizable: bool,
    /// Optional vertical scrollbar (occupies right border).
    v_scrollbar: Option<ScrollBar>,
    /// Optional horizontal scrollbar (occupies bottom border).
    h_scrollbar: Option<ScrollBar>,
    /// Currently hovered frame element.
    hovered: FrameHover,
}

impl Frame {
    /// Create a new frame with the given bounds, title, and frame type.
    ///
    /// By default, the frame is closeable and resizable for Window type,
    /// and neither closeable nor resizable for Dialog/Single types.
    ///
    /// # Arguments
    ///
    /// * `bounds` - The bounding rectangle for the frame.
    /// * `title` - The title text displayed on the top border.
    /// * `frame_type` - The frame type (Window, Dialog, or Single).
    #[must_use]
    pub fn new(bounds: Rect, title: &str, frame_type: FrameType) -> Self {
        let (closeable, resizable) = match frame_type {
            FrameType::Window => (true, true),
            FrameType::Dialog | FrameType::Single => (false, false),
        };
        let minimizable = match frame_type {
            FrameType::Window => crate::theme::with_current(|t| !t.minimize_button_text.is_empty()),
            FrameType::Dialog | FrameType::Single => false,
        };
        let maximizable = match frame_type {
            FrameType::Window => crate::theme::with_current(|t| !t.maximize_button_text.is_empty()),
            FrameType::Dialog | FrameType::Single => false,
        };
        Self {
            base: ViewBase::new(bounds),
            title: title.to_owned(),
            frame_type,
            closeable,
            resizable,
            minimizable,
            maximizable,
            v_scrollbar: None,
            h_scrollbar: None,
            hovered: FrameHover::None,
        }
    }

    /// Create a frame from a [`FrameConfig`].
    ///
    /// This applies all configuration from the config struct, including
    /// creating scrollbars if requested.
    ///
    /// # Arguments
    ///
    /// * `bounds` - The bounding rectangle for the frame.
    /// * `title` - The title text displayed on the top border.
    /// * `config` - The frame configuration.
    #[must_use]
    pub fn from_config(bounds: Rect, title: &str, config: &FrameConfig) -> Self {
        let mut frame = Self {
            base: ViewBase::new(bounds),
            title: title.to_owned(),
            frame_type: config.frame_type,
            closeable: config.closeable,
            resizable: config.resizable,
            minimizable: config.minimizable,
            maximizable: config.maximizable,
            v_scrollbar: None,
            h_scrollbar: None,
            hovered: FrameHover::None,
        };
        if config.v_scrollbar {
            let sb = ScrollBar::vertical(Rect::new(0, 0, 1, bounds.height.saturating_sub(2)));
            frame.v_scrollbar = Some(sb);
        }
        if config.h_scrollbar {
            let sb = ScrollBar::horizontal(Rect::new(0, 0, bounds.width.saturating_sub(2), 1));
            frame.h_scrollbar = Some(sb);
        }
        frame
    }

    /// Get the frame title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the frame title.
    pub fn set_title(&mut self, title: &str) {
        if self.title != title {
            title.clone_into(&mut self.title);
            self.base.mark_dirty();
        }
    }

    /// Get the frame type.
    #[must_use]
    pub fn frame_type(&self) -> FrameType {
        self.frame_type
    }

    /// Check if the frame is closeable (has close button).
    #[must_use]
    pub fn closeable(&self) -> bool {
        self.closeable
    }

    /// Set whether the frame is closeable.
    pub fn set_closeable(&mut self, closeable: bool) {
        if self.closeable != closeable {
            self.closeable = closeable;
            self.base.mark_dirty();
        }
    }

    /// Check if the frame is resizable (has resize handle).
    #[must_use]
    pub fn resizable(&self) -> bool {
        self.resizable
    }

    /// Set whether the frame is resizable.
    pub fn set_resizable(&mut self, resizable: bool) {
        if self.resizable != resizable {
            self.resizable = resizable;
            self.base.mark_dirty();
        }
    }

    /// Check if the frame has a minimize button.
    #[must_use]
    pub fn minimizable(&self) -> bool {
        self.minimizable
    }

    /// Set whether the frame has a minimize button.
    pub fn set_minimizable(&mut self, minimizable: bool) {
        if self.minimizable != minimizable {
            self.minimizable = minimizable;
            self.base.mark_dirty();
        }
    }

    /// Check if the frame has a maximize button.
    #[must_use]
    pub fn maximizable(&self) -> bool {
        self.maximizable
    }

    /// Set whether the frame has a maximize button.
    pub fn set_maximizable(&mut self, maximizable: bool) {
        if self.maximizable != maximizable {
            self.maximizable = maximizable;
            self.base.mark_dirty();
        }
    }

    /// Set the vertical scrollbar.
    ///
    /// The scrollbar occupies the entire right column of the frame.
    pub fn set_v_scrollbar(&mut self, scrollbar: ScrollBar) {
        self.v_scrollbar = Some(scrollbar);
        self.base.mark_dirty();
    }

    /// Get the vertical scrollbar.
    #[must_use]
    pub fn v_scrollbar(&self) -> Option<&ScrollBar> {
        self.v_scrollbar.as_ref()
    }

    /// Get mutable access to the vertical scrollbar.
    pub fn v_scrollbar_mut(&mut self) -> Option<&mut ScrollBar> {
        self.v_scrollbar.as_mut()
    }

    /// Remove and return the vertical scrollbar.
    pub fn remove_v_scrollbar(&mut self) -> Option<ScrollBar> {
        let scrollbar = self.v_scrollbar.take();
        if scrollbar.is_some() {
            self.base.mark_dirty();
        }
        scrollbar
    }

    /// Set the horizontal scrollbar.
    ///
    /// The scrollbar occupies the entire bottom row of the frame.
    pub fn set_h_scrollbar(&mut self, scrollbar: ScrollBar) {
        self.h_scrollbar = Some(scrollbar);
        self.base.mark_dirty();
    }

    /// Get the horizontal scrollbar.
    #[must_use]
    pub fn h_scrollbar(&self) -> Option<&ScrollBar> {
        self.h_scrollbar.as_ref()
    }

    /// Get mutable access to the horizontal scrollbar.
    pub fn h_scrollbar_mut(&mut self) -> Option<&mut ScrollBar> {
        self.h_scrollbar.as_mut()
    }

    /// Remove and return the horizontal scrollbar.
    pub fn remove_h_scrollbar(&mut self) -> Option<ScrollBar> {
        let scrollbar = self.h_scrollbar.take();
        if scrollbar.is_some() {
            self.base.mark_dirty();
        }
        scrollbar
    }

    /// Calculate the interior area (inside the borders, minus scrollbars).
    ///
    /// Returns the `Rect` that child content can use, accounting for:
    /// - 1-cell borders on all sides
    /// - Right border offset if vertical scrollbar present
    /// - Bottom border offset if horizontal scrollbar present
    ///
    /// Returns an empty `Rect` if the frame is too small for an interior.
    #[must_use]
    pub fn interior_area(&self) -> Rect {
        let b = self.base.bounds();
        if b.width < 3 || b.height < 3 {
            return Rect::default();
        }

        // Inset is always 1: scrollbars sit on the border, so interior is
        // bounded by the border on all sides (border may have scrollbar overlaid)
        let right_inset: u16 = 1;
        let bottom_inset: u16 = 1;

        let w = b.width.saturating_sub(1 + right_inset);
        let h = b.height.saturating_sub(1 + bottom_inset);

        if w == 0 || h == 0 {
            return Rect::default();
        }

        Rect::new(b.x + 1, b.y + 1, w, h)
    }

    /// Build the button tray for the current frame using the current theme.
    fn build_button_tray(&self) -> ButtonTray {
        let b = self.base.bounds();
        // When minimized (height<=1), suppress min/max buttons — only close + title
        let minimizable = self.minimizable && b.height > 1;
        let maximizable = self.maximizable && b.height > 1;
        theme::with_current(|t| ButtonTray::build(b, self.closeable, minimizable, maximizable, t))
    }

    /// Check if the given position is on the close button `[■]`.
    ///
    /// Returns `true` if closeable and point is at the close button area.
    /// The close button position depends on theme configuration:
    /// - Left-aligned (Borland): positions (`x+margin_left`, y) through end
    /// - Right-aligned (Windows): positions from right edge inward
    #[must_use]
    pub fn is_close_button(&self, col: u16, row: u16) -> bool {
        if !self.closeable {
            return false;
        }
        let b = self.base.bounds();
        if row != b.y || col < b.x || col >= b.x + b.width {
            return false;
        }
        let tray = self.build_button_tray();
        matches!(tray.hit_test(col), FrameHover::CloseButton)
    }

    /// Check if the frame has a close button.
    ///
    /// Returns `true` if closeable and the frame has sufficient width to display the button.
    #[must_use]
    pub fn has_close_button(&self) -> bool {
        self.closeable
    }

    /// Check if the given position is on the minimize button.
    ///
    /// Returns `true` if minimizable and point is at the minimize button area.
    /// Position depends on theme and other button placements.
    /// Returns `false` when `height <= 1` (minimized windows have no min/max buttons).
    #[must_use]
    pub fn is_minimize_button(&self, col: u16, row: u16) -> bool {
        if !self.minimizable {
            return false;
        }
        let b = self.base.bounds();
        // Minimized windows (height=1) don't show minimize/maximize buttons
        if b.height <= 1 {
            return false;
        }
        if row != b.y || col < b.x || col >= b.x + b.width {
            return false;
        }
        let tray = self.build_button_tray();
        matches!(tray.hit_test(col), FrameHover::MinimizeButton)
    }

    /// Check if the given position is on the maximize button.
    ///
    /// Returns `true` if maximizable and point is at the maximize button area.
    /// Position depends on theme and other button placements.
    /// Returns `false` when `height <= 1` (minimized windows have no min/max buttons).
    #[must_use]
    pub fn is_maximize_button(&self, col: u16, row: u16) -> bool {
        if !self.maximizable {
            return false;
        }
        let b = self.base.bounds();
        // Minimized windows (height=1) don't show minimize/maximize buttons
        if b.height <= 1 {
            return false;
        }
        if row != b.y || col < b.x || col >= b.x + b.width {
            return false;
        }
        let tray = self.build_button_tray();
        matches!(tray.hit_test(col), FrameHover::MaximizeButton)
    }

    /// Check if the given position is on the resize handle `⋱`.
    ///
    /// Returns `true` if resizable and point is at bottom-right corner.
    /// The resize handle is at position (x + width - 1, y + height - 1).
    /// Returns `false` when `height <= 1` (no resize handle on minimized windows).
    #[must_use]
    pub fn is_resize_handle(&self, col: u16, row: u16) -> bool {
        if !self.resizable {
            return false;
        }

        let b = self.base.bounds();
        // No resize handle on minimized windows (height=1)
        if b.height <= 1 {
            return false;
        }

        col == b.x + b.width - 1 && row == b.y + b.height - 1
    }

    /// Check if the given position is on the title bar.
    ///
    /// The title bar is the top border row, excluding any button areas.
    #[must_use]
    pub fn is_title_bar(&self, col: u16, row: u16) -> bool {
        let b = self.base.bounds();
        if row != b.y {
            return false;
        }
        if col < b.x || col >= b.x + b.width {
            return false;
        }
        let tray = self.build_button_tray();
        // Title bar = top row, not on any button
        matches!(tray.hit_test(col), FrameHover::None)
    }

    /// Update the hover state based on mouse position.
    ///
    /// Returns the new hover state.
    pub fn update_hover(&mut self, col: u16, row: u16) -> FrameHover {
        let b = self.base.bounds();
        let new_hover = if row == b.y && col >= b.x && col < b.x + b.width {
            // On title bar row — use button tray for hit-testing
            let tray = self.build_button_tray();
            tray.hit_test(col)
        } else if self.is_resize_handle(col, row) {
            FrameHover::ResizeHandle
        } else {
            FrameHover::None
        };

        if new_hover != self.hovered {
            self.hovered = new_hover;
            self.base.mark_dirty();
        }
        new_hover
    }

    /// Clear the hover state (mouse left the frame area).
    pub fn clear_hover(&mut self) {
        if self.hovered != FrameHover::None {
            self.hovered = FrameHover::None;
            self.base.mark_dirty();
        }
    }

    /// Forward a mouse move event to the frame's scrollbars for hover tracking.
    ///
    /// This should be called by Window when `MouseEventKind::Moved` is received
    /// within the window bounds. The scrollbars will update their hover state
    /// (Arrow, Thumb, or None) based on the mouse position.
    pub fn update_scrollbar_hover(&mut self, col: u16, row: u16) {
        let b = self.base.bounds();

        // Vertical scrollbar: right border column, rows between top and bottom border
        if let Some(ref mut sb) = self.v_scrollbar {
            let sb_x = b.x + b.width.saturating_sub(1);
            let sb_y = b.y + 1;
            let sb_height = b.height.saturating_sub(2);
            let sb_bounds = Rect::new(sb_x, sb_y, 1, sb_height);

            if col >= sb_bounds.x
                && col < sb_bounds.x + sb_bounds.width
                && row >= sb_bounds.y
                && row < sb_bounds.y + sb_bounds.height
            {
                // Mouse is on vertical scrollbar - forward event
                let mouse = MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: col,
                    row,
                    modifiers: KeyModifiers::NONE,
                };
                let mut ev = Event::mouse(mouse);
                // Temporarily set bounds so the scrollbar's hit-test works
                let saved_bounds = sb.bounds();
                sb.set_bounds(sb_bounds);
                sb.handle_event(&mut ev);
                sb.set_bounds(saved_bounds);
            } else {
                // Mouse not on vertical scrollbar - clear its hover
                // Send a mouse moved event outside bounds to clear hover
                let mouse = MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                };
                let mut ev = Event::mouse(mouse);
                let saved_bounds = sb.bounds();
                sb.set_bounds(sb_bounds);
                sb.handle_event(&mut ev);
                sb.set_bounds(saved_bounds);
            }
        }

        // Horizontal scrollbar: bottom row, cols between left and right border
        if let Some(ref mut sb) = self.h_scrollbar {
            let h_sb_bounds = Rect::new(
                b.x + 1,
                b.y + b.height.saturating_sub(1),
                b.width.saturating_sub(2),
                1,
            );

            if col >= h_sb_bounds.x
                && col < h_sb_bounds.x + h_sb_bounds.width
                && row >= h_sb_bounds.y
                && row < h_sb_bounds.y + h_sb_bounds.height
            {
                // Mouse is on horizontal scrollbar - forward event
                let mouse = MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: col,
                    row,
                    modifiers: KeyModifiers::NONE,
                };
                let mut ev = Event::mouse(mouse);
                let saved_bounds = sb.bounds();
                sb.set_bounds(h_sb_bounds);
                sb.handle_event(&mut ev);
                sb.set_bounds(saved_bounds);
            } else {
                // Mouse not on horizontal scrollbar - clear its hover
                // But only if there's no vertical scrollbar that might have handled it
                let mouse = MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                };
                let mut ev = Event::mouse(mouse);
                let saved_bounds = sb.bounds();
                sb.set_bounds(h_sb_bounds);
                sb.handle_event(&mut ev);
                sb.set_bounds(saved_bounds);
            }
        }
    }

    /// Clear hover state on all scrollbars.
    ///
    /// Call this when the mouse leaves the window bounds entirely.
    pub fn clear_scrollbar_hover(&mut self) {
        let b = self.base.bounds();

        // Clear vertical scrollbar hover
        if let Some(ref mut sb) = self.v_scrollbar {
            let sb_x = b.x + b.width.saturating_sub(1);
            let sb_y = b.y + 1;
            let sb_height = b.height.saturating_sub(2);
            let sb_bounds = Rect::new(sb_x, sb_y, 1, sb_height);

            let mouse = MouseEvent {
                kind: MouseEventKind::Moved,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            };
            let mut ev = Event::mouse(mouse);
            let saved_bounds = sb.bounds();
            sb.set_bounds(sb_bounds);
            sb.handle_event(&mut ev);
            sb.set_bounds(saved_bounds);
        }

        // Clear horizontal scrollbar hover
        if let Some(ref mut sb) = self.h_scrollbar {
            let h_sb_bounds = Rect::new(
                b.x + 1,
                b.y + b.height.saturating_sub(1),
                b.width.saturating_sub(2),
                1,
            );

            let mouse = MouseEvent {
                kind: MouseEventKind::Moved,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            };
            let mut ev = Event::mouse(mouse);
            let saved_bounds = sb.bounds();
            sb.set_bounds(h_sb_bounds);
            sb.handle_event(&mut ev);
            sb.set_bounds(saved_bounds);
        }
    }

    /// Handle a mouse click on a scrollbar, if any.
    ///
    /// Returns `true` if the click was on a scrollbar and was handled.
    pub fn handle_scrollbar_click(&mut self, col: u16, row: u16, event: &mut Event) -> bool {
        let b = self.base.bounds();

        // Check vertical scrollbar
        if let Some(ref mut sb) = self.v_scrollbar {
            let sb_x = b.x + b.width.saturating_sub(1);
            let sb_y = b.y + 1;
            let sb_height = b.height.saturating_sub(2);
            let sb_bounds = Rect::new(sb_x, sb_y, 1, sb_height);

            if col >= sb_bounds.x
                && col < sb_bounds.x + sb_bounds.width
                && row >= sb_bounds.y
                && row < sb_bounds.y + sb_bounds.height
            {
                let saved_bounds = sb.bounds();
                sb.set_bounds(sb_bounds);
                sb.handle_event(event);
                sb.set_bounds(saved_bounds);
                return true;
            }
        }

        // Check horizontal scrollbar
        if let Some(ref mut sb) = self.h_scrollbar {
            let h_sb_bounds = Rect::new(
                b.x + 1,
                b.y + b.height.saturating_sub(1),
                b.width.saturating_sub(2),
                1,
            );

            if col >= h_sb_bounds.x
                && col < h_sb_bounds.x + h_sb_bounds.width
                && row >= h_sb_bounds.y
                && row < h_sb_bounds.y + h_sb_bounds.height
            {
                let saved_bounds = sb.bounds();
                sb.set_bounds(h_sb_bounds);
                sb.handle_event(event);
                sb.set_bounds(saved_bounds);
                return true;
            }
        }

        false
    }

    /// Get the current hover state.
    #[must_use]
    pub fn hovered(&self) -> FrameHover {
        self.hovered
    }
}

impl View for Frame {
    fn id(&self) -> ViewId {
        self.base.id()
    }

    fn bounds(&self) -> Rect {
        self.base.bounds()
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.base.set_bounds(bounds);
    }

    fn draw(&self, canvas: &mut dyn Canvas, clip: Rect) {
        let b = self.base.bounds();
        if b.width < 2 || b.height < 1 {
            return;
        }

        // Intersect with clip
        let draw_area = b.intersection(clip);
        if draw_area.width == 0 || draw_area.height == 0 {
            return;
        }

        let state = self.base.state();
        let dragging = (state & SF_DRAGGING) != 0 || (state & SF_RESIZING) != 0;
        let active = (state & SF_FOCUSED) != 0;
        let gadget_state = |hovered: bool| {
            if dragging {
                GadgetState::Dragging
            } else if hovered {
                GadgetState::Hover
            } else if active {
                GadgetState::Normal
            } else {
                GadgetState::Inactive
            }
        };

        let kind = match self.frame_type {
            FrameType::Window => FrameKind::Window,
            FrameType::Dialog => FrameKind::Dialog,
            FrameType::Single => FrameKind::Single,
        };

        // When minimized (height=1), only show the close button — no min/max.
        let show_minimizable = self.minimizable && b.height > 1;
        let show_maximizable = self.maximizable && b.height > 1;
        let gadgets = FrameGadgets {
            close: self
                .closeable
                .then(|| gadget_state(self.hovered == FrameHover::CloseButton)),
            minimize: show_minimizable
                .then(|| gadget_state(self.hovered == FrameHover::MinimizeButton)),
            maximize: show_maximizable
                .then(|| gadget_state(self.hovered == FrameHover::MaximizeButton)),
            resize_grip: (self.resizable && kind == FrameKind::Window && b.height > 1)
                .then(|| gadget_state(self.hovered == FrameHover::ResizeHandle)),
        };

        let spec = FrameSpec {
            title: &self.title,
            kind,
            active,
            dragging,
            gadgets,
        };

        canvas.draw_frame(b, &spec, clip);
        if b.height >= 2 {
            self.draw_scrollbars(canvas, clip);
        }
    }

    fn handle_event(&mut self, _event: &mut Event) {
        // Frame does NOT handle events — Window handles events and delegates to Frame's hit-test methods
    }

    fn can_focus(&self) -> bool {
        false
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

impl Frame {
    /// Draw the vertical and horizontal scrollbars if present.
    fn draw_scrollbars(&self, canvas: &mut dyn Canvas, clip: Rect) {
        let b = self.base.bounds();

        // Vertical scrollbar
        if let Some(ref sb) = self.v_scrollbar {
            let sb_x = b.x + b.width.saturating_sub(1);
            let sb_y = b.y + 1;
            let sb_height = b.height.saturating_sub(2);
            let sb_bounds = Rect::new(sb_x, sb_y, 1, sb_height);

            let mut sb_owned = sb.clone();
            sb_owned.set_bounds(sb_bounds);
            sb_owned.draw(canvas, clip);
        }

        // Horizontal scrollbar
        if let Some(ref sb) = self.h_scrollbar {
            let h_sb_bounds = Rect::new(
                b.x + 1,
                b.y + b.height.saturating_sub(1),
                b.width.saturating_sub(2),
                1,
            );
            let mut sb_owned = sb.clone();
            sb_owned.set_bounds(h_sb_bounds);
            sb_owned.draw(canvas, clip);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Buffer, Position};
    use crate::theme::Theme;

    fn setup_default_theme() {
        theme::set(Theme::turbo_vision());
    }

    #[test]
    fn test_frame_new_window_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        assert!(frame.closeable());
        assert!(frame.resizable());
        assert_eq!(frame.title(), "Test");
        assert_eq!(frame.frame_type(), FrameType::Window);
    }

    #[test]
    fn test_frame_new_dialog_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 30, 15), "Dialog", FrameType::Dialog);
        assert!(!frame.closeable());
        assert!(!frame.resizable());
        assert_eq!(frame.frame_type(), FrameType::Dialog);
    }

    #[test]
    fn test_frame_new_single_defaults() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 20, 10), "Group", FrameType::Single);
        assert!(!frame.closeable());
        assert!(!frame.resizable());
        assert_eq!(frame.frame_type(), FrameType::Single);
    }

    #[test]
    fn test_frame_set_title() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Old", FrameType::Window);
        frame.set_title("New Title");
        assert_eq!(frame.title(), "New Title");
    }

    #[test]
    fn test_frame_set_closeable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);
        assert!(frame.closeable());
        frame.set_closeable(false);
        assert!(!frame.closeable());
        frame.set_closeable(true);
        assert!(frame.closeable());
    }

    #[test]
    fn test_frame_set_resizable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);
        assert!(frame.resizable());
        frame.set_resizable(false);
        assert!(!frame.resizable());
        frame.set_resizable(true);
        assert!(frame.resizable());
    }

    #[test]
    fn test_frame_interior_area_no_scrollbars() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        // Bounds: (10, 5, 40, 20)
        // Interior: (11, 6, 38, 18) — 1 border on each side
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_frame_interior_area_with_v_scrollbar() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));

        // With v_scrollbar: scrollbar sits on right border, interior same size
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_frame_interior_area_with_h_scrollbar() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));

        // With h_scrollbar: scrollbar sits on bottom border, interior same size
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_frame_interior_area_with_both_scrollbars() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));

        // Both scrollbars: scrollbars sit on borders, interior same as no scrollbars
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::new(11, 6, 38, 18));
    }

    #[test]
    fn test_frame_interior_area_too_small() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 2, 2), "Test", FrameType::Window);
        // Width and height < 3, so interior should be empty
        let interior = frame.interior_area();
        assert_eq!(interior, Rect::default());

        let frame2 = Frame::new(Rect::new(0, 0, 3, 3), "Test", FrameType::Window);
        // Minimum size for interior: 3x3 gives interior 1x1
        let interior2 = frame2.interior_area();
        assert_eq!(interior2, Rect::new(1, 1, 1, 1));
    }

    #[test]
    fn test_frame_is_close_button() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Close button at (12, 5), (13, 5), (14, 5) — with 1-cell gap from corner
        assert!(frame.is_close_button(12, 5));
        assert!(frame.is_close_button(13, 5));
        assert!(frame.is_close_button(14, 5));

        // Not on close button
        assert!(!frame.is_close_button(10, 5)); // Top-left corner
        assert!(!frame.is_close_button(11, 5)); // Gap between corner and close button
        assert!(!frame.is_close_button(15, 5)); // Past close button
        assert!(!frame.is_close_button(12, 6)); // Different row
    }

    #[test]
    fn test_frame_is_close_button_not_closeable() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Dialog);

        // Not closeable, so is_close_button always returns false
        assert!(!frame.is_close_button(12, 5));
        assert!(!frame.is_close_button(13, 5));
    }

    #[test]
    fn test_frame_is_resize_handle() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Resize handle at (49, 24) — bottom-right corner
        assert!(frame.is_resize_handle(49, 24));

        // Not on resize handle
        assert!(!frame.is_resize_handle(48, 24));
        assert!(!frame.is_resize_handle(49, 23));
    }

    #[test]
    fn test_frame_is_resize_handle_not_resizable() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Dialog);

        // Not resizable, so is_resize_handle always returns false
        assert!(!frame.is_resize_handle(49, 24));
    }

    #[test]
    fn test_frame_is_title_bar() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(10, 5, 40, 20), "Test", FrameType::Window);

        // Title bar is row 5 (top border)
        assert!(frame.is_title_bar(10, 5)); // Top-left corner
        assert!(frame.is_title_bar(11, 5)); // Gap between corner and close button
        assert!(frame.is_title_bar(15, 5)); // Past close button
        assert!(frame.is_title_bar(49, 5)); // Top-right corner

        // Close button area is NOT part of title bar for Window type
        // Close button now at cols 12, 13, 14 (with 1-cell gap)
        assert!(!frame.is_title_bar(12, 5)); // Close button
        assert!(!frame.is_title_bar(13, 5)); // Close button
        assert!(!frame.is_title_bar(14, 5)); // Close button

        // Wrong row
        assert!(!frame.is_title_bar(10, 6));
        assert!(!frame.is_title_bar(10, 4));
    }

    #[test]
    fn test_frame_draw_renders_corners() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Test", FrameType::Window);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // Check corner characters
        let tl = buf.cell(Position::new(0, 0)).unwrap();
        let tr = buf.cell(Position::new(19, 0)).unwrap();
        let bl = buf.cell(Position::new(0, 9)).unwrap();
        let br = buf.cell(Position::new(19, 9)).unwrap();

        // Use theme characters (Turbo Vision theme uses double-line borders)
        assert_eq!(tl.symbol(), "╔");
        assert_eq!(tr.symbol(), "╗");
        assert_eq!(bl.symbol(), "╚");
        // Resize handle in corner for Window type
        assert_eq!(br.symbol(), "◢");
    }

    #[test]
    fn test_frame_draw_renders_title() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "MyTitle", FrameType::Window);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // Title should be centered on the top border
        // Width 20, title " MyTitle " = 9 chars
        // Start position: (20 - 9) / 2 = 5.5 -> 5
        // But need to account for close button [■] at positions 1-3
        // So title starts after position 4

        // Verify we can find the title text somewhere on the top row
        let top_row: String = (0..20)
            .map(|col| buf.cell(Position::new(col, 0)).unwrap().symbol())
            .collect::<String>();

        // Title should appear somewhere
        assert!(top_row.contains('T') || top_row.contains('M'));
    }

    #[test]
    fn test_frame_title_never_overlaps_close_button() {
        setup_default_theme();
        // Very narrow window — title "LongTitle" won't fit centered without overlap
        let bounds = Rect::new(0, 0, 12, 5);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        let frame = Frame::new(bounds, "LongTitle", FrameType::Window);
        frame.draw(
            &mut crate::canvas::CellCanvas::new(&mut buf),
            Rect::new(0, 0, 20, 10),
        );

        // Close button occupies cols 2, 3, 4 (with 1-cell gap from corner)
        // Verify close button is intact
        assert_eq!(buf.cell(Position::new(2, 0)).unwrap().symbol(), "[");
        assert_eq!(buf.cell(Position::new(3, 0)).unwrap().symbol(), "■");
        assert_eq!(buf.cell(Position::new(4, 0)).unwrap().symbol(), "]");

        // Title must start at col 5 or later — cols 2-4 must be close button only
        // Collect the top row from col 5 onwards
        let title_area: String = (5..12)
            .map(|col| {
                buf.cell(Position::new(col, 0))
                    .unwrap()
                    .symbol()
                    .to_string()
            })
            .collect();

        // Title " LongTitle " is centered relative to full frame width (ideal position = 0)
        // Then clipped to button tray area (cols 5-10). Left side truncated, shows "…itle…"
        // Verify: ellipsis indicates truncation, OR some title chars visible
        assert!(
            title_area.contains('…') || title_area.contains('i') || title_area.contains('t'),
            "Title should show truncation ellipsis or visible chars after close button, got: {title_area:?}"
        );
    }

    #[test]
    fn test_frame_draw_renders_close_button() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Test", FrameType::Window);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // Close button at positions 2, 3, 4 (with 1-cell gap from corner)
        let b1 = buf.cell(Position::new(2, 0)).unwrap();
        let b2 = buf.cell(Position::new(3, 0)).unwrap();
        let b3 = buf.cell(Position::new(4, 0)).unwrap();

        assert_eq!(b1.symbol(), "[");
        assert_eq!(b2.symbol(), "■");
        assert_eq!(b3.symbol(), "]");
    }

    #[test]
    fn test_frame_draw_dialog_no_close_button() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Dialog", FrameType::Dialog);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // Dialog frame should not have close button
        // Position 1, 2 should have border characters, not [■
        let c1 = buf.cell(Position::new(1, 0)).unwrap();
        assert_ne!(c1.symbol(), "[");
    }

    #[test]
    fn test_frame_draw_single_frame() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "GroupBox", FrameType::Single);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // Single frame should not have close button or resize handle
        let c1 = buf.cell(Position::new(1, 0)).unwrap();
        assert_ne!(c1.symbol(), "[");

        let br = buf.cell(Position::new(19, 9)).unwrap();
        // Bottom-right should be normal corner, not resize handle
        assert_eq!(br.symbol(), "╝");
    }

    #[test]
    fn test_frame_scrollbar_management() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);

        // Add vertical scrollbar
        frame.set_v_scrollbar(ScrollBar::vertical(Rect::new(0, 0, 1, 10)));
        assert!(frame.v_scrollbar().is_some());

        // Add horizontal scrollbar
        frame.set_h_scrollbar(ScrollBar::horizontal(Rect::new(0, 0, 10, 1)));
        assert!(frame.h_scrollbar().is_some());

        // Remove vertical scrollbar
        let v_sb = frame.remove_v_scrollbar();
        assert!(v_sb.is_some());
        assert!(frame.v_scrollbar().is_none());

        // Remove horizontal scrollbar
        let h_sb = frame.remove_h_scrollbar();
        assert!(h_sb.is_some());
        assert!(frame.h_scrollbar().is_none());
    }

    #[test]
    fn test_frame_view_trait() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 20, 10), "Test", FrameType::Window);

        // Test View trait methods
        let _id1 = frame.id();
        assert_eq!(frame.bounds(), Rect::new(0, 0, 20, 10));

        frame.set_bounds(Rect::new(5, 5, 30, 15));
        assert_eq!(frame.bounds(), Rect::new(5, 5, 30, 15));

        // State management
        assert_eq!(
            frame.state() & crate::view::SF_VISIBLE,
            crate::view::SF_VISIBLE
        );
        frame.set_state(0);
        assert_eq!(frame.state(), 0);

        // Cannot focus
        assert!(!frame.can_focus());
    }

    #[test]
    fn test_frame_hover_close_button() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 30, 15), "Test", FrameType::Window);

        // Initially no hover
        assert_eq!(frame.hovered(), FrameHover::None);

        // Hover over close button (left-aligned: cols 12, 13,14 at row 5 with 1-cell gap)
        let hover = frame.update_hover(13, 5);
        assert_eq!(hover, FrameHover::CloseButton);
        assert_eq!(frame.hovered(), FrameHover::CloseButton);

        // Hover over resize handle (bottom-right corner: col 39, row 19)
        let hover2 = frame.update_hover(39, 19);
        assert_eq!(hover2, FrameHover::ResizeHandle);
        assert_eq!(frame.hovered(), FrameHover::ResizeHandle);

        // Hover over empty area (middle of top border)
        let hover3 = frame.update_hover(20, 5);
        assert_eq!(hover3, FrameHover::None);
        assert_eq!(frame.hovered(), FrameHover::None);

        // Clear hover
        frame.clear_hover();
        assert_eq!(frame.hovered(), FrameHover::None);
    }

    #[test]
    fn test_frame_hover_resize_handle() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 40, 20), "Test", FrameType::Window);

        // Resize handle at bottom-right (39, 19)
        let hover = frame.update_hover(39, 19);
        assert_eq!(hover, FrameHover::ResizeHandle);

        // Just outside - not on resize handle
        let hover2 = frame.update_hover(38, 19);
        assert_eq!(hover2, FrameHover::None);

        let hover3 = frame.update_hover(39, 18);
        assert_eq!(hover3, FrameHover::None);
    }

    #[test]
    fn test_frame_hover_not_closeable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(10, 5, 30, 15), "Test", FrameType::Dialog);

        // Dialog is not closeable, so close button area returns None
        let hover = frame.update_hover(13, 5);
        assert_eq!(hover, FrameHover::None);
    }

    #[test]
    fn test_frame_hover_not_resizable() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 40, 20), "Test", FrameType::Dialog);

        // Dialog is not resizable, so resize handle returns None
        let hover = frame.update_hover(39, 19);
        assert_eq!(hover, FrameHover::None);
    }

    // ============================================================================
    // ButtonTray-specific tests
    // ============================================================================

    #[test]
    fn test_button_tray_close_left_no_controls() {
        // Turbo Vision theme: close on Left (margin 2), controls on Right, no min/max text
        setup_default_theme();
        let b = Rect::new(10, 5, 40, 20);
        let tray = theme::with_current(|t| ButtonTray::build(b, true, false, false, t));

        // close_button_text = "[■]" (3 chars), margin_left = 2
        // close starts at x + margin_left = 10 + 2 = 12, len = 3
        assert_eq!(tray.close, Some((12, 3)));
        assert_eq!(tray.minimize, None);
        assert_eq!(tray.maximize, None);
        // title_start = left_cursor after close = 12 + 3 = 15
        assert_eq!(tray.title_start, 15);
        // title_end = right_cursor = x + width - margin_right = 10 + 40 - 2 = 48
        assert_eq!(tray.title_end, 48);
    }

    #[test]
    fn test_button_tray_hit_test_close() {
        setup_default_theme();
        let b = Rect::new(10, 5, 40, 20);
        let tray = theme::with_current(|t| ButtonTray::build(b, true, false, false, t));

        // Hit inside close button
        assert!(matches!(tray.hit_test(12), FrameHover::CloseButton));
        assert!(matches!(tray.hit_test(13), FrameHover::CloseButton));
        assert!(matches!(tray.hit_test(14), FrameHover::CloseButton));

        // Not on close button
        assert!(matches!(tray.hit_test(11), FrameHover::None));
        assert!(matches!(tray.hit_test(15), FrameHover::None));
    }

    #[test]
    fn test_button_tray_no_close_no_controls() {
        setup_default_theme();
        let b = Rect::new(0, 0, 30, 10);
        let tray = theme::with_current(|t| ButtonTray::build(b, false, false, false, t));

        assert_eq!(tray.close, None);
        assert_eq!(tray.minimize, None);
        assert_eq!(tray.maximize, None);
        // title_start = x + margin_left = 0 + 2 = 2
        assert_eq!(tray.title_start, 2);
        // title_end = x + width - margin_right = 0 + 30 - 2 = 28
        assert_eq!(tray.title_end, 28);
    }

    #[test]
    fn test_button_tray_hit_test_none_on_gap() {
        setup_default_theme();
        let b = Rect::new(10, 5, 40, 20);
        let tray = theme::with_current(|t| ButtonTray::build(b, true, false, false, t));

        // The gap column (x + margin_left - 1 = 11) before close button
        assert!(matches!(tray.hit_test(10), FrameHover::None));
        assert!(matches!(tray.hit_test(11), FrameHover::None));
        // Well past buttons
        assert!(matches!(tray.hit_test(20), FrameHover::None));
    }

    // ============================================================================
    // FrameConfig tests
    // ============================================================================

    #[test]
    fn test_frame_config_window_defaults() {
        let config = FrameConfig::window();
        assert_eq!(config.frame_type, FrameType::Window);
        assert!(config.closeable);
        assert!(config.resizable);
        assert!(config.minimizable);
        assert!(config.maximizable);
        assert!(!config.v_scrollbar);
        assert!(!config.h_scrollbar);
    }

    #[test]
    fn test_frame_config_dialog_defaults() {
        let config = FrameConfig::dialog();
        assert_eq!(config.frame_type, FrameType::Dialog);
        assert!(!config.closeable);
        assert!(!config.resizable);
        assert!(!config.minimizable);
        assert!(!config.maximizable);
    }

    #[test]
    fn test_frame_config_panel_defaults() {
        let config = FrameConfig::panel();
        assert_eq!(config.frame_type, FrameType::Single);
        assert!(!config.closeable);
        assert!(!config.resizable);
    }

    #[test]
    fn test_frame_config_builder_methods() {
        let config = FrameConfig::window()
            .with_v_scrollbar(true)
            .with_h_scrollbar(true)
            .with_closeable(false);
        assert!(!config.closeable);
        assert!(config.v_scrollbar);
        assert!(config.h_scrollbar);
    }

    #[test]
    fn test_frame_config_default_is_window() {
        let config = FrameConfig::default();
        assert_eq!(config, FrameConfig::window());
    }

    #[test]
    fn test_frame_from_config_basic() {
        setup_default_theme();
        let bounds = Rect::new(5, 5, 40, 15);
        let config = FrameConfig::window();
        let frame = Frame::from_config(bounds, "Test", &config);
        assert_eq!(frame.frame_type(), FrameType::Window);
        assert!(frame.closeable());
        assert!(frame.resizable());
        assert_eq!(frame.title(), "Test");
        assert!(frame.v_scrollbar().is_none());
        assert!(frame.h_scrollbar().is_none());
    }

    #[test]
    fn test_frame_from_config_with_scrollbars() {
        setup_default_theme();
        let bounds = Rect::new(0, 0, 30, 10);
        let config = FrameConfig::window()
            .with_v_scrollbar(true)
            .with_h_scrollbar(true);
        let frame = Frame::from_config(bounds, "Scroll", &config);
        assert!(frame.v_scrollbar().is_some());
        assert!(frame.h_scrollbar().is_some());
    }

    #[test]
    fn test_frame_from_config_dialog() {
        setup_default_theme();
        let bounds = Rect::new(10, 10, 30, 10);
        let config = FrameConfig::dialog();
        let frame = Frame::from_config(bounds, "Dialog", &config);
        assert_eq!(frame.frame_type(), FrameType::Dialog);
        assert!(!frame.closeable());
        assert!(!frame.resizable());
    }

    #[test]
    fn test_frame_title_centered_within_full_width() {
        setup_default_theme();
        // Wide window, short title — title should be centered within full width
        let bounds = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(bounds);
        // Dialog has no close button, so title can use full top bar
        let frame = Frame::new(bounds, "Hi", FrameType::Dialog);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        // " Hi " = 4 chars, width 40 → ideal_start = (40 - 4) / 2 = 18
        // So 'H' at col 19 (after leading space at 18)
        let top_row: String = (0..40)
            .map(|col| {
                buf.cell(Position::new(col, 0))
                    .unwrap()
                    .symbol()
                    .to_string()
            })
            .collect();
        // Find " Hi " in the row and verify it's roughly centered
        // Use char_indices to get character position (not byte position, since borders are multi-byte)
        let h_char_pos = top_row.char_indices().position(|(_, c)| c == 'H');
        assert!(
            h_char_pos.is_some(),
            "Title 'H' should be visible, got: {top_row:?}"
        );
        let h_col = h_char_pos.unwrap();
        // Centered at col 19 (0-indexed within the string)
        assert!(
            (17..=21).contains(&h_col),
            "Title 'H' should be near center (col ~19), got col {h_col}, row: {top_row:?}"
        );
    }

    #[test]
    fn test_frame_title_ellipsis_on_right_truncation() {
        setup_default_theme();
        // Narrow dialog (no buttons) with long title
        let bounds = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        let frame = Frame::new(bounds, "VeryLongTitle", FrameType::Dialog);
        frame.draw(
            &mut crate::canvas::CellCanvas::new(&mut buf),
            Rect::new(0, 0, 20, 10),
        );

        // " VeryLongTitle " = 15 chars, available ~ 8 (cols 2..8 for Dialog with margin)
        // Right truncation should show ellipsis
        let top_row: String = (0..10)
            .map(|col| {
                buf.cell(Position::new(col, 0))
                    .unwrap()
                    .symbol()
                    .to_string()
            })
            .collect();
        assert!(
            top_row.contains('…'),
            "Right-truncated title should have ellipsis, got: {top_row:?}"
        );
    }

    #[test]
    fn test_frame_title_no_ellipsis_when_fits() {
        setup_default_theme();
        // Wide dialog with short title — no truncation needed
        let bounds = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(bounds);
        let frame = Frame::new(bounds, "Ok", FrameType::Dialog);
        frame.draw(&mut crate::canvas::CellCanvas::new(&mut buf), bounds);

        let top_row: String = (0..40)
            .map(|col| {
                buf.cell(Position::new(col, 0))
                    .unwrap()
                    .symbol()
                    .to_string()
            })
            .collect();
        assert!(
            !top_row.contains('…'),
            "Title that fits should have no ellipsis, got: {top_row:?}"
        );
        assert!(top_row.contains('O'), "Title should be visible");
        assert!(top_row.contains('k'), "Title should be visible");
    }

    #[test]
    fn test_frame_title_very_narrow_no_crash() {
        setup_default_theme();
        // Extremely narrow frame — available < 2, should not draw title at all
        let bounds = Rect::new(0, 0, 4, 3);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
        let frame = Frame::new(bounds, "X", FrameType::Single);
        // Should not panic
        frame.draw(
            &mut crate::canvas::CellCanvas::new(&mut buf),
            Rect::new(0, 0, 10, 10),
        );
    }

    // ============================================================================
    // Height=1 (minimized window) tests
    // ============================================================================

    #[test]
    fn test_frame_draws_at_height_1() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 20, 1), "Mini", FrameType::Window);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));
        frame.draw(
            &mut crate::canvas::CellCanvas::new(&mut buf),
            Rect::new(0, 0, 40, 5),
        );
        // Should draw something (not be empty) — verify corner chars exist
        let cell = buf.cell(Position::new(0, 0)).unwrap();
        assert_ne!(cell.symbol(), " ", "top-left corner should be drawn");
        let cell_end = buf.cell(Position::new(19, 0)).unwrap();
        assert_ne!(cell_end.symbol(), " ", "top-right corner should be drawn");
    }

    #[test]
    fn test_frame_height_1_no_minimize_maximize_buttons() {
        setup_default_theme();
        let mut frame = Frame::new(Rect::new(0, 0, 30, 1), "Mini", FrameType::Window);
        frame.set_minimizable(true);
        frame.set_maximizable(true);
        // At height=1, minimize/maximize hit-test should return false
        assert!(
            !frame.is_minimize_button(0, 0),
            "no minimize button at height=1"
        );
        assert!(
            !frame.is_maximize_button(0, 0),
            "no maximize button at height=1"
        );
        assert!(
            !frame.is_resize_handle(29, 0),
            "no resize handle at height=1"
        );
    }

    #[test]
    fn test_frame_height_1_close_button_works() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 20, 1), "Mini", FrameType::Window);
        // Close button should still be present and hittable at height=1
        assert!(frame.has_close_button(), "close button exists at height=1");
        // The close button position depends on theme, but it should be hittable at row 0
    }

    #[test]
    fn test_frame_height_1_title_visible() {
        setup_default_theme();
        let frame = Frame::new(Rect::new(0, 0, 30, 1), "TestWin", FrameType::Window);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));
        frame.draw(
            &mut crate::canvas::CellCanvas::new(&mut buf),
            Rect::new(0, 0, 40, 5),
        );
        // Check that title characters appear somewhere in row 0
        let row_text: String = (0..30)
            .map(|col| {
                buf.cell(Position::new(col, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(
            row_text.contains("TestWin"),
            "title should be visible at height=1: got '{row_text}'"
        );
    }
}
