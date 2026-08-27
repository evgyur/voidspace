# Voidspace Full Tactical HUD Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Voidspace's current chrome with the approved Full Tactical HUD and Tactical Arc while preserving scanning, navigation, file-operation safety, stable live views, and release performance.

**Architecture:** Keep scanner/index/layout crates unchanged and make the redesign an app-crate presentation layer. Split reusable HUD primitives, overlay coordination, status layout, volume display identity, Tactical Arc state, and shell panels into focused Rust modules; `VoidspaceApp` remains the owner of application data and executes typed UI intents after rendering.

**Tech Stack:** Rust 1.98, eframe/egui 0.36, wgpu, existing embedded JetBrains Mono typography, cargo test/clippy, PowerShell packaging and installer scripts.

---

## File map

- Create `crates/voidspace-app/src/hud.rs`: Full HUD tokens, cut-corner frames, state squares, corner brackets, reticle, and instrument cells.
- Create `crates/voidspace-app/src/volume_display_registry.rs`: stable session-local `PathBuf -> VOL:##` identity.
- Create `crates/voidspace-app/src/overlay_coordinator.rs`: modal/transient/toast precedence, exclusivity, focus restoration, and Escape routing.
- Create `crates/voidspace-app/src/status_bar.rs`: priority-based status modules, `MORE +N`, and Status Details.
- Create `crates/voidspace-app/src/tactical_arc.rs`: radial geometry, hover/click hit testing, keyboard navigation, edge clamping, and action intents.
- Create `crates/voidspace-app/src/shell.rs`: top command bar, volume tabs, breadcrumb, disk picker, inspector, About, and responsive composition.
- Modify `crates/voidspace-app/src/treemap.rs`: static grid, cut-corner tile paint, size-first labels, selection brackets/reticle, and Tactical Arc target capture.
- Modify `crates/voidspace-app/src/app.rs`: state ownership, typed intent execution, overlay preprocessing, file-operation revalidation, and panel orchestration.
- Modify `crates/voidspace-app/src/lib.rs`: module declarations and test-visible exports.
- Modify `crates/voidspace-app/tests/ui_state.rs`: cross-module interaction and responsive-state coverage.
- Create `crates/voidspace-app/tests/full_tactical_hud.rs`: visual-state, overlay, accessibility, and performance-contract tests.
- Modify `scripts/smoke.ps1`: launch/idle diagnostics and installed-release checks.

### Task 1: HUD tokens and reusable instrument primitives

**Files:**
- Create: `crates/voidspace-app/src/hud.rs`
- Modify: `crates/voidspace-app/src/theme.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Test: `crates/voidspace-app/src/hud.rs`

- [ ] **Step 1: Write failing token and geometry tests**

```rust
#[test]
fn cut_corner_points_stay_inside_source_rect() {
    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 60.0));
    for point in cut_corner_points(rect, 8.0) {
        assert!(rect.contains(point));
    }
}

#[test]
fn semantic_states_have_text_cues() {
    assert_eq!(HudState::Active.label(), "ACTIVE");
    assert_eq!(HudState::Warning.label(), "WARNING");
    assert_eq!(HudState::Danger.label(), "DANGER");
}
```

- [ ] **Step 2: Verify the tests fail before implementation**

Run: `cargo test -p voidspace-app hud::tests -- --nocapture`
Expected: compilation fails because `hud` and its primitives do not exist.

- [ ] **Step 3: Implement the exact primitive API**

```rust
pub const GRID: egui::Color32 = egui::Color32::from_rgb(31, 36, 38);
pub const CYAN: egui::Color32 = egui::Color32::from_rgb(30, 205, 226);
pub const LIME: egui::Color32 = egui::Color32::from_rgb(189, 255, 62);
pub const ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 83, 43);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudState { Neutral, Active, Warning, Danger }

impl HudState {
    pub const fn label(self) -> &'static str {
        match self { Self::Neutral => "IDLE", Self::Active => "ACTIVE", Self::Warning => "WARNING", Self::Danger => "DANGER" }
    }
}

