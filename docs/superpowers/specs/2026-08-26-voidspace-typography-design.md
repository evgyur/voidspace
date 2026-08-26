# Voidspace typography design

Date: 2026-08-26  
Status: user-approved visual direction; ready for independent spec review

## Decision

Voidspace uses a three-family type system:

- **Unbounded** for the wordmark, primary screen headings, active mode labels, and rare high-emphasis commands.
- **Golos Text** for Russian and English interface copy, navigation, controls, tile names, dialogs, and the inspector.
- **JetBrains Mono** for sizes, paths, scan counters, timestamps, keyboard hints, technical statuses, and other tabular data.

The user selected this direction as option 10 from ten typography studies and approved it in a full Voidspace disk-map mockup. The intended character is a futuristic technical instrument with excellent Cyrillic readability, not a decorative sci-fi poster.

The canonical visual reference is [`assets/voidspace-typography-option-10-approved.png`](assets/voidspace-typography-option-10-approved.png), SHA-256 `e9f664fce4fe5c154e94bb3d110de0976dd09ee23dee2a21d6ab09935120d169`. Its lower-right `ОДОБРИТЬ №10` companion control is review chrome and is excluded from the application target. The image content hash is the visual revision identifier; replacing it requires a new user-approved artifact and hash in this specification.

## Goals

- Give Voidspace a distinctive, modern identity that fits its black, orange, and bright-accent visual system.
- Keep dense Russian and English UI copy readable at Windows desktop sizes.
- Make byte sizes, paths, and live scan statistics stable and easy to compare.
- Preserve the treemap rule that every visible rectangle shows a size.
- Bundle the fonts so the packaged application looks identical offline and on machines without the fonts installed.

## Non-goals

- Unbounded is not the default body font and is not used for dense tile labels, file names, paths, tables, dialogs, or long text.
- This change does not redesign the approved palette, treemap geometry, navigation model, or delete flows.
- The application does not download fonts at runtime or depend on a system font installation.
- The first release does not expose user-selectable font themes.

## Reference lock

Primary direction: the selected option 10 mockup, using a dark command-center composition with Unbounded display accents, Golos Text UI copy, and JetBrains Mono data.

Preserve:

- black and near-black canvas;
- orange reserved for primary action, current navigation state, and critical emphasis;
- compact, squared technical composition;
- highly legible Cyrillic UI copy;
- monospaced numeric and path information;
- restrained use of Unbounded so its width does not consume treemap space.

Borrow only:

- Axiom-style discipline for monospaced operational data;
- BaseHub-style dark command-center density;
- Sanity-style restrained cyan, lime, magenta, and violet status/category accents.

Reject:

- Unbounded on every label;
- rounded consumer-app typography;
- decorative gradients or neon glow behind text;
- uppercase body copy;
- dynamic web-font loading;
- a fallback that silently changes treemap label measurements.

## Font roles

### Display family: Unbounded

Use for:

- `VOIDSPACE` wordmark;
- major view titles such as `КАРТА ДИСКА`;
- primary mode controls such as `TURBO / F5`;
- rare short confirmation headings.

Rules:

- Use only Medium, SemiBold, or Bold.
- Prefer 10–18 px in application chrome; larger sizes are allowed only in empty/onboarding states.
- Keep labels short and avoid all-caps Cyrillic sentences.
- Do not use inside treemap rectangles, except for a future dedicated empty-state heading outside tile geometry.
- `TURBO / F5` is a composite `egui::text::LayoutJob`: `TURBO` uses `display.action`, while the separator and `F5` use `data.micro`. The whole composite remains one accessible button label and one hit target.

### UI family: Golos Text

Use for:

- treemap names;
- drive names and metadata;
- breadcrumbs, buttons, menus, dialogs, warnings, and inspector copy;
- Russian and English prose.

Rules:

- Regular is the default; Medium distinguishes interactive labels; SemiBold is reserved for short hierarchy cues.
- Tile names remain single-line and ellipsize deterministically in the large and compact label tiers.
- Text wrapping is allowed in dialogs and inspector prose, but not in paths, sizes, or status fields.

### Data family: JetBrains Mono

Use for:

- all formatted byte sizes;
- filesystem paths;
- volume capacity/free-space figures;
- item counts, timestamps, scan rate, and technical status;
- keyboard shortcuts and compact diagnostic messages.

