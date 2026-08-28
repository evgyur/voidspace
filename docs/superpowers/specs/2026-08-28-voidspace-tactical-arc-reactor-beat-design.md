# Voidspace Tactical Arc — Reactor Beat Hover Design

Date: 2026-08-28  
Status: selected by user (`03 / REACTOR BEAT` + `01 / SIDE TARGET PLATE`)

## 1. Scope

Replace the Tactical Arc's translucent hover treatment with an opaque, saturated active sector and a compact two-beat pulse on the text inside that sector. When the menu opens, dim the rest of the application and move object identity out of the small hub into a large edge-aware side plate.

This specification changes presentation, foreground composition, geometry containment, and repaint scheduling only. Hit testing, keyboard selection, action dispatch, Recycle behavior, and permanent-delete confirmation remain unchanged.

## 2. Reference lock

Selected visual prototypes:

- `.superpowers/brainstorm/64926-1787865841/hover-effects.html`, card `03 / REACTOR BEAT`.
- `.superpowers/brainstorm/64926-1787865841/modal-target-options.html`, card `01 / SIDE TARGET PLATE`.

Preserve:

- The existing semantic colors: cyan for Explorer, acid lime for Recycle, magenta for permanent deletion.
- The near-black Tactical Arc hub, thin technical geometry, pointer ray, compact machine typography, and current sector dimensions.
- Instant selection feedback and crisp readable labels.

Change:

- A hovered sector uses its exact fully opaque semantic token instead of the current alpha-44 translucent fill: cyan `#1ECDE2`, lime `#BDFF3E`, or magenta `#FF49BC`.
- Its border uses a bright inner stroke plus a restrained outer bloom in the same semantic color.
- The in-sector shortcut and short action label pulse together with a two-beat `Reactor Beat` rhythm.
- Active label ink is fixed at near-black `#07090A`; its WCAG contrast ratios against the three active fills are respectively `10.34:1`, `16.66:1`, and `6.59:1`. The ratio must remain at least `4.5:1` at baseline and both peaks.
- A modal black scrim dims the complete application content viewport behind the Tactical Arc by 72%.
- Object size, name, and absolute path move from the hub to a large side plate opposite the fan.
- The hub replaces its small object text with the static two-line cue `TARGET / LOCKED`.

Reject:

- Transparency on the active sector.
- Geometry wobble, rotation, spring motion, blur fog, hue cycling, or movement of the sector itself.
- Continuous repaint when no pointer-hovered sector exists.
- Background blur, transparency on the target plate, or allowing the plate to obscure the sector fan.

## 3. Modal focus layer

Opening Tactical Arc paints a single black `#000000` scrim at alpha `184` (72.16%) over the entire egui content viewport, including the top bar, tabs, treemap, inspector, and status bar. The operating-system title bar is outside this viewport and is not part of the contract.

Render order is fixed:

1. Normal application UI.
2. Full-viewport dimming scrim and its existing input shield.
3. Pointer-origin tether.
4. Tactical Arc fan and hub.
5. Side Target Plate.

The scrim has no blur, gradient, texture, animation, or pointer behavior of its own. It is removed in the same frame that the Tactical Arc closes. The existing outside-click, Escape, repeated-right-click, and target-invalidation dismissal behavior remains authoritative. No background control may receive pointer or keyboard input while the scrim is visible.

## 4. Side Target Plate

Replace the existing 152×44-point detail rail with one edge-aware target plate opposite the sector fan. At scale `1.0`, the plate is 280×94 logical points with a 12-point gap from the fan. At compact scale `0.75`, all plate dimensions, padding, font sizes, and gap scale uniformly.

The plate contains, in order:

1. `OBJECT / ACTION TARGET` — 8-point orange machine label.
2. Object size — 13-point semantic accent, followed by object name — 20-point white display text on the same baseline.
3. Absolute path — 9-point muted text, single line with middle ellipsis when necessary; hover exposes the complete path in a tooltip.
4. Current full action label — 9-point semantic color below a separator, for example `DELETE WITHOUT RECOVERY`.

