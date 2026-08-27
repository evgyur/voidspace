# Voidspace Full Tactical HUD — Design

Date: 2026-08-27  
Status: selected by user (`03 / FULL TACTICAL HUD`)

## 1. Scope and relationship

This specification extends the approved [Tactical Arc design](./2026-08-27-voidspace-tactical-arc-design.md) from one interaction into a coherent visual system for the whole Voidspace shell.

The two documents form one implementation target:

- Tactical Arc owns right-click interaction, file-action safety, status-bar centering, and About content/lifecycle.
- This document owns the global HUD styling, top bar, volume tabs, breadcrumb, treemap presentation, inspector, disk picker, dialogs, notifications, and responsive density.

Where the documents overlap, the stricter safety and lifecycle rule wins. This visual redesign does not change scanner, watcher, index, deletion, or volume-switching semantics.

## 2. Reference lock

Selected prototype: `C:\Users\user\.github\voidspace\.superpowers\brainstorm\46223-1787827624\screen-modernization-levels.html`, card `03 / FULL TACTICAL HUD`.

Primary foundation: Axiom's dark observability-console system.

- Near-black canvas and flat graphite surfaces.
- Thin structural borders instead of card shadows.
- Dense monospace instrumentation.
- Orange reserved for active focus and the primary Turbo control.

Borrowed details:

- GT Planar: uppercase system codes, angular HUD marks, precise linework.
- Stryds: circular targeting rhythm, used only by Tactical Arc and compact disk-capacity indicators.
- Factory and Parallel product screens: compact dark developer-tool navigation, clear inspector hierarchy, and disciplined orange active states.

User-selected signature traits that must survive:

- Subtle technical grid over the treemap canvas.
- A visible selection reticle.
- Cut-corner/hard-surface presentation.
- Volume and object system codes.
- Instrument-style inspector and status line.
- JetBrains Mono as the primary readable machine typeface.

Reject:

- Glassmorphism, blur, frosted surfaces, diffuse shadows, gradients used as decoration, rounded SaaS cards, giant text, ornamental purple, animated scanlines, glitch effects, and permanent neon glow.
- Any styling that changes treemap area proportions, obscures labels, reduces hit targets, or competes with file-size colors.

## 3. Design tokens

### Typography

- `DisplayBrand`: existing Voidspace display face, 17–20 pt, bold.
- `HudHeading`: JetBrains Mono SemiBold, 11–13 pt, uppercase when short.
- `HudValue`: JetBrains Mono Medium, 10–12 pt.
- `HudLabel`: JetBrains Mono Medium, 7.5–9 pt, uppercase, tracked.
- `TileSize`: JetBrains Mono SemiBold, responsive 8–12 pt.
- `TileName`: JetBrains Mono Medium, responsive 8–12 pt.
- Body/help copy: existing readable UI face, 10–12 pt; long prose never uses all-uppercase.

At 100%, 125%, and 150% Windows scaling, text is laid out from measured galleys. Text never paints outside its owning rectangle.

### Colors

- Canvas: existing near-black.
- Chrome surface: three existing graphite levels.
- Structural border: existing line gray, 1 logical point.
- Primary focus/Turbo: existing vivid orange.
- Explorer/data: existing cyan.
- Recycle/healthy/live: acid lime.
- Permanent delete/error: danger red/pink.
- Violet/magenta remain treemap category colors, not navigation chrome.

### Geometry

- Shell panels: 0–2 pt corner radius.
- Cut corner: 7 logical points, only on sufficiently large treemap tiles, selected panels, and destructive dialogs.
- Standard control height: 30–34 points.
- Touch/mouse minimum actionable target: 28×28 points; destructive controls use at least 32 points height.
- Base spacing unit: 4 points.

## 4. Global shell

The layout remains four functional bands plus the workspace:

1. Top command bar.
2. Volume tab rail.
3. Workspace: treemap plus inspector.
4. Bottom instrument status bar.

No ornamental left navigation is added. Treemap remains the dominant canvas.

Chrome uses flat tonal separation. Scan progress remains orange and does not animate autonomously; no optional readiness ornament is added.

## 5. Top command bar

The current controls become connected instrument cells:

- Brand cell: two-tone `VOID` orange + `SPACE` white, with a micro version label below when width allows.
- Volume cell: `VOL:01 · C:\ ▾`; one click opens the existing disk picker.
- Authoritative path cell: full editable path remains unchanged functionally.
- Query cell: compact filter expression with `QUERY` micro-label.
- Turbo: solid orange, the strongest top-bar action.
- About: 28-point `i` cell, following the approved About lifecycle.

