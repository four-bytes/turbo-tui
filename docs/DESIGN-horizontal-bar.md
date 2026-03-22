# HorizontalBar — Unified MenuBar + StatusLine

**Status:** Implementation in progress  
**Date:** 2026-03-22  
**Affects:** `menu_bar.rs`, `status_line.rs`, `application.rs`, `lib.rs`, `examples/demo.rs`

## Problem

`MenuBar` (1070 lines) and `StatusLine` (664 lines) are structurally nearly identical:

- Both are full-width, single-row bars with `OF_PRE_PROCESS`
- Both render `~X~` hotkey markers with the same theme styles (`menu_bar_*`)
- Both compute horizontal positions for entries
- Both track hover state
- Both handle mouse click + keyboard shortcuts
- Both use `ViewBase` identically

The only differences:
1. **Drop direction:** MenuBar drops down, StatusLine drops up
2. **Entry types:** MenuBar only has dropdown entries, StatusLine only has direct-action entries

Both bars should support **both** entry types (dropdown and action).

## Solution: Single `HorizontalBar` Struct

### Data Model

```rust
/// Drop direction for dropdown overlays.
pub enum DropDirection { Up, Down }

/// A single entry in a horizontal bar.
pub enum BarEntry {
    /// Direct command — click/hotkey fires immediately.
    Action { label: String, command: CommandId, key_code: u16 },
    /// Dropdown trigger — click/hotkey opens a MenuBox.
    Dropdown { label: String, items: Vec<MenuItem>, key_code: u16 },
}

pub struct HorizontalBar {
    base: ViewBase,
    entries: Vec<BarEntry>,
    entry_positions: Vec<u16>,
    drop_direction: DropDirection,
    hovered_entry: Option<usize>,
    active_dropdown: Option<usize>,
    selected_item: Option<usize>,
    hint_text: Option<String>,
}
```

### Constructor Helpers

```rust
HorizontalBar::menu_bar(bounds, entries)    // DropDirection::Down
HorizontalBar::status_line(bounds, entries) // DropDirection::Up
```

### Backward Compatibility

- `pub type MenuBar = HorizontalBar;` in `menu_bar.rs`
- `pub type StatusLine = HorizontalBar;` in `status_line.rs`
- `From<Menu>` → `BarEntry::Dropdown`
- `From<StatusItem>` → `BarEntry::Action`
- Old KB_* constants stay in `status_line.rs`
- Old `MenuItem` stays in `menu_bar.rs`

### New Capabilities

**StatusLine with dropdown (drops UP):**
```rust
BarEntry::Dropdown { label: "~F2~ Theme".into(), items: theme_items, key_code: KB_F2 }
```

**MenuBar with direct action (no dropdown):**
```rust
BarEntry::Action { label: "~H~elp".into(), command: CM_HELP, key_code: 0 }
```

### Unified Logic

| Before (duplicated) | After (shared) |
|---------------------|----------------|
| `extract_hotkey()` × 2 | `extract_hotkey()` × 1 |
| `strip_hotkey_markers()` × 2 | `strip_hotkey_markers()` × 1 |
| `parse_hotkey_text()` / inline loop | `parse_hotkey_text()` × 1 |
| `compute_positions()` × 2 | `compute_positions()` × 1 |
| `draw_bar()` × 2 | `draw_bar()` × 1 |
| `menu_at_column()` / `item_at_x()` | `entry_at_column()` × 1 |
| hover tracking × 2 | hover tracking × 1 |

### Application Changes

```rust
pub struct Application {
    menu_bar: Option<HorizontalBar>,    // was: Option<MenuBar>
    status_line: Option<HorizontalBar>, // was: Option<StatusLine>
    // ... rest unchanged
}
```

### Dropdown Direction

- `DropDirection::Down` → box at `bar_y + 1`
- `DropDirection::Up` → box at `bar_y - dropdown_height`

### Files

| File | Change |
|------|--------|
| **NEW** `src/horizontal_bar.rs` | All unified bar logic (~700 lines) |
| `src/menu_bar.rs` | Thin re-export + MenuItem/Menu types + From impls |
| `src/status_line.rs` | Thin re-export + StatusItem/KB_* + From impls |
| `src/application.rs` | Use HorizontalBar type |
| `src/lib.rs` | Add module |
| `examples/demo.rs` | Migrate to BarEntry API |
