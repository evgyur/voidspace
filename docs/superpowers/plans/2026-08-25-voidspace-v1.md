# Voidspace v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and package a production-grade Windows x64 disk-space analyzer in Rust with a modern Spectral UI, streaming treemap, continuous filesystem updates, optional elevation, safe deletion, snapshots, and exports.

**Architecture:** A Cargo workspace separates lossless Windows filesystem identity, the arena index/reducer, treemap layout, scanners/watchers, filters, file operations, artifacts, the eframe application, and the elevated helper. All mutation is serialized through the index reducer; workers publish bounded typed events and the UI consumes immutable snapshots. The first shipping package contains `voidspace.exe` and `voidspace-elevated.exe`.

**Tech Stack:** Rust 1.98 stable, eframe/egui 0.36, wgpu/DX12, windows-rs 0.62, rayon, crossbeam-channel, notify, serde, ciborium, zstd, blake3, csv, tracing, proptest.

---

## File map

- `Cargo.toml`: workspace members, shared dependency versions, release profile.
- `crates/voidspace-model/`: lossless names, identities, metrics, events, snapshots.
- `crates/voidspace-index/`: compact arena, reducer, aggregate propagation, immutable view.
- `crates/voidspace-layout/`: deterministic squarified treemap, LOD, hit testing.
- `crates/voidspace-filter/`: typed lexer/parser/evaluator and presets.
- `crates/voidspace-scan/`: normal parallel directory scanner and volume discovery.
- `crates/voidspace-watch/`: Windows recursive watcher, coalescing, invalidation.
- `crates/voidspace-fileops/`: Explorer/properties, Recycle Bin, guarded permanent deletion.
- `crates/voidspace-export/`: versioned snapshots and CSV/JSON/HTML/text reports.
- `crates/voidspace-app/`: eframe shell, coordinator, treemap renderer, dialogs, settings.
- `crates/voidspace-elevated/`: typed local helper for privileged scan/delete operations.
- `tests/fixtures/`: generated trees, malicious artifact fixtures, visual fixtures.
- `scripts/package.ps1`: release build, manifest/icon assembly, portable zip.
- `scripts/smoke.ps1`: deterministic end-to-end scan/export/delete smoke on a temp root.

### Task 1: Bootstrap the Rust workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/*/Cargo.toml`
- Create: `crates/*/src/lib.rs`
- Create: `crates/voidspace-app/src/main.rs`
- Create: `crates/voidspace-elevated/src/main.rs`

