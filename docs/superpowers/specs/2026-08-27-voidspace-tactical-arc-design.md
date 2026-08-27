# Voidspace Tactical Arc Context Menu — Design

Date: 2026-08-27  
Status: selected by user (`03 / TACTICAL ARC`)

## 1. Scope

Replace the rectangular treemap context menu with a game-like radial action menu, center the two-line bottom status bar vertically, and add a compact author/about surface.

This slice changes presentation and interaction only. File-operation semantics remain authoritative in the existing application layer.

## 2. Reference lock

Build target: the selected interactive `TACTICAL ARC` prototype from the Voidspace Radial Command System visual lab.

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

### Edge awareness

- The fan chooses the side with the most available viewport space.
- Near the right edge it opens leftward; near the left edge it opens rightward.
- Near top or bottom edges, its center is clamped so the full fan and label rail stay visible.
- The interaction origin remains visually connected to the clamped hub when displacement is required.

### Pointer line and selection

- A 1–2 px luminous line runs from the hub toward the live pointer position.
- The pointer angle selects one wedge; selection does not execute an action.
- The selected wedge receives a brighter outline, low-opacity semantic fill, and readable action label.
- Movement remains immediate and deterministic; no spring physics or delayed hover timers.
- `Esc`, right-click again, or clicking outside closes the menu without action.
- Left-click inside the selected wedge executes that action and closes the menu.
- Left-click inside the hub closes without action.

### Keyboard and accessibility

- While open, `1`, `2`, and `3` select Explorer, Recycle, and permanent delete.
- `Enter` executes the selected action.
- Arrow Up/Down cycles sectors.
- Focus remains trapped in the menu until action or dismissal.
- Every sector exposes a semantic label and action description for egui accessibility metadata.

### File-operation safety

- Explorer keeps the existing reveal behavior.
- Recycle executes immediately, with no confirmation, matching the user's prior requirement.
- Permanent delete continues through the existing `DELETE` confirmation dialog.
- The radial renderer never directly deletes or opens paths; it emits the existing typed `TreemapContextAction` values.

## 4. Rendering architecture

Create a focused `radial_menu` module owned by `voidspace-app`.

Responsibilities:

- `TacticalArcState`: target node, invocation origin, clamped hub, orientation, highlighted action.
- Pure geometry functions: fan orientation, sector paths, hit testing, clamping, and keyboard selection.
- `show`: render the overlay at foreground order and return an optional existing `TreemapContextAction`.

The treemap remains responsible for detecting the right-clicked real node and returning the context target. The application owns opening/closing radial state and translating the returned action into the existing inspector/file-operation pipeline.

The old `egui::Response::context_menu` block is removed after the new path is verified. No second context-menu implementation remains.

## 5. Bottom status bar alignment

- Retain the current 48-point bar and two-line metric hierarchy.
- The horizontal metric row fills the available content height.
- Every metric block uses a vertical layout centered on the cross axis.
- Label and value remain grouped with a fixed internal gap; separators span the intended instrument height and are centered with the group.
- The result must have visually equal breathing room above and below the tallest value line at 100%, 125%, and 150% Windows scaling.

## 6. About / Author

Add a compact `ABOUT` trigger in the top bar, visually subordinate to scan controls and Turbo.

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

## 7. Motion

- Open: 90–120 ms opacity and radial expansion from the hub.
- Sector change: immediate geometry, 70–90 ms color/outline interpolation.
- Pointer ray updates every repaint while the menu is open.
- Reduced-motion mode removes expansion and interpolation while retaining selection feedback.
- No infinite decorative animation.

## 8. Tests

Focused unit tests cover:

- Fan orientation at all four viewport edges.
- Full geometry remains inside viewport bounds.
- Angle-to-sector mapping and dead-zone behavior.
- Keyboard cycling and direct numeric selection.
- Aggregate tiles cannot emit file actions.
- Action mapping preserves Reveal / Recycle / Permanent semantics.
- Status-bar geometry centers the metric group vertically.
- About links match the approved public URLs.

Runtime verification covers:

- Right-click a real folder near the center and all four window edges.
- Move between all sectors and verify the line follows the pointer.
- Execute Explorer and Recycle; confirm Recycle has no extra dialog.
- Enter permanent deletion and verify the existing confirmation still gates execution.
- Dismiss with Escape, outside click, hub click, and repeated right-click.
- Open About and verify every link is visible and clickable.
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
