//! View trait — foundation of the widget hierarchy.
//!
//! All UI components in turbo-tui implement the [`View`] trait, following
//! the Borland Turbo Vision pattern.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::command::CommandId;

// ============================================================================
// ViewId — Unique identifier per view instance
// ============================================================================

/// Unique identifier per view instance.
///
/// Generated from a thread-safe atomic counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId(usize);

impl ViewId {
    /// Generate a new unique `ViewId`.
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        ViewId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ViewId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// State Flags (bitfield)
// ============================================================================

/// View is visible.
pub const SF_VISIBLE: u16 = 0x0001;
/// View has input focus.
pub const SF_FOCUSED: u16 = 0x0002;
/// View is disabled (does not accept input).
pub const SF_DISABLED: u16 = 0x0004;
/// View is a modal dialog.
pub const SF_MODAL: u16 = 0x0008;
/// View is being dragged.
pub const SF_DRAGGING: u16 = 0x0010;
/// View is being resized.
pub const SF_RESIZING: u16 = 0x0020;
/// View has a shadow (for windows/dialogs).
pub const SF_SHADOW: u16 = 0x0040;
/// View is the active window.
pub const SF_ACTIVE: u16 = 0x0080;
/// View is minimized (collapsed to title bar).
pub const SF_MINIMIZED: u16 = 0x0100;

// ============================================================================
// Option Flags (bitfield)
// ============================================================================

/// View can receive focus.
pub const OF_SELECTABLE: u16 = 0x0001;
/// View receives events before children.
pub const OF_PRE_PROCESS: u16 = 0x0002;
/// View receives events after children.
pub const OF_POST_PROCESS: u16 = 0x0004;
/// View should appear on top when selected.
pub const OF_TOP_SELECT: u16 = 0x0008;
/// View can be tiled (for window managers).
pub const OF_TILEABLE: u16 = 0x0010;

// ============================================================================
// Event
// ============================================================================

/// Event kind — what type of event occurred.
#[derive(Debug, Clone)]
pub enum EventKind {
    /// Keyboard event from crossterm.
    Key(crossterm::event::KeyEvent),
    /// Mouse event from crossterm.
    Mouse(crossterm::event::MouseEvent),
    /// Command event (from menu, shortcut, etc.).
    Command(CommandId),
    /// Broadcast event (sent to all views).
    Broadcast(CommandId),
    /// Terminal resize event.
    Resize(u16, u16),
    /// No event (cleared).
    None,
}

/// Event wrapper passed through the view hierarchy.
#[derive(Debug, Clone)]
pub struct Event {
    /// The kind of event.
    pub kind: EventKind,
    /// Whether the event has been handled.
    pub handled: bool,
    /// Deferred events to be dispatched after the current dispatch cycle.
    /// Child views can push events here for parent/application-level processing.
    pub deferred: Vec<Event>,
}

impl Event {
    /// Create a new event with the given kind.
    #[must_use]
    pub fn new(kind: EventKind) -> Self {
        Self {
            kind,
            handled: false,
            deferred: Vec::new(),
        }
    }

    /// Create a command event.
    #[must_use]
    pub fn command(cmd: CommandId) -> Self {
        Self::new(EventKind::Command(cmd))
    }

    /// Create a broadcast event.
    #[must_use]
    pub fn broadcast(cmd: CommandId) -> Self {
        Self::new(EventKind::Broadcast(cmd))
    }

    /// Create a key event.
    #[must_use]
    pub fn key(key: crossterm::event::KeyEvent) -> Self {
        Self::new(EventKind::Key(key))
    }

    /// Create a mouse event.
    #[must_use]
    pub fn mouse(mouse: crossterm::event::MouseEvent) -> Self {
        Self::new(EventKind::Mouse(mouse))
    }

    /// Create a resize event.
    #[must_use]
    pub fn resize(width: u16, height: u16) -> Self {
        Self::new(EventKind::Resize(width, height))
    }

    /// Clear this event (mark as handled and set kind to None).
    pub fn clear(&mut self) {
        self.kind = EventKind::None;
        self.handled = true;
    }

    /// Check if the event has been cleared.
    #[must_use]
    pub fn is_cleared(&self) -> bool {
        matches!(self.kind, EventKind::None)
    }

    /// Check if this is a command event.
    #[must_use]
    pub fn is_command(&self) -> bool {
        matches!(self.kind, EventKind::Command(_))
    }

    /// Check if this is a broadcast event.
    #[must_use]
    pub fn is_broadcast(&self) -> bool {
        matches!(self.kind, EventKind::Broadcast(_))
    }

