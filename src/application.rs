//! Application — central orchestrator for a turbo-tui program.
//!
//! Manages the top-level view hierarchy, event dispatch, and screen layout.
//! Application does **not** own a terminal — it receives `&mut ratatui::Frame`
//! for drawing and raw crossterm events for processing.
//!
//! # Layout
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ MenuBar (row 0)                          │
//! ├─────────────────────────────────────────┤
//! │                                          │
//! │ Desktop (rows 1..n-1)                    │
//! │   ├── Window 1                           │
//! │   ├── Window 2                           │
//! │   └── ...                                │
//! │                                          │
//! ├─────────────────────────────────────────┤
//! │ StatusBar (last row)                    │
//! └─────────────────────────────────────────┘
//! │ OverlayManager (above everything)        │
//! ```
//!
//! # Event Dispatch Order
//!
//! 1. Overlay (topmost overlay first)
//! 2. `MenuBar` (F10, Alt+letter)
//! 3. `StatusBar` (`PreProcess` — F-keys)
//! 4. Desktop → focused Window → three-phase dispatch
//! 5. Application handles unhandled commands (`CM_QUIT`, `CM_CLOSE`)
//! 6. Process deferred event queue

use crate::command::{
    CommandId, CM_CASCADE, CM_CLOSE, CM_CLOSE_ALL, CM_CONTEXT_MENU, CM_DROPDOWN_CLOSED,
    CM_DROPDOWN_NAVIGATE, CM_DRAG_END, CM_DRAG_MOVE, CM_DRAG_START, CM_OPEN_DROPDOWN, CM_QUIT,
    CM_TILE,
};
use crate::desktop::Desktop;
use crate::menu_bar::MenuBar;
use crate::menu_bar::MenuItem;
use crate::menu_box::MenuBox;
use crate::overlay::{calculate_overlay_bounds, DropDirection, Overlay, OverlayManager};
use crate::status_bar::StatusBar;
use crate::view::{Event, EventKind, View, ViewId};
use crate::window::Window;
use ratatui::layout::{Position, Rect};
use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Application — central orchestrator for a turbo-tui program.
///
/// Manages the top-level view hierarchy and event dispatch:
///
/// ```text
/// ┌─────────────────────────────────────────┐
/// │ MenuBar (row 0)                          │
/// ├─────────────────────────────────────────┤
/// │                                          │
/// │ Desktop (rows 1..n-1)                    │
/// │   ├── Window 1                           │
/// │   ├── Window 2                           │
/// │   └── ...                                │
/// │                                          │
/// ├─────────────────────────────────────────┤
/// │ StatusBar (last row)                    │
/// └─────────────────────────────────────────┘
/// │ OverlayManager (above everything)        │
/// ```
///
/// # Event Dispatch Order
///
/// 1. Overlay (topmost overlay first)
/// 2. `MenuBar` (F10, Alt+letter)
/// 3. `StatusBar` (`PreProcess` — F-keys)
/// 4. Desktop → focused Window → three-phase dispatch
/// 5. Application handles unhandled commands (`CM_QUIT`, `CM_CLOSE`)
/// 6. Process deferred event queue
///
/// # Usage
///
/// ```ignore
/// use turbo_tui::application::Application;
/// use ratatui::layout::Rect;
///
/// let mut app = Application::new(Rect::new(0, 0, 80, 24));
/// // ... add windows, configure menus ...
///
/// // In your event loop:
/// terminal.draw(|f| app.draw(f))?;
/// app.handle_crossterm_event(crossterm::event::read()?);
/// if !app.is_running() { break; }
/// ```
pub struct Application {
    /// Current screen size.
    screen_size: Rect,
    /// Desktop window manager.
    desktop: Desktop,
    /// Optional menu bar (top row).
    menu_bar: Option<MenuBar>,
    /// Optional status bar (bottom row).
    status_bar: Option<StatusBar>,
    /// Overlay manager (menus, tooltips above everything).
    overlay_manager: OverlayManager,
    /// Whether the application is still running.
    running: bool,
    /// Last unhandled command (for the consumer to read).
    last_unhandled_command: Option<CommandId>,
    /// Thread-safe queue for externally posted events (from background tasks).
    deferred_events: Arc<Mutex<VecDeque<Event>>>,
    /// Items to display in a context menu.
    context_menu_items: Vec<MenuItem>,
    /// Last known mouse position (for context menu placement).
    last_mouse_pos: Position,
    /// Origin position where the current drag operation started, if any.
    drag_origin: Option<Position>,
    /// Arbitrary payload data associated with the current drag operation.
    drag_payload: Option<Box<dyn Any>>,
    /// Accumulated dirty rectangle for partial invalidation.
    /// Set to `None` to redraw the full screen.
    dirty_rect: Option<Rect>,
}

impl Application {
    /// Create a new application with the given screen size.
    ///
    /// The desktop occupies the full screen initially. Call [`set_menu_bar`] and
    /// [`set_status_bar`] to install those components; the desktop area will be
    /// recalculated automatically.
    ///
    /// [`set_menu_bar`]: Application::set_menu_bar
    /// [`set_status_bar`]: Application::set_status_bar
    #[must_use]
    pub fn new(screen_size: Rect) -> Self {
        Self {
            screen_size,
            desktop: Desktop::new(screen_size),
            menu_bar: None,
            status_bar: None,
            overlay_manager: OverlayManager::new(screen_size.width, screen_size.height),
            running: true,
            last_unhandled_command: None,
            deferred_events: Arc::new(Mutex::new(VecDeque::new())),
            context_menu_items: Vec::new(),
            last_mouse_pos: Position::ORIGIN,
            drag_origin: None,
            drag_payload: None,
            dirty_rect: None,
        }
    }

    // -------------------------------------------------------------------------
    // Running state
    // -------------------------------------------------------------------------

