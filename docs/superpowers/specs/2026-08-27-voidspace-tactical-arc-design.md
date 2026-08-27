# Voidspace Tactical Arc Context Menu — Design

Date: 2026-08-27  
Status: selected by user (`03 / TACTICAL ARC`)

## 1. Scope

Replace the rectangular treemap context menu with a game-like radial action menu, center the two-line bottom status bar vertically, and add a compact author/about surface.

This slice changes presentation and interaction only. File-operation semantics remain authoritative in the existing application layer.

## 2. Reference lock

Build target: the selected interactive `TACTICAL ARC` prototype from the Voidspace Radial Command System visual lab.

Prototype artifact: `C:\Users\user\.github\voidspace\.superpowers\brainstorm\46223-1787827624\radial-menu-options.html`, card `03 / TACTICAL ARC`.

Primary reference: Axiom's dark observability-console system.

- Preserve near-black and graphite surfaces, thin structural borders, compact machine typography, and orange reserved for active focus.

Borrowed details:

- GT Planar: kinetic ray, uppercase technical labels, hard-edged HUD geometry.
- Stryds: circular composition and clear concentric spatial rhythm.

Must not drift:

- Existing black/orange Voidspace identity.
- JetBrains Mono for compact technical labels.
- Flat, precise geometry; no glass cards, soft material shadows, gradients, or decorative neon fog.
- Cyan for Explorer, acid lime for Recycle, red/pink for permanent deletion.
- The menu is a semicircular tactical control, not a conventional popup and not a full 360-degree weapon wheel.

## 3. Tactical Arc interaction

### Opening

- Right-clicking a real treemap node opens Tactical Arc at the pointer origin.
- The clicked node becomes the current context target and selection, preserving existing behavior.
- Aggregate `OTHER` tiles remain non-file actions and do not expose destructive actions until opened to real members.
- Only one Tactical Arc may be open at a time.
- `Shift+F10` or the keyboard Context Menu key opens Tactical Arc for the focused real tile; if no real tile has focus, the command does nothing.
- While Tactical Arc is open, its full-canvas input shield consumes pointer and keyboard events before the treemap. A second right-click dismisses the current menu; it never simultaneously retargets another tile or activates underlying treemap content.

The menu stores a fail-closed `ContextTarget` containing the tab identity, node identity, snapshot version, captured absolute path, node kind, and invocation-time treemap root. Before emitting any action, the application resolves the node again and requires every stored field to match. A tab switch, rescan/restart, missing or renamed node, changed path/kind, changed treemap root, or snapshot replacement dismisses the menu and shows `TARGET CHANGED · OPEN AGAIN`. No action is emitted from stale state.

### Geometry

- Hub radius: approximately 30 logical points.
- Three wedge sectors occupy a 128-degree fan with a small gap between sectors.
- Inner radius: approximately 42 points.
- Outer radius: approximately 106 points.
- The hub shows size first and a shortened object name below.
- Sector order follows the safest-to-most-destructive progression:
  1. `OPEN` / Explorer — cyan.
  2. `BIN` / Recycle — acid lime.
  3. `VOID` / permanent deletion — danger red.
- A compact detail rail opposite the fan shows the full action label and `LMB TO EXECUTE`.

The sector definition is one constant table shared by rendering, hit testing, accessibility labels, keyboard selection, and tests. It contains action, short label, full label, semantic color, and keyboard index. Rendered ordering never comes from a separate list.

### Edge awareness

- Geometry is contained by the current treemap canvas rectangle, excluding the top bar, tab bar, status bar, and inspector.
- The fan evaluates right-facing and left-facing candidates, including a fixed 152×44-point label rail and an 8-point safe margin. It chooses a fully contained candidate; if both fit, it chooses the side with more free horizontal space.
- Near the right edge it opens leftward; near the left edge it opens rightward. In both orientations the visual top-to-bottom order remains Explorer, Recycle, Permanent; semantic ordering is not accidentally reversed by mirroring.
- Near top or bottom edges, its hub is clamped so the full fan and label rail stay visible.
- The interaction origin remains visually connected to the clamped hub when displacement is required.
- The tether is a straight 1-point line from the original click point to the clamped hub and is painted only when displacement exceeds 2 points.
- The minimum supported menu bounding box is 224×224 points plus the label rail. If the treemap canvas is smaller, the menu uses a 0.75 geometry scale, ellipsizes the detail rail, and centers the combined bounds. If even the minimum scaled bounds cannot fit, opening fails closed with `WINDOW TOO SMALL FOR COMMAND MENU` rather than painting outside the canvas.

### Pointer line and selection

- A 1–2 px luminous line runs from the hub toward the live pointer position.
- The hub is a dead zone. Gaps between sectors and coordinates outside the fan are neutral zones. Pointer opening starts with no selected action until the pointer enters a sector.
- The pointer angle and radius together select one wedge; selection does not execute an action.
- The selected wedge receives a brighter outline, low-opacity semantic fill, and readable action label.
- Movement remains immediate and deterministic; no spring physics or delayed hover timers.
- `Esc`, right-click again, or clicking outside closes the menu without action.
- Left-click resolves hit testing again at the click coordinate. It executes only the wedge under that coordinate, never a previously stored highlight.
- Left-click in the hub, a sector gap, outside the fan, or outside the menu dismisses without action.

### Keyboard and accessibility

- Keyboard opening starts with Explorer selected as the safe default. Pointer opening starts with no selection.
- While open, `1`, `2`, and `3` directly select Explorer, Recycle, and permanent delete.
- `Enter` executes the selected action. With no selection it does nothing.
- Arrow Up/Down and Tab/Shift+Tab cycle sectors in safe-to-destructive / reverse order.
- Focus remains trapped in the menu until action or dismissal, then returns to the originating tile when it still exists; otherwise it returns to the treemap canvas.
- The foreground `egui::Area` creates three explicit focusable accessibility responses with button roles, action names, object name, and destructive-state descriptions. The custom-painted wedges do not rely on paint alone for accessibility.
- All handled pointer buttons and keys are explicitly consumed so underlying treemap navigation cannot fire in the same frame.

