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
- The inline preview renders one child level. A nested preview directory may be double-clicked directly to zoom into it, but recursive multi-level inline nesting inside one small rectangle is intentionally excluded.

## Interaction model

### Hover

- Hovering a non-aggregated directory that has children and enough display area activates a temporary inline preview.
- Hover never mutates persistent navigation or selection state.
- Hovering a leaf or an aggregated `OTHER` tile shows normal emphasis/tooltip only.
- A pinned preview wins after the pointer leaves. A temporary hover preview may temporarily appear over another eligible base tile; when hover ends, the pinned preview returns.

### Single click

- A single click is applied immediately; Voidspace does not delay every click for the system double-click interval.
- Clicking a depth-one, non-aggregated directory with children emits `ActivateBaseDirectory`: select it for the inspector and replace the existing pin with that directory.
- The selected tile receives the orange selection border. The pinned depth-one directory independently receives a `PINNED` text cue plus a lime secondary border, so selection and pin remain distinguishable when a nested child is selected.
- Clicking a depth-one leaf emits `ActivateBaseLeaf`: select it and clear any existing pin.
- Clicking a nested preview tile emits `ActivateNested`: select it and preserve the enclosing depth-one pin so the clicked tile does not disappear immediately. Nested single-click does not add another inline expansion level.
- Clicking empty canvas emits `ClearPreview`: clear the pin while preserving the last inspector selection.
- Clicking `OTHER` emits `OpenAggregate`: clear the pin, select the real parent for the inspector, and open the existing aggregate details for the exact aggregate membership. It never treats the synthetic aggregate as a zoom root.

### Double click

- The first click may already have applied its immediate single-click action. When egui recognizes the second click as a double click, `Zoom` atomically supersedes the second single-click action and rolls back all provisional pin/aggregate state from the first click before changing `view_root`. A transient first-click highlight is acceptable; no pinned state survives the zoom transition.
- Double clicking any visible non-aggregated directory with children, including a nested preview child, makes it the new `view_root`, selects it, clears inline preview and aggregate details, and rebuilds the canonical ancestor path described below.
- The newly selected directory’s children are laid out across the full treemap canvas.
- Double clicking a leaf only selects it; it does not add a history entry or show an empty view.
- Double clicking `OTHER` opens aggregate details but does not zoom.

### Back and breadcrumb

- A navigation strip sits directly above the treemap and remains visible at every zoom level.
- It contains a high-contrast `← BACK` control followed by breadcrumb segments from the scan root to `view_root`.
- Each tab stores `view_path`, a canonical root-to-current chain of real node IDs. The first element is always the scan root and the last is always `view_root`; it is not a visit stack.
- Every zoom rebuilds `view_path` from snapshot parent links, including skipped inline-preview ancestors. Therefore, zooming directly from the scan root into a nested child produces `[scan root, base directory, nested child]` and Back goes to the real parent directory.
- `← BACK` is disabled only when `view_path.len() == 1`. It removes the last element and makes the new last element `view_root`.
- Each ancestor breadcrumb segment is clickable and jumps directly to that ancestor by truncating `view_path` after that segment. A subsequent Back therefore goes to the target’s real parent, never an abandoned visit branch.
- The current segment is orange and non-clickable.
- `Alt+Left` performs the same one-level Back action.
- Zoom/back clears pinned preview and aggregate-detail state, and selects the resulting `view_root`.

## Always-visible size labels

### Rendering tiers

The renderer must never intentionally paint a visible tile without a size string. All base, nested, and aggregate tiles use the active `ViewState.size_mode`; the current app passes `Allocated`.

Tier selection uses measured egui text galleys and the post-inset inner rectangle:

1. Large: two lines, full or ellipsized name plus the normal `format_bytes` size, when both measured lines and vertical spacing fit.
2. Compact: two lines, more aggressively ellipsized name plus a compact size (`13.0G`, `820M`, `42K`), when the compact two-line block fits.
3. Size-only: one centered compact-size line when the compact size fits but a name does not.

The size-only footprint is the minimum footprint supplied to layout. Tier choice is deterministic for the same fonts, DPI, rectangle, and size mode.

### Aggregation contract