At the compact breakpoint, version and micro-labels hide first, then the filter collapses to a 32-point filter icon. Clicking or pressing Enter on that icon opens one anchored filter editor containing the complete authoritative expression. Enter applies, Escape cancels and restores the prior expression, outside click applies only when parsing succeeds, and a parse error keeps the editor open with readable error text. The icon shows a text badge for `ACTIVE` or `ERROR`, so state is not color-only. Opening the filter editor closes other transient overlays through the shared overlay coordinator. Volume, authoritative path, Turbo, and About remain reachable. No control overlaps or paints beyond the viewport.

## 6. Volume display registry

One app-owned `VolumeDisplayRegistry` maps normalized volume-root identity to a session-local display number used by the command bar, tabs, and disk picker.

- Numbers are assigned monotonically on first observation during the process lifetime.
- Refresh/reorder, tab close, and temporary hot-unplug do not renumber or release an existing mapping.
- Re-observing the same normalized root reuses its number.
- A previously unseen root receives the next number.
- Codes are presentation only, may change after application restart, and never participate in scan/file-operation identity.
- The registry is the only producer of `VOL:##`; individual components do not cache their own numbers.

## 7. Volume tabs

- Each tab shows a stable display index for the current session, root, and state: `VOL:01 · C:\ · SCANNING`.
- Healthy/live uses a lime square indicator; scanning uses orange; error uses danger red.
- Active tab has a two-point orange bottom rule and a slightly elevated graphite fill.
- Existing close control becomes an explicit 28-point `×` target with tooltip and keyboard focus.
- Closing and switching semantics do not change.
- When space is insufficient, tabs scroll horizontally; labels do not overlap and close controls remain attached to their tab.

## 8. Breadcrumb and navigation

- Root back action is rendered as `← LVL-1` and keeps the approved behavior of returning to the disk picker when already at scan root.
- Path segments are individual hard-edged cells separated by `//`.
- Every visible segment is clickable and keyboard focusable.
- Long paths collapse middle segments to one `…` cell; root and current segment always remain visible.
- The current segment uses orange text; ancestor segments use muted white.
- Navigation changes only on click/Enter, never on hover.

## 9. Treemap canvas

### Grid and background

- A static 22–28 point technical grid is painted beneath tiles at no more than 10% effective contrast relative to the canvas.
- The grid does not animate, capture input, or extend over the inspector/status bar.
- Grid rendering is clipped to the treemap canvas and skipped when its lines would alias densely at the current scale.

### Tiles

- Layout rectangles and hit-testing remain unchanged; visual cutting happens inside the allocated rectangle and never changes area computation.
- Tiles large enough to spare the 7-point corners use a six-point cut-corner polygon. Small tiles remain rectangular to preserve visible area.
- Borders remain one point. Selected and hovered states may add an inset overlay but never increase the outer rectangle or overlap neighbors.
- Every rendered tile always shows formatted size when the measured size galley fits the tile's existing label bounds; existing aggregation already prevents tiles too small for that minimum. Filename/folder name appears on the next line only when its measured galley also fits without changing layout, aggregation thresholds, or hit testing.
- Large tiles additionally show `ID:####` and child count in a muted micro-row.
- Medium tiles show size and name only.
- Size-only tiles remain size-only. Tiny tiles remain aggregated into `OTHER`; they do not receive unreadable system-code labels.

`ID:####` is supplementary display data derived from the current snapshot's node identifier, truncated/formatted for readability. It is collision-tolerant, valid only for the current scan epoch, hidden when space is insufficient, and never used to resolve a path or authorize an action.

### Selection and hover

- Selection uses four 12-point orange corner brackets inset by 4 points plus an `OBJECT LOCKED` micro-label when space permits.
- Hover uses a single cyan inset outline without pulsing.
- The crosshair/reticle appears only inside the currently hovered or keyboard-focused tile. It is clipped to that tile and disappears immediately on exit.
- Pinned nested detail remains visible after hover loss, preserving existing behavior.
- Tactical Arc paints above all tile states.

## 10. Inspector

The inspector becomes a compact instrument column:

- Header: `TARGET / LOCKED` or `OBJECT / 01`, object name, absolute path.
- Capacity meter: flat four-point bar using semantic category color; no gradient.
- Data rows: label left, value right, one-point separators.
- Optional live rows: watcher state and last change, only when authoritative values exist.
- Navigation actions: `ZOOM / ENTER`, `BACK`.
- Existing file actions Explorer and Copy Path are intentionally preserved in this slice without semantic changes; destructive actions stay in Tactical Arc and approved dialogs, preventing duplicate dangerous controls.