Rules:

- Numeric data uses fixed-width digits from the font without synthetic spacing.
- Byte-size labels never switch to Golos Text, including size-only treemap tiles and `OTHER` aggregates.
- Paths remain single-line and use middle or trailing ellipsis according to the existing field behavior.

## Typography tokens

Token names are semantic. Each token resolves to an exact named egui font family registered at the stated variable-font weight and to a logical-point size.

| Token | Family | Weight | Nominal size | Use |
|---|---|---:|---:|---|
| `display.brand` | Unbounded | 700 | 16 pt | wordmark |
| `display.view` | Unbounded | 600 | 15 pt | major view title |
| `display.action` | Unbounded | 700 | 10 pt | Turbo and rare primary commands |
| `ui.title` | Golos Text | 600 | 16 pt | panel/dialog title |
| `ui.body` | Golos Text | 400 | 13 pt | normal UI copy |
| `ui.control` | Golos Text | 500 | 13 pt | controls/navigation |
| `tile.name.large` | Golos Text | 500 | 15 pt | large treemap tile name |
| `tile.name.compact` | Golos Text | 500 | 12 pt | compact treemap tile name |
| `data.normal` | JetBrains Mono | 500 | 12 pt | sizes and data |
| `data.compact` | JetBrains Mono | 500 | 10 pt | compact sizes/status |
| `data.micro` | JetBrains Mono | 500 | 9 pt | keyboard hints/eyebrows |

`pt` means egui logical points. Egui UI rectangles, galley sizes, and every rectangle passed to or returned from `voidspace-layout` use the same logical-point coordinate space. `pixels_per_point` affects rasterization only; callers must not pre-scale a `FontId`, galley extent, padding, or layout rectangle into physical pixels. A DPI change creates a new typography epoch because hinting/raster behavior can change, but it never causes a second coordinate conversion.

## Treemap integration

Typography is part of the layout contract, not a paint-only choice.

- The application loads, validates, and atomically registers the bundled families before computing any label footprint or treemap layout.
- The label-footprint measurement described in the treemap drill-down design uses Golos Text for name tiers and JetBrains Mono for all size tiers.
- `tile.name.large`, `tile.name.compact`, `data.normal`, and `data.compact` are the only font tokens used by treemap label measurement and rendering.
- Each frame pins one immutable scan snapshot revision and one immutable typography epoch. Live scan updates received during the frame become eligible only for the next frame.
- The app creates one `TreemapLabelPlan` for that pinned pair. It contains the exact normal and compact size strings, final ellipsized name strings, chosen tier, `FontId` values, shaped `Arc<Galley>` values, logical extents, padding/insets/stroke, and target node/aggregate identity. The layout footprint is derived from the plan's candidate size galleys. Painting reuses the plan's final galleys; it does not reshape, reformat, or re-ellipsize text.
- A `TreemapLabelPlan` is invalid if its snapshot revision, typography epoch, active size mode, or layout bounds differ from the current frame. Invalid plans are discarded before hit testing and painting.
- Every visible real or aggregate rectangle still renders a size. If a JetBrains Mono compact-size galley does not fit, the node is aggregated into `OTHER`; if the canvas itself is too small, the non-tile overflow state is used.
- Normal byte labels keep the existing spaced IEC/SI formatter, for example `120.9 GiB` and `1.30 TB`. Compact labels use the treemap compact formatter with no spaces, for example `0B`, `999M`, `1023M`, `1.0G`, and `1.7G`. Candidate measurement and final painting call the same formatter and reuse the resulting string from the plan.
- A missing or unreadable bundled font is a startup/configuration fault. Voidspace enters a clearly reported atomic fallback epoch and recomputes all label plans with fallback fonts before producing a treemap. It must never retain metrics from a missing or previous font epoch.

## Asset manifest, packaging, and licensing