### File-operation safety

- Explorer keeps the existing reveal behavior.
- Recycle executes immediately, with no confirmation, matching the user's prior requirement.
- Permanent delete continues through the existing `DELETE` confirmation dialog.
- The radial renderer never directly deletes or opens paths; it emits the existing typed `TreemapContextAction` values.

## 4. Rendering architecture

Create a focused `radial_menu` module owned by `voidspace-app`.

Responsibilities:

- `TacticalArcState`: validated `ContextTarget`, invocation origin, clamped hub, orientation, highlighted action, input mode, and origin focus identity.
- Pure geometry functions: fan orientation, sector paths, hit testing, clamping, and keyboard selection.
- `show`: render a foreground `egui::Area` with a full-canvas input shield and return an explicit lifecycle outcome: `Open(updated_state)`, `Dismissed(reason)`, or `Action(TreemapContextAction)`.

The treemap remains responsible for detecting the right-clicked real node and returning the context target. The application owns opening/closing radial state and translating the returned action into the existing inspector/file-operation pipeline.

State is invalidated before painting whenever target validation fails. The old `egui::Response::context_menu` block is removed after the new path is verified. No second context-menu implementation remains.

## 5. Bottom status bar alignment

- Retain the current 48-point bar and two-line metric hierarchy.
- The horizontal metric row fills the available content height.
- Every metric block uses a vertical layout centered on the cross axis.
- Label and value remain grouped with a fixed internal gap; separators span the intended instrument height and are centered with the group.
- Metric label/value group height is measured from actual galley rectangles. Its top and bottom free space inside the panel must differ by no more than one logical point.
- Separators are 30 logical points tall and share the panel's vertical center within one logical point.
- These invariants hold at 100%, 125%, and 150% Windows scaling.

## 6. About / Author

Add a compact `ABOUT` trigger in the top bar, visually subordinate to scan controls and Turbo. At widths below the existing compact top-bar breakpoint it collapses to a 28-point `i` control with tooltip `ABOUT / AUTHOR`; it never pushes the path, filter, or Turbo controls outside the window.

The panel contains:

- `VOIDSPACE`
- Version from the package metadata.
- `Created by Евгений “Chip” Юрченко`.
- Short public descriptor: `AI, рынки, агенты · Человек 2.0`.
- Clickable links:
  - Website: `https://evgyur.pro`
  - Telegram channel: `https://t.me/chipda`
  - Telegram chat: `https://t.me/chipdachat`
  - Direct Telegram: `https://t.me/chipcr`
  - X: `https://x.com/chip1cr`
  - Human 2.0: `https://human20.app`
  - Human 2.0 Telegram: `https://t.me/human20`
  - Hyperliquid RU: `https://t.me/hyperliquid_ru`

Links open through the operating system's default browser. The panel contains no tracking, remote embeds, or account data.

The link definitions are one constant table shared by rendering and tests. About is a single non-modal foreground panel: the trigger toggles it; `Esc`, outside click, or opening the disk switcher closes it. It is clamped inside the application viewport and cannot coexist with Tactical Arc.

## 7. Motion

- Tactical Arc appears immediately with no timed opening animation.
- Sector geometry and color change immediately with pointer or keyboard selection.
- The pointer ray repaints only in response to input or normal application repaints; there is no self-scheduled animation loop.
- No infinite decorative animation, spring physics, glow pulsing, or reduced-motion preference is required because the interaction contains no autonomous/timed motion.

## 8. Tests

Focused unit tests cover:

- Fan orientation at all four viewport edges.
- Full fan, label rail, and safe margin remain inside the treemap canvas, including mirrored and minimum-scale cases.
- Angle/radius-to-sector mapping, gaps, hub dead zone, outside-fan behavior, and click-time re-hit-testing.
- Keyboard cycling and direct numeric selection.
- Keyboard opening, focus trap/restore, and handled-input consumption.
- Aggregate tiles cannot emit file actions.
- Action mapping preserves Reveal / Recycle / Permanent semantics.
- ContextTarget validation fails on tab, snapshot, path, kind, root, rename, removal, and rescan changes.
- Status-bar geometry centers the metric group vertically.
- About links match the approved public URLs.

Runtime verification covers:

- Right-click a real folder near the center and all four window edges.
- Move between all sectors and verify the line follows the pointer.
- Execute Explorer and Recycle against a unique temporary test root created for this verification. Resolve both source and target paths and prove they remain inside that root before Recycle is enabled; otherwise abort the destructive check. Confirm Recycle has no extra dialog.
- Enter permanent deletion and verify the existing confirmation still gates execution.
- Dismiss with Escape, outside click, hub click, and repeated right-click.
- Open About and verify every link is visible and clickable.
- Verify About and its compact trigger at the minimum supported window width.
- Inspect the bottom bar at common Windows scaling values.
- Package, install, update the desktop shortcut, open the installed build, and capture visual evidence.

## 9. Acceptance criteria

- The old rectangular node context menu is gone.
- Tactical Arc opens reliably on right-click and never leaves the viewport.
- The pointer ray visibly tracks the mouse and only highlights; execution requires left-click or Enter.
- Existing file-operation safety behavior is unchanged.
- The bottom status content is vertically centered.
- About displays the approved author identity and public contact links.
- Focused tests, full workspace tests, Clippy, release build, smoke test, installation, shortcut refresh, and installed-app visual check pass before completion is claimed.