The inspector is 250–320 points depending on workspace width. At the compact breakpoint it becomes the existing drawer; all content remains reachable by scrolling.

## 11. Disk picker

- Preserve immediate disk choice with label, root, capacity, used, and free space.
- Cards become hard-surface drive modules with `VOL:##`, state square, and a thin capacity bar.
- Capacity uses numeric values as the primary evidence; the bar is secondary.
- Existing tab state is explicit: `OPEN TAB`, `SCANNING`, `LIVE`, or `START SCAN`.
- Keyboard navigation, focus repair, hot-plug refresh, and one-click open/activate behavior remain unchanged.
- Picker layout is a responsive grid; it becomes one column before cards can overlap.

## 12. Overlay coordination

One app-owned `OverlayCoordinator` governs transient and modal UI. Components request a typed overlay; they do not independently choose z-order or coexistence.

Precedence from highest to lowest:

1. Modal file-operation/permanent-delete dialog.
2. One transient overlay: Tactical Arc, disk picker, About, compact filter editor, inspector drawer, or Status Details.
3. Passive toast/notice.
4. Base shell and docked inspector.

Rules:

- Opening a modal closes all transient overlays, disables base interaction, and holds focus until completion/cancel.
- Only one transient overlay can exist. Opening another first dismisses the current one and restores/assigns focus to the new owner.
- Docked inspector is base content and can coexist; inspector drawer is a transient overlay and therefore exclusive.
- Toasts never capture focus. The coordinator places them above the status bar and outside the active overlay bounds. If the remaining safe rectangle is smaller than the toast's measured bounds, the toast is queued and shown after the blocking overlay closes.
- Tooltips are subordinate to their owning overlay and clipped to the application viewport.
- Escape closes the highest active dismissible layer only.

## 13. Dialogs, notifications, and file operations

- Dialogs use flat graphite panels, one-point borders, 2-point radius, JetBrains Mono labels, and a short system code in the header.
- Recycle remains immediate and creates no confirmation dialog.
- Permanent deletion uses danger red only for the destructive action, retains the typed `DELETE` gate, and shows the complete target path.
- Progress/file-operation state appears as a compact bottom-right instrument toast plus status-bar state, not a blocking decorative overlay.
- Errors use `ERR / category` plus clear readable prose; no fake hexadecimal codes.
- Toasts never cover Tactical Arc, About, disk picker controls, or the bottom status metrics.

## 14. About / Author

The About content and URLs are authoritative in the Tactical Arc specification.

Full Tactical HUD presentation adds:

- `ABOUT / AUTHOR` system header.
- Voidspace mark and package version.
- Author identity and public descriptor.
- Contact links as compact action rows with service label, handle/domain, and external-link mark.
- A restrained orange author signature line; no avatar or remote image is required.

## 15. Bottom status bar

The centering geometry from the Tactical Arc specification remains authoritative.

Full Tactical HUD adds:

- Stable module labels such as `SCAN / 01`, `ENTRIES`, `INDEXED`, `DISK USED`, `ENGINE`.
- Flat graphite background with a one-point top border; no gradient.
- Semantic value colors only, with the majority of values in neutral white.
- Responsive priority from highest to lowest is: `SCAN`, `ENGINE`, `FILE OP`, `NOTICE`, `DISK USED`, `INDEXED`, `ENTRIES`, `WATCH`, `FILTER`. Scan and Engine always remain. Beginning with the lowest-priority item, modules collapse into one focusable `MORE +N` cell before overlap.
- Clicking/pressing Enter on `MORE +N` opens a transient status-detail overlay through the coordinator. It lists every hidden module using the same label/value/color semantics, supports keyboard scrolling, and closes on Escape/outside click. Thus collapsing never makes a metric unreachable.

## 16. Motion and performance

