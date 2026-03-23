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
    CommandId, CM_CASCADE, CM_CLOSE, CM_CLOSE_ALL, CM_DROPDOWN_CLOSED, CM_DROPDOWN_NAVIGATE,
    CM_OPEN_DROPDOWN, CM_QUIT, CM_TILE,
};
use crate::desktop::Desktop;
use crate::menu_bar::MenuBar;
use crate::menu_box::MenuBox;
use crate::overlay::{calculate_overlay_bounds, Overlay, OverlayManager};
use crate::status_bar::StatusBar;
use crate::view::{Event, EventKind, View, ViewId};
use crate::window::Window;
use ratatui::layout::Rect;

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
    }

    /// Install a status bar.
    ///
    /// Recalculates the desktop area so the desktop ends above the last row.
    pub fn set_status_bar(&mut self, status_bar: StatusBar) {
        self.status_bar = Some(status_bar);
        self.recalculate_layout();
    }

    /// Add a window to the desktop and return its [`ViewId`].
    ///
    /// Convenience wrapper around [`Desktop::add_window`].
    pub fn add_window(&mut self, window: Window) -> ViewId {
        self.desktop.add_window(window)
    }

    /// Close a window by its [`ViewId`].
    ///
    /// Convenience wrapper around [`Desktop::close_window`].
    pub fn close_window(&mut self, id: ViewId) {
        self.desktop.close_window(id);
    }

    // -------------------------------------------------------------------------
    // Screen resize
    // -------------------------------------------------------------------------

    /// Update the screen size and recalculate all component layouts.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.screen_size = Rect::new(0, 0, width, height);
        self.overlay_manager.set_screen_size(width, height);
        self.recalculate_layout();
    }

    // -------------------------------------------------------------------------
    // Drawing
    // -------------------------------------------------------------------------

    /// Draw the entire application to a ratatui frame.
    ///
    /// Rendering order (back to front):
    /// 1. Desktop (background + windows)
    /// 2. `MenuBar`
    /// 3. `StatusBar`
    /// 4. Overlays
    pub fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        // 1. Desktop (background + windows)
        self.desktop.draw(buf, area);

        // 2. Menu bar (top row)
        if let Some(ref mb) = self.menu_bar {
            mb.draw(buf, area);
        }

        // 3. Status bar (bottom row)
        if let Some(ref sl) = self.status_bar {
            sl.draw(buf, area);
        }

        // 4. Overlays (above everything)
        self.overlay_manager.draw(buf, area);
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
            }
            crossterm::event::Event::Mouse(mouse) => {
                let mut event = Event::mouse(*mouse);
                self.dispatch(&mut event);
            }
            crossterm::event::Event::Resize(w, h) => {
                self.resize(*w, *h);
                let mut event = Event::resize(*w, *h);
                self.dispatch(&mut event);
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
        event.clear();
    }

    /// Navigate to the adjacent dropdown (Left/Right arrow in open menu).
    fn handle_navigate_dropdown(&mut self, event: &mut Event) {
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
                            self.desktop.close_window(id);
                            event.clear();
                        }
                    }
                }
                CM_OPEN_DROPDOWN | CM_DROPDOWN_CLOSED | CM_DROPDOWN_NAVIGATE => {
                    self.handle_dropdown_commands(event);
                }
                CM_CLOSE_ALL => {
                    self.desktop.close_all_windows();
                    event.clear();
                }
                CM_TILE => {
                    self.desktop.tile();
                    event.clear();
                }
                CM_CASCADE => {
                    self.desktop.cascade();
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
    use ratatui::layout::Rect;

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
}