    /// Get the command ID if this is a command or broadcast event.
    #[must_use]
    pub fn command_id(&self) -> Option<CommandId> {
        match &self.kind {
            EventKind::Command(id) | EventKind::Broadcast(id) => Some(*id),
            _ => None,
        }
    }

    /// Post a deferred event to be dispatched after the current cycle.
    pub fn post(&mut self, event: Event) {
        self.deferred.push(event);
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new(EventKind::None)
    }
}

// ============================================================================
// OwnerType — For palette selection
// ============================================================================

/// Owner type determines the palette used for rendering.
///
/// Windows and dialogs may have different color schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OwnerType {
    /// No owner (background, top-level).
    #[default]
    None,
    /// Owned by a window.
    Window,
    /// Owned by a dialog.
    Dialog,
}

// ============================================================================
// View Trait
// ============================================================================

/// Base trait for all UI components.
///
/// All views in turbo-tui implement this trait. It provides:
///
/// - Identity via [`ViewId`]
/// - Geometry via `bounds()`/`set_bounds()`
/// - Drawing via `draw()`
/// - Event handling via `handle_event()`
/// - Focus management
/// - State and option flags
/// - Owner type for palette selection
///
/// # Example
///
/// ```ignore
/// struct Button {
///     base: ViewBase,
///     label: String,
/// }
///
/// impl View for Button {
///     fn id(&self) -> ViewId { self.base.id() }
///     fn bounds(&self) -> Rect { self.base.bounds() }
///     fn set_bounds(&mut self, bounds: Rect) { self.base.set_bounds(bounds); }
///     fn draw(&self, buf: &mut Buffer, area: Rect) { /* ... */ }
///     fn handle_event(&mut self, event: &mut Event) { /* ... */ }
///     fn state(&self) -> u16 { self.base.state() }
///     fn set_state(&mut self, state: u16) { self.base.set_state(state); }
///     fn as_any(&self) -> &dyn Any { self }
///     fn as_any_mut(&mut self) -> &mut dyn Any { self }
/// }
/// ```
pub trait View {
    // --- Identity ---

    /// Get the unique identifier for this view.
    fn id(&self) -> ViewId;

    // --- Geometry ---

    /// Get the bounding rectangle.
    fn bounds(&self) -> Rect;

    /// Set the bounding rectangle.
    fn set_bounds(&mut self, bounds: Rect);

    // --- Drawing ---

    /// Draw the view to the buffer.
    ///
    /// The `clip` parameter defines the clip region — only draw within this
    /// rectangle. Views should draw at their own `bounds()` coordinates,
    /// skipping any cells outside `clip`. Containers pass the intersection
    /// of the parent area and child bounds.
    fn draw(&self, buf: &mut Buffer, clip: Rect);

    // --- Events ---

    /// Handle an event.
    ///
    /// Implementations should set `event.handled = true` and optionally
    /// call `event.clear()` when the event is consumed.
    fn handle_event(&mut self, event: &mut Event);

    // --- Focus ---

    /// Whether this view can receive focus.
    fn can_focus(&self) -> bool {
        false
    }

    /// Return the desired terminal cursor position, if any.
    ///
    /// Views that need a visible terminal cursor (e.g., text editors) should
    /// return `Some(Position { x, y })` with absolute screen coordinates.
    /// The application will call `frame.set_cursor_position()` with this value.
    ///
    /// Default returns `None` (no cursor).
    fn cursor_position(&self) -> Option<Position> {
        None
    }

    /// Return the logical content size of this view, if known.
    ///
    /// Views that contain scrollable content (e.g. text editors, lists) should
    /// return `Some((width, height))` representing the total content dimensions
    /// in cells. The owning `Window` uses this to set scrollbar ranges.
    ///
    /// Default returns `None` (content size equals view bounds).
    fn content_size_hint(&self) -> Option<(u16, u16)> {
        None
    }

    /// Notify the view that the owning window's scroll offset has changed.
    ///
    /// Self-scrolling views (e.g. text editors, lists) that manage their own
    /// viewport should override this to update their internal scroll state
    /// and return `true`. The window will then skip its bitmap-shifting
    /// scroll approach for this view.
    ///
    /// Default returns `false` (view does not manage its own scrolling).
    fn scroll_to(&mut self, _x: i32, _y: i32) -> bool {
        false
    }