pub fn cut_corner_points(rect: egui::Rect, cut: f32) -> [egui::Pos2; 6] {
    let cut = cut.clamp(0.0, rect.width().min(rect.height()) * 0.25);
    [rect.left_top(), egui::pos2(rect.right() - cut, rect.top()), egui::pos2(rect.right(), rect.top() + cut), rect.right_bottom(), egui::pos2(rect.left() + cut, rect.bottom()), rect.left_bottom()]
}
```

Add painter helpers `paint_cut_frame`, `paint_state_square`, `paint_corner_brackets`, `paint_reticle`, and `instrument_cell`; every helper accepts explicit bounds and clips within them. Add HUD typography tokens in `theme.rs` using the already embedded JetBrains Mono assets.

- [ ] **Step 4: Run focused and complete app tests**

Run: `cargo test -p voidspace-app hud::tests && cargo test -p voidspace-app`
Expected: all tests pass.

- [ ] **Step 5: Commit the primitive layer**

Run: `git add crates/voidspace-app/src/hud.rs crates/voidspace-app/src/theme.rs crates/voidspace-app/src/lib.rs && git commit -m "feat: add tactical hud primitives"`

### Task 2: Stable volume identity registry

**Files:**
- Create: `crates/voidspace-app/src/volume_display_registry.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Test: `crates/voidspace-app/src/volume_display_registry.rs`

- [ ] **Step 1: Write failing stability tests**

```rust
#[test]
fn ids_are_stable_across_refresh_order() {
    let mut registry = VolumeDisplayRegistry::default();
    assert_eq!(registry.label_for(Path::new("C:\\")), "VOL:01");
    assert_eq!(registry.label_for(Path::new("F:\\")), "VOL:02");
    assert_eq!(registry.label_for(Path::new("C:\\")), "VOL:01");
}

#[test]
fn normalization_is_case_insensitive_for_windows_roots() {
    let mut registry = VolumeDisplayRegistry::default();
    assert_eq!(registry.label_for(Path::new("c:\\")), registry.label_for(Path::new("C:\\")));
}
```

- [ ] **Step 2: Verify the registry tests fail**

Run: `cargo test -p voidspace-app volume_display_registry::tests`
Expected: compilation fails because `VolumeDisplayRegistry` is undefined.

- [ ] **Step 3: Implement session-local allocation**

```rust
#[derive(Default)]
pub struct VolumeDisplayRegistry {
    labels: std::collections::BTreeMap<String, u16>,
    next: u16,
}

impl VolumeDisplayRegistry {
    pub fn label_for(&mut self, root: &std::path::Path) -> String {
        let key = root.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_uppercase();
        let id = *self.labels.entry(key).or_insert_with(|| { self.next += 1; self.next });
        format!("VOL:{id:02}")
    }
}
```

- [ ] **Step 4: Run registry and volume-switcher tests**

Run: `cargo test -p voidspace-app volume_display_registry::tests && cargo test -p voidspace-app --test volume_switcher`
Expected: all tests pass.

- [ ] **Step 5: Commit stable display identities**

Run: `git add crates/voidspace-app/src/volume_display_registry.rs crates/voidspace-app/src/lib.rs && git commit -m "feat: add stable volume display ids"`

### Task 3: Overlay coordinator and responsive status system

**Files:**
- Create: `crates/voidspace-app/src/overlay_coordinator.rs`
- Create: `crates/voidspace-app/src/status_bar.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Test: `crates/voidspace-app/tests/full_tactical_hud.rs`

- [ ] **Step 1: Write failing precedence, collapse, and focus tests**

```rust
#[test]
fn modal_replaces_transient_and_transient_is_exclusive() {
    let mut overlays = OverlayCoordinator::default();
    overlays.open_transient(TransientOverlay::DiskPicker);
    overlays.open_transient(TransientOverlay::StatusDetails);
    assert_eq!(overlays.transient(), Some(&TransientOverlay::StatusDetails));
    overlays.open_modal(ModalOverlay::PermanentDelete);
    assert!(overlays.transient().is_none());
}

