# Voidspace Drive Picker Design

## Context

Voidspace currently opens to a mostly empty canvas with a path field in the top bar. The approved change replaces that empty canvas with an immediate, information-rich choice of mounted Windows volumes. The user should understand disk capacity at a glance and start a scan with one click.

## Goals

- List every ready Windows volume that has a drive-letter root on the empty screen.
- Show the drive letter/root, Windows volume label, total capacity, used space, free space, and used percentage.
- Start scanning the selected volume when the user clicks anywhere on its card.
- Switch to any mounted volume from the active workspace with one click in the top bar.
- Preserve the approved native black/orange Voidspace visual language.
- Remain usable without administrator rights and without a localhost/web dependency.
- Prevent text or cards from overlapping from narrow desktop widths through large monitors.

## Non-goals

- No disk partitioning, formatting, mounting, ejecting, or health diagnostics.
- No removal of the existing editable path field or folder scan workflow; the drive trigger and path editor remain separate hit targets inside one split control.
- No new onboarding flow, settings page, or decorative imagery.
- No unconditional background polling while the active-workspace drive menu is closed.

## Reference lock

Primary direction: the existing approved Voidspace interface, reinforced by BaseHub's restrained near-black technical surfaces.

Preserve:

- Near-black canvas, thin neutral borders, the approved Unbounded/Golos Text/JetBrains Mono typography system, and flat depth.
- Voidspace orange only for primary interaction, selected/hovered states, and the used-capacity signal.
- Existing square/technical geometry; no glass, gradients, soft shadows, or oversized pill cards.
- High information density with deliberate empty space around the card grid.

Borrow only:

- Three's sharp ember-orange active-state emphasis.
- Clipchamp's storage hierarchy: capacity summary, progress bar, and legible used/free metrics.

Reject:

- Generic dashboard KPI tiles, disk illustrations, neon glows, large rounded cards, and multi-color progress bars.

The canonical active-workspace reference is [`assets/voidspace-drive-switcher-approved.png`](assets/voidspace-drive-switcher-approved.png), SHA-256 `f9833a97c8f7d962aa7dfa85df6ac641aeb9ef3758c9eba05d4fdb60c19f9491`. Its lower-right approval control is review chrome and is excluded from the application target. Replacing the image requires a new user-approved artifact and hash.

## Start-screen hierarchy

The central panel contains:

1. Eyebrow: `STORAGE / WINDOWS`.
2. Heading: `CHOOSE A VOLUME`.
3. Supporting text: `Select a disk to start scanning. Mounted volumes refresh automatically.`
4. A responsive grid of volume cards.
5. A quiet footer hint that the top path field can still scan a folder directly.

The heading is left-aligned with the grid rather than centered in the whole window. This makes the screen read like a functional launcher instead of a marketing hero.

## Volume card

Each card is one large click target and contains:

- top-left: drive root such as `C:` in 26 px strong text;
- below it: Windows volume label such as `Windows`, with `Local Disk` as the fallback;
- top-right: total capacity and the small label `TOTAL`;
- middle: a thin used-capacity bar with a dark track and orange fill;
- bottom-left: `USED 1.30 TB`;
- bottom-right: `FREE 698.9 GB`;
- the used percentage, rounded to a whole number such as `65%`, beside the bar and visible at every supported card width.

Normal state uses `SURFACE` with a one-pixel `LINE` border. Hover changes the border to orange and slightly raises the surface color. Pressed state uses the existing orange interaction treatment. The whole card receives pointer and keyboard focus semantics.

Long volume labels are truncated with an ellipsis inside the card and exposed in a hover tooltip. Numeric values never wrap. Cards participate in normal focus order; Enter and Space activate the focused card, and its accessible label contains the drive root, volume label, total capacity, and free space.

## Active-workspace drive switcher

The top path area becomes one visually unified split control with two distinct interactions:

- a fixed-width drive button on the left, rendered exactly as `H:\ ▾` when closed and `H:\ ▴` when open;
- the existing editable path field on the right, such as `\books\FLIBUSTA`.