    /// Return the current scroll position of this view.
    ///
    /// Self-scrolling views should override this to return their internal
    /// scroll state as `(x, y)`. The owning Window uses this to sync
    /// scrollbar thumb position after keyboard scrolling.
    ///
    /// Default returns `(0, 0)` (no scroll).
    fn scroll_position(&self) -> (i32, i32) {
        (0, 0)
    }

    /// Whether this view currently has focus.
    fn is_focused(&self) -> bool {
        self.state() & SF_FOCUSED != 0
    }

    /// Set focus state.
    fn set_focused(&mut self, focused: bool) {
        let state = self.state();
        if focused {
            self.set_state(state | SF_FOCUSED);
        } else {
            self.set_state(state & !SF_FOCUSED);
        }
    }

    // --- State ---

    /// Get the state flags.
    fn state(&self) -> u16;

    /// Set the state flags.
    fn set_state(&mut self, state: u16);

    /// Get the option flags.
    fn options(&self) -> u16 {
        0
    }

    // --- Owner type (for palette) ---

    /// Get the owner type.
    fn owner_type(&self) -> OwnerType {
        OwnerType::None
    }

    /// Set the owner type.
    fn set_owner_type(&mut self, _owner_type: OwnerType) {}

    // --- Modal support ---

    /// Get the end state (command that closed the modal).
    fn end_state(&self) -> CommandId {
        0
    }

    /// Set the end state.
    fn set_end_state(&mut self, _cmd: CommandId) {}

    // --- Validation ---

    /// Validate a command.
    ///
    /// Called before executing a command. Return `true` to allow,
    /// `false` to reject.
    fn valid(&mut self, _command: CommandId) -> bool {
        true
    }

    // --- Downcasting ---

    /// Downcast to `Any` for concrete type access.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to `Any` for mutable concrete type access.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // --- Lifecycle hooks ---

    /// Called when this view is inserted into a container.
    ///
    /// `parent_bounds` is the container's bounds, useful for relative positioning.
    /// Default implementation does nothing.
    fn on_insert(&mut self, _parent_bounds: Rect) {}

    /// Called when this view is removed from a container.
    ///
    /// Default implementation does nothing.
    fn on_remove(&mut self) {}

    /// Called when the terminal is resized.
    ///
    /// `new_size` is the new terminal dimensions `(width, height)`.
    /// Default implementation does nothing.
    fn on_resize(&mut self, _new_size: (u16, u16)) {}

    /// Called when this view receives focus (`SF_FOCUSED` set).
    ///
    /// Default implementation does nothing. Override to react to focus
    /// changes, e.g., showing a cursor or updating visual state.
    fn on_focus(&mut self) {}

    /// Called when this view loses focus (`SF_FOCUSED` cleared).
    ///
    /// Default implementation does nothing. Override to react to blur,
    /// e.g., hiding a cursor or updating visual state.
    fn on_blur(&mut self) {}
}

// ============================================================================
// ViewBase — Common base struct
// ============================================================================

/// Common base struct that views can embed to avoid boilerplate.
///
/// Provides implementations for:
/// - `ViewId` storage and generation
/// - `Rect` bounds
/// - State and option flags
/// - `OwnerType` and `end_state`
///
/// # Example
///
/// ```ignore
/// struct Label {
///     base: ViewBase,
///     text: String,
/// }
///
/// impl View for Label {
///     fn id(&self) -> ViewId { self.base.id() }
///     fn bounds(&self) -> Rect { self.base.bounds() }
///     fn set_bounds(&mut self, bounds: Rect) { self.base.set_bounds(bounds); }
///     fn draw(&self, buf: &mut Buffer, area: Rect) { /* ... */ }
///     fn handle_event(&mut self, event: &mut Event) { event.handled = true; }
///     fn state(&self) -> u16 { self.base.state() }
///     fn set_state(&mut self, state: u16) { self.base.set_state(state); }
///     fn as_any(&self) -> &dyn Any { self }
///     fn as_any_mut(&mut self) -> &mut dyn Any { self }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ViewBase {
    /// Unique identifier for this view.
    id: ViewId,
    /// Bounding rectangle.
    bounds: Rect,
    /// State flags (SF_*).
    state: u16,
    /// Option flags (OF_*).
    options: u16,
    /// Owner type for palette selection.
    owner_type: OwnerType,
    /// End state for modal dialogs.
    end_state: CommandId,
    /// Whether this view needs redrawing.
    dirty: bool,
}