    /// Check if the application is still running.
    ///
    /// Returns `false` after [`quit`] is called or a `CM_QUIT` command is
    /// dispatched.
    ///
    /// [`quit`]: Application::quit
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Stop the application — the next [`is_running`] check will return `false`.
    ///
    /// [`is_running`]: Application::is_running
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Invalidate a rectangle for redrawing.
    ///
    /// Unions the given `rect` with any existing dirty rectangle so that
    /// the combined area will be redrawn on the next call to [`draw`].
    /// Call this when a region of the screen has changed and needs repainting.
    ///
    /// [`draw`]: Application::draw
    pub fn invalidate_rect(&mut self, rect: Rect) {
        self.dirty_rect = Some(match self.dirty_rect {
            Some(existing) => {
                let x = existing.x.min(rect.x);
                let y = existing.y.min(rect.y);
                let right = (existing.x + existing.width).max(rect.x + rect.width);
                let bottom = (existing.y + existing.height).max(rect.y + rect.height);
                Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
            }
            None => rect,
        });
    }

    /// Take the last unhandled command, if any.
    ///
    /// Returns the command ID and clears it. This allows the consumer
    /// to handle custom commands that the library doesn't know about.
    ///
    /// # Example
    ///
    /// ```ignore
    /// app.handle_crossterm_event(&event);
    /// if let Some(cmd) = app.take_unhandled_command() {
    ///     match cmd {
    ///         MY_CUSTOM_COMMAND => { /* handle it */ }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn take_unhandled_command(&mut self) -> Option<CommandId> {
        self.last_unhandled_command.take()
    }

    // -------------------------------------------------------------------------
    // Component access
    // -------------------------------------------------------------------------

    /// Get an immutable reference to the desktop.
    #[must_use]
    pub fn desktop(&self) -> &Desktop {
        &self.desktop
    }

    /// Get a mutable reference to the desktop.
    pub fn desktop_mut(&mut self) -> &mut Desktop {
        &mut self.desktop
    }

    /// Get an immutable reference to the menu bar, if installed.
    #[must_use]
    pub fn menu_bar(&self) -> Option<&MenuBar> {
        self.menu_bar.as_ref()
    }

    /// Get a mutable reference to the menu bar, if installed.
    pub fn menu_bar_mut(&mut self) -> Option<&mut MenuBar> {
        self.menu_bar.as_mut()
    }

    /// Get an immutable reference to the status bar, if installed.
    #[must_use]
    pub fn status_bar(&self) -> Option<&StatusBar> {
        self.status_bar.as_ref()
    }

    /// Get a mutable reference to the status bar, if installed.
    pub fn status_bar_mut(&mut self) -> Option<&mut StatusBar> {
        self.status_bar.as_mut()
    }

    /// Get an immutable reference to the overlay manager.
    #[must_use]
    pub fn overlay_manager(&self) -> &OverlayManager {
        &self.overlay_manager
    }

    /// Get a mutable reference to the overlay manager.
    pub fn overlay_manager_mut(&mut self) -> &mut OverlayManager {
        &mut self.overlay_manager
    }

    // -------------------------------------------------------------------------
    // Setup
    // -------------------------------------------------------------------------

    /// Install a menu bar.
    ///
    /// Recalculates the desktop area so the desktop starts below row 0.
    pub fn set_menu_bar(&mut self, menu_bar: MenuBar) {
        self.menu_bar = Some(menu_bar);
        self.recalculate_layout();
        self.invalidate_rect(self.screen_size);
    }

    /// Install a status bar.
    ///
    /// Recalculates the desktop area so the desktop ends above the last row.
    pub fn set_status_bar(&mut self, status_bar: StatusBar) {
        self.status_bar = Some(status_bar);
        self.recalculate_layout();
        self.invalidate_rect(self.screen_size);
    }

    /// Add a window to the desktop and return its [`ViewId`].
    ///
    /// Convenience wrapper around [`Desktop::add_window`].
    pub fn add_window(&mut self, window: Window) -> ViewId {
        let bounds = window.bounds();
        let id = self.desktop.add_window(window);
        self.invalidate_rect(bounds);
        id
    }

    /// Close a window by its [`ViewId`].
    ///
    /// Convenience wrapper around [`Desktop::close_window`].
    pub fn close_window(&mut self, id: ViewId) {
        // We invalidate the full screen since we don't know the exact bounds
        // of the window being closed without iterating children
        self.desktop.close_window(id);
        self.invalidate_rect(self.screen_size);
    }

    // -------------------------------------------------------------------------
    // Screen resize
    // -------------------------------------------------------------------------

    /// Update the screen size and recalculate all component layouts.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.screen_size = Rect::new(0, 0, width, height);
        self.overlay_manager.set_screen_size(width, height);
        self.recalculate_layout();
        self.invalidate_rect(self.screen_size);
    }

    // -------------------------------------------------------------------------
    // Drawing
    // -------------------------------------------------------------------------

    /// Draw the application to a ratatui frame, clipping to dirty regions.
    ///
    /// Rendering order (back to front):
    /// 1. Desktop (background + windows)
    /// 2. `MenuBar`
    /// 3. `StatusBar`
    /// 4. Overlays
    ///
    /// Only the currently dirty rectangle (tracked via [`invalidate_rect`]) is
    /// redrawn. If no dirty rectangle has been explicitly set, the entire screen
    /// area is drawn.
    ///
    /// After drawing, the dirty rectangle is cleared so the next draw call will
    /// be a full redraw unless a new region is invalidated.
    ///
    /// [`invalidate_rect`]: Application::invalidate_rect
    pub fn draw(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        // Use the accumulated dirty rect, or fall back to full area
        let clip = self.dirty_rect.unwrap_or(area);

        // Collect cursor position before borrowing the buffer
        let cursor_pos: Option<Position> = if self.overlay_manager.is_empty() {
            self.desktop.cursor_position()
        } else {
            None
        };

        {
            let buf = frame.buffer_mut();

            // 1. Desktop (background + windows) — clipped to dirty region
            self.desktop.draw(buf, clip);

            // 2. Menu bar (top row) — clipped to dirty region
            if let Some(ref mb) = self.menu_bar {
                mb.draw(buf, clip);
            }

            // 3. Status bar (bottom row) — clipped to dirty region
            if let Some(ref sl) = self.status_bar {
                sl.draw(buf, clip);
            }

            // 4. Overlays (above everything) — clipped to dirty region
            self.overlay_manager.draw(buf, clip);
        }

        // 5. Terminal cursor — from focused window's child view
        if let Some(pos) = cursor_pos {
            frame.set_cursor_position(pos);
        }

        // 6. Clear dirty rect — everything has been redrawn
        self.dirty_rect = None;
    }

    // -------------------------------------------------------------------------
    // Event handling
    // -------------------------------------------------------------------------

    /// Handle a raw crossterm event.
    ///
    /// Converts the crossterm event to a turbo-tui [`Event`] and dispatches it
    /// through the view hierarchy.
    ///
    /// - Key events: only `KeyEventKind::Press` is processed.
    /// - Mouse events: forwarded as-is.
    /// - Resize events: calls [`resize`] then broadcasts a resize event.
    /// - All other events (`FocusGained`, `FocusLost`, Paste): ignored.
    ///
    /// [`resize`]: Application::resize
    pub fn handle_crossterm_event(&mut self, ct_event: &crossterm::event::Event) {
        // Clear any previous unhandled command
        self.last_unhandled_command = None;

        match ct_event {
            crossterm::event::Event::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    let mut event = Event::key(*key);
                    self.dispatch(&mut event);
                }
                // Always invalidate on key press (changes may occur)
                self.invalidate_rect(self.screen_size);
            }
            crossterm::event::Event::Mouse(mouse) => {
                let mut event = Event::mouse(*mouse);
                self.dispatch(&mut event);
                self.invalidate_rect(self.screen_size);
            }
            crossterm::event::Event::Resize(w, h) => {
                self.resize(*w, *h);
                let mut event = Event::resize(*w, *h);
                self.dispatch(&mut event);
                // resize() already invalidated, but also add resize event
                self.invalidate_rect(self.screen_size);
            }
            // Ignore FocusGained, FocusLost, Paste
            _ => {}
        }
    }

    /// Dispatch a turbo-tui [`Event`] through the full dispatch chain.
    ///
    /// Dispatch order:
    /// 1. Overlay layer (topmost first)
    /// 2. `MenuBar`
    /// 3. `StatusBar` (`OF_PRE_PROCESS` — intercepts F-keys)
    /// 4. Desktop → focused Window → three-phase dispatch
    /// 5. Application-level command handling (`CM_QUIT`, `CM_CLOSE`)
    /// 6. Deferred event queue processing
    pub fn dispatch(&mut self, event: &mut Event) {
        // 1. Overlay layer — if it consumed the event, stop here
        if self.overlay_manager.handle_event(event) && event.is_cleared() {
            self.process_deferred(event);
            return;
        }

        // 2. Menu bar
        if !event.is_cleared() {
            if let Some(ref mut mb) = self.menu_bar {
                mb.handle_event(event);
            }
        }

        // 3. Status line (OF_PRE_PROCESS — intercepts F-keys before desktop)
        if !event.is_cleared() {
            if let Some(ref mut sl) = self.status_bar {
                sl.handle_event(event);
            }
        }

        // 4. Desktop (three-phase dispatch through focused window)
        if !event.is_cleared() {
            self.desktop.handle_event(event);
        }

        // 5. Application-level command handling
        if !event.is_cleared() {
            self.handle_application_commands(event);
        }

        // 6. Deferred event queue
        self.process_deferred(event);
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Handle dropdown orchestration commands.
    ///
    /// These commands are posted as deferred events by `HorizontalBar` and
    /// `MenuBox` to coordinate overlay lifecycle:
    /// - `CM_OPEN_DROPDOWN` → create a `MenuBox` overlay
    /// - `CM_DROPDOWN_CLOSED` → dismiss the overlay and reset bar state
    /// - `CM_DROPDOWN_NAVIGATE` → close current, open adjacent dropdown
    fn handle_dropdown_commands(&mut self, event: &mut Event) {
        let EventKind::Command(cmd) = event.kind else {
            return;
        };

        match cmd {
            CM_OPEN_DROPDOWN => {
                self.handle_open_dropdown(event);
            }
            CM_DROPDOWN_CLOSED => {
                self.handle_close_dropdown(event);
            }
            CM_DROPDOWN_NAVIGATE => {
                self.handle_navigate_dropdown(event);
            }
            _ => {}
        }
    }

    /// Create a `MenuBox` overlay for the pending dropdown.
    fn handle_open_dropdown(&mut self, event: &mut Event) {
        // Invalidate the area where the dropdown will appear
        self.invalidate_rect(self.screen_size);
        // Try menu bar first, then status line
        let bar_data = if let Some(ref mut mb) = self.menu_bar {
            if let Some(idx) = mb.take_pending_dropdown() {
                if let (Some(items), Some(anchor)) =
                    (mb.dropdown_items_for(idx), mb.dropdown_anchor(idx))
                {
                    Some((mb.id(), items.to_vec(), anchor, mb.drop_direction()))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let bar_data = if bar_data.is_none() {
            if let Some(ref mut sl) = self.status_bar {
                if let Some(idx) = sl.take_pending_dropdown() {
                    if let (Some(items), Some(anchor)) =
                        (sl.dropdown_items_for(idx), sl.dropdown_anchor(idx))
                    {
                        Some((sl.id(), items.to_vec(), anchor, sl.drop_direction()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            bar_data
        };

        let Some((bar_id, items, anchor, direction)) = bar_data else {
            return;
        };

        // Remove any existing overlay from this bar
        self.overlay_manager.pop_by_owner(bar_id);

        // Calculate MenuBox bounds and adjust for screen overflow
        let menu_bounds = MenuBox::calculate_bounds(anchor.0, anchor.1, &items);
        let screen = Rect::new(0, 0, self.screen_size.width, self.screen_size.height);

        let (overlay_rect, _actual_dir) = calculate_overlay_bounds(
            anchor,
            (menu_bounds.width, menu_bounds.height),
            screen,
            direction,
        );

        // Create MenuBox with owner so it emits commands through the event system
        let menu_box = MenuBox::new(overlay_rect, items).with_owner(bar_id);

        self.overlay_manager.push(Overlay {
            view: Box::new(menu_box),
            owner_id: bar_id,
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        });

        event.clear();
    }

    /// Close the dropdown overlay and reset the owning bar state.
    fn handle_close_dropdown(&mut self, event: &mut Event) {
        // Close overlays owned by menu bar
        if let Some(ref mb) = self.menu_bar {
            let id = mb.id();
            self.overlay_manager.pop_by_owner(id);
        }
        // Close overlays owned by status bar
        if let Some(ref sl) = self.status_bar {
            let id = sl.id();
            self.overlay_manager.pop_by_owner(id);
        }
        // Reset bar states
        if let Some(ref mut mb) = self.menu_bar {
            mb.close();
        }
        if let Some(ref mut sl) = self.status_bar {
            sl.close();
        }
        self.invalidate_rect(self.screen_size);
        event.clear();
    }

    /// Navigate to the adjacent dropdown (Left/Right arrow in open menu).
    fn handle_navigate_dropdown(&mut self, event: &mut Event) {
        self.invalidate_rect(self.screen_size);
        // Determine which bar owns the current overlay and read navigate direction
        // before popping (direction is stored in the MenuBox overlay).
        let menu_bar_owner = self.menu_bar.as_ref().and_then(|mb| {
            if self.overlay_manager.has_overlay_for(mb.id()) {
                Some(mb.id())
            } else {
                None
            }
        });

        if let Some(owner_id) = menu_bar_owner {
            let delta = self.read_navigate_direction_from_overlay(owner_id);
            self.overlay_manager.pop_by_owner(owner_id);
            if let Some(ref mut mb) = self.menu_bar {
                mb.navigate_dropdown(delta, event);
            }
        } else {
            let status_bar_owner = self.status_bar.as_ref().and_then(|sl| {
                if self.overlay_manager.has_overlay_for(sl.id()) {
                    Some(sl.id())
                } else {
                    None
                }
            });
            if let Some(owner_id) = status_bar_owner {
                let delta = self.read_navigate_direction_from_overlay(owner_id);
                self.overlay_manager.pop_by_owner(owner_id);
                if let Some(ref mut sl) = self.status_bar {
                    sl.navigate_dropdown(delta, event);
                }
            }
        }
        event.clear();
    }

    /// Read the navigate direction from the `MenuBox` overlay owned by `owner_id`.
    ///
    /// Downcasts the overlay view to `MenuBox` and returns its stored direction,
    /// defaulting to `1` (right) if not found.
    fn read_navigate_direction_from_overlay(&self, owner_id: ViewId) -> isize {
        for overlay in self.overlay_manager.overlays_iter() {
            if overlay.owner_id == owner_id {
                if let Some(menu_box) = overlay.view.as_any().downcast_ref::<MenuBox>() {
                    return menu_box.navigate_direction().unwrap_or(1);
                }
            }
        }
        1
    }

    /// Open a context menu overlay at the last known mouse position.
    ///
    /// Creates a `MenuBox` positioned at `self.last_mouse_pos` with the items
    /// previously set via [`set_context_menu_items`]. If no items have been set,
    /// this is a no-op.
    ///
    /// [`set_context_menu_items`]: Application::set_context_menu_items
    fn handle_context_menu(&mut self) {
        if self.context_menu_items.is_empty() {
            return;
        }

        self.invalidate_rect(self.screen_size);
        let items = self.context_menu_items.clone();
        let menu_bounds = MenuBox::calculate_bounds(
            self.last_mouse_pos.x,
            self.last_mouse_pos.y,
            &items,
        );
        let screen = Rect::new(0, 0, self.screen_size.width, self.screen_size.height);

        let (overlay_rect, _actual_dir) = calculate_overlay_bounds(
            (self.last_mouse_pos.x, self.last_mouse_pos.y),
            (menu_bounds.width, menu_bounds.height),
            screen,
            DropDirection::Down,
        );

        let menu_box = MenuBox::new(overlay_rect, items);

        self.overlay_manager.push(Overlay {
            view: Box::new(menu_box),
            owner_id: ViewId::new(),
            dismiss_on_outside_click: true,
            dismiss_on_escape: true,
        });
    }

    /// Handle application-level commands.
    ///
    /// Currently handles:
    /// - `CM_QUIT` → sets `running = false`
    /// - `CM_CLOSE` → closes the currently focused window on the desktop
    fn handle_application_commands(&mut self, event: &mut Event) {
        if let EventKind::Command(cmd) = event.kind {
            match cmd {
                CM_QUIT => {
                    self.running = false;
                    event.clear();
                }
                CM_CLOSE => {
                    // Close the focused window on the desktop
                    if let Some(focused_idx) = self.desktop.windows().focused_index() {
                        if let Some(child) = self.desktop.windows().child_at(focused_idx) {
                            let id = child.id();
                            let old_bounds = child.bounds();
                            self.desktop.close_window(id);
                            // Invalidate the area the window occupied
                            self.invalidate_rect(old_bounds);
                            event.clear();
                        }
                    }
                }
                CM_OPEN_DROPDOWN | CM_DROPDOWN_CLOSED | CM_DROPDOWN_NAVIGATE => {
                    self.handle_dropdown_commands(event);
                }
                CM_CLOSE_ALL => {
                    self.desktop.close_all_windows();
                    self.invalidate_rect(self.screen_size);
                    event.clear();
                }
                CM_TILE => {
                    self.desktop.tile();
                    self.invalidate_rect(self.screen_size);
                    event.clear();
                }
                CM_CASCADE => {
                    self.desktop.cascade();
                    self.invalidate_rect(self.screen_size);
                    event.clear();
                }
                CM_CONTEXT_MENU => {
                    self.handle_context_menu();
                    event.clear();
                }
                CM_DRAG_START => {
                    // Store the origin at the current mouse position.
                    // The view that initiated the drag may also call
                    // start_drag() to attach payload data.
                    self.drag_origin = Some(self.last_mouse_pos);
                    event.clear();
                }
                CM_DRAG_MOVE => {
                    // Drag in progress — the origin is already set.
                    // Drop targets can check `is_dragging()` / `drag_payload()`
                    // to react to the drag entering their area.
                    self.invalidate_rect(self.screen_size);
                    event.clear();
                }
                CM_DRAG_END => {
                    // Clear all drag state.
                    self.drag_origin = None;
                    self.drag_payload = None;
                    self.invalidate_rect(self.screen_size);
                    event.clear();
                }
                other => {
                    // Unknown command — store for consumer to handle
                    self.last_unhandled_command = Some(other);
                }
            }
        }
    }

    /// Process the deferred event queue.
    ///
    /// After the main dispatch cycle, any [`Event::post`]ed deferred events are
    /// dispatched in order. The loop repeats until the queue is empty or the
    /// safety limit of 100 iterations is reached (prevents infinite loops from
    /// views that keep posting new events).
    fn process_deferred(&mut self, event: &mut Event) {
        let mut iterations: u32 = 100;
        while !event.deferred.is_empty() && iterations > 0 {
            let deferred: Vec<Event> = event.deferred.drain(..).collect();
            for mut def in deferred {
                self.dispatch_single(&mut def);
                // Carry any further deferred events back onto the queue
                event.deferred.append(&mut def.deferred);
            }
            iterations -= 1;
        }
    }

    /// Dispatch a single event through the chain **without** deferred processing.
    ///
    /// Used internally by [`process_deferred`] to avoid recursive deferred
    /// dispatch.
    ///
    /// [`process_deferred`]: Application::process_deferred
    fn dispatch_single(&mut self, event: &mut Event) {
        // 1. Overlay
        if self.overlay_manager.handle_event(event) && event.is_cleared() {
            return;
        }

        // 2. Menu bar
        if !event.is_cleared() {
            if let Some(ref mut mb) = self.menu_bar {
                mb.handle_event(event);
            }
        }

        // 2. Status line
        if !event.is_cleared() {
            if let Some(ref mut sl) = self.status_bar {
                sl.handle_event(event);
            }
        }

        // 4. Desktop
        if !event.is_cleared() {
            self.desktop.handle_event(event);
        }

        // 5. Application-level commands
        if !event.is_cleared() {
            self.handle_application_commands(event);
        }
    }

    /// Recalculate the bounds of all components based on `screen_size` and the
    /// presence of a menu bar / status line.
    ///
    /// - `MenuBar`  → row 0, full width
    /// - `StatusBar` → last row, full width
    /// - Desktop  → everything in between
    fn recalculate_layout(&mut self) {
        let s = self.screen_size;
        let mut desktop_y = s.y;
        let mut desktop_h = s.height;

        // Menu bar takes the top row
        if let Some(ref mut mb) = self.menu_bar {
            mb.set_bounds(Rect::new(s.x, s.y, s.width, 1));
            desktop_y = desktop_y.saturating_add(1);
            desktop_h = desktop_h.saturating_sub(1);
        }

        // StatusBar takes the bottom row
        if let Some(ref mut sl) = self.status_bar {
            desktop_h = desktop_h.saturating_sub(1);
            let status_y = s.y.saturating_add(s.height).saturating_sub(1);
            sl.set_bounds(Rect::new(s.x, status_y, s.width, 1));
        }

        // Desktop occupies the remaining area
        self.desktop
            .set_bounds(Rect::new(s.x, desktop_y, s.width, desktop_h));
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{
        CM_CASCADE, CM_CLOSE, CM_CLOSE_ALL, CM_DROPDOWN_CLOSED, CM_OPEN_DROPDOWN, CM_QUIT, CM_TILE,
    };
    use crate::view::{Event, EventKind};
    use ratatui::layout::{Position, Rect};

    fn screen() -> Rect {
        Rect::new(0, 0, 80, 24)
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_new() {
        let app = Application::new(screen());
        assert!(app.is_running(), "new application must be running");
        assert_eq!(app.desktop().window_count(), 0, "desktop starts empty");
        assert!(app.menu_bar().is_none(), "no menu bar by default");
        assert!(app.status_bar().is_none(), "no status bar by default");
        assert!(app.overlay_manager().is_empty(), "no overlays by default");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_quit() {
        let mut app = Application::new(screen());
        assert!(app.is_running());
        app.quit();
        assert!(!app.is_running(), "quit() must stop the application");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_cm_quit() {
        let mut app = Application::new(screen());
        assert!(app.is_running());

        let mut event = Event::command(CM_QUIT);
        app.dispatch(&mut event);

        assert!(!app.is_running(), "CM_QUIT must stop the application");
        assert!(event.is_cleared(), "CM_QUIT event must be consumed");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_add_window() {
        let mut app = Application::new(screen());
        assert_eq!(app.desktop().window_count(), 0);

        let window = Window::new(Rect::new(5, 5, 30, 10), "Test");
        let _id = app.add_window(window);

        assert_eq!(
            app.desktop().window_count(),
            1,
            "add_window delegates to desktop"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_close_window() {
        let mut app = Application::new(screen());
        let window = Window::new(Rect::new(5, 5, 30, 10), "Test");
        let id = app.add_window(window);
        assert_eq!(app.desktop().window_count(), 1);

        app.close_window(id);
        assert_eq!(
            app.desktop().window_count(),
            0,
            "close_window delegates to desktop"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_resize() {
        let mut app = Application::new(screen());
        app.resize(120, 40);

        // overlay_manager must know the new size
        assert_eq!(
            app.overlay_manager().screen_size(),
            (120, 40),
            "overlay manager must reflect new screen size"
        );

        // desktop bounds must fit within new size
        let db = app.desktop().bounds();
        assert!(db.width <= 120, "desktop width must be <= screen width");
        assert!(db.height <= 40, "desktop height must be <= screen height");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_layout_with_menu_and_status() {
        use crate::menu_bar::{menu_bar_from_menus, Menu};
        use crate::status_bar::status_bar_from_items;

        let mut app = Application::new(screen());

        let mb = menu_bar_from_menus(screen(), vec![Menu::new("~F~ile", vec![])]);
        app.set_menu_bar(mb);

        let sl = status_bar_from_items(screen(), vec![]);
        app.set_status_bar(sl);

        // Menu bar must be at row 0
        let mb_bounds = app.menu_bar().unwrap().bounds();
        assert_eq!(mb_bounds.y, 0, "menu bar must occupy row 0");
        assert_eq!(mb_bounds.height, 1, "menu bar must be 1 row tall");

        // Status line must be at the last row
        let sl_bounds = app.status_bar().unwrap().bounds();
        assert_eq!(
            sl_bounds.y,
            screen().height - 1,
            "status line must occupy last row"
        );
        assert_eq!(sl_bounds.height, 1, "status line must be 1 row tall");

        // Desktop must be between menu bar and status line
        let db = app.desktop().bounds();
        assert_eq!(db.y, 1, "desktop must start below menu bar");
        assert_eq!(
            db.height,
            screen().height - 2,
            "desktop height = screen height − menu − status"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_dispatch_reaches_desktop() {
        let mut app = Application::new(screen());

        // Add a window so that the desktop has something to focus
        let window = Window::new(Rect::new(2, 2, 20, 8), "Win");
        let _id = app.add_window(window);
        assert_eq!(app.desktop().window_count(), 1);

        // Dispatch CM_CLOSE: application-level handler should close the focused window
        let mut event = Event::command(CM_CLOSE);
        app.dispatch(&mut event);

        assert_eq!(
            app.desktop().window_count(),
            0,
            "CM_CLOSE must remove the focused window"
        );
        assert!(app.is_running(), "app must still be running after CM_CLOSE");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_cm_close_multiple_windows() {
        let mut app = Application::new(screen());

        let _id1 = app.add_window(Window::new(Rect::new(0, 0, 20, 8), "W1"));
        let _id2 = app.add_window(Window::new(Rect::new(5, 5, 20, 8), "W2"));
        let _id3 = app.add_window(Window::new(Rect::new(10, 10, 20, 8), "W3"));
        assert_eq!(app.desktop().window_count(), 3);

        // First CM_CLOSE removes front window (W3)
        app.dispatch(&mut Event::command(CM_CLOSE));
        assert_eq!(app.desktop().window_count(), 2, "first CM_CLOSE removes W3");

        // Second CM_CLOSE removes new front (W2)
        app.dispatch(&mut Event::command(CM_CLOSE));
        assert_eq!(
            app.desktop().window_count(),
            1,
            "second CM_CLOSE removes W2"
        );

        // Third CM_CLOSE removes last window (W1)
        app.dispatch(&mut Event::command(CM_CLOSE));
        assert_eq!(app.desktop().window_count(), 0, "third CM_CLOSE removes W1");

        // Fourth CM_CLOSE with no windows — must not panic, app still running
        app.dispatch(&mut Event::command(CM_CLOSE));
        assert!(app.is_running(), "CM_CLOSE with no windows must not crash");
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_cm_close_no_windows() {
        let mut app = Application::new(screen());
        assert_eq!(app.desktop().window_count(), 0);

        // Must not panic when there are no windows
        let mut event = Event::command(CM_CLOSE);
        app.dispatch(&mut event);

        assert!(app.is_running());
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_deferred_events() {
        let mut app = Application::new(screen());
        assert!(app.is_running());

        // Create a "carrier" event with a deferred CM_QUIT inside
        let mut carrier = Event::new(EventKind::None);
        carrier.post(Event::command(CM_QUIT));

        // process_deferred is called inside dispatch; trigger it by dispatching
        // the carrier (which is already cleared — it will fall through to deferred processing)
        app.dispatch(&mut carrier);

        assert!(
            !app.is_running(),
            "deferred CM_QUIT must stop the application"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_open_dropdown_creates_overlay() {
        use crate::command::CM_NEW;
        use crate::menu_bar::{menu_bar_from_menus, Menu, MenuItem};

        let mut app = Application::new(screen());
        let menus = vec![Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)])];
        let mb = menu_bar_from_menus(screen(), menus);
        app.set_menu_bar(mb);

        // Simulate F10 to open dropdown
        let f10 = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::F(10),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(f10));

        // After dispatch + deferred processing, overlay must exist
        assert!(
            !app.overlay_manager().is_empty(),
            "F10 must create a dropdown overlay"
        );
        assert!(
            app.menu_bar().unwrap().is_active(),
            "menu bar must show active dropdown"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_dropdown_escape_dismisses() {
        use crate::command::CM_NEW;
        use crate::menu_bar::{menu_bar_from_menus, Menu, MenuItem};

        let mut app = Application::new(screen());
        let menus = vec![Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)])];
        let mb = menu_bar_from_menus(screen(), menus);
        app.set_menu_bar(mb);

        // Open dropdown via F10
        let f10 = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::F(10),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(f10));
        assert!(
            !app.overlay_manager().is_empty(),
            "overlay must exist after F10"
        );

        // Press Escape — OverlayManager dismisses the overlay and posts CM_DROPDOWN_CLOSED
        let esc = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Esc,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(esc));

        assert!(
            app.overlay_manager().is_empty(),
            "Escape must dismiss dropdown overlay"
        );
        assert!(
            !app.menu_bar().unwrap().is_active(),
            "menu bar must be deactivated after Escape"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_dropdown_enter_emits_command() {
        use crate::command::CM_NEW;
        use crate::menu_bar::{menu_bar_from_menus, Menu, MenuItem};

        let mut app = Application::new(screen());
        let menus = vec![Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)])];
        let mb = menu_bar_from_menus(screen(), menus);
        app.set_menu_bar(mb);

        // Open dropdown via F10
        let f10 = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::F(10),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(f10));
        assert!(
            !app.overlay_manager().is_empty(),
            "overlay must exist after F10"
        );

        // Press Enter — selects first item (CM_NEW) and emits the command
        let enter = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(enter));

        // The command should be stored as unhandled (no window handles CM_NEW)
        let unhandled = app.take_unhandled_command();
        assert_eq!(
            unhandled,
            Some(CM_NEW),
            "Enter in dropdown must emit the selected item's command"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_cm_open_dropdown_without_bar_is_noop() {
        let mut app = Application::new(screen());
        // No menu bar installed — CM_OPEN_DROPDOWN must not panic
        let mut event = Event::command(CM_OPEN_DROPDOWN);
        app.dispatch(&mut event);
        assert!(app.overlay_manager().is_empty());
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_application_cm_dropdown_closed_resets_bar() {
        use crate::command::CM_NEW;
        use crate::menu_bar::{menu_bar_from_menus, Menu, MenuItem};

        let mut app = Application::new(screen());
        let mb = menu_bar_from_menus(
            screen(),
            vec![Menu::new("~F~ile", vec![MenuItem::new("~N~ew", CM_NEW)])],
        );
        app.set_menu_bar(mb);

        // Open dropdown
        let f10 = crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::F(10),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_crossterm_event(&crossterm::event::Event::Key(f10));
        assert!(app.menu_bar().unwrap().is_active());

        // Dispatch CM_DROPDOWN_CLOSED directly
        let mut event = Event::command(CM_DROPDOWN_CLOSED);
        app.dispatch(&mut event);

        assert!(
            app.overlay_manager().is_empty(),
            "overlay must be dismissed"
        );
        assert!(
            !app.menu_bar().unwrap().is_active(),
            "bar must be inactive after CM_DROPDOWN_CLOSED"
        );
    }

    #[test]
    fn test_application_cm_close_all() {
        let mut app = Application::new(screen());
        app.add_window(Window::new(Rect::new(0, 0, 20, 8), "W1"));
        app.add_window(Window::new(Rect::new(5, 5, 20, 8), "W2"));
        app.add_window(Window::new(Rect::new(10, 10, 20, 8), "W3"));
        assert_eq!(app.desktop().window_count(), 3);

        let mut event = Event::command(CM_CLOSE_ALL);
        app.dispatch(&mut event);

        assert_eq!(
            app.desktop().window_count(),
            0,
            "CM_CLOSE_ALL removes all windows"
        );
        assert!(event.is_cleared());
        assert!(app.is_running(), "app still running after close all");
    }

    #[test]
    fn test_application_cm_tile() {
        let mut app = Application::new(screen());
        app.add_window(Window::new(Rect::new(0, 0, 20, 8), "W1"));
        app.add_window(Window::new(Rect::new(5, 5, 20, 8), "W2"));
        assert_eq!(app.desktop().window_count(), 2);

        let mut event = Event::command(CM_TILE);
        app.dispatch(&mut event);

        assert!(event.is_cleared());
        // Windows should now be tiled (arranged in grid)
        let b0 = app.desktop().windows().child_at(0).unwrap().bounds();
        let b1 = app.desktop().windows().child_at(1).unwrap().bounds();
        // Two windows tile into 1x2 or 2x1 grid — just check they've been repositioned
        assert!(b0 != b1, "tiled windows should have different bounds");
    }

    #[test]
    fn test_application_cm_cascade() {
        let mut app = Application::new(screen());
        app.add_window(Window::new(Rect::new(0, 0, 20, 8), "W1"));
        app.add_window(Window::new(Rect::new(0, 0, 20, 8), "W2"));
        assert_eq!(app.desktop().window_count(), 2);

        let mut event = Event::command(CM_CASCADE);
        app.dispatch(&mut event);

        assert!(event.is_cleared());
        // Windows should now be cascaded (offset from each other)
        let b0 = app.desktop().windows().child_at(0).unwrap().bounds();
        let b1 = app.desktop().windows().child_at(1).unwrap().bounds();
        assert!(
            b1.x > b0.x || b1.y > b0.y,
            "cascaded windows should be offset"
        );
    }

    // -------------------------------------------------------------------------

    #[test]
    fn test_desktop_cursor_position_propagates_from_focused_child() {
        use crate::view::{View, ViewBase, ViewId, OF_SELECTABLE};
        use ratatui::buffer::Buffer;
        use std::any::Any;

        // A custom view that always reports a fixed cursor position.
        struct CursorView {
            base: ViewBase,
            pos: Position,
        }

        impl View for CursorView {
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
            fn handle_event(&mut self, _event: &mut Event) {}
            fn can_focus(&self) -> bool {
                true
            }
            fn options(&self) -> u16 {
                OF_SELECTABLE
            }
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, s: u16) {
                self.base.set_state(s);
            }
            fn cursor_position(&self) -> Option<Position> {
                Some(self.pos)
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let mut app = Application::new(screen());

        // Add a window with a CursorView child
        let mut win = Window::new(Rect::new(5, 2, 40, 15), "Editor");
        let cursor_pos = Position::new(10, 5);
        win.add(Box::new(CursorView {
            base: ViewBase::new(Rect::new(0, 0, 38, 12)),
            pos: cursor_pos,
        }));
        app.add_window(win);

        // The desktop must propagate the cursor position
        let reported = app.desktop().cursor_position();
        assert_eq!(
            reported,
            Some(cursor_pos),
            "desktop.cursor_position() must return the focused child's position"
        );
    }

    // -------------------------------------------------------------------------
    // Partial invalidation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_invalidate_rect_initial() {
        let mut app = Application::new(screen());
        // By default, dirty_rect is None (full redraw)
        app.invalidate_rect(Rect::new(10, 10, 20, 20));
        let dr = app.dirty_rect.unwrap();
        assert_eq!(dr, Rect::new(10, 10, 20, 20));
    }

    #[test]
    fn test_invalidate_rect_unions() {
        let mut app = Application::new(screen());
        app.invalidate_rect(Rect::new(0, 0, 10, 10));
        app.invalidate_rect(Rect::new(20, 20, 10, 10));
        let dr = app.dirty_rect.unwrap();
        // Union covers (0,0) to (30,30)
        assert_eq!(dr, Rect::new(0, 0, 30, 30));
    }

    #[test]
    fn test_invalidate_rect_non_overlapping_union() {
        let mut app = Application::new(screen());
        app.invalidate_rect(Rect::new(5, 5, 15, 15));
        app.invalidate_rect(Rect::new(50, 50, 20, 20));
        // x=5, y=5 to x=70, y=70
        assert_eq!(app.dirty_rect.unwrap(), Rect::new(5, 5, 65, 65));
    }

    #[test]
    fn test_draw_clears_dirty_rect() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = Application::new(screen());
        app.invalidate_rect(Rect::new(5, 5, 10, 10));
        assert!(app.dirty_rect.is_some());

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();

        // After draw, dirty_rect must be cleared
        assert!(app.dirty_rect.is_none());
    }

    #[test]
    fn test_add_window_invalidates_region() {
        let mut app = Application::new(screen());
        assert!(app.dirty_rect.is_none());

        let win = Window::new(Rect::new(5, 5, 30, 10), "Test");
        let _id = app.add_window(win);

        // add_window should invalidate the window's bounds
        assert!(app.dirty_rect.is_some());
        let dr = app.dirty_rect.unwrap();
        assert!(dr.x <= 5);
        assert!(dr.y <= 5);
        assert!(dr.width >= 30);
        assert!(dr.height >= 10);
    }

    #[test]
    fn test_draw_narrows_clip_to_dirty_rect() {
        use crate::view::{ViewBase, OF_SELECTABLE};
        use ratatui::buffer::Buffer;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use std::sync::Mutex;

        // Track the clip rect that was actually passed to child views
        let last_clip: std::sync::Arc<Mutex<Option<Rect>>> =
            std::sync::Arc::new(Mutex::new(None));

        // A custom view that records the clip rect it receives
        struct ClipRecorder {
            base: ViewBase,
            recorded: std::sync::Arc<Mutex<Option<Rect>>>,
        }

        impl View for ClipRecorder {
            fn id(&self) -> ViewId {
                self.base.id()
            }
            fn bounds(&self) -> Rect {
                self.base.bounds()
            }
            fn set_bounds(&mut self, b: Rect) {
                self.base.set_bounds(b);
            }
            fn state(&self) -> u16 {
                self.base.state()
            }
            fn set_state(&mut self, s: u16) {
                self.base.set_state(s);
            }
            fn draw(&self, _buf: &mut Buffer, clip: Rect) {
                let mut guard = self.recorded.lock().unwrap();
                *guard = Some(clip);
            }
            fn handle_event(&mut self, _event: &mut Event) {}
            fn can_focus(&self) -> bool {
                true
            }
            fn options(&self) -> u16 {
                OF_SELECTABLE
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }

        let recorded_clone = std::sync::Arc::clone(&last_clip);
        let mut app = Application::new(screen());

        // Add a window with the recorder
        let mut win = Window::new(Rect::new(5, 5, 30, 10), "Recorder");
        win.add(Box::new(ClipRecorder {
            base: ViewBase::new(Rect::new(0, 0, 28, 8)),
            recorded: recorded_clone,
        }));
        app.add_window(win);

        // Invalidate a specific region
        app.invalidate_rect(Rect::new(5, 5, 30, 10));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();

        // The clip rect passed to the view should be restricted to the
        // intersection of the dirty rect and the window area
        let recorded = *last_clip.lock().unwrap();
        assert!(
            recorded.is_some(),
            "the child view must receive a clip rect"
        );
        let rc = recorded.unwrap();
        // The clip should be at most as large as the dirty rect area
        assert!(rc.width <= 30, "clip width should be <= dirty width");
        assert!(rc.height <= 10, "clip height should be <= dirty height");
    }

    #[test]
    fn test_resize_invalidates_full_screen() {
        let mut app = Application::new(screen());
        app.dirty_rect = None;
        app.resize(120, 40);
        // resize must invalidate the full screen
        assert!(app.dirty_rect.is_some());
        let dr = app.dirty_rect.unwrap();
        assert_eq!(dr.width, 120);
        assert_eq!(dr.height, 40);
    }

    #[test]
    fn test_menu_bar_change_invalidates() {
        use crate::menu_bar::{menu_bar_from_menus, Menu};

        let mut app = Application::new(screen());
        app.dirty_rect = None;

        let mb = menu_bar_from_menus(screen(), vec![Menu::new("~F~ile", vec![])]);
        app.set_menu_bar(mb);
        assert!(
            app.dirty_rect.is_some(),
            "set_menu_bar should invalidate"
        );
    }

    #[test]
    fn test_close_window_invalidates() {
        let mut app = Application::new(screen());
        let win = Window::new(Rect::new(5, 5, 30, 10), "Test");
        let id = app.add_window(win);

        // Clear dirty rect after add
        app.dirty_rect = None;

        app.close_window(id);
        assert!(
            app.dirty_rect.is_some(),
            "close_window should invalidate"
        );
    }

    #[test]
    fn test_quit_does_not_invalidate() {
        let mut app = Application::new(screen());
        app.dirty_rect = None;

        app.quit();
        // quit() only changes running state, no visual change
        assert!(app.dirty_rect.is_none(), "quit should NOT invalidate");
    }
}
