# Voidspace Tactical Arc — Reactor Beat Hover Design

Date: 2026-08-28  
Status: selected by user (`03 / REACTOR BEAT`)

## 1. Scope

Replace the Tactical Arc's translucent hover treatment with an opaque, saturated active sector and a compact two-beat pulse on the text inside that sector.

This specification changes presentation and repaint scheduling only. Hit testing, keyboard selection, action dispatch, Recycle behavior, and permanent-delete confirmation remain unchanged.

## 2. Reference lock

Selected visual prototype: `.superpowers/brainstorm/64926-1787865841/hover-effects.html`, card `03 / REACTOR BEAT`.

Preserve:

- The existing semantic colors: cyan for Explorer, acid lime for Recycle, magenta for permanent deletion.
- The near-black Tactical Arc hub, thin technical geometry, pointer ray, compact machine typography, and current sector dimensions.
- Instant selection feedback and crisp readable labels.

Change:

- A hovered sector uses a fully opaque, high-saturation semantic fill instead of the current alpha-44 translucent fill.
- Its border uses a bright inner stroke plus a restrained outer bloom in the same semantic color.
- The in-sector shortcut and short action label pulse together with a two-beat `Reactor Beat` rhythm.

Reject:

- Transparency on the active sector.
- Geometry wobble, rotation, spring motion, blur fog, hue cycling, or movement of the sector itself.
- Continuous repaint when no pointer-hovered sector exists.

## 3. Hover behavior

Pointer entry into a real sector starts a local 1.35-second animation cycle at phase zero. Moving directly to another sector restarts the cycle for the newly hovered action. Leaving all sectors stops the animation and clears its local start time.

The cycle has two quick peaks followed by a rest:

1. First peak near 12% of the cycle.
2. Return to baseline near 22%.
3. Second peak near 34%.
4. Return to baseline near 45%.
5. Stable rest through the end of the cycle.

At each peak:

- Label scale rises smoothly from `1.0` to approximately `1.095`.
- The sector's outer bloom strengthens without changing its bounds or hit target.
- The label remains high-contrast against every semantic fill.

The animation repeats only while the pointer remains over that sector. Keyboard selection remains fully opaque and bright but static unless the pointer also hovers the selected sector; the user specifically requested hover motion.

## 4. Rendering

Add a small deterministic styling layer inside `tactical_arc`:

- A pure `reactor_beat(phase)` function returns a normalized intensity in `[0, 1]`.
- A pure active-sector style helper derives fill, inner stroke, outer bloom, label color, and label scale from semantic color and beat intensity.
- `TacticalArcState` stores the currently hovered action and the time when that hover began.
- The existing sector mesh stays unchanged. Only its vertex color changes for the active style.
- The bloom is simulated with layered vector strokes; no blur pass or new renderer dependency is introduced.
- The label is repainted at the same center with a scaled font. Its anchor never moves.

The active fill must have alpha `255`. Inactive sectors retain their current near-black opaque fill. A keyboard-only selected sector uses the same opaque active fill and bright border at baseline intensity.

## 5. Repaint and performance

While a pointer-hovered sector exists, request the next repaint after approximately 16 ms. Do not request autonomous repaints when the pointer is in the hub, a sector gap, outside the fan, or when only a keyboard selection exists.

No heap allocation, texture upload, shadow texture, or new dependency is allowed per animation frame. Geometry remains deterministic and bounded by the existing Tactical Arc clip.

## 6. Accessibility and safety

- The pulse supplements existing color, text, and focus cues; it is never the only indication of selection.
- Accessible names, numeric shortcuts, focus order, and hit targets remain unchanged.
- Recycle still executes immediately.
- Permanent deletion still enters the existing confirmation flow.
- If timed animation cannot advance, the static fallback is the fully opaque bright sector with a readable label.

## 7. Verification

Focused tests cover:

- `reactor_beat` baseline, both peaks, rest interval, periodicity, and `[0, 1]` bounds.
- Active-sector fill alpha is always `255` for all semantic colors and animation phases.
- Label scale stays between `1.0` and `1.095`.
- Keyboard-only selection stays static.
- Autonomous repaint is requested only for pointer hover.
- Existing rendered-frame Tactical Arc labels remain inside the foreground clip.

Release verification runs formatting, Clippy with warnings denied, the full workspace tests, release packaging, local installation, desktop-shortcut refresh, and launch of the installed build.

## 8. Acceptance criteria

- Hovered sectors never become transparent.
- The semantic color is visibly bright and saturated.
- The in-sector word and shortcut perform the selected double-beat pulse without shifting their anchor.
- Sector bounds, selection, and file-operation behavior do not change.
- Idle Tactical Arc rendering does not acquire a permanent animation loop.
- The installed current build opens successfully after verification.

This document supersedes the low-opacity selected-sector fill and no-autonomous-animation statements in sections 3, 7, and 9 of `2026-08-27-voidspace-tactical-arc-design.md` only. All other requirements of the earlier Tactical Arc specification remain authoritative.