impl ViewBase {
    /// Create a new `ViewBase` with the given bounds.
    ///
    /// The view starts with `SF_VISIBLE` set.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            id: ViewId::new(),
            bounds,
            state: SF_VISIBLE,
            options: 0,
            owner_type: OwnerType::None,
            end_state: 0,
            dirty: true,
        }
    }

    /// Create a new `ViewBase` with custom options.
    #[must_use]
    pub fn with_options(bounds: Rect, options: u16) -> Self {
        Self {
            id: ViewId::new(),
            bounds,
            state: SF_VISIBLE,
            options,
            owner_type: OwnerType::None,
            end_state: 0,
            dirty: true,
        }
    }

    /// Get the view ID.
    #[must_use]
    pub fn id(&self) -> ViewId {
        self.id
    }

    /// Get the bounds.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Get the state flags.
    #[must_use]
    pub fn state(&self) -> u16 {
        self.state
    }

    /// Get the option flags.
    #[must_use]
    pub fn options(&self) -> u16 {
        self.options
    }

    /// Check if visible.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.state & SF_VISIBLE != 0
    }

    /// Check if focused.
    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.state & SF_FOCUSED != 0
    }

    /// Check if disabled.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.state & SF_DISABLED != 0
    }

    /// Check if this view needs redrawing.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark this view as needing redraw.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark this view as clean (drawn).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Set the option flags.
    pub fn set_options(&mut self, options: u16) {
        self.options = options;
    }
}
impl View for ViewBase {
    fn id(&self) -> ViewId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        if self.bounds != bounds {
            self.bounds = bounds;
            self.dirty = true;
        }
    }

    fn draw(&self, _buf: &mut Buffer, _clip: Rect) {
        // Base implementation draws nothing
    }

    fn handle_event(&mut self, event: &mut Event) {
        // Base implementation marks events as handled
        event.handled = true;
    }

    fn state(&self) -> u16 {
        self.state
    }

    fn set_state(&mut self, state: u16) {
        if self.state != state {
            self.state = state;
            self.dirty = true;
        }
    }

    fn options(&self) -> u16 {
        self.options
    }

    fn owner_type(&self) -> OwnerType {
        self.owner_type
    }

    fn set_owner_type(&mut self, owner_type: OwnerType) {
        self.owner_type = owner_type;
    }

    fn end_state(&self) -> CommandId {
        self.end_state
    }

    fn set_end_state(&mut self, cmd: CommandId) {
        self.end_state = cmd;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Hash implementation for trait objects
// ============================================================================

impl dyn View {
    /// Compute a hash of this view's ID.
    ///
    /// Useful for using views as hash keys.
    pub fn hash_id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.id().hash(&mut hasher);
        hasher.finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CM_CANCEL, CM_CLOSE, CM_OK};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[test]
    fn test_view_id_uniqueness() {
        let id1 = ViewId::new();
        let id2 = ViewId::new();
        let id3 = ViewId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_state_flags() {
        // Test SF_FOCUSED
        let state = SF_VISIBLE;
        assert_eq!(state & SF_FOCUSED, 0);

        let state = state | SF_FOCUSED;
        assert_ne!(state & SF_FOCUSED, 0);

        let state = state & !SF_FOCUSED;
        assert_eq!(state & SF_FOCUSED, 0);

        // Test SF_MODAL
        let state = SF_VISIBLE;
        assert_eq!(state & SF_MODAL, 0);

        let state = state | SF_MODAL;
        assert_ne!(state & SF_MODAL, 0);
    }

    #[test]
    fn test_event_command() {
        let event = Event::command(CM_OK);
        assert!(event.is_command());
        assert!(!event.is_broadcast());
        assert_eq!(event.command_id(), Some(CM_OK));
        assert!(!event.handled);
    }

    #[test]
    fn test_event_broadcast() {
        let event = Event::broadcast(CM_CLOSE);
        assert!(event.is_broadcast());
        assert!(!event.is_command());
        assert_eq!(event.command_id(), Some(CM_CLOSE));
    }

    #[test]
    fn test_event_clear() {
        let mut event = Event::command(CM_OK);
        assert!(!event.is_cleared());

        event.clear();

        assert!(event.is_cleared());
        assert!(event.handled);
    }

    #[test]
    fn test_event_default() {
        let event = Event::default();
        assert!(event.is_cleared());
        assert!(!event.handled);
    }

    #[test]
    fn test_view_base_defaults() {
        let base = ViewBase::new(Rect::new(10, 20, 30, 40));

        assert_eq!(base.bounds(), Rect::new(10, 20, 30, 40));
        assert_ne!(base.state() & SF_VISIBLE, 0); // Visible by default
        assert_eq!(base.state() & SF_FOCUSED, 0); // Not focused
        assert_eq!(base.state() & SF_DISABLED, 0); // Not disabled
        assert_eq!(base.options(), 0);
        assert_eq!(base.owner_type(), OwnerType::None);
        assert_eq!(base.end_state(), 0);
    }

    #[test]
    fn test_view_base_with_options() {
        let base = ViewBase::with_options(Rect::new(0, 0, 10, 10), OF_SELECTABLE | OF_PRE_PROCESS);

        assert_eq!(base.options(), OF_SELECTABLE | OF_PRE_PROCESS);
        assert_ne!(base.options() & OF_SELECTABLE, 0);
        assert_ne!(base.options() & OF_PRE_PROCESS, 0);
        assert_eq!(base.options() & OF_POST_PROCESS, 0);
    }

    #[test]
    fn test_view_base_setters() {
        let mut base = ViewBase::new(Rect::new(0, 0, 10, 10));

        base.set_bounds(Rect::new(5, 5, 20, 15));
        assert_eq!(base.bounds(), Rect::new(5, 5, 20, 15));

        base.set_state(SF_VISIBLE | SF_FOCUSED | SF_MODAL);
        assert!(base.is_focused());
        assert_ne!(base.state() & SF_MODAL, 0);
        assert!(base.is_visible());

        base.set_owner_type(OwnerType::Dialog);
        assert_eq!(base.owner_type(), OwnerType::Dialog);

        base.set_end_state(CM_OK);
        assert_eq!(base.end_state(), CM_OK);
    }

    #[test]
    fn test_owner_type_default() {
        let owner_type = OwnerType::default();
        assert_eq!(owner_type, OwnerType::None);
    }

    #[test]
    fn test_view_trait_focus() {
        let mut base = ViewBase::new(Rect::new(0, 0, 10, 10));

        // Initially not focused
        assert!(!base.is_focused());

        // Set focused
        base.set_focused(true);
        assert!(base.is_focused());
        assert_ne!(base.state() & SF_FOCUSED, 0);

        // Clear focused
        base.set_focused(false);
        assert!(!base.is_focused());
        assert_eq!(base.state() & SF_FOCUSED, 0);
    }

    #[test]
    fn test_view_base_visibility() {
        let mut base = ViewBase::new(Rect::new(0, 0, 10, 10));
        assert!(base.is_visible());

        // Hide
        base.set_state(base.state() & !SF_VISIBLE);
        assert!(!base.is_visible());

        // Show again
        base.set_state(base.state() | SF_VISIBLE);
        assert!(base.is_visible());
    }

    #[test]
    fn assert_send_sync() {
        // ViewId must be Send + Sync for cross-thread use
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ViewId>();
    }

    #[test]
    fn test_event_deferred_empty_by_default() {
        let event = Event::key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(event.deferred.is_empty());
    }

    #[test]
    fn test_event_post_deferred() {
        let mut event = Event::key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        event.post(Event::command(CM_OK));
        assert_eq!(event.deferred.len(), 1);
        match &event.deferred[0].kind {
            EventKind::Command(id) => assert_eq!(*id, CM_OK),
            _ => panic!("expected Command event"),
        }
    }

    #[test]
    fn test_event_post_multiple_deferred() {
        let mut event = Event::command(CM_CLOSE);
        event.post(Event::command(CM_OK));
        event.post(Event::command(CM_CANCEL));
        assert_eq!(event.deferred.len(), 2);
    }

    #[test]
    fn test_event_deferred_independent_of_handled() {
        let mut event = Event::command(CM_OK);
        event.post(Event::command(CM_CLOSE));
        event.clear();
        // Deferred events survive even when the event is cleared
        assert!(event.is_cleared());
        assert_eq!(event.deferred.len(), 1);
    }

    #[test]
    fn test_on_focus_default_is_noop() {
        let mut base = ViewBase::new(Rect::new(0, 0, 10, 1));
        // Should compile and not panic
        base.on_focus();
    }

    #[test]
    fn test_on_blur_default_is_noop() {
        let mut base = ViewBase::new(Rect::new(0, 0, 10, 1));
        // Should compile and not panic
        base.on_blur();
    }

    #[test]
    fn test_view_base_cursor_position_default_is_none() {
        // ViewBase uses the default View::cursor_position() which returns None.
        let base = ViewBase::new(Rect::new(0, 0, 10, 5));
        assert_eq!(base.cursor_position(), None);
    }
}