#[test]
fn status_keeps_scan_and_engine_then_collapses_low_priority_items() {
    let layout = layout_modules(420.0, &StatusSnapshot::fixture_full());
    assert!(layout.visible.iter().any(|m| m.kind == StatusKind::Scan));
    assert!(layout.visible.iter().any(|m| m.kind == StatusKind::Engine));
    assert!(!layout.hidden.is_empty());
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p voidspace-app --test full_tactical_hud overlay status`
Expected: compilation fails because coordinator and status types do not exist.

- [ ] **Step 3: Implement typed overlay and status APIs**

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransientOverlay { TacticalArc(ContextTarget), DiskPicker, About, CompactFilter, InspectorDrawer, StatusDetails }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalOverlay { RecycleFailure, PermanentDelete, FileOperation }

#[derive(Default)]
pub struct OverlayCoordinator { transient: Option<TransientOverlay>, modal: Option<ModalOverlay>, restore_focus: Option<egui::Id> }
```

Implement outside-click/Escape dismissal, toast queuing, deterministic focus restoration, the documented priority order `SCAN, ENGINE, FILE OP, NOTICE, DISK USED, INDEXED, ENTRIES, WATCH, FILTER`, and a keyboard-scrollable Status Details overlay opened by a focusable `MORE +N` cell.

- [ ] **Step 4: Run all focused tests**

Run: `cargo test -p voidspace-app --test full_tactical_hud && cargo test -p voidspace-app`
Expected: all tests pass.

- [ ] **Step 5: Commit overlay and status behavior**

Run: `git add crates/voidspace-app/src/overlay_coordinator.rs crates/voidspace-app/src/status_bar.rs crates/voidspace-app/src/lib.rs crates/voidspace-app/tests/full_tactical_hud.rs && git commit -m "feat: coordinate hud overlays and status"`

### Task 4: Tactical Arc interaction model

**Files:**
- Create: `crates/voidspace-app/src/tactical_arc.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Test: `crates/voidspace-app/src/tactical_arc.rs`

- [ ] **Step 1: Write failing radial geometry and input tests**

```rust
#[test]
fn center_and_outer_dead_zones_do_not_activate_actions() {
    let arc = TacticalArcGeometry::new(egui::pos2(200.0, 200.0), 42.0, 112.0);
    assert_eq!(arc.hit_test(egui::pos2(200.0, 200.0)), None);
    assert_eq!(arc.hit_test(egui::pos2(400.0, 400.0)), None);
}

#[test]
fn clamped_arc_stays_inside_work_area() {
    let area = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));
    let arc = TacticalArcGeometry::clamped(egui::pos2(4.0, 4.0), area, 112.0);
    assert!(area.contains(arc.bounds().min) && area.contains(arc.bounds().max));
}
```

- [ ] **Step 2: Verify Tactical Arc tests fail**

Run: `cargo test -p voidspace-app tactical_arc::tests`
Expected: compilation fails because Tactical Arc types do not exist.

- [ ] **Step 3: Implement geometry, state, and actions**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TacticalAction { OpenInExplorer, Recycle, DeletePermanently }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextTarget { pub tab_scan_id: u64, pub scan_epoch: u64, pub node_id: NodeId, pub path: PathBuf, pub is_directory: bool, pub root: PathBuf }

pub struct TacticalArcState { pub center: egui::Pos2, pub hovered: Option<TacticalAction>, pub keyboard_index: usize }
```

Implement pointer-line drawing, click-time hit testing, right-click and `Shift+F10` opening, arrow/Tab selection, Enter activation, Escape cancellation, accessible action labels, and danger coloring only on permanent deletion.

- [ ] **Step 4: Run interaction tests**

Run: `cargo test -p voidspace-app tactical_arc::tests && cargo test -p voidspace-app --test full_tactical_hud`
Expected: all tests pass.