The source is the official [`google/fonts`](https://github.com/google/fonts) repository at immutable commit `6a003b5eb672dc8bf5bff5937cf5863f8b175445`. The application embeds the upright variable TTFs directly with `include_bytes!`; it does not generate static instances. One source byte slice may be registered under several internal family names by cloning `FontData` and setting `FontTweak::coords` for the `wght` axis.

| Packaged source | Bytes | SHA-256 |
|---|---:|---|
| `ofl/unbounded/Unbounded[wght].ttf` | 778272 | `323b511be380c8d474ef030686b71aedde501f8d9cd46da558b7c40454372c3f` |
| `ofl/golostext/GolosText[wght].ttf` | 184292 | `17bb58fb69aec2dfb047a2ebf52534023e9b688c97a6b7ac795b0a72912c2063` |
| `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | 187208 | `48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda` |
| `ofl/unbounded/OFL.txt` | 4392 | `31e5d4e83955e7103c34570dd49b0570ef490800bd65b42923c0dd02445263b3` |
| `ofl/golostext/OFL.txt` | 4394 | `ff532f9e8789f09a9fdffc3c0954eedfb0a48be77b2e2eb90f5f82e4f347f50c` |
| `ofl/jetbrainsmono/OFL.txt` | 4399 | `b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d` |

Weight registrations are exact: Golos Text `400`, `500`, and `600`; Unbounded `500`, `600`, and `700`; JetBrains Mono `400`, `500`, and `600`. Each internal egui family name includes source family and weight, such as `voidspace.golos.500`. Registration sets only `wght`; every other variation axis remains at the font's declared default. Startup validation requires a `wght` axis whose range contains every registered value.

All three families use the SIL Open Font License 1.1. The portable archive and permanent installation include the three exact OFL files above under `licenses/fonts/<family>/OFL.txt` plus a generated `THIRD-PARTY-FONTS.txt` naming the family, source URL, pinned commit, embedded font hash, and license path. Font binaries are compiled into `voidspace.exe`; the selected families are never read from the system or an adjacent runtime file.

The atomic fallback uses the fonts already embedded by Cargo dependency `epaint_default_fonts = 0.36.1`, locked by `Cargo.lock`. Their redistribution manifest is also part of the release contract:

| Embedded fallback source | Bytes | SHA-256 | Installed notice |
|---|---:|---|---|
| `fonts/Hack-Regular.ttf` | 309408 | `15f55cc0c85a2988d2b4b3a8cdb5d77fdfbaf319e1bb5309d725db9818fb7125` | `licenses/fonts/hack/Hack-Regular.txt` |
| `fonts/Hack-Regular.txt` | 3734 | `47c0cccbeec7e8614548cc485588b28149e7874188df5f41b36efebcee285c87` | same file |
| `fonts/Ubuntu-Light.ttf` | 361676 | `80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70` | `licenses/fonts/ubuntu/UFL.txt` |
| `fonts/UFL.txt` | 4673 | `2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002` | same file |

`THIRD-PARTY-FONTS.txt` also names Hack and Ubuntu Light, attributes `epaint_default_fonts 0.36.1` as their packaged source, records the embedded font hashes, and points to their installed notices. A future dependency update that changes any fallback asset or license hash must deliberately update this manifest and the packaging checks.

The release package, permanent local installation, and Desktop shortcut target all use the same executable and license set; no runtime network request is permitted. Fonts are registered once during app initialization and shared through egui's font definitions. No frame-level file reads or decoding are allowed.

## Failure behavior

- A build treats a missing source file or manifest/hash mismatch as a build or packaging failure.
- Before the first layout, initialization verifies each embedded SHA-256, parses each face, verifies the required `wght` range, and verifies nonzero glyph IDs for ASCII, digits, and the required Cyrillic sentinel strings `КАРТА ДИСКА`, `Свободно`, and `Удалить навсегда` at every registered weight.
- Any corrupt, unparseable, wrong-axis, wrong-hash, or glyph-incomplete selected asset activates one complete fallback epoch before the first layout. Fallback uses egui's own embedded default proportional and monospace fonts for all roles; it never depends on Segoe UI or another system font. Display, UI, and data roles switch together, a visible diagnostic names the failed validation, and all selected-font plans/caches are absent.
- Fallback activation is atomic across all typography tokens. Mixing selected and fallback metrics in one treemap layout or frame is forbidden.
- When DPI or the app-wide pixels-per-point value changes, cached label footprints are invalidated and layout is recomputed before repainting tiles at the new scale.

## Accessibility and localization

- All three chosen families include Cyrillic coverage required by the current Russian UI.
- Text color and state color remain separate concerns. Selection, pinning, scan status, and destructive actions are not communicated by font family alone.
- Unbounded is never used for long warnings or destructive-action explanations; Golos Text carries those messages.
- The existing tooltip and keyboard-navigation requirements remain unchanged.
- At 100%, 125%, 150%, and 200% Windows scaling, text may ellipsize or aggregate but may not overlap, clip outside its tile, or become smaller than its assigned token.

## Components and boundaries

### `voidspace-app::theme`

- Owns embedded asset/hash/axis/glyph validation, weighted variable-font registration, family ordering, semantic font tokens, and fallback activation.
- Exposes selected `FontId` values to UI components without requiring callers to know resource paths.
- Produces an immutable typography configuration with a monotonically increasing epoch identifier.

### `voidspace-app::treemap`

- Consumes semantic tile-name and data tokens for measurement and painting.
- Does not choose font families or synthesize its own font sizes.
- Owns `TreemapLabelPlan` construction for one pinned snapshot/typography epoch and reuses its shaped galleys for painting.
- Invalidates its label plan when the snapshot revision, typography/DPI epoch, size mode, or layout bounds change.

### Packaging scripts

- Verify all selected and fallback manifest hashes before build/package, verify the selected and fallback font hashes are reported as embedded by the built executable, and verify the three selected-family OFL files, Hack notice, Ubuntu notice, and `THIRD-PARTY-FONTS.txt` beside both the staged and permanently installed executable.
- Preserve the existing stable local installation and Desktop shortcut behavior.

## Verification

### Automated tests

- Theme initialization resolves each semantic token to its required family and weight.
- Manifest tests verify the pinned commit, filename, byte count, SHA-256, parseability, required weight range, and required glyph IDs for every source asset.
- Required Cyrillic sample strings produce non-empty galleys without replacement glyphs: `КАРТА ДИСКА`, `Свободно`, `Удалить навсегда`.
- Normal formatter samples include `120.9 GiB` and `1.30 TB`. Compact formatter samples include the exact no-space strings `0B`, `999M`, `1023M`, `1.0G`, and `1.7G`, including the intermediate suffix-width boundary from the treemap specification.
- Treemap footprint tests use the registered production fonts and confirm that every emitted tile fits the exact reused size galley in its `TreemapLabelPlan`.
- A mid-frame live-scan update cannot change strings, galleys, snapshot identity, or hit targets in the active plan; the next frame receives a new plan.
- Changing the DPI epoch invalidates cached footprints and produces a fresh layout.
- Forced validation failures cover hash, parse, axis range, and missing glyphs; each activates the complete embedded fallback epoch before layout and never uses selected-font metrics.
- Release packaging checks the exact installed selected-font and fallback-font license tree plus `THIRD-PARTY-FONTS.txt`, not only the staging manifest.

### Manual release verification

- Compare the packaged app with the canonical approved image at 100%, 125%, 150%, and 200% Windows scaling, excluding the image's lower-right companion approval control.
- Verify Russian and English text in drive picker, treemap, breadcrumb, inspector, delete confirmation, and status bar.
- Verify every visible rectangle shows a JetBrains Mono size without overlap.
- Verify long Cyrillic and Latin folder names ellipsize cleanly.
- In a clean Windows Sandbox/VM where Unbounded, Golos Text, and JetBrains Mono are not installed system-wide, disconnect the network and launch the permanent installation through `Voidspace.lnk`. The app diagnostics must report `typography_source=embedded`, the pinned Google Fonts commit, and the three expected embedded font hashes.
- Resolve the shortcut target, verify its executable SHA-256 equals the staged executable, and verify all three selected-family OFL files, Hack notice, Ubuntu notice, and `THIRD-PARTY-FONTS.txt` at the target installation directory against the manifest.

## Acceptance criteria

- The packaged application visibly matches the canonical option 10 reference artifact.
- Unbounded is distinctive but limited to short display/action roles.
- Golos Text remains readable for dense Cyrillic UI and tile names.
- JetBrains Mono is used consistently for sizes, paths, counts, and statuses.
- Font metrics cannot cause unlabeled or overlapping treemap rectangles.
- The application renders the selected typography offline from the permanent Desktop shortcut installation.
- Required font licenses and notices ship with the release.
