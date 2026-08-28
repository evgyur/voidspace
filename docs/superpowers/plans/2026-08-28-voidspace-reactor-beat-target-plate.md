# Voidspace Reactor Beat Target Plate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox notation so verification state stays explicit.

**Goal:** Ship the approved Tactical Arc variant with a modal viewport dimmer, saturated Reactor Beat hover treatment, and a readable side target plate while preserving the existing immediate recycle and confirmed permanent-delete behavior.

**Architecture:** Keep Tactical Arc as a self-contained egui overlay. Render the dimmer, tether, fan, hub, and target plate in one full-content foreground area so paint order and pointer shielding are deterministic. Extract pulse, color, geometry, and grapheme-safe fitting into pure helpers, then cover those helpers plus rendered shape bounds and input semantics with focused tests.

**Tech Stack:** Rust 2024, egui/eframe 0.32, Windows `SystemParametersInfoW`, `unicode-segmentation`, cargo test/clippy/fmt, PowerShell packaging, GitHub CLI.

---

## Task 1: Add deterministic motion, color, and text-fitting primitives

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/voidspace-app/Cargo.toml`
- Modify: `crates/voidspace-app/src/tactical_arc.rs`
- Test: `crates/voidspace-app/src/tactical_arc.rs`

- [ ] Add `unicode-segmentation = "1.13"` to workspace dependencies and consume it from `voidspace-app`.
- [ ] Enable `Win32_UI_WindowsAndMessaging` (and `Win32_Foundation` if required by generated bindings) for the app's Windows dependency.
- [ ] Introduce exact active HUD tokens `#1ECDE2`, `#BDFF3E`, and `#FF49BC`, all with alpha 255, plus dark label ink `#07090A`.
- [ ] Implement a pure 1.35-second Reactor Beat function with smoothstep interpolation through these normalized keyframes: `(0,.0)`, `(.12,1)`, `(.22,0)`, `(.34,1)`, `(.45,0)`, `(1,0)`.
- [ ] Derive label scale as `1 + .095 * intensity`; keep bloom width at or below `5 * geometry.scale`.
- [ ] Add pure grapheme-aware end-ellipsis and middle-ellipsis helpers. The name must retain at least four leading grapheme clusters where geometry allows; the path must preserve the root and final component.
- [ ] Add tests for exact keyframes, interpolation bounds, rest phase, loop boundary, label scale, active alpha, contrast ratios, and Unicode-safe truncation.

Verification:

```powershell
cargo test -p voidspace-app tactical_arc --lib
```

## Task 2: Replace the compact rail with the approved side target plate

**Files:**

- Modify: `crates/voidspace-app/src/tactical_arc.rs`
- Test: `crates/voidspace-app/src/tactical_arc.rs`

- [ ] Replace the 152×44 action rail with a 280×94 plate at scale 1.0, gap 12, centered vertically on the hub and placed opposite the fan.
- [ ] Scale the plate and fan together down to a floor of 0.75 and retain an unscaled 8-pixel safety margin.
- [ ] Implement the exact internal bands: tag `10..20`, identity `24..48`, path `50..62`, separator at `68`, action `74..84`, with 12-pixel horizontal insets.
- [ ] Render large size and filename/folder name in the identity band, full fitted path below, and the current command in the action band. Use white neutral command styling until an action is selected, then apply semantic color.
- [ ] Reduce hub copy to `TARGET` and `LOCKED` only.
- [ ] Give the plate a stable focusable widget identity and `WidgetInfo::labeled(WidgetType::Window, ...)` description containing full size, full name, full path, and logical action. Show the same untruncated identity in a hover/focus tooltip.
- [ ] Keep a primary click on the plate neutral: it dismisses the arc and never executes a command.
- [ ] Add geometry tests for both orientations, 1.0 and 0.75 scale, safe containment, plate bands, and side placement.

Verification:

```powershell
cargo test -p voidspace-app tactical_arc --lib
```

## Task 3: Make Tactical Arc a stable modal overlay with Reactor Beat hover

**Files:**

- Modify: `crates/voidspace-app/src/tactical_arc.rs`
- Modify if needed: `crates/voidspace-app/src/app.rs`
- Test: `crates/voidspace-app/src/tactical_arc.rs`
- Test if needed: `crates/voidspace-app/src/app.rs`

- [ ] Render one black alpha-184 scrim over `Context::content_rect()` before all Tactical Arc foreground shapes.
- [ ] Use one full-content foreground `Area` as the pointer shield; paint scrim first, tether/fan next, and target plate last so no underlying widget can receive a modal click.
- [ ] Track pointer hover separately from keyboard selection. Pointer hover visually owns the single active sector; keyboard selection stays the logical Enter target and becomes visible again when pointer leaves.
- [ ] Restart the beat only when hovered action changes. Repaint after 16 ms only while a pointer sector is hovered and Windows client-area animations are enabled.
- [ ] Query `SPI_GETCLIENTAREAANIMATION` when the arc opens. A disabled setting or query failure produces the same saturated static treatment with no animation repaint loop.
- [ ] On hover, keep the active sector fully opaque and brighten it with bounded layered strokes; pulse only the shortcut/word label's scale and glow.
- [ ] Preserve second-right-click/Escape/outside-primary dismissal, keyboard arrows/number shortcuts/Enter activation, immediate recycle, and existing permanent-delete confirmation.
- [ ] Add tests for pointer-vs-keyboard precedence, hover restart, reduced-motion behavior, scrim order and bounds, a single active sector, neutral plate click, outside dismissal, and rendered shape containment.

Verification:

```powershell
cargo test -p voidspace-app
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Task 4: Version, package, install, and verify v0.1.2

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `scripts/package.ps1`
- Modify if generated by the existing packaging path: release artifacts under `dist/`

- [ ] Bump workspace/app version and package name from 0.1.1 to 0.1.2.
- [ ] Run the repository's full release verification and optimized build.
- [ ] Package the executable using `scripts/package.ps1` and verify the produced EXE exists and reports a non-zero size/hash.
- [ ] Install/copy the verified build to the stable application location used by the desktop shortcut.
- [ ] Recreate or refresh the desktop shortcut so it always resolves to the stable current executable and retains the approved icon.
- [ ] Launch the installed executable and capture a fresh screenshot showing the modal Tactical Arc and target plate for visual verification.

Verification:

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --workspace
powershell -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

## Task 5: Ship source and binary release

**Files:**

- Commit all intended source/spec/plan changes; do not add the user's untracked `artifacts/` directory.

- [ ] Review `git diff --check`, `git diff --stat`, and `git status --short` for accidental files.
- [ ] Commit the implementation with verification evidence in the handoff.
- [ ] Push `master` to `origin`.
- [ ] Create and push tag `v0.1.2`.
- [ ] Create GitHub Release `v0.1.2` and upload the packaged Windows EXE/archive.
- [ ] Verify the release URL and attached asset through GitHub CLI/API.
- [ ] Open the installed application for the user's final check.

Final evidence must include: exact test/build commands and outcomes, packaged artifact path and SHA-256, installed executable/shortcut paths, commit/tag, GitHub release URL, and a clickable link to this canonical plan file.