- [ ] **Step 5: Commit Tactical Arc**

Run: `git add crates/voidspace-app/src/tactical_arc.rs crates/voidspace-app/src/lib.rs && git commit -m "feat: add tactical arc context actions"`

### Task 5: Full HUD shell, responsive disk picker, inspector, and About

**Files:**
- Create: `crates/voidspace-app/src/shell.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Modify: `crates/voidspace-app/tests/ui_state.rs`
- Test: `crates/voidspace-app/tests/full_tactical_hud.rs`

- [ ] **Step 1: Write failing responsive and intent tests**

```rust
#[test]
fn compact_shell_preserves_disk_filter_and_inspector_access() {
    let layout = ShellLayout::for_width(760.0);
    assert_eq!(layout.inspector, InspectorPlacement::Drawer);
    assert_eq!(layout.filter, FilterPlacement::OverlayTrigger);
    assert!(layout.disk_picker_trigger_visible);
}

#[test]
fn about_contains_verified_author_links() {
    let links = about_links();
    assert!(links.contains(&("Telegram", "https://t.me/chipda")));
    assert!(links.contains(&("Website", "https://evgyur.pro")));
}
```

- [ ] **Step 2: Verify shell tests fail**

Run: `cargo test -p voidspace-app --test full_tactical_hud shell about`
Expected: compilation fails because shell layout and About link table are absent.

- [ ] **Step 3: Implement shell components and typed intents**

```rust
pub enum ShellIntent { OpenDiskPicker, ActivateTab(usize), CloseTab(usize), NavigateTo(NodeId), Back, OpenInspector, OpenFilter, OpenAbout, CopyPath(PathBuf), Reveal(PathBuf) }

pub const ABOUT_LINKS: &[(&str, &str)] = &[
    ("Telegram", "https://t.me/chipda"),
    ("Community", "https://t.me/chipdachat"),
    ("X", "https://x.com/chip1cr"),
    ("Website", "https://evgyur.pro"),
    ("Human 2.0", "https://human20.app"),
];
```

Render the top command rail, stable `VOL:##` tabs, collapsible breadcrumb, responsive disk-card grid with name/capacity/free space, docked/drawer inspector, compact filter editor, and About panel. Keep every close/back/disk action one click away and route all transient UI through `OverlayCoordinator`.

- [ ] **Step 4: Run shell and existing state tests**

Run: `cargo test -p voidspace-app --test ui_state && cargo test -p voidspace-app --test full_tactical_hud`
Expected: all tests pass.

- [ ] **Step 5: Commit the Full HUD shell**

Run: `git add crates/voidspace-app/src/shell.rs crates/voidspace-app/src/app.rs crates/voidspace-app/src/lib.rs crates/voidspace-app/tests/ui_state.rs crates/voidspace-app/tests/full_tactical_hud.rs && git commit -m "feat: build full tactical hud shell"`

### Task 6: Treemap tactical rendering without layout drift

**Files:**
- Modify: `crates/voidspace-app/src/treemap.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Test: `crates/voidspace-app/src/treemap.rs`
- Test: `crates/voidspace-app/tests/full_tactical_hud.rs`

- [ ] **Step 1: Write failing label, clipping, and invariance tests**

```rust
#[test]
fn size_is_preferred_and_name_only_appears_when_measured_to_fit() {
    let tiny = choose_tactical_label([46.0, 22.0], 28.0, 90.0);
    assert_eq!(tiny, TacticalLabel::SizeOnly);
    let wide = choose_tactical_label([180.0, 80.0], 28.0, 90.0);
    assert_eq!(wide, TacticalLabel::SizeAndName);
}

