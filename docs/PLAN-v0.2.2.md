# turbo-tui v0.2.2 — MenuBar → Overlay Dropdown Refactor

> **Status:** In Progress (2026-03-22)
> **Branch:** `v0.2-rebuild`
> **Basis:** v0.2.1 — 321 tests, ~15,000 lines, 21 source files
> **Predecessor:** v0.2.1 (PLAN-v0.2.1.md)

---

## Problem Statement

`HorizontalBar` currently self-draws its dropdown box (~170 lines of drawing code in `draw_dropdown()`) using the same `clip` rectangle as the bar itself. This means:

1. **Dropdowns get clipped** if a window overlaps the bar's clip area
2. **Duplicate code** — `HorizontalBar` reimplements much of what `MenuBox` already does (border drawing, item rendering, hotkey parsing)
3. **Event handling is tangled** — `HorizontalBar` manages dropdown keyboard/mouse navigation internally (~100 lines), duplicating `MenuBox`'s logic

The `OverlayManager` already exists and renders above everything. `MenuBox` already exists as a standalone dropdown widget with full keyboard/mouse support. The refactor connects these pieces.

---

## Architecture Decision

### Communication Pattern: Deferred Command Events

When `HorizontalBar` wants to open a dropdown, it **cannot** directly push onto `OverlayManager` (it doesn't have access). Instead:

1. `HorizontalBar` posts a **deferred event** with a new internal command `CM_OPEN_DROPDOWN` carrying the bar's `ViewId` and dropdown index
2. `Application::process_deferred()` intercepts this command and:
   - Creates a `MenuBox` with the correct items and position
   - Pushes it onto `OverlayManager` as an overlay owned by the bar's `ViewId`
3. When `MenuBox` selects an item, it posts a **command event** (`EventKind::Command(selected_cmd)`)
4. `OverlayManager` dismisses the overlay, the command flows through the normal dispatch chain
5. `HorizontalBar` gets notified of dismiss via `CM_DROPDOWN_CLOSED` so it can reset its active state

### Why Not Direct Access?

- `HorizontalBar` is a `View` — it doesn't own a reference to `Application` or `OverlayManager`
- The deferred event pattern is already established (v0.2.0 architecture)
- Keeps the clean separation: views emit events, `Application` orchestrates

### Key Insight: MenuBox Needs Enhancement

Current `MenuBox` stores `result()` internally and calls `event.clear()` after confirmation. For the overlay pattern, `MenuBox` must instead:
- Set `event.kind = EventKind::Command(selected_cmd)` on Enter/click (so the command propagates)
- The overlay dismiss-on-result pattern handles the rest

### Left/Right Arrow Navigation

When a dropdown is open and the user presses Left/Right, `HorizontalBar` should close the current dropdown and open the adjacent one. This requires:
- `MenuBox` forwards Left/Right keys as deferred `CM_DROPDOWN_NAVIGATE` events
- `Application` intercepts and coordinates: close current overlay → tell `HorizontalBar` → open next

---

## New Internal Commands

Add to `src/command.rs`:

```rust
/// Internal: request to open a dropdown overlay.
/// Payload: bar ViewId + dropdown index (encoded in the event).
pub const CM_OPEN_DROPDOWN: CommandId = CommandId(INTERNAL_COMMAND_BASE + 10);

/// Internal: dropdown overlay was closed (dismissed or item selected).
/// Sent to the owning HorizontalBar so it can reset active_dropdown state.
pub const CM_DROPDOWN_CLOSED: CommandId = CommandId(INTERNAL_COMMAND_BASE + 11);

/// Internal: navigate to adjacent dropdown (Left/Right in open menu).
/// Payload direction encoded in broadcast data.
pub const CM_DROPDOWN_NAVIGATE: CommandId = CommandId(INTERNAL_COMMAND_BASE + 12);
```

---

## Phase 1: MenuBox Enhancement — Command Emission

**Priority:** High | **Agent:** developer-mid | **Est. ~40 lines changed**

### Changes to `src/menu_box.rs`

1. Add `owner_bar_id: Option<ViewId>` field — set when created by a `HorizontalBar` (vs. standalone use)
2. On `confirm_selection()`: instead of only setting `self.result`, also set `event.kind = EventKind::Command(cmd)` so the command propagates through the dispatch chain
3. On Left/Right arrow keys: post deferred `CM_DROPDOWN_NAVIGATE` event (only when `owner_bar_id.is_some()`)
4. Keep backward compat: `result()` still works for standalone usage

### Tests
- MenuBox emits Command event on Enter
- MenuBox emits Command event on mouse click
- MenuBox posts CM_DROPDOWN_NAVIGATE on Left/Right (when owned by bar)
- Standalone MenuBox still stores result() correctly

---

## Phase 2: HorizontalBar Simplification — Remove Self-Draw

**Priority:** High | **Agent:** developer-mid | **Est. ~200 lines removed, ~50 added**

### Changes to `src/horizontal_bar.rs`

1. **Remove** `draw_dropdown()`, `draw_dropdown_border_row()`, `draw_dropdown_item_row()` (~170 lines)
2. **Remove** `dropdown_width()` helper (MenuBox calculates its own bounds)
3. **Remove** `item_at_position()` helper (MenuBox handles its own hit-testing)
4. **Remove** dropdown keyboard navigation from `handle_key()`: Up/Down/Enter when active (MenuBox handles these)
5. **Remove** dropdown mouse handling from `handle_mouse()`: click-inside-dropdown, hover-over-items (MenuBox handles these)
6. **Keep** `active_dropdown: Option<usize>` — tracks which dropdown is logically open (for bar highlight)
7. **Keep** `open_dropdown()` — but change to post `CM_OPEN_DROPDOWN` deferred event instead of just setting state
8. **Keep** `close()` — resets `active_dropdown` (called on `CM_DROPDOWN_CLOSED`)
9. **Keep** bar-row mouse handling: click on entry, hover switching between entries
10. **Keep** F10, Escape, Alt+letter keyboard handling
11. **Add** `dropdown_items_for(&self, index: usize) -> Option<&[MenuItem]>` — public getter for Application to create MenuBox
12. **Add** `dropdown_anchor(&self, index: usize) -> Option<(u16, u16)>` — public getter returning (x, y) anchor point for dropdown positioning

### Draw Changes
- `draw()` only calls `draw_bar()` — never `draw_dropdown()`
- Active dropdown entry gets highlighted style (already works via `active_dropdown`)

### Event Changes
- When active and receives Left/Right: post `CM_DROPDOWN_NAVIGATE` instead of internal `move_entry()`
- When active and receives Escape: post `CM_DROPDOWN_CLOSED` + close locally
- Clicks outside bar when active: don't handle (OverlayManager dismiss-on-outside-click handles it)

### Tests
- Bar highlights active entry when dropdown is open
- F10 posts CM_OPEN_DROPDOWN
- Alt+letter posts CM_OPEN_DROPDOWN for matching Dropdown entry
- Click on bar entry posts CM_OPEN_DROPDOWN
- Escape when active closes and posts CM_DROPDOWN_CLOSED
- Left/Right when active posts CM_DROPDOWN_NAVIGATE

---

## Phase 3: Application Orchestration

**Priority:** High | **Agent:** developer (Sonnet) | **Est. ~80 lines**

### Changes to `src/application.rs`

Add dropdown orchestration in `dispatch()` / `process_deferred()`:

1. **Intercept `CM_OPEN_DROPDOWN`:** 
   - Read the bar ViewId and dropdown index from the event
   - Get items from `HorizontalBar::dropdown_items_for(index)`
   - Calculate position using `HorizontalBar::dropdown_anchor(index)` + `calculate_overlay_bounds()`
   - Create `MenuBox` with owner_bar_id set
   - Push as overlay with `dismiss_on_outside_click: true`, `dismiss_on_escape: true`
   - Clear the event

2. **Intercept `CM_DROPDOWN_CLOSED`:**
   - Pop overlay by bar's ViewId
   - Call `HorizontalBar::close()` to reset active state
   - Clear the event

3. **Intercept `CM_DROPDOWN_NAVIGATE`:**
   - Pop current overlay
   - Call `HorizontalBar::move_entry(delta)` + `open_dropdown(new_index)`
   - Create new MenuBox overlay for the new dropdown
   - Push onto OverlayManager

4. **Handle dismiss feedback:**
   - When `OverlayManager` dismisses an overlay (outside click), it needs to notify the owning bar
   - Add `OverlayManager::pop()` return value check → post `CM_DROPDOWN_CLOSED` if owner was a bar

### Event Payload Design

The deferred event needs to carry the bar's ViewId and dropdown index. Options:
- **Option A:** New `EventKind::DropdownRequest { bar_id: ViewId, index: usize }` — type-safe but adds a variant
- **Option B:** Encode in `Broadcast(u32)` with packed bar_id + index — hacky
- **Option C:** `CM_OPEN_DROPDOWN` command + store `pending_dropdown: Option<(ViewId, usize)>` on HorizontalBar, Application reads it — clean, no event changes

**Decision: Option C** — simplest, no event system changes. `HorizontalBar` sets `pending_dropdown` and posts `CM_OPEN_DROPDOWN`. Application reads and clears it.

### Tests
- CM_OPEN_DROPDOWN creates overlay with MenuBox
- MenuBox Enter → command propagates + overlay dismissed
- CM_DROPDOWN_NAVIGATE switches to adjacent dropdown
- Outside click dismisses overlay + resets bar state
- Escape dismisses overlay + resets bar state

---

## Phase 4: OverlayManager Enhancement — Dismiss Callback

**Priority:** Medium | **Agent:** developer-mid | **Est. ~30 lines**

### Changes to `src/overlay.rs`

1. Add `on_dismiss: Option<CommandId>` to `Overlay` struct — command to post when this overlay is dismissed
2. When `OverlayManager` dismisses an overlay (outside click, escape), post the `on_dismiss` command as a deferred event
3. This ensures `HorizontalBar` always gets notified when its dropdown closes, regardless of how

### Tests
- Overlay with on_dismiss posts command on outside-click dismiss
- Overlay with on_dismiss posts command on escape dismiss
- Overlay without on_dismiss (None) works as before

---

## Phase 5: Demo + Integration Tests

**Priority:** Medium | **Agent:** developer-mid | **Est. ~40 lines**

### Changes
- `examples/demo.rs` — verify menus still work (no API changes needed if Application handles orchestration)
- Add integration test: open menu → navigate → select → verify command emitted
- Add integration test: open menu → click outside → verify dismissed + bar reset
- Add integration test: open menu → Left/Right → verify adjacent menu opens

---

## Execution Order & Dependencies

```
Phase 1 (MenuBox enhance) ──────┐
                                 ├── Phase 3 (Application orchestration)
Phase 2 (HorizontalBar simplify)┘        │
Phase 4 (Overlay dismiss callback) ──────┘
                                          │
                                 Phase 5 (Demo + tests)
```

**Parallel groups:**
- Group A: Phase 1 + Phase 2 (independent — different files)
- Group B: Phase 3 + Phase 4 (can be parallel, but Phase 3 depends on 1+2)
- Group C: Phase 5 (depends on all)

### Agent Strategy
| Phase | Agent | Why |
|-------|-------|-----|
| 1 | developer-mid | MenuBox changes with event emission, moderate complexity |
| 2 | developer-mid | Large removal + small additions, clear spec |
| 3 | developer | Application orchestration — complex coordination logic, needs Sonnet |
| 4 | developer-mid | Small addition to Overlay struct |
| 5 | developer-mid | Tests and demo verification |

---

## Risk Assessment

### Risk 1: Bar-Row Hover Switching
When a dropdown is open and the user hovers over a different bar entry, the current code directly opens that dropdown. With overlays, this needs: dismiss current overlay → create new overlay. This is latency-sensitive (visual flicker).

**Mitigation:** `Application` can do both in one dispatch cycle (pop + push). No frame boundary between.

### Risk 2: Backward Compatibility
Consumer code that directly calls `HorizontalBar::close()` or checks `active_dropdown` may break.

**Mitigation:** Keep `active_dropdown` field and `is_active()` / `close()` methods. They now reflect the logical state, even though rendering is via overlay.

### Risk 3: StatusLine Dropdowns
`StatusLine` is also a `HorizontalBar` (drops up). The refactor must handle `DropDirection::Up` correctly in `calculate_overlay_bounds()`.

**Mitigation:** Already handled — `calculate_overlay_bounds()` supports both `Down` and `Up` with overflow flip.

---

## Success Criteria

1. All existing tests pass (321+)
2. Menu dropdowns render above windows (not clipped)
3. Full keyboard navigation works: F10, arrows, Enter, Escape, Alt+letter
4. Full mouse navigation works: click, hover-switch, click-outside-dismiss
5. StatusLine dropdowns still work (DropDirection::Up)
6. No visual flicker on hover-switching between menus
7. clippy pedantic clean, zero unsafe