The drive button is always visible in a scan workspace and remains reachable in one click. It never requires focusing or clearing the path field. If the current scan root belongs to a ready drive-letter volume, the button shows that canonical root. For a UNC/custom root or a state without an active volume, it shows `DISKS ▾`. The path editor retains its existing Enter-to-scan behavior.

### Path-state contract

`scope_text` remains the single authoritative full path string. The split control does not store a second editable suffix and never constructs a scan path from independently mutable button/editor strings.

- When the editor is unfocused and `scope_text` is an absolute drive path whose prefix matches the drive button, its visual presentation may omit the redundant drive prefix and show a suffix such as `\books\FLIBUSTA`, as in the approved artifact.
- When the editor receives focus, it reveals and edits the complete authoritative value such as `H:\books\FLIBUSTA`. Copy, paste, selection, and Enter therefore operate on a normal full path.
- For UNC, relative, drive-relative, malformed, or otherwise custom input, the editor always displays the full value and the button displays `DISKS ▾` unless the active scan tab itself has a canonical volume root.
- Opening or closing the drive overlay never edits `scope_text`.
- Activating another tab replaces `scope_text` with that tab's full scan root. Starting a new volume scan sets it to the selected `VolumeInfo.root_path`, always `X:\`. An unsubmitted path draft is discarded only when the user explicitly activates another volume/tab.
- Enter submits the authoritative full `scope_text` through the existing scan-start path. A bare `H:` remains Windows drive-relative input and is never rewritten or deduplicated as the `H:\` volume root.

Clicking the drive button opens an anchored overlay directly below the split control without moving or resizing the treemap. The button receives the orange active fill and changes its chevron direction while open. The overlay:

- shows every cached ready volume in drive-letter order;
- shows drive letter, Windows volume label, total capacity, free space, and a thin used-capacity bar on every row;
- marks the current volume with an orange border and a text/check cue, not color alone;
- uses a 500 logical-point preferred width, clamps to the viewport, and hides the less important total-capacity column before free space when narrow;
- prefers placement below the button, but uses the side with more available height when the preferred content height does not fit; it remains inside an eight-point viewport margin;
- limits its body to the available height and puts volume rows in a vertical scroll area under a sticky header, so all 26 possible drive-letter rows remain reachable without covering outside the viewport;
- ellipsizes long volume labels while keeping the drive letter and free-space value visible;
- opens immediately from the cache and displays a quiet live-refresh status in its header.

One click on a volume row performs exactly one action:

- if that volume already has an open scan tab whose canonical root equals the volume root, activate that tab and close the overlay;
- if it has no matching open tab, create and activate a new scan tab for the volume root, preserving the previous tab;
- if it is the already active root, only close the overlay and keep the current scan untouched.

Canonical root comparison uses a typed `VolumeRootKey` rather than general path equality. `VolumeRootKey::from_scan_root` returns a key only for a true drive root matching `^[A-Za-z]:\\$`; it uppercases the ASCII drive letter and retains the trailing backslash. It returns no key for the drive-relative `H:`, folders such as `H:\books`, UNC paths, or arbitrary custom paths. Every `VolumeInfo.root_path` already has exact `X:\` form and therefore produces a key.

Every scan-start entry point—start-screen card, top switcher, editable path, CLI, or internal action—routes through one `open_or_activate_scan` boundary. If the requested root has a `VolumeRootKey`, the boundary activates the lowest-index existing tab with the same key and creates no new tab. This prevents future duplicate whole-volume tabs regardless of entry point. If legacy/in-memory duplicates somehow already exist, the lowest-index match wins deterministically and the boundary does not close the other tabs. Non-root folder scans retain the existing behavior and may coexist with their volume-root tab.

### Keyboard and accessibility

- The drive button is a real focused button whose accessible name includes `Choose disk` and the current volume.
- Enter or Space opens the overlay. Up/Down moves between volume rows; Enter or Space activates the focused row.
- On open, focus goes to the current volume row when present, otherwise the first row. If the cache is empty/detecting, focus remains on the drive button.
- A refresh preserves focus by `VolumeRootKey`. If the focused drive disappears, focus moves to the row now occupying the removed row's sorted index, then to the preceding last row if the index is past the end; if no rows remain, focus returns to the drive button.
- Escape closes the overlay and returns focus to the drive button without changing the scan.
- Clicking outside closes the overlay without changing the scan.
- Each row's accessible label includes drive root, volume label, total capacity, free space, current-state cue, and whether activation switches to an existing tab or starts a scan.

## Responsive layout

- Minimum card width: 280 logical pixels.
- Maximum grid width: 1280 logical pixels.
- Column count is derived from available width and capped at four.
- At least 16 pixels separate cards horizontally and vertically.
- Narrow windows fall back to one column inside a vertical scroll area.
- Card height stays fixed, so adjacent rows align and values cannot collide.

## Windows volume discovery

`volume.rs` owns the platform boundary. The supported set is deliberately precise: ready volumes exposed by Windows with roots `A:\` through `Z:\`. Directory-only mount points are outside this feature because they are not presented to users as disks with drive letters.

It exposes a small `VolumeInfo` model:

- root path;
- display root (`C:`);
- volume label;
- `VolumeUsage { total, free }`.

The module retains the existing `query(path) -> Option<VolumeUsage>` and `format_decimal_bytes(bytes) -> String` contracts used by active scan tabs. Discovery adds, rather than replaces, these APIs:

```rust
pub fn list() -> Result<Vec<VolumeInfo>, String>;
pub fn used_ratio(usage: VolumeUsage) -> f32;
```

`list` is a best-effort collection with one global failure boundary. Failure to enumerate the logical-drive bitmask returns `Err` and no candidate replacement. Once the mask is available, an individual drive whose capacity query fails is skipped. Failure to read a volume label does not remove the drive; it uses `Local Disk`. A successful enumeration may legitimately return an empty vector.

On Windows, discovery uses the existing `windows` crate:

- `GetLogicalDrives` to enumerate drive letters;
- `GetDiskFreeSpaceExW` to retain only ready volumes and obtain total/free bytes;
- `GetVolumeInformationW` to obtain the user-visible Windows volume label.

Results are sorted by drive letter. Unready optical drives, disconnected mappings, or paths whose capacity query fails are omitted. A missing volume label becomes `Local Disk`.

The non-Windows implementation returns an empty list so the crate remains portable.

## Application data flow

`VoidspaceApp` stores the current volume list, the last refresh time, an in-flight flag, and a channel for discovery results.

1. After startup arguments are processed, app creation starts one short-lived discovery worker even when `--scan` already created a scan tab, because the top-bar switcher must have a cache; Windows volume APIs never run on the UI thread.
2. The worker sends `Result<Vec<VolumeInfo>, String>` through the channel and requests an egui repaint when it finishes.
3. While no scan tabs exist, the app schedules another refresh no sooner than three seconds after the previous worker started. When scan tabs exist, the closed switcher does not poll. Opening the switcher requests a refresh if the cache is at least three seconds old; while it remains open it uses the same three-second minimum interval. Only one worker can be in flight.
4. The empty-state or open-switcher render calls `request_repaint_after` for the remaining refresh interval, so the reactive egui loop wakes even when the user is idle.
5. A successful result atomically replaces the cached list, including with an empty list. A failed initial result shows the discovery error state; a later transient failure retains the last successful list and records a quiet issue message.
6. Rendering is read-only over the cached list.
7. Clicking or keyboard-activating a start-screen card or switcher row records its root and, after the UI borrow ends, calls the shared `open_or_activate_scan` boundary. That boundary delegates to the existing scan creation path only when deduplication finds no matching volume-root tab.
8. Once any scan tab exists, the established treemap workspace replaces the start-screen cards. Discovery polling stops while the top-bar switcher is closed and resumes on demand while it is open.

## Error and empty states

- While the first worker is running, show `DETECTING VOLUMES` without a fake spinner.
- If a successful discovery finds no ready volumes, show `NO READY VOLUMES` plus guidance to use the path field.
- If the initial discovery fails globally, show `VOLUME DISCOVERY FAILED` and the same path-field guidance; later transient failures keep the existing cards visible.
- A single drive disappearing between display and click is handled by the existing `describe_root` failure toast.
- If the top-bar cache is empty, its overlay opens immediately with `DETECTING VOLUMES`; a successful empty result becomes `NO READY VOLUMES` and keeps the path editor available.
- A later switcher refresh failure retains cached rows and shows a quiet non-modal issue line. A row that disappears during activation closes no tab and surfaces the existing scan-start error.
- Failed row activation leaves the overlay open, preserves focus on that row while it remains cached, and shows the non-modal issue line so the user can retry or choose another volume.
- Volume enumeration never requests elevation and never turns a discovery error into a modal.
- Capacity arithmetic saturates and the progress ratio is clamped to `0..=1` for inconsistent filesystem values. If total capacity is zero, used bytes, percentage, and bar fill are all deterministically zero.

## Testing and verification

- Unit-test conversion of a logical-drive bitmask into ordered drive roots.
- Unit-test label fallback and capacity percentage clamping.
- Unit-test zero-capacity percentage and a transient discovery failure retaining the previous cache.
- Unit-test responsive column calculation at narrow, standard, and wide widths.
- Unit-test percentage visibility and label truncation at the minimum 280-pixel card width.
- Verify Enter/Space card activation and the idle-loop repaint schedule.
- Verify the top drive button opens in one click without changing the path text or treemap bounds.
- Verify current-root selection is a no-op, an existing root tab is activated without duplication, and a new root creates exactly one new active tab.
- Verify canonical comparison accepts only case-insensitive `X:\` roots: it matches `h:\` with `H:\`, but rejects drive-relative `H:`, `H:\books`, UNC, and malformed inputs.
- Verify all scan-start entry points share root-tab deduplication, the lowest-index legacy duplicate wins, and folder scans remain distinct.
- Verify Escape/outside-click close behavior, keyboard row traversal, accessible labels, narrow overlay clamping, and refresh-on-open behavior.
- Verify a 26-row menu remains inside short viewports with sticky-header scrolling, below/above placement, and deterministic focus repair after removal of the focused volume.
- Verify failed row activation leaves the overlay open, preserves focused row and all tabs, and exposes the non-modal error state.
- Run formatting, Clippy with warnings denied, the full workspace test suite, release packaging, and smoke verification.
- Launch the packaged executable and visually verify the real C: card contains its Windows label, approximately `2.00 TB` total, `1.30 TB` used, and current free space with no overlap.
- Click the C: card and verify it transitions directly into the scanning treemap.
- From the active treemap, click the drive button once, verify all real volumes show label/capacity/free space, select another drive, and verify the correct existing/new tab behavior.

## Decision ledger

| Decision | Source | Reason |
| --- | --- | --- |
| Left-aligned launcher hierarchy | User request + existing product context | Makes disks the primary action rather than leaving a decorative empty state. |
| Flat dark cards with thin borders | Existing Voidspace + BaseHub | Preserves the approved technical visual language. |
| Orange used bar and hover border | Existing Voidspace + Three | Keeps orange tied to action and active capacity. |
| Total/used/free displayed together | User requirement + Clipchamp storage pattern | Answers the storage question without opening another view. |
| Cached three-second discovery | Runtime constraint | Supports hot-plugged disks without Windows API calls every frame. |
| Entire card starts scanning | User requirement | Provides the shortest path from launch to useful work. |
| Split drive/path control | User-approved mockup | Makes drive switching a one-click action without sacrificing folder-path entry. |
| Activate existing root tab before creating one | Existing tab model + user goal | Keeps switching instant and avoids duplicate whole-volume scans. |
| Refresh on menu open | Runtime constraint | Keeps hot-plugged volumes current without permanent polling during scans. |