#[test]
fn tactical_paint_does_not_change_layout_or_hit_rects() {
    let original = fixture_layout();
    let painted = tactical_visual_rects(&original);
    assert_eq!(painted.iter().map(|p| p.hit_rect).collect::<Vec<_>>(), original.nodes.iter().map(|n| n.rect).collect::<Vec<_>>());
}
```

- [ ] **Step 2: Verify treemap tests fail**

Run: `cargo test -p voidspace-app treemap::label_tests treemap::interaction_tests`
Expected: compilation fails because tactical label and visual-rect helpers are absent.

- [ ] **Step 3: Implement the tactical paint pass**

Keep existing squarify rectangles, `OTHER` aggregation, stable click/double-click semantics, preview pinning, and hit-test order untouched. Add only a static clipped grid, in-bounds cut corners, size-first/name-second measured labels, hover/selection brackets and reticle, and a context-target response captured from non-aggregate nodes. Suppress base treemap interactions whenever the overlay coordinator reports a modal or transient pointer-owning overlay.

- [ ] **Step 4: Run layout, treemap, and UI state suites**

Run: `cargo test -p voidspace-layout && cargo test -p voidspace-app treemap && cargo test -p voidspace-app --test ui_state && cargo test -p voidspace-app --test full_tactical_hud`
Expected: all tests pass and existing property tests remain unchanged.

- [ ] **Step 5: Commit the tactical treemap renderer**

Run: `git add crates/voidspace-app/src/treemap.rs crates/voidspace-app/src/app.rs crates/voidspace-app/tests/full_tactical_hud.rs && git commit -m "feat: render tactical treemap states"`

### Task 7: Fail-closed file actions and live-scan stability

**Files:**
- Modify: `crates/voidspace-app/src/app.rs`
- Modify: `crates/voidspace-app/src/tactical_arc.rs`
- Modify: `crates/voidspace-app/tests/ui_state.rs`
- Test: `crates/voidspace-app/tests/full_tactical_hud.rs`

- [ ] **Step 1: Write failing revalidation and dismissal tests**

```rust
#[test]
fn stale_context_target_cannot_execute_after_tab_or_epoch_change() {
    let target = fixture_context_target();
    assert_eq!(revalidate_context_target(&target, &fixture_app_with_other_epoch()), Err(TargetInvalid::Epoch));
}

#[test]
fn recycle_is_immediate_but_permanent_delete_requires_delete_word() {
    assert_eq!(delete_dispatch(OperationKind::Recycle), DeleteDispatch::Immediate);
    assert_eq!(delete_dispatch(OperationKind::Permanent), DeleteDispatch::Confirm);
}
```

- [ ] **Step 2: Verify safety tests fail for the new target model**

Run: `cargo test -p voidspace-app --test full_tactical_hud stale_context target recycle`
Expected: the new revalidation tests fail until app execution is wired.

- [ ] **Step 3: Wire validated action execution**

Before Explorer, Recycle, or permanent delete, match `tab_scan_id`, `scan_epoch`, `node_id`, canonical path, kind, and root against current app state. Close the arc and show a neutral notice if validation fails. Recycle dispatches immediately; permanent delete retains the existing `DELETE` confirmation and fileops root safety. Preserve pinned previews, zoom path, aggregate state, and active selection across background snapshot refreshes through the existing bookmark repair path.

- [ ] **Step 4: Run app and file-operation safety suites**

Run: `cargo test -p voidspace-app && cargo test -p voidspace-fileops && cargo test -p voidspace-elevated`
Expected: all tests pass.

- [ ] **Step 5: Commit validated action routing**

Run: `git add crates/voidspace-app/src/app.rs crates/voidspace-app/src/tactical_arc.rs crates/voidspace-app/tests/ui_state.rs crates/voidspace-app/tests/full_tactical_hud.rs && git commit -m "fix: fail closed on stale tactical targets"`

### Task 8: Accessibility, idle repaint, and release performance gates

**Files:**
- Modify: `crates/voidspace-app/src/app.rs`
- Modify: `crates/voidspace-app/src/bin/voidspace-smoke.rs`
- Modify: `scripts/smoke.ps1`
- Test: `crates/voidspace-app/tests/full_tactical_hud.rs`

- [ ] **Step 1: Add failing diagnostic contract tests**

```rust
#[test]
fn static_hud_components_do_not_request_autonomous_repaint() {
    assert!(!hud_requires_autonomous_repaint(HudMotionState::Idle));
}

