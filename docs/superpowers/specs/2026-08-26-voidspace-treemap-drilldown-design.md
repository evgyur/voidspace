# Voidspace treemap drill-down and navigation design

Date: 2026-08-26  
Status: approved interaction direction; ready for independent spec review

## Problem

The treemap currently has click, hover preview, and zoom primitives, but their visible feedback is too weak. A user can click a directory such as `Traum_v2.31` and reasonably conclude that nothing happened. Zoom navigation is hidden in the inspector, and many small rectangles omit their size label entirely.

## Goals

- Hovering a directory previews one level of its children inside its rectangle.
- A single click pins that inline expansion so it remains after the pointer leaves.
- A double click makes the directory the treemap view root and fills the treemap canvas with its children.
- Navigation back to a parent is always obvious and available directly above the treemap.
- Every visible rectangle carries a readable size; rectangles too small to carry one are aggregated into `OTHER` rather than rendered unlabeled.
- Existing selection, inspector, filtering, live scan updates, `OTHER`, and deletion safety continue to work.

## Non-goals

- True Windows fullscreen mode is not part of this change. “Full screen” means the chosen directory fills the existing treemap canvas while app chrome and the inspector remain available.
- This change does not add USN acceleration, alter scan semantics, or change file-operation permissions.
- The inline preview renders one child level. Deeper traversal uses another double click after zooming; recursive multi-level nesting inside one small rectangle is intentionally excluded.

## Interaction model

### Hover

- Hovering a non-aggregated directory that has children and enough display area activates a temporary inline preview.
- Hover never mutates persistent navigation or selection state.
- Hovering a leaf or an aggregated `OTHER` tile shows normal emphasis/tooltip only.
- A pinned preview wins after the pointer leaves. A temporary hover preview may temporarily appear over another eligible base tile; when hover ends, the pinned preview returns.

### Single click

- A single click selects the hit tile for the inspector.
- If the tile is a non-aggregated directory with children and is a depth-one tile in the current view, it becomes the pinned inline-preview root.
- The selected directory receives an orange border and a small `PINNED` state cue in its header; its children subdivide the remaining interior below the header.
- A single click on a leaf selects it but does not create an empty preview.
- A single click on empty canvas clears the pinned preview while leaving the last inspector selection intact.
- A single click on `OTHER` preserves the existing aggregate-details behavior and never treats the synthetic aggregate node as a zoom root.

### Double click

- Double click is resolved before single-click side effects for that frame.
- Double clicking a non-aggregated directory with children pushes the current `view_root` onto history, makes the directory the new `view_root`, selects it, and clears the inline preview.
- The newly selected directory’s children are laid out across the full treemap canvas.
- Double clicking a leaf only selects it; it does not add a history entry or show an empty view.
- Double clicking `OTHER` opens aggregate details but does not zoom.

### Back and breadcrumb

- A navigation strip sits directly above the treemap and remains visible at every zoom level.
- It contains a high-contrast `← BACK` control followed by breadcrumb segments from the scan root to `view_root`.
- `← BACK` is disabled only at the scan root. It pops exactly one history entry.
- Each ancestor breadcrumb segment is clickable and jumps directly to that ancestor. History is truncated consistently so a subsequent Back returns to the ancestor’s parent, not to an abandoned branch.
- The current segment is orange and non-clickable.
- `Alt+Left` performs the same one-level Back action.
- Zoom/back clears pinned preview and aggregate-detail state, and selects the resulting `view_root`.

## Always-visible size labels

### Rendering tiers

Every visible tile must use one of these tiers:

1. Large: full/truncated name on line one and formatted size (`120.9 GiB`) on line two.
2. Compact: truncated name plus formatted size when both fit.
3. Size-only: compact size such as `13.0G`, `820M`, or `42K` when the name cannot fit.

The renderer must never intentionally paint a visible tile without a size string.

### Aggregation contract

- Layout eligibility must include a minimum label footprint, not area alone. The exact constants belong to implementation, but must cover the compact size-only text plus padding at the active DPI.
- Children predicted or produced below that footprint are folded into the parent’s synthetic `OTHER` tile.
- `OTHER` always displays its aggregate size, and displays its item count when the rectangle is large enough.
- The existing right-hand `OTHER` column direction remains unchanged.
- If the entire available canvas cannot fit even one labeled child, the view shows one labeled `OTHER` aggregate rather than unlabeled micro-rectangles.

## Components and boundaries

### `voidspace-layout`

- Owns the guarantee that emitted rectangles meet the caller-provided label footprint or are represented by `OTHER`.
- Extends `ViewState` with explicit minimum label width/height (or an equivalent typed footprint) rather than embedding font assumptions in the squarifier.
- Remains deterministic and UI-framework-independent.

### `voidspace-app::treemap`

- Owns hit testing, hover/pin state, double-click precedence, nested preview painting, and the three label rendering tiers.
- Returns typed interaction intent (`select`, `pin`, `zoom`, `aggregate`) to the app; it does not mutate tab history itself.
- Uses one shared size-formatting path for base, nested, and aggregate tiles.

### `VoidspaceApp`

- Owns persistent `view_root`, history, selection, and breadcrumb navigation.
- Applies zoom/back/breadcrumb transitions atomically and clears transient state consistently.
- Keeps the inspector synchronized with the selected node.

## Live-update behavior

- Scan/index updates may replace the layout each frame without clearing a still-valid pinned preview or current `view_root`.
- If a pinned node disappears or becomes ineligible, the preview clears safely.
- If `view_root` disappears, Voidspace returns to the nearest surviving ancestor; if none survives, it returns to the scan root and records a non-blocking diagnostic.
- Size labels update with the current snapshot values.

## Accessibility and discoverability

- Interactive tiles expose the node name, size, and whether they can be expanded.
- The navigation strip uses real egui buttons/links with keyboard focus.
- Tooltip copy teaches the interaction succinctly: `Click: inspect · Double-click: zoom`.
- The pinned state is communicated by both color/border and text, not color alone.

## Verification

### Unit tests

- Hover temporarily overrides and then restores a pinned preview.
- Single click pins an eligible directory; empty canvas clears only the pin.
- Double click emits zoom intent without a conflicting pin intent.
- Leaf and `OTHER` interactions never emit zoom.
- Back pops one entry; ancestor breadcrumb truncates history correctly.
- Layout emits no non-aggregate rectangle below the supplied label footprint.
- Undersized children are included exactly once in `OTHER`, with correct count and saturated size sum.
- Size label tier selection covers large, compact, size-only, and aggregate cases.

### Property and integration tests

- Layout remains deterministic, contained, and non-overlapping with the new footprint constraint.
- Allocated bytes represented by visible children plus `OTHER` equal the parent’s represented child total.
- UI-state tests cover pin → zoom → back and direct breadcrumb jumps.

### Manual release verification

- On the observed `Traum_v2.31` case: hover previews children, one click pins them, double click fills the treemap canvas with them, and Back returns to `books (FLIBUSTA)`.
- On the observed Google Drive case: every visible rectangle contains a size string; previously blank micro-tiles are absent and their bytes appear in `OTHER`.
- Resize the window through docked/drawer breakpoints and confirm that labels do not overlap and navigation remains visible.

## Acceptance criteria

- The user can understand the result of a single click without moving the pointer.
- The user can zoom with a double click and return without opening the inspector.
- No visible treemap rectangle is intentionally rendered without a size.
- No click path creates overlapping rectangles or stale hit targets.
- Existing full workspace tests, release build, smoke test, and a manual packaged-app interaction pass.