The target name is the dominant line. It may truncate with a terminal ellipsis only after reserving space for the complete size. The plate background is opaque `#0B0F11`; border is `#3A4449`; no content relies on the dimmed treemap for identification.

The plate follows the existing fan orientation: right-facing fan places the plate to the left, left-facing fan places it to the right. Geometry fitting includes the plate, gap, maximum 5-point bloom, and 8-point safe margin. If full scale does not fit, the complete fan-plus-plate composition retries at scale `0.75`; if that also cannot fit, opening fails closed through the existing window-too-small behavior. The plate never overlaps the fan, leaves the work area, or changes sector hit bounds.

The plate always describes the captured `ContextTarget`, while its bottom row follows pointer visual precedence and falls back to the keyboard-selected action or `SELECT COMMAND`. The permanent-delete row is informational only; executing it still uses the existing confirmation flow.

## 5. Hover behavior

Pointer entry into a real sector starts a local 1.35-second animation cycle at phase zero. Moving directly to another sector restarts the cycle for the newly hovered action. Leaving all sectors stops the animation and clears its local start time.

For elapsed time `t >= 0`, normalized phase is `p = (t / 1.35).rem_euclid(1.0)`. Intensity is defined by these exact keyframes:

1. `(0.00, 0.0)`
2. `(0.12, 1.0)`
3. `(0.22, 0.0)`
4. `(0.34, 1.0)`
5. `(0.45, 0.0)`
6. `(1.00, 0.0)`

Within each non-constant segment, normalize the segment position to `x` in `[0, 1]` and interpolate endpoint intensity with cubic smoothstep `s(x) = 3x² - 2x³`. Intensity is exactly zero from phase `0.45` through the wrap at `1.0`. Unit comparisons use tolerance `1e-6`.

At each peak:

- Label scale is exactly `1.0 + 0.095 × intensity`, producing a maximum of `1.095`.
- The sector's outer bloom strengthens without changing its bounds or hit target.
- The label remains high-contrast against every semantic fill.

The animation repeats only while the pointer remains over that sector. Pointer hover temporarily owns the visual highlight: if it differs from `keyboard_index`, only the hovered sector is painted active and pulsing. The keyboard selection remains the logical `Enter` target and reappears as a static opaque highlight immediately after pointer hover exits. Pointer hover does not mutate `keyboard_index`, so Enter behavior remains unchanged and two sectors are never simultaneously active.

## 6. Rendering

Add a small deterministic styling layer inside `tactical_arc`:

- A pure `reactor_beat(phase)` function returns a normalized intensity in `[0, 1]`.
- A pure active-sector style helper derives fill, inner stroke, outer bloom, label color, and label scale from semantic color and beat intensity. It returns a small `Copy` value with no owned strings, vectors, meshes, or galleys.
- `TacticalArcState` stores the currently hovered action and the time when that hover began.
- The existing sector mesh stays unchanged. Only its vertex color changes for the active style.
- The bloom is simulated with layered vector strokes; no blur pass or new renderer dependency is introduced.
- The label is repainted at the same center with a scaled font. Its anchor never moves.
- The dimming scrim is painted once per menu frame as a single filled rectangle beneath the menu.
- Side Target Plate layout is derived from the same `TacticalArcGeometry` orientation and scale used by bounds fitting, painting, and containment tests.

The active fill must have alpha `255`. Inactive sectors retain their current near-black opaque fill. A keyboard-only selected sector uses the same opaque active fill and bright border at baseline intensity. The outer bloom is a layered vector stroke no wider than 5 logical points at peak, remains inside the existing 8-point safe margin, and is clipped to the Tactical Arc foreground area.

## 7. Repaint and performance

While a pointer-hovered sector exists and full motion is enabled, request the next repaint after 16 ms. Do not request autonomous repaints when the pointer is in the hub, a sector gap, outside the fan, when only a keyboard selection exists, or when reduced motion is active.