#[test]
fn every_tactical_action_has_accessible_name_and_shortcut() {
    for action in TacticalAction::ALL {
        assert!(!action.accessible_name().is_empty());
        assert!(!action.keyboard_hint().is_empty());
    }
}
```

- [ ] **Step 2: Verify the diagnostic tests fail**

Run: `cargo test -p voidspace-app --test full_tactical_hud autonomous accessible`
Expected: new diagnostic helpers are missing.

- [ ] **Step 3: Implement deterministic diagnostics**

Add a debug-only frame counter, a fixed 1024-tile render fixture, 60 warm-up frames, 600 measured frames, and output for median/p95. Keep forced repainting inside the benchmark path only. Extend smoke mode to verify embedded fonts, app startup, no autonomous idle repaint request, and one accessible name per actionable control.

- [ ] **Step 4: Run quality and performance gates**

Run: `cargo fmt --all -- --check`
Expected: no formatting differences.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no warnings.

Run: `cargo test --workspace`
Expected: all workspace tests pass.

Run: `cargo run -p voidspace-app --bin voidspace-smoke --release -- --hud-benchmark`
Expected: p95 below 16.7 ms and median/p95 no more than 10% slower than the recorded `fc46ec4` baseline on the same machine.

- [ ] **Step 5: Commit diagnostics and accessibility gates**

Run: `git add crates/voidspace-app/src/app.rs crates/voidspace-app/src/bin/voidspace-smoke.rs crates/voidspace-app/tests/full_tactical_hud.rs scripts/smoke.ps1 && git commit -m "test: verify tactical hud quality gates"`

### Task 9: Package, install, shortcut refresh, and visible verification

**Files:**
- Modify only if a packaging defect is found: `scripts/package.ps1`, `scripts/install-local.ps1`
- Preserve: user-owned untracked `artifacts/`

- [ ] **Step 1: Build the packaged release**

Run: `powershell -ExecutionPolicy Bypass -File scripts/package.ps1`
Expected: release executable and bundle are produced successfully after workspace tests and release build.

- [ ] **Step 2: Run packaged smoke tests**

Run: `powershell -ExecutionPolicy Bypass -File scripts/smoke.ps1`
Expected: startup, font, icon, and non-destructive smoke checks pass.

- [ ] **Step 3: Install and refresh the persistent desktop shortcut**

Run: `powershell -ExecutionPolicy Bypass -File scripts/install-local.ps1`
Expected: the desktop `Voidspace` shortcut targets the installed current release, carries the application icon, and remains the stable entry point for later local installs.

- [ ] **Step 4: Open the installed application for user verification**

Run: `Start-Process -FilePath "$env:LOCALAPPDATA\Voidspace\Voidspace.exe"`
Expected: one visible Full Tactical HUD window opens to the disk picker or active scan without a localhost dependency.

- [ ] **Step 5: Record final evidence and commit packaging fixes if needed**

Run: `Get-FileHash "$env:LOCALAPPDATA\Voidspace\Voidspace.exe" -Algorithm SHA256; git status --short; git log --oneline -12`
Expected: installed hash is recorded, only `artifacts/` remains untracked, and every implementation slice is committed. If packaging scripts changed, commit them with `git commit -m "fix: package full tactical hud release"`.

## Self-review result

- Spec coverage: every Full Tactical HUD and Tactical Arc section maps to Tasks 1–9, including responsive overlays, stable volume IDs, treemap invariants, About links, file-action safety, accessibility, idle repaint, performance, packaging, shortcut refresh, and visible verification.
- Placeholder scan: no deferred implementation markers or unspecified test steps remain.
- Type consistency: `ContextTarget`, `TacticalAction`, `TransientOverlay::StatusDetails`, `VolumeDisplayRegistry`, `StatusSnapshot`, and `ShellIntent` are introduced once and reused with matching names.

