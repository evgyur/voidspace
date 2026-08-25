# Voidspace Approved UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the visually overlapping recursive treemap and current stacked toolbar UI with the already-approved geometric Voidspace reference, including hover-to-preview and click-to-pin child tiles.

**Architecture:** Keep the existing Rust/egui application and scan/index/file-operation backends. Restrict the persistent canvas to one sibling level, render at most one transient or pinned child layer inside its parent tile, and move scan utilities into the 320 px inspector so the window follows the approved four-row shell: topbar, tabs, workspace, status.

**Tech Stack:** Rust 2024, egui/eframe 0.36, voidspace-layout, existing HTML geometry reference.

---

### Task 1: Lock the treemap geometry contract

**Files:**
- Modify: `crates/voidspace-layout/src/squarify.rs`
- Modify: `crates/voidspace-layout/tests/properties.rs`

- [x] Add a regression test asserting that a wide 1000×500 canvas gives the largest weighted item a left-hand vertical tile instead of a full-width horizontal stripe.
- [x] Add a regression test asserting that all sibling rectangles remain contained and have zero overlap after the orientation change.
- [x] Swap the wide/portrait placement branches in `place_row` so wide canvases consume vertical columns from left to right and portrait canvases consume horizontal rows from top to bottom.
- [x] Run `cargo test -p voidspace-layout`; expected result: all layout tests pass.

### Task 2: Implement hover preview and click pinning

**Files:**
- Modify: `crates/voidspace-app/src/treemap.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Test: `crates/voidspace-app/tests/ui_state.rs`

- [x] Add `pinned_preview: Option<NodeId>` to each scan tab and clear it on root changes/rescans.
- [x] Make the persistent `ViewState` use `max_depth: 1`, so parents and descendants are never painted together by default.
- [x] Determine the hovered base tile from the pointer; choose `hovered expandable tile -> pinned tile -> none` as the active child layer.
- [x] Lay out only the active tile's immediate children inside its inset content rectangle and paint them after the parent with 4 px gutters.
- [x] Return the visible child hit, base hit, hovered preview root, and clicked pin root from `treemap::show`.
- [x] On left click, pin the active expandable parent; clicking empty canvas clears the pin. Double-click still zooms to the visible non-aggregate node.
- [x] Add focused state tests for hover precedence, pin persistence, and clearing.
- [x] Run `cargo test -p voidspace-app`; expected result: all UI state tests pass.

### Task 3: Port the approved shell literally

**Files:**
- Modify: `crates/voidspace-app/src/theme.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Modify: `crates/voidspace-app/src/treemap.rs`
- Reference: `docs/design/voidspace-layout-reference.html`

- [x] Match the approved tokens: `#070809` background, `#0d0f11` surface, `#111419` raised surface, `#2a2e34` line, `#ff5a2f` primary accent, Segoe UI Variable/Segoe UI typography, and tabular monospace metrics.
- [x] Rebuild the 56 px topbar as brand, editable breadcrumbs/scope, filter, orange `TURBO / F5`, and outlined privilege badge. Enter in scope starts a normal scan.
- [x] Keep the tab row at 38 px with a 2 px orange active underline and remove the separate 42 px scan toolbar.
- [x] Keep the inspector docked at exactly 320 px for widths ≥900 and use the existing Details drawer below that breakpoint.
- [x] Move Pause/Rescan/Snapshot/Export into the inspector below object metrics; preserve Explorer, Recycle, and guarded permanent deletion.
- [x] Paint tiles with opaque dark color-mixed fills, rank-based accent borders, 4 px gaps, 10 px padding, clipped names, and no labels below the tested size threshold.
- [x] Keep the status bar at 30 px with live/indexing state, entry count, allocated size, watch health, and Turbo state.
- [x] Run `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`; expected result: exit code 0.

### Task 4: Prove the redesign on real data

**Files:**
- Modify: `scripts/package.ps1` only if verification exposes a packaging regression.
- Output: `dist/Voidspace-0.1.0-windows-x64.zip`

- [x] Run `cargo test --workspace`; expected result: zero failures.
- [x] Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1`; expected markers: `VOIDSPACE_SMOKE_OK` and `VOIDSPACE_PACKAGE_OK`.
- [x] Launch the packaged `voidspace.exe`, scan `C:\`, wait until representative top-level data is visible, and capture the full window.
- [x] Visually verify: no permanent nested stripes, largest tile reads as a stable block, hover reveals one child layer, click pins it, inspector/topbar do not overlap at 1440×900.
- [x] Commit the verified redesign and report the portable ZIP path, screenshot, tests, and any remaining performance risk.
