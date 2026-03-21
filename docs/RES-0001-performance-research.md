# RES-0001: Rust TUI Performance Research

**Date:** 2026-03-21
**Context:** turbo-tui v0.2 rebuild — performance-first architecture

## Key Findings

### Ratatui Cell-Level Diffing
- Ratatui diffs two buffers per frame, only writes changed cells to terminal
- Diffing itself is the bottleneck for static content — `.width()` called twice per cell (~17% overhead)
- 60 FPS wasteful for static content — dirty-flag to skip diff when unchanged

### Zero-Allocation Hot Path
- Pre-allocate Vec with capacity
- Stack arrays over heap Vec when size known
- Reuse String buffers instead of format!() per frame
- Iterator-based loops avoid bounds checks

### Mouse Drag Performance
- Poll interval critical: 50ms = 20 FPS max, 16ms = 60 FPS
- Main-thread blocking kills responsiveness
- Event coalescing (skip intermediate drag events) — not built into Ratatui

### Trait Object vs Enum Dispatch
- No concrete TUI-specific data found
- Box<dyn View> has vtable overhead but is standard in Rust UI frameworks
- For our use case: widget count is small (<50), vtable overhead negligible
- Flexibility of dyn View outweighs marginal perf gain of enum dispatch

## Architecture Decisions for v0.2

1. **Dirty flag per ViewBase** — skip draw if clean, skip diff if no view is dirty
2. **Event coalescing** — during drag, coalesce multiple Drag events into one
3. **16ms poll** — ~60 FPS baseline
4. **Pre-allocated draw buffers** — no allocations in draw hot path
5. **Keep Box<dyn View>** — flexibility over marginal perf, widget count is small
6. **Draw-before-poll pattern** — like reference: draw() → poll() → handle_event()