- [ ] **Step 1: Define workspace members and pinned shared dependencies**

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
anyhow = "1"
blake3 = "1"
crossbeam-channel = "0.5"
ciborium = "0.2"
csv = "1"
eframe = { version = "0.36.1", default-features = false, features = ["accesskit", "default_fonts", "persistence", "wgpu"] }
egui = "0.36.1"
notify = "8"
parking_lot = "0.12"
proptest = "1"
rayon = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "2"
tracing = "0.1"
windows = "0.62.2"
zstd = "0.13"
```

- [ ] **Step 2: Add minimal library and binary entry points**

```rust
fn main() -> eframe::Result<()> {
    eframe::run_native("Voidspace", eframe::NativeOptions::default(), Box::new(|cc| {
        Ok(Box::new(voidspace_app::VoidspaceApp::new(cc)))
    }))
}
```

- [ ] **Step 3: Verify the empty workspace**

Run: `cargo fmt --all -- --check && cargo check --workspace`
Expected: both commands exit `0`.

- [ ] **Step 4: Commit**

Run: `git add Cargo.toml rust-toolchain.toml crates && git commit -m "chore: bootstrap Voidspace Rust workspace"`

### Task 2: Implement lossless model and reducer index

**Files:**
- Create: `crates/voidspace-model/src/{lib,name,identity,event,node}.rs`
- Create: `crates/voidspace-index/src/{lib,arena,reducer,snapshot}.rs`
- Test: `crates/voidspace-index/tests/reducer.rs`

- [ ] **Step 1: Write model/index tests first**

```rust
#[test]
fn preserves_unpaired_surrogate_and_propagates_size_delta() {
    let name = WinName::from_units(vec![0x0061, 0xD800]);
    assert_eq!(name.units(), &[0x0061, 0xD800]);
    let mut index = Index::new(test_root());
    let child = index.apply(upsert(1, index.root(), name, 4096)).unwrap();
    assert_eq!(index.node(child).allocated, 4096);
    assert_eq!(index.node(index.root()).allocated, 4096);
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p voidspace-index --test reducer`
Expected: compile failure because `WinName` and `Index` do not exist.

- [ ] **Step 3: Implement stable types and reducer contracts**

```rust
pub struct EventEnvelope {
    pub scan_id: ScanId,
    pub generation: u64,
    pub branch_epoch: Option<u64>,
    pub producer: ProducerId,
    pub sequence: u64,
    pub payload: EventPayload,
}

pub trait IndexReducer {
    fn apply(&mut self, event: EventEnvelope) -> Result<DirtySet, ReduceError>;
    fn snapshot(&self) -> IndexSnapshot;
}
```

Use arena-backed `Vec<Node>`, `HashMap<FileIdentity, NodeId>`, interned `WinName`, checked signed aggregate deltas, tombstones, pending-parent staging, and hard-link allocation ownership.

- [ ] **Step 4: Add property tests for ordering, hard links, and branch ownership**

Run: `cargo test -p voidspace-index`
Expected: unit and property tests pass, including one-event/one-commit for disjoint rescans.

- [ ] **Step 5: Commit**

Run: `git add crates/voidspace-model crates/voidspace-index && git commit -m "feat: add lossless model and reducer index"`

### Task 3: Implement deterministic treemap layout

**Files:**
- Create: `crates/voidspace-layout/src/{lib,squarify,hit_test,lod}.rs`
- Test: `crates/voidspace-layout/tests/properties.rs`

- [ ] **Step 1: Add property tests**

```rust
proptest! {
    #[test]
    fn children_are_contained_and_non_overlapping(weights in prop::collection::vec(1u64..1_000_000, 1..500)) {
        let out = layout_weights(&weights, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_contained(&out);
        assert_non_overlapping(&out);
    }
}
```

- [ ] **Step 2: Implement squarified layout, stable secondary ordering, LOD, and hit testing**

```rust
pub fn layout(snapshot: &IndexSnapshot, view: &ViewState, dirty: &DirtySet) -> LayoutSnapshot;
pub fn hit_test(layout: &LayoutSnapshot, point: egui::Pos2) -> Option<NodeId>;
```

- [ ] **Step 3: Verify and commit**

Run: `cargo test -p voidspace-layout`
Expected: all deterministic/property tests pass.

Run: `git add crates/voidspace-layout && git commit -m "feat: add deterministic treemap layout"`

### Task 4: Implement scanner, volume discovery, and typed filters

**Files:**
- Create: `crates/voidspace-scan/src/{lib,request,volume,walk,metadata}.rs`
- Create: `crates/voidspace-filter/src/{lib,lexer,parser,eval,preset}.rs`
- Test: `crates/voidspace-scan/tests/streaming.rs`
- Test: `crates/voidspace-filter/tests/grammar.rs`

- [ ] **Step 1: Write streaming and grammar tests**

```rust
#[test]
fn filter_precedence_matches_spec() {
    let expr = parse("size > 1GiB AND NOT attr:system OR ext:zip").unwrap();
    assert!(matches!(expr, Expr::Or(_, _)));
}
```

Create a temporary tree and assert `BaselineStarted`, `UpsertNode`, `DirectoryEnumerated`, `BaselineFinished` ordering and cooperative cancellation.

- [ ] **Step 2: Implement scanner with bounded event sink**

```rust
pub fn start(request: ScanRequest, sink: Sender<EventEnvelope>) -> ScanHandle;
impl ScanHandle { pub fn pause(&self); pub fn resume(&self); pub fn cancel(&self); }
```

Use lossless `OsString::encode_wide`, Win32 metadata on Windows, no reparse traversal by default, rayon workers, and permission errors as `NodeError`.

- [ ] **Step 3: Implement the typed filter grammar exactly as design section 24**

Run: `cargo test -p voidspace-scan -p voidspace-filter`
Expected: streaming, cancellation, parser, timezone, unknown, and escape tests pass.

- [ ] **Step 4: Commit**

Run: `git add crates/voidspace-scan crates/voidspace-filter && git commit -m "feat: add streaming scanner and typed filters"`

### Task 5: Implement watcher and live reconciliation

**Files:**
- Create: `crates/voidspace-watch/src/{lib,coalesce,notify_backend,branch}.rs`
- Create: `crates/voidspace-app/src/coordinator.rs`
- Test: `crates/voidspace-watch/tests/reconcile.rs`

- [ ] **Step 1: Write mutation-during-baseline and cross-branch move tests**

The final independent filesystem walk must equal the reducer snapshot after quiescence. A move from two active disjoint rescans must cancel both shadows, rescan the LCA, and commit each producer sequence once.

- [ ] **Step 2: Implement recursive Windows watcher and coalescing**

```rust
pub fn watch(request: WatchRequest, sink: Sender<FsDelta>) -> notify::Result<WatchHandle>;
```

Use `notify`'s Windows backend (`ReadDirectoryChangesW`), 40 ms semantic coalescing, bounded buffers, overflow invalidation, branch fences, and polling fallback for unsupported UNC notification.

- [ ] **Step 3: Implement coordinator state machine and latest-wins snapshots**

Run: `cargo test -p voidspace-watch -p voidspace-app coordinator`
Expected: baseline race, pause/resume, overflow, cancellation, and reconnect tests pass.

- [ ] **Step 4: Commit**

Run: `git add crates/voidspace-watch crates/voidspace-app/src/coordinator.rs && git commit -m "feat: add continuous filesystem reconciliation"`

### Task 6: Build the Spectral eframe application

**Files:**
- Create: `crates/voidspace-app/src/{lib,app,theme,treemap,toolbar,inspector,dialogs,settings}.rs`
- Create: `crates/voidspace-app/assets/voidspace.ico`
- Test: `crates/voidspace-app/tests/ui_state.rs`

- [ ] **Step 1: Test pure UI state and responsive breakpoints**

```rust
#[test]
fn inspector_docks_at_1024_and_closes_at_800() {
    assert_eq!(workspace_mode(1024.0), WorkspaceMode::Docked);
    assert_eq!(workspace_mode(800.0), WorkspaceMode::DrawerClosed);
}
```

- [ ] **Step 2: Implement the theme and main workspace**

Use the exact palette `#070809`, `#FF5A2F`, `#19D3FF`, `#C9F65A`, `#FF4ECD`, `#8B5CF6`; render tiles with child content below a reserved title strip; ellipsize all text; batch visible shapes; request repaint only while scanning/animating.

- [ ] **Step 3: Implement navigation, tabs, filters, tags, status, errors, and danger dialogs**

One click selects, double click zooms, breadcrumbs/history work, snapshot tabs are read-only, permanent delete requires `DELETE`, and `ADMIN` is shown only for elevated tokens.

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p voidspace-app && cargo check -p voidspace-app`
Expected: responsive/state tests pass and app compiles.

Run: `git add crates/voidspace-app && git commit -m "feat: build Spectral Voidspace desktop UI"`

### Task 7: Add Windows file operations and safe permanent deletion

**Files:**
- Create: `crates/voidspace-fileops/src/{lib,open,recycle,manifest,delete}.rs`
- Test: `crates/voidspace-fileops/tests/safety.rs`

- [ ] **Step 1: Write temp-root safety tests**

Tests must reject volume roots, unresolved reparse roots, identity changes after confirmation, and new manifest entries; permanent deletion executes only under a unique temporary directory.

- [ ] **Step 2: Implement Explorer/properties/recycle and manifest preparation**

```rust
pub fn prepare(draft: OperationDraft, snapshot: &IndexSnapshot) -> Result<ConfirmableOperation>;
pub fn execute(op: ConfirmedOperation, progress: Sender<Progress>, cancel: CancellationToken) -> OperationReport;
```

Use Shell APIs for Explorer/Recycle Bin and a lossless confirmed manifest for permanent deletion. Never follow reparse targets; deduplicate nested selections; return per-item outcomes.

- [ ] **Step 3: Verify and commit**

Run: `cargo test -p voidspace-fileops`
Expected: all fail-closed safety tests pass; test roots are cleaned.

Run: `git add crates/voidspace-fileops && git commit -m "feat: add guarded Windows file operations"`

### Task 8: Add elevated helper and Turbo protocol

**Files:**
- Create: `crates/voidspace-elevated/src/{main,protocol,peer,turbo,delete}.rs`
- Create: `crates/voidspace-app/src/elevation.rs`
- Create: `crates/voidspace-elevated/voidspace-elevated.manifest`
- Test: `crates/voidspace-elevated/tests/protocol.rs`

- [ ] **Step 1: Write protocol framing and rejection tests**

Reject oversized frames, unknown kinds, duplicate request IDs, non-monotonic sequence, remote pipes, wrong peer PID/path/session/token, and malformed payloads before any effect.

- [ ] **Step 2: Implement local named-pipe protocol and UAC launch**

```rust
pub enum Request { TurboStart(TurboRequest), PrivilegedSubtreeScan(ScanRequest), ProtectedDelete(ConfirmedOperation), Cancel(RequestId) }
```

The helper owns elevated volume/delete handles. Turbo enumerates NTFS records with documented `FSCTL_ENUM_USN_DATA`, resolves parent file references into paths, captures the journal cursor before baseline, and continues through `FSCTL_READ_USN_JOURNAL`; unsupported/error cases fall back explicitly to privileged Win32 traversal with a visible degraded-mode badge.

- [ ] **Step 3: Embed `requireAdministrator`, detect elevation, and implement always-request-admin relaunch**

Run: `cargo test -p voidspace-elevated && cargo check -p voidspace-app -p voidspace-elevated`
Expected: protocol tests pass and both binaries compile.

- [ ] **Step 4: Commit**

Run: `git add crates/voidspace-elevated crates/voidspace-app/src/elevation.rs && git commit -m "feat: add typed elevated helper and Turbo mode"`

### Task 9: Implement snapshots, reports, settings, and diagnostics

**Files:**
- Create: `crates/voidspace-export/src/{lib,snapshot,report,template}.rs`
- Modify: `crates/voidspace-app/src/settings.rs`
- Create: `crates/voidspace-app/src/diagnostics.rs`
- Test: `crates/voidspace-export/tests/artifacts.rs`

- [ ] **Step 1: Add golden/malicious artifact tests**

Cover exact UTF-16LE names, checksum failure, duplicate IDs, excessive ratio/count/depth, CSV formula injection, HTML escaping, template depth, cancellation, and atomic target preservation.

- [ ] **Step 2: Implement deterministic snapshot v1 and four reports**

Use fixed little-endian header/table, deterministic CBOR, zstd frames, BLAKE3, UTF-8 safe export escaping, same-directory temp files, flush, verify, and atomic replacement.

- [ ] **Step 3: Implement versioned atomic settings and redacted rotating diagnostics**

Run: `cargo test -p voidspace-export -p voidspace-app settings diagnostics`
Expected: golden, malicious, migration, corruption, and redaction tests pass.

- [ ] **Step 4: Commit**

Run: `git add crates/voidspace-export crates/voidspace-app && git commit -m "feat: add snapshots exports and durable settings"`

### Task 10: Integrate, harden, and package Windows release

**Files:**
- Create: `tests/fixtures/README.md`
- Create: `scripts/smoke.ps1`
- Create: `scripts/package.ps1`
- Create: `README.md`
- Create: `LICENSE`

- [ ] **Step 1: Add end-to-end smoke script**

The script creates a temp tree, runs headless scan/export verification, mutates it, verifies reconciliation, recycles one file, permanently deletes only a second temp subtree, and asserts no path escapes the temp root.

- [ ] **Step 2: Run quality gates**

Run: `cargo fmt --all -- --check`
Expected: exit `0`.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit `0`.

Run: `cargo test --workspace`
Expected: all tests pass.

Run: `cargo build --workspace --release`
Expected: both Windows binaries build.

- [ ] **Step 3: Run smoke and performance probes**

Run: `powershell -ExecutionPolicy Bypass -File scripts/smoke.ps1`
Expected: `VOIDSPACE_SMOKE_OK` and no surviving temp tree.

Run: `cargo run -p voidspace-app --release -- --benchmark-layout 1000000`
Expected: emits machine-readable frame/index timings without panic or unbounded memory growth.

- [ ] **Step 4: Package portable artifact**

Run: `powershell -ExecutionPolicy Bypass -File scripts/package.ps1`
Expected: `dist/Voidspace-0.1.0-windows-x64.zip` containing app, helper, README, LICENSE, and checksums.

- [ ] **Step 5: Final review and commit**

Run: `git status --short && git diff --check`
Expected: only intentional release artifacts ignored or tracked; diff check clean.

Run: `git add README.md LICENSE scripts tests Cargo.lock && git commit -m "release: package Voidspace 0.1.0 for Windows"`

## Self-review record

- Spec coverage: sections 1–29 map to Tasks 1–10; no launch subsystem is unowned.
- Type consistency: `EventEnvelope`, `SourceRevision`, `DirtySet`, `IndexSnapshot`, `LayoutSnapshot`, `OperationReport`, and `Request` retain the names and ownership from the approved design.
- Placeholder scan: the plan contains no deferred implementation decisions; Turbo uses documented USN/FSCTL enumeration with an explicit visible fallback.
- Execution mode: inline/autonomous, explicitly selected by the user; no additional handoff question is required.
