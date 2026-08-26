# Voidspace Drive Picker Design

## Context

Voidspace currently opens to a mostly empty canvas with a path field in the top bar. The approved change replaces that empty canvas with an immediate, information-rich choice of mounted Windows volumes. The user should understand disk capacity at a glance and start a scan with one click.

## Goals

- List every mounted, readable Windows volume immediately on the empty screen.
- Show the drive letter/root, Windows volume label, total capacity, used space, free space, and used percentage.
- Start scanning the selected volume when the user clicks anywhere on its card.
- Preserve the approved native black/orange Voidspace visual language.
- Remain usable without administrator rights and without a localhost/web dependency.
- Prevent text or cards from overlapping from narrow desktop widths through large monitors.

## Non-goals

- No disk partitioning, formatting, mounting, ejecting, or health diagnostics.
- No replacement for the existing path field or folder scan workflow.
- No new onboarding flow, settings page, or decorative imagery.
- No background drive enumeration once a scan tab exists.

## Reference lock

Primary direction: the existing approved Voidspace interface, reinforced by BaseHub's restrained near-black technical surfaces.

Preserve:

- Near-black canvas, thin neutral borders, compact Segoe UI typography, and flat depth.
- Voidspace orange only for primary interaction, selected/hovered states, and the used-capacity signal.
- Existing square/technical geometry; no glass, gradients, soft shadows, or oversized pill cards.
- High information density with deliberate empty space around the card grid.

Borrow only:

- Three's sharp ember-orange active-state emphasis.
- Clipchamp's storage hierarchy: capacity summary, progress bar, and legible used/free metrics.

Reject:

- Generic dashboard KPI tiles, disk illustrations, neon glows, large rounded cards, and multi-color progress bars.

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
- optional percentage beside the bar when space permits.

Normal state uses `SURFACE` with a one-pixel `LINE` border. Hover changes the border to orange and slightly raises the surface color. Pressed state uses the existing orange interaction treatment. The whole card receives pointer and keyboard focus semantics.

Long volume labels are truncated with an ellipsis inside the card and exposed in a hover tooltip. Numeric values never wrap.

## Responsive layout

- Minimum card width: 280 logical pixels.
- Maximum grid width: 1280 logical pixels.
- Column count is derived from available width and capped at four.
- At least 16 pixels separate cards horizontally and vertically.
- Narrow windows fall back to one column inside a vertical scroll area.
- Card height stays fixed, so adjacent rows align and values cannot collide.

## Windows volume discovery

`volume.rs` owns the platform boundary.

It exposes a small `VolumeInfo` model:

- root path;
- display root (`C:`);
- volume label;
- `VolumeUsage { total, free }`.

On Windows, discovery uses the existing `windows` crate:

- `GetLogicalDrives` to enumerate drive letters;
- `GetDiskFreeSpaceExW` to retain only ready volumes and obtain total/free bytes;
- `GetVolumeInformationW` to obtain the user-visible Windows volume label.

Results are sorted by drive letter. Unready optical drives, disconnected mappings, or paths whose capacity query fails are omitted instead of blocking the screen. A missing volume label becomes `Local Disk`.

The non-Windows implementation returns an empty list so the crate remains portable.

## Application data flow

`VoidspaceApp` stores the current volume list and the last refresh time.

1. The list is populated when the app is created.
2. While no scan tabs exist, the app refreshes the list every three seconds to pick up removable or newly mounted drives.
3. Rendering is read-only over the cached list; Windows API calls do not occur on every frame.
4. Clicking a card records its root and, after the UI borrow ends, calls the existing `start_scan` path.
5. Once any scan tab exists, the established treemap workspace replaces the picker and discovery polling stops.

## Error and empty states

- If no ready volumes are found, show `NO READY VOLUMES` plus guidance to use the path field.
- A single drive disappearing between display and click is handled by the existing `describe_root` failure toast.
- Volume enumeration never requests elevation and never turns a discovery error into a modal.
- Capacity arithmetic saturates and the progress ratio is clamped to `0..=1` for inconsistent filesystem values.

## Testing and verification

- Unit-test conversion of a logical-drive bitmask into ordered drive roots.
- Unit-test label fallback and capacity percentage clamping.
- Unit-test responsive column calculation at narrow, standard, and wide widths.
- Run formatting, Clippy with warnings denied, the full workspace test suite, release packaging, and smoke verification.
- Launch the packaged executable and visually verify the real C: card contains its Windows label, approximately `2.00 TB` total, `1.30 TB` used, and current free space with no overlap.
- Click the C: card and verify it transitions directly into the scanning treemap.

## Decision ledger

| Decision | Source | Reason |
| --- | --- | --- |
| Left-aligned launcher hierarchy | User request + existing product context | Makes disks the primary action rather than leaving a decorative empty state. |
| Flat dark cards with thin borders | Existing Voidspace + BaseHub | Preserves the approved technical visual language. |
| Orange used bar and hover border | Existing Voidspace + Three | Keeps orange tied to action and active capacity. |
| Total/used/free displayed together | User requirement + Clipchamp storage pattern | Answers the storage question without opening another view. |
| Cached three-second discovery | Runtime constraint | Supports hot-plugged disks without Windows API calls every frame. |
| Entire card starts scanning | User requirement | Provides the shortest path from launch to useful work. |