The Reactor Beat styling path must introduce no new per-frame heap allocation: the pulse and style helpers operate only on `Copy` scalar/color values and reuse the renderer's existing sector points, mesh, and label string. Existing Tactical Arc paint allocations are outside this slice and are not claimed to be eliminated. No texture upload, shadow texture, or new rendering dependency is allowed. Geometry remains deterministic and bounded by the existing Tactical Arc clip. The existing frame benchmark remains the release performance gate; its current 600-frame median and p95 limits may not regress.

## 8. Accessibility and safety

- The pulse supplements existing color, text, and focus cues; it is never the only indication of selection.
- Accessible names, numeric shortcuts, focus order, and hit targets remain unchanged.
- The menu's accessible summary includes target size, target name, full path, and the current full action label. The scrim is decorative and is excluded from the accessibility tree.
- On Windows, motion preference is resolved from `SPI_GETCLIENTAREAANIMATION` when the Tactical Arc opens. A disabled setting or a failed query selects reduced motion. Reduced motion keeps the static fully opaque active fill, dark readable label, and bright baseline border, but fixes intensity at zero and schedules no animation repaint.
- Recycle still executes immediately.
- Permanent deletion still enters the existing confirmation flow.
- If timed animation cannot advance, the static fallback is the fully opaque bright sector with a readable label.

## 9. Verification

Focused tests cover:

- `reactor_beat` exact keyframes, smoothstep interpolation samples, phase wrap, periodicity, and `[0, 1]` bounds within tolerance `1e-6`.
- Active-sector fill alpha is always `255`, and near-black label contrast remains at least `4.5:1`, for cyan, lime, and magenta at baseline and both peaks.
- Label scale stays between `1.0` and `1.095`.
- Pointer hover temporarily replaces the keyboard visual highlight without mutating the logical Enter target; the keyboard highlight returns on hover exit.
- Keyboard-only selection and reduced-motion hover stay static.
- Autonomous repaint is requested only for pointer hover with full motion enabled.
- Existing rendered-frame Tactical Arc labels remain inside the foreground clip.
- Rendered baseline and both peak frames cover all three actions/colors, both fan orientations, and geometry scales `1.0` and `0.75`. They assert that scaled label bounds remain inside the corresponding sector and foreground clip, the maximum 5-point bloom remains inside the work area and does not paint across sector gaps, and hit-test results are identical to the static geometry.
- Modal render tests assert one alpha-184 full-viewport black scrim below all Tactical Arc shapes and no scrim after dismissal.
- Side Target Plate geometry tests cover both orientations, both scales, all four work-area edges, long Unicode names, long absolute paths, and the minimum supported viewport. The full composition stays inside the work area without overlapping the fan.
- Rendered plate tests assert that size remains complete, name is the dominant text, path middle-ellipsizes, full action label follows pointer precedence, and the hub contains `TARGET` plus `LOCKED` instead of the old small size/name.

Release verification runs formatting, Clippy with warnings denied, the full workspace tests, release packaging, local installation, desktop-shortcut refresh, and launch of the installed build.

## 10. Acceptance criteria

- Hovered sectors never become transparent.
- The semantic color is visibly bright and saturated.
- The in-sector word and shortcut perform the selected double-beat pulse without shifting their anchor.
- The entire application content behind the open menu is uniformly dimmed by the selected 72% scrim, while the menu and target plate remain undimmed.
- The selected object's size, name, path, and current full action are clearly readable in the large side plate; object identity is no longer squeezed into the hub.
- The plate automatically mirrors with fan orientation and never overlaps or leaves the work area.
- Sector bounds, selection, and file-operation behavior do not change.
- Idle Tactical Arc rendering does not acquire a permanent animation loop.
- The installed current build opens successfully after verification.

This document supersedes the low-opacity selected-sector fill, 152×44 detail rail, hub object text, foreground composition, bounds width, and no-autonomous-animation statements in sections 3, 4, 7, and 9 of `2026-08-27-voidspace-tactical-arc-design.md` only. All other requirements of the earlier Tactical Arc specification remain authoritative.