- No animated grid, scanline, glitch, chromatic aberration, pulsing reticle, or infinite glow.
- Hover/focus updates only on input or normal app repaint.
- All HUD graphics use egui painter primitives and embedded fonts; no network assets or runtime image generation.
- Geometry and label measurement are cached where the existing treemap cache permits.
- The redesign must not introduce continuous idle repainting. A runtime diagnostic counter records app UI frames; after a one-second settling window with scanning complete/paused, pointer stationary, and no overlays, a five-second observation may contain at most three UI frames excluding OS expose, resize, and DPI events.
- Release performance is compared on the same otherwise-idle machine against baseline commit `fc46ec4`. A deterministic temporary tree and the same scanner settings are scanned five times per candidate; the median indexed entries/second may regress by no more than 5%. The benchmark runs with the window visible and the 1024-tile HUD workspace active. All five completed runs are included; there is no subjective outlier removal.
- A UI render benchmark uses a fixed 1024-tile snapshot at 1920×1080. The harness forces repaint only for measurement, runs 60 unmeasured warm-up frames, then measures 600 frames. Candidate median and p95 frame time may regress by no more than 10% versus `fc46ec4`, and p95 must remain below 16.7 ms on the verification machine. Forced benchmark repainting is isolated from production idle-repaint assertions.

## 17. Accessibility

- Color is never the only state cue: every semantic color has a label, icon/square, border pattern, or text state.
- Keyboard focus receives a two-point visible outline.
- Micro-labels are supplementary; essential meaning appears again in readable values or tooltips.
- At 150% scaling and the minimum supported window size, no essential action is clipped or overlapped.
- Screen-reader metadata uses plain action names, not system-code copy alone.

## 18. Component boundaries

- `theme`: tokens and typography roles only.
- `hud`: reusable instrument cells, cut-corner frame, state square, metric row, focus brackets, and micro-label helpers.
- `volume_display_registry`: the sole session-local root-to-`VOL:##` mapping.
- `overlay_coordinator`: typed transient set including `StatusDetails`, exclusivity, precedence, focus handoff, toast safe placement/queueing, and Escape routing.
- `status_bar`: metric priority/collapse, `MORE +N`, Status Details rendering/actions, keyboard scrolling, and restoration of focus to `MORE +N` after dismissal.
- `top_bar`: command cells and compact breakpoint behavior.
- `tab_bar`: volume-tab presentation and overflow.
- `breadcrumb`: path collapsing and navigation responses.
- `treemap`: tile paint states and static grid; layout semantics remain unchanged.
- `inspector`: object presentation and existing actions.
- `volume_switcher`: drive-module presentation; existing actions remain unchanged.
- `about`: content table, link actions, and lifecycle.
- `radial_menu`: Tactical Arc behavior from the companion specification.

These modules return typed actions; visual helpers do not mutate scan/index/file-operation state.

## 19. Tests and visual QA

Focused tests cover:

- Token roles and semantic color mapping.
- Compact top-bar priority and no-overlap geometry.
- Compact filter editor apply/cancel/error lifecycle.
- Stable session-local volume codes across refresh, reorder, close/reopen, and hot-plug removal/reappearance.
- Volume-tab close target and overflow layout.
- Breadcrumb collapse while preserving root/current segments.
- Cut-corner paint bounds stay inside the original treemap rectangle.
- Size-first/name-second label measurement at small, medium, and large tile sizes.
- Selection brackets and reticle remain clipped to the active tile.
- Inspector/disk-picker/status responsive breakpoints.
- Overlay precedence, exclusivity, Escape routing, and queued toast placement.
- Status `MORE +N` priority and hidden-metric reachability.
- About link table and permanent-delete danger styling.
- No autonomous repaint request from static HUD components.
- Runtime idle frame-count threshold, scanner throughput comparison, and 1024-tile render benchmark.

Runtime QA states:

- Disk picker at minimum and wide window widths.
- Active scanning workspace with two volume tabs.
- Deep breadcrumb path.
- Large, medium, tiny, selected, hovered, and pinned tiles.
- Inspector docked and drawer modes.
- Tactical Arc at all edges.
- About, error toast, Recycle, and permanent-delete confirmation.
- 100%, 125%, and 150% Windows scaling.

Visual source truth: selected `03 / FULL TACTICAL HUD` prototype plus the token/behavior constraints in this document. Fix all P0–P2 drift before handoff.

## 20. Acceptance criteria

- The installed application visibly matches the selected Full Tactical HUD direction across all named shell components.
- Treemap layout proportions, hit targets, size-first labeling, aggregation, and live-scan stability do not regress.
- No text, tabs, cards, dialog controls, or status modules overlap at supported widths/scales.
- Tactical Arc and destructive safety satisfy their approved specification.
- Scanner performance and idle behavior remain within the existing package smoke expectations.
- Full workspace tests, Clippy, release build, smoke test, installation, desktop-shortcut refresh, and installed-app visual inspection pass before completion is claimed.