- Layout eligibility must include a minimum outer label footprint, not area alone. The app measures the compact size-only galley at the active DPI, then adds tile padding, both visual insets, and stroke allowance before passing the footprint to `voidspace-layout`. Preview header space is removed from the nested-layout bounds before this calculation is applied. A rectangle that satisfies the layout footprint must therefore still satisfy the renderer after clipping and insets.
- Positive-size children are sorted deterministically by descending active size and then node ID. Layout chooses a prefix to keep. If any produced kept rectangle misses the outer footprint, it iteratively shortens that prefix and re-lays out until all kept rectangles fit. The remaining deterministic suffix is `OTHER`; arbitrary non-suffix membership is forbidden.
- `max_rectangles` and any layout-budget limit also cap the kept prefix. Every omitted positive-size child, whether omitted for footprint or budget, belongs exactly once to the same `OTHER` suffix. Zero-size children in the active size mode are explicitly excluded from both visible tiles and `OTHER`.
- `OTHER` always displays its aggregate size, and displays its item count when the rectangle is large enough.
- The layout output exposes the aggregate suffix boundary (`kept_count` or equivalent) together with count and size so aggregate details enumerate exactly the same sorted members; parent/count guessing is not an accepted interface.
- The existing right-hand `OTHER` column direction remains unchanged.
- Conservation invariant: the sum of active sizes for all emitted real children plus `OTHER.aggregate_size` equals the sum for all positive-size input children, even when `max_rectangles` is reached.
- If the canvas can fit the minimum size-only footprint but no individual child can be kept, the view shows one labeled `OTHER` aggregate. If the canvas itself is smaller than that footprint, layout emits no child rectangle and the app shows a non-tile overflow message containing the root’s formatted size; it never emits an unlabeled rectangle.

## Components and boundaries

### `voidspace-layout`

- Owns the guarantee that emitted rectangles meet the caller-provided label footprint or are represented by `OTHER`.
- Extends `ViewState` with explicit minimum label width/height (or an equivalent typed footprint) rather than embedding font assumptions in the squarifier.
- Remains deterministic and UI-framework-independent.

### `voidspace-app::treemap`

- Owns hit testing, hover/pin state, double-click rollback, nested preview painting, and the three label rendering tiers.
- Returns exactly one mutually exclusive action per activation: `ActivateBaseDirectory`, `ActivateBaseLeaf`, `ActivateNested`, `Zoom`, `OpenAggregate`, or `ClearPreview`. It does not mutate tab navigation state itself.
- Uses one shared size-formatting path for base, nested, and aggregate tiles.

### `VoidspaceApp`

- Owns persistent `view_path`, `view_root`, selection, and breadcrumb navigation.
- Applies zoom/back/breadcrumb transitions atomically and clears transient state consistently.
- Keeps the inspector synchronized with the selected node.

## Live-update behavior

- Scan/index updates may replace the layout each frame without clearing a still-valid pinned preview or current `view_root`.
- If a pinned node disappears or becomes ineligible, the preview clears safely.
- `view_path` retains ancestry across snapshots. After each update it is pruned from the end to the deepest surviving node whose retained parent relationship still forms a valid chain. That node becomes `view_root`. If the chain cannot be validated, the tab resets to the surviving scan root and records a non-blocking diagnostic.
- After path repair, a missing selection resets to `view_root`; a missing/ineligible pin clears; aggregate details clear if the parent or aggregate suffix boundary no longer matches the current layout.
- Size labels update with the current snapshot values.

## Accessibility and discoverability

- Every visible hit rectangle receives a stable egui interaction overlay ID derived from tab/view root, real node ID, depth, and aggregate flag. It exposes `WidgetInfo::Button` containing node name, formatted active size, and expandable/aggregate state.
- Tab moves focus between visible tile overlays. Enter or Space performs the same action as single click; Ctrl+Enter performs `Zoom` for an eligible directory. Escape performs `ClearPreview`. These keyboard activations flow through the same mutually exclusive action reducer as pointer input.
- The navigation strip uses real egui buttons/links with keyboard focus.
- Tooltip copy teaches the interaction succinctly: `Click: inspect · Double-click: zoom`.
- The pinned state is communicated by both color/border and text, not color alone.

## Verification

### Unit tests

- Hover temporarily overrides and then restores a pinned preview.
- Transition-table tests cover base directory, base leaf, nested tile, `OTHER`, empty canvas, keyboard activation, and first-click/second-click timing. Each activation emits one action with the specified pin transition.
- Single click pins an eligible base directory; empty canvas clears only the pin; a nested click preserves the enclosing pin.
- Double click rolls back the first-click pin and aggregate state, then emits only `Zoom` as its recognized second-click action.
- Leaf and `OTHER` interactions never zoom.
- `view_path` tests cover base zoom, nested direct zoom, one-level Back, direct ancestor breadcrumb, and invalidated live-update entries.
- Layout emits no non-aggregate rectangle below the supplied label footprint.
- Undersized and budget-exhausted positive children form one deterministic aggregate suffix and are included exactly once in `OTHER`, with exact membership boundary, count, and saturated size sum.
- Size label tier selection uses measured post-inset bounds and covers large, compact, size-only, aggregate, and minimum-canvas overflow cases.

### Property and integration tests

- Layout remains deterministic, contained, and non-overlapping with the new footprint constraint.
- Active-size bytes represented by visible children plus `OTHER` equal the positive-size input child total for footprint and `max_rectangles` cases.
- UI-state tests cover pin → nested select → nested zoom → parent Back, direct breadcrumb jumps, and live path repair.

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
