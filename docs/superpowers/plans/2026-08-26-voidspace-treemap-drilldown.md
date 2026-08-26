# Voidspace Treemap Drill-down Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make one click visibly pin a directory’s children inside its tile, double click zoom into that directory, provide direct Back/breadcrumb navigation, guarantee a size label on every visible rectangle, and keep a Desktop shortcut pointed at the latest locally installed release.

**Architecture:** `voidspace-layout` will emit only real rectangles that meet a UI-supplied outer label footprint and will expose exact deterministic `OTHER` membership. A new app-local state module will own canonical root-to-view navigation and pure transition reduction. `treemap.rs` will produce one typed action per activation and render measured three-tier labels; `app.rs` will apply actions and render the navigation strip.

**Tech Stack:** Rust 1.98, egui/eframe 0.36, existing `voidspace-index`, `voidspace-layout`, `voidspace-model`, Cargo tests, proptest, PowerShell packaging.

---

## File map

- Modify `crates/voidspace-layout/src/lib.rs`: label-footprint API, deterministic prefix fitting, exact aggregate groups, saturating conservation.
- Modify `crates/voidspace-layout/src/lib.rs` tests: containment, footprint, budget, and saturating-conservation properties around the new partition boundary.
- Create `crates/voidspace-app/src/treemap_state.rs`: canonical `ViewPath`, aggregate selection, mutually exclusive action reducer.
- Modify `crates/voidspace-app/src/lib.rs`: expose testable state types used by integration tests.
- Modify `crates/voidspace-app/src/treemap.rs`: typed hit/action model, double-click arbitration, keyboard overlays, compact formatting, measured label tiers, pin styling.
- Modify `crates/voidspace-app/src/app.rs`: `ScanTab` state migration, action application, label-footprint calculation, breadcrumb/Back/Alt+Left integration, exact `OTHER` inspector membership.
- Modify `crates/voidspace-app/tests/ui_state.rs`: pin/zoom/back/breadcrumb/live-repair state tests.
- Modify `README.md`: document click, double-click, keyboard, and Back behavior.
- Create `scripts/install-local.ps1`: atomically refresh a stable per-user install and recreate the Desktop shortcut.
- Modify `scripts/package.ps1`: install the exact packaged candidate after build/smoke/package success.

### Task 1: Make layout preserve labeled rectangles and exact `OTHER` membership

**Files:**
- Modify: `crates/voidspace-layout/src/lib.rs`
- Modify: `crates/voidspace-app/src/app.rs` (compile-compatible `ViewState`/`LayoutSnapshot` initialization only)
- Modify: `crates/voidspace-app/src/treemap.rs` (compile-compatible `ViewState` initialization only)

- [ ] **Step 1: Add failing unit tests for footprint, zero budget, suffix membership, and saturated conservation**

Append a private test module in `crates/voidspace-layout/src/lib.rs` that exercises a pure partition helper before wiring it to index snapshots:

```rust
#[cfg(test)]
mod footprint_tests {
    use super::*;

    const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 240.0);
    const LABEL: LabelFootprint = LabelFootprint::new(54.0, 22.0);

    #[test]
    fn undersized_rectangles_move_to_one_suffix_aggregate() {
        let weights = [600, 220, 80, 40, 20, 10];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 128);

        assert!(partition.rectangles.iter().all(|rect| LABEL.fits(*rect)));
        assert_eq!(partition.aggregate_start, partition.rectangles.len());
        assert_eq!(partition.aggregate_count, weights.len() - partition.aggregate_start);
        assert_eq!(
            partition.aggregate_size,
            weights[partition.aggregate_start..]
                .iter()
                .copied()
                .fold(0_u64, u64::saturating_add)
        );
    }

    #[test]
    fn zero_real_tile_budget_still_produces_structural_other() {
        let weights = [8, 5, 3];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 0);

        assert!(partition.rectangles.is_empty());
        assert_eq!(partition.aggregate_start, 0);
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 16);
        assert!(partition.aggregate_rect.is_some_and(|rect| LABEL.fits(rect)));
    }

    #[test]
    fn one_real_tile_budget_keeps_the_largest_and_aggregates_the_exact_suffix() {
        let weights = [90, 60, 30, 10];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 1);

        assert_eq!(partition.aggregate_start, 1);
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 100);
    }

    #[test]
    fn canvas_smaller_than_the_label_footprint_emits_no_child_rectangle() {
        let tiny = Rect::new(0.0, 0.0, 30.0, 12.0);
        let partition = partition_children(&[8, 5, 3], tiny, LABEL, 196.0, 0);
        assert!(partition.rectangles.is_empty());
        assert!(partition.aggregate_rect.is_none());
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 16);
    }

    #[test]
    fn conservation_uses_saturating_u64_arithmetic() {
        let weights = [u64::MAX, 10, 5];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 1);
        let visible = weights[..partition.aggregate_start]
            .iter()
            .copied()
            .fold(0_u64, u64::saturating_add);

        assert_eq!(
            visible.saturating_add(partition.aggregate_size),
            weights.iter().copied().fold(0_u64, u64::saturating_add)
        );
    }
}
```

- [ ] **Step 2: Run the focused tests and verify the red state**

Run:

```powershell
cargo test -p voidspace-layout footprint_tests -- --nocapture
```

Expected: compilation fails because `LabelFootprint` and `partition_children` do not exist.

- [ ] **Step 3: Add the layout API and iterative deterministic-prefix partition**

Add these public types beside `ViewState` and `LayoutSnapshot`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelFootprint {
    pub width: f32,
    pub height: f32,
}

impl LabelFootprint {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn fits(self, rect: Rect) -> bool {
        rect.width() >= self.width && rect.height() >= self.height
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AggregateGroup {
    pub parent_id: NodeId,
    pub depth: u8,
    pub member_ids: Vec<NodeId>,
    pub size: u64,
}
```

Extend the existing state/output types exactly as follows:

```rust
pub struct ViewState {
    pub root: NodeId,
    pub bounds: Rect,
    pub size_mode: SizeMode,
    pub max_depth: u8,
    pub min_area: f32,
    pub min_label: LabelFootprint,
    /// Global budget for real, non-root, non-aggregate rectangles.
    pub max_rectangles: usize,
}

pub struct LayoutSnapshot {
    pub index_version: u64,
    pub root: NodeId,
    pub nodes: Vec<LayoutNode>,
    #[serde(default)]
    pub aggregates: Vec<AggregateGroup>,
}
```

Because the serialized layout shape changes, bump `LAYOUT_SCHEMA_VERSION` from `1` to `2`; `#[serde(default)]` keeps older snapshots readable with no aggregate metadata.

Implement the pure helper before `layout_children` and make it the only path that decides the kept prefix:

```rust
#[derive(Clone, Debug, PartialEq)]
struct ChildPartition {
    rectangles: Vec<Rect>,
    aggregate_rect: Option<Rect>,
    aggregate_start: usize,
    aggregate_count: usize,
    aggregate_size: u64,
}

fn partition_children(
    weights: &[u64],
    bounds: Rect,
    min_label: LabelFootprint,
    min_area: f32,
    real_budget: usize,
) -> ChildPartition {
    let mut keep = weights.len().min(real_budget).min(128);
    loop {
        let aggregate_size = weights[keep..]
            .iter()
            .copied()
            .fold(0_u64, u64::saturating_add);
        let has_other = keep < weights.len() && aggregate_size > 0;
        let (content, aggregate_rect) = if has_other && keep == 0 {
            // With no real tiles, OTHER owns the full child canvas; do not leave the
            // content portion of the usual right-hand split empty.
            (bounds, Some(bounds))
        } else if has_other {
            split_other_on_right(bounds, aggregate_size, weights.iter().copied().fold(
                0_u128,
                |sum, value| sum.saturating_add(u128::from(value)),
            ))
        } else {
            (bounds, None)
        };
        let rectangles = layout_weights(&weights[..keep], content);
        let real_fit = rectangles
            .iter()
            .copied()
            .all(|rect| min_label.fits(rect) && rect.area() >= min_area);
        let aggregate_fit = aggregate_rect.is_none_or(|rect| min_label.fits(rect));
        if real_fit && aggregate_fit {
            return ChildPartition {
                rectangles,
                aggregate_rect,
                aggregate_start: keep,
                aggregate_count: weights.len() - keep,
                aggregate_size,
            };
        }
        if keep == 0 {
            return ChildPartition {
                rectangles: Vec::new(),
                aggregate_rect: None,
                aggregate_start: 0,
                aggregate_count: weights.len(),
                aggregate_size,
            };
        }
        keep -= 1;
    }
}
```

Change `layout_children` to receive `real_budget: &mut usize` and `aggregates: &mut Vec<AggregateGroup>`. Call `partition_children(&child_weights, bounds, view.min_label, view.min_area, *real_budget)`. Use `partition.aggregate_start` as the only `children.take(...)` count, decrement `real_budget` for each emitted real child, and remove the old post-partition `if rect.area() < view.min_area { continue; }` branch so no kept child can disappear outside `OTHER`. Create the aggregate from the exact sorted suffix:

```rust
if partition.aggregate_count > 0 {
    let members = children[partition.aggregate_start..]
        .iter()
        .map(|(node_id, _)| *node_id)
        .collect::<Vec<_>>();
    aggregates.push(AggregateGroup {
        parent_id: parent,
        depth,
        member_ids: members.clone(),
        size: partition.aggregate_size,
    });
    if let Some(rect) = partition.aggregate_rect {
        output.push(LayoutNode {
            node_id: parent,
            parent_id: Some(parent),
            rect,
            depth,
            aggregated: true,
            aggregate_count: members.len().min(u32::MAX as usize) as u32,
            aggregate_size: partition.aggregate_size,
        });
    }
}
```

When even `aggregate_rect` cannot meet the footprint, emit no child nodes but retain the exact `AggregateGroup`; the caller uses that group to render the non-tile overflow message.

- [ ] **Step 4: Keep downstream constructors compiling and add partition properties**

In both current app constructors (`app.rs::workspace` and `treemap.rs::show`), import `LabelFootprint` and add this temporary conservative value; Task 4 replaces it with measured footprints:

```rust
min_label: LabelFootprint::new(1.0, 1.0),
```

In `app.rs::empty_layout`, initialize the new output field:

```rust
aggregates: Vec::new(),
```

Add this proptest inside the private `footprint_tests` module so it can exercise `partition_children` without exposing an implementation-only function:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn partition_is_labeled_budgeted_and_conservative(
        mut weights in prop::collection::vec(1_u64..1_000_000, 1..200),
        budget in 0_usize..64,
    ) {
        weights.sort_unstable_by(|left, right| right.cmp(left));
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, budget);
        prop_assert!(partition.rectangles.len() <= budget);
        prop_assert!(partition.rectangles.iter().copied().all(|rect| LABEL.fits(rect)));
        prop_assert_eq!(partition.aggregate_start, partition.rectangles.len());
        let visible = weights[..partition.aggregate_start]
            .iter().copied().fold(0_u64, u64::saturating_add);
        let total = weights.iter().copied().fold(0_u64, u64::saturating_add);
        prop_assert_eq!(visible.saturating_add(partition.aggregate_size), total);

        let mut all_rectangles = partition.rectangles.clone();
        all_rectangles.extend(partition.aggregate_rect);
        prop_assert!(all_rectangles.iter().all(|rect| BOUNDS.contains(*rect)));
        for left in 0..all_rectangles.len() {
            for right in (left + 1)..all_rectangles.len() {
                let overlap_width = all_rectangles[left].max_x.min(all_rectangles[right].max_x)
                    - all_rectangles[left].min_x.max(all_rectangles[right].min_x);
                let overlap_height = all_rectangles[left].max_y.min(all_rectangles[right].max_y)
                    - all_rectangles[left].min_y.max(all_rectangles[right].min_y);
                prop_assert!(overlap_width <= f32::EPSILON || overlap_height <= f32::EPSILON);
            }
        }
        prop_assert_eq!(partition_children(&weights, BOUNDS, LABEL, 196.0, budget), partition);
    }
}
```

- [ ] **Step 5: Run layout tests and commit**

Run:

```powershell
cargo test -p voidspace-layout
cargo check -p voidspace-app
```

Expected: all layout unit/property tests pass, including `other_is_always_a_right_hand_column`; the app compiles against the extended layout API.

Commit:

```powershell
git add crates/voidspace-layout/src/lib.rs crates/voidspace-app/src/app.rs crates/voidspace-app/src/treemap.rs
git commit -m "feat: preserve labeled treemap tiles"
```

### Task 2: Add canonical navigation and persistent interaction state

**Files:**
- Create: `crates/voidspace-app/src/treemap_state.rs`
- Modify: `crates/voidspace-app/src/lib.rs`
- Modify: `crates/voidspace-app/tests/ui_state.rs`

- [ ] **Step 1: Write failing navigation and transition tests**

Append these tests to `crates/voidspace-app/tests/ui_state.rs`:

```rust
use voidspace_app::{AggregateSelection, TreemapAction, TreemapState, ViewPath};

#[test]
fn nested_zoom_builds_canonical_path_and_back_returns_real_parent() {
    let root = NodeId(1);
    let parent = NodeId(2);
    let nested = NodeId(3);
    let parent_of = |id| match id {
        NodeId(2) => Some(root),
        NodeId(3) => Some(parent),
        _ => None,
    };
    let mut path = ViewPath::root(root);
    path.rebuild(nested, parent_of).unwrap();

    assert_eq!(path.as_slice(), &[root, parent, nested]);
    assert_eq!(path.back(), Some(parent));
    assert_eq!(path.as_slice(), &[root, parent]);
}

#[test]
fn action_reducer_distinguishes_selection_pin_and_zoom() {
    let root = NodeId(1);
    let directory = NodeId(2);
    let nested = NodeId(3);
    let mut state = TreemapState::new(root);

    state.apply(TreemapAction::ActivateBaseDirectory(directory));
    assert_eq!(state.selected, Some(directory));
    assert_eq!(state.pinned, Some(directory));

    state.apply(TreemapAction::ActivateNested(nested));
    assert_eq!(state.selected, Some(nested));
    assert_eq!(state.pinned, Some(directory));

    state.apply(TreemapAction::Zoom(nested));
    assert_eq!(state.selected, Some(nested));
    assert_eq!(state.pinned, None);
    assert_eq!(state.aggregate, None);

    state.apply(TreemapAction::ActivateBaseDirectory(directory));
    state.apply(TreemapAction::ActivateBaseLeaf(NodeId(4)));
    assert_eq!(state.selected, Some(NodeId(4)));
    assert_eq!(state.pinned, None);

    state.apply(TreemapAction::OpenAggregate(AggregateSelection {
        parent: directory,
        depth: 1,
        members: vec![NodeId(5), NodeId(6)],
    }));
    assert_eq!(state.selected, Some(directory));
    assert_eq!(state.aggregate.as_ref().map(|group| group.members.as_slice()), Some(&[NodeId(5), NodeId(6)][..]));
    state.apply(TreemapAction::ClearPreview);
    assert_eq!(state.selected, Some(directory));
    assert_eq!(state.aggregate.as_ref().map(|group| group.members.len()), Some(2));
}

#[test]
fn live_repair_prunes_missing_tail_and_clears_stale_transient_state() {
    let mut state = TreemapState::new(NodeId(1));
    state.view_path = ViewPath::from_ids(vec![NodeId(1), NodeId(2), NodeId(3)]).unwrap();
    state.selected = Some(NodeId(3));
    state.pinned = Some(NodeId(2));
    state.aggregate = Some(AggregateSelection {
        parent: NodeId(3),
        depth: 1,
        members: vec![NodeId(4)],
    });

    state.repair(|id| id != NodeId(3), |id| match id {
        NodeId(2) => Some(NodeId(1)),
        _ => None,
    });

    assert_eq!(state.view_path.as_slice(), &[NodeId(1), NodeId(2)]);
    assert_eq!(state.selected, Some(NodeId(2)));
    assert_eq!(state.pinned, Some(NodeId(2)));
    assert_eq!(state.aggregate, None);
}

#[test]
fn live_repair_truncates_at_the_first_broken_internal_link() {
    let mut state = TreemapState::new(NodeId(1));
    state.view_path = ViewPath::from_ids(vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)]).unwrap();
    state.repair(|_| true, |id| match id {
        NodeId(2) => Some(NodeId(1)),
        NodeId(3) => Some(NodeId(99)),
        NodeId(4) => Some(NodeId(3)),
        _ => None,
    });
    assert_eq!(state.view_path.as_slice(), &[NodeId(1), NodeId(2)]);
}
```

- [ ] **Step 2: Run the UI-state test and verify it fails to compile**

Run:

```powershell
cargo test -p voidspace-app --test ui_state
```

Expected: unresolved imports for `ViewPath`, `TreemapState`, `TreemapAction`, and `AggregateSelection`.

- [ ] **Step 3: Create the pure state module**

Create `crates/voidspace-app/src/treemap_state.rs` with these public types and complete transition behavior:

```rust
use voidspace_model::NodeId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateSelection {
    pub parent: NodeId,
    pub depth: u8,
    pub members: Vec<NodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreemapAction {
    ActivateBaseDirectory(NodeId),
    ActivateBaseLeaf(NodeId),
    ActivateNested(NodeId),
    Zoom(NodeId),
    OpenAggregate(AggregateSelection),
    ClearPreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewPath(Vec<NodeId>);

impl ViewPath {
    pub fn root(root: NodeId) -> Self { Self(vec![root]) }

    pub fn from_ids(ids: Vec<NodeId>) -> Option<Self> {
        (!ids.is_empty()).then_some(Self(ids))
    }

    pub fn as_slice(&self) -> &[NodeId] { &self.0 }
    pub fn current(&self) -> NodeId { *self.0.last().expect("view path is non-empty") }

    pub fn rebuild(
        &mut self,
        target: NodeId,
        mut parent_of: impl FnMut(NodeId) -> Option<NodeId>,
    ) -> Option<()> {
        let root = self.0[0];
        let mut reverse = vec![target];
        let mut current = target;
        while current != root {
            current = parent_of(current)?;
            if reverse.contains(&current) { return None; }
            reverse.push(current);
        }
        reverse.reverse();
        self.0 = reverse;
        Some(())
    }

    pub fn back(&mut self) -> Option<NodeId> {
        (self.0.len() > 1).then(|| {
            self.0.pop();
            self.current()
        })
    }

    pub fn jump_to(&mut self, target: NodeId) -> Option<NodeId> {
        let index = self.0.iter().position(|id| *id == target)?;
        self.0.truncate(index + 1);
        Some(self.current())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreemapState {
    pub view_path: ViewPath,
    pub selected: Option<NodeId>,
    pub pinned: Option<NodeId>,
    pub aggregate: Option<AggregateSelection>,
}

impl TreemapState {
    pub fn new(root: NodeId) -> Self {
        Self { view_path: ViewPath::root(root), selected: None, pinned: None, aggregate: None }
    }

    pub fn apply(&mut self, action: TreemapAction) {
        match action {
            TreemapAction::ActivateBaseDirectory(id) => {
                self.selected = Some(id);
                self.pinned = Some(id);
                self.aggregate = None;
            }
            TreemapAction::ActivateBaseLeaf(id) => {
                self.selected = Some(id);
                self.pinned = None;
                self.aggregate = None;
            }
            TreemapAction::ActivateNested(id) => {
                self.selected = Some(id);
                self.aggregate = None;
            }
            TreemapAction::Zoom(id) => {
                self.selected = Some(id);
                self.pinned = None;
                self.aggregate = None;
            }
            TreemapAction::OpenAggregate(selection) => {
                self.selected = Some(selection.parent);
                self.pinned = None;
                self.aggregate = Some(selection);
            }
            TreemapAction::ClearPreview => self.pinned = None,
        }
    }

    pub fn repair(
        &mut self,
        mut exists: impl FnMut(NodeId) -> bool,
        mut parent_of: impl FnMut(NodeId) -> Option<NodeId>,
    ) {
        let invalid = self.view_path.as_slice().windows(2).position(|pair| {
            !exists(pair[1]) || parent_of(pair[1]) != Some(pair[0])
        });
        if let Some(index) = invalid {
            self.view_path.0.truncate(index + 1);
        }
        let current = self.view_path.current();
        if self.selected.is_none_or(|id| !exists(id)) { self.selected = Some(current); }
        if self.pinned.is_some_and(|id| !exists(id)) { self.pinned = None; }
        if self.aggregate.as_ref().is_some_and(|group| {
            !exists(group.parent) || group.members.iter().any(|id| !exists(*id))
        }) { self.aggregate = None; }
    }
}
```

Add `mod treemap_state;` and re-export the four public types from `crates/voidspace-app/src/lib.rs`.

- [ ] **Step 4: Run the UI-state tests and commit**

Run:

```powershell
cargo test -p voidspace-app --test ui_state
```

Expected: all existing and new tests pass.

Commit:

```powershell
git add crates/voidspace-app/src/treemap_state.rs crates/voidspace-app/src/lib.rs crates/voidspace-app/tests/ui_state.rs
git commit -m "feat: add canonical treemap navigation state"
```

### Task 3: Emit one typed action for pointer and keyboard activation

**Files:**
- Modify: `crates/voidspace-app/src/treemap.rs`
- Test: `crates/voidspace-app/src/treemap.rs` unit tests

- [ ] **Step 1: Write failing action-arbitration tests**

Add a `#[cfg(test)] mod interaction_tests` using a pure hit description:

```rust
#[test]
fn recognized_double_click_supersedes_pin() {
    let hit = ActionHit::base_directory(NodeId(7));
    assert_eq!(action_for_hit(&hit, Activation::Single), TreemapAction::ActivateBaseDirectory(NodeId(7)));
    assert_eq!(action_for_hit(&hit, Activation::Double), TreemapAction::Zoom(NodeId(7)));
}

#[test]
fn nested_leaf_preserves_pin_and_other_never_zooms() {
    let nested = ActionHit::nested(NodeId(8), false);
    assert_eq!(action_for_hit(&nested, Activation::Single), TreemapAction::ActivateNested(NodeId(8)));
    assert_eq!(action_for_hit(&nested, Activation::KeyboardZoom), TreemapAction::ActivateNested(NodeId(8)));

    let expandable_nested = ActionHit::nested(NodeId(11), true);
    assert_eq!(action_for_hit(&expandable_nested, Activation::KeyboardZoom), TreemapAction::Zoom(NodeId(11)));

    let other = ActionHit::aggregate(NodeId(2), 1, vec![NodeId(9), NodeId(10)]);
    assert!(matches!(action_for_hit(&other, Activation::Double), TreemapAction::OpenAggregate(_)));
}

#[test]
fn leaves_never_zoom_and_recognized_second_click_clears_the_first_click_pin() {
    let leaf = ActionHit::base_leaf(NodeId(12));
    assert_eq!(action_for_hit(&leaf, Activation::Double), TreemapAction::ActivateBaseLeaf(NodeId(12)));

    let root = NodeId(1);
    let directory = NodeId(7);
    let hit = ActionHit::base_directory(directory);
    let mut state = TreemapState::new(root);
    state.apply(action_for_hit(&hit, Activation::Single));
    assert_eq!(state.pinned, Some(directory));
    state.apply(action_for_hit(&hit, Activation::Double));
    assert_eq!(state.selected, Some(directory));
    assert_eq!(state.pinned, None);
    assert_eq!(state.aggregate, None);
}
```

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```powershell
cargo test -p voidspace-app treemap::interaction_tests --lib
```

Expected: compilation fails because `ActionHit`, `Activation`, and `action_for_hit` do not exist.

- [ ] **Step 3: Add the pure action model without changing the public renderer contract yet**

Import `AggregateSelection`, `TreemapAction`, and `TreemapState` from `crate::treemap_state` (`TreemapState` is test-only here) before adding the action types below.

Add:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Activation { Single, Double, KeyboardZoom }

#[derive(Clone, Debug, Eq, PartialEq)]
enum HitKind {
    BaseDirectory,
    BaseLeaf,
    Nested { expandable: bool },
    Aggregate { parent: NodeId, depth: u8, members: Vec<NodeId> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionHit { node_id: NodeId, kind: HitKind }

impl ActionHit {
    fn base_directory(node_id: NodeId) -> Self {
        Self { node_id, kind: HitKind::BaseDirectory }
    }

    fn base_leaf(node_id: NodeId) -> Self {
        Self { node_id, kind: HitKind::BaseLeaf }
    }

    fn nested(node_id: NodeId, expandable: bool) -> Self {
        Self { node_id, kind: HitKind::Nested { expandable } }
    }

    fn aggregate(parent: NodeId, depth: u8, members: Vec<NodeId>) -> Self {
        Self { node_id: parent, kind: HitKind::Aggregate { parent, depth, members } }
    }
}
```

Implement `action_for_hit` with double/keyboard zoom only for expandable real directories and aggregate precedence:

```rust
fn action_for_hit(hit: &ActionHit, activation: Activation) -> TreemapAction {
    match &hit.kind {
        HitKind::Aggregate { parent, depth, members } => TreemapAction::OpenAggregate(AggregateSelection {
            parent: *parent,
            depth: *depth,
            members: members.clone(),
        }),
        HitKind::BaseDirectory if matches!(activation, Activation::Double | Activation::KeyboardZoom) =>
            TreemapAction::Zoom(hit.node_id),
        HitKind::Nested { expandable: true }
            if matches!(activation, Activation::Double | Activation::KeyboardZoom) =>
            TreemapAction::Zoom(hit.node_id),
        HitKind::BaseDirectory => TreemapAction::ActivateBaseDirectory(hit.node_id),
        HitKind::BaseLeaf => TreemapAction::ActivateBaseLeaf(hit.node_id),
        HitKind::Nested { .. } => TreemapAction::ActivateNested(hit.node_id),
    }
}
```

Keep the existing `TreemapResponse`, `VisibleHit`, and `show` signature unchanged in this task. Task 5 wires this tested pure arbitration into renderer and app in one compile-safe slice; this commit therefore introduces no temporary compatibility fields or duplicated app reducer.

- [ ] **Step 4: Run interaction tests and commit**

Run:

```powershell
cargo test -p voidspace-app treemap::interaction_tests --lib
```

Expected: all interaction tests pass, including leaf/`OTHER` non-zoom and first-click/recognized-double-click rollback cases.

Commit:

```powershell
git add crates/voidspace-app/src/treemap.rs
git commit -m "feat: add deterministic treemap actions"
```

### Task 4: Guarantee a measured size label on every emitted tile

**Files:**
- Modify: `crates/voidspace-app/src/treemap.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Test: `crates/voidspace-app/src/treemap.rs` unit tests

- [ ] **Step 1: Write failing compact-format and tier-selection tests**

Add:

```rust
#[test]
fn compact_sizes_are_short_and_unit_boundary_candidates_are_all_measured() {
    assert_eq!(compact_bytes(13 * 1024_u64.pow(3)), "13.0G");
    assert_eq!(compact_bytes(820 * 1024_u64.pow(2)), "820M");
    let labels = footprint_labels(&[
        717 * 1024_u64.pow(3),
        700 * 1024_u64.pow(2),
        323 * 1024_u64.pow(2),
    ], 1);
    assert!(labels.iter().any(|label| label == "1023M"));
}

#[test]
fn label_tiers_never_choose_name_only() {
    let metrics = LabelMeasurements {
        large_width: 120.0,
        large_height: 36.0,
        compact_width: 72.0,
        compact_height: 30.0,
        size_width: 38.0,
        size_height: 14.0,
    };
    assert_eq!(choose_label_tier([160.0, 50.0], metrics), Some(LabelTier::Large));
    assert_eq!(choose_label_tier([90.0, 34.0], metrics), Some(LabelTier::Compact));
    assert_eq!(choose_label_tier([45.0, 20.0], metrics), Some(LabelTier::SizeOnly));
    assert_eq!(choose_label_tier([30.0, 12.0], metrics), None);
}
```

- [ ] **Step 2: Run focused tests and verify red**

Run:

```powershell
cargo test -p voidspace-app treemap::label_tests --lib
```

Expected: unresolved formatter/measurement/tier symbols.

- [ ] **Step 3: Implement compact formatting and all suffix candidates**

Add:

```rust
pub fn compact_bytes(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else if value < 100.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

fn footprint_labels(sorted_sizes: &[u64], real_budget: usize) -> Vec<String> {
    let keep_limit = sorted_sizes.len().min(real_budget).min(128);
    let mut labels = sorted_sizes[..keep_limit]
        .iter()
        .map(|size| compact_bytes(*size))
        .collect::<Vec<_>>();
    let mut suffix = sorted_sizes[keep_limit..]
        .iter()
        .copied()
        .fold(0_u64, u64::saturating_add);
    labels.push(compact_bytes(suffix));
    for size in sorted_sizes[..keep_limit].iter().rev() {
        suffix = suffix.saturating_add(*size);
        labels.push(compact_bytes(suffix));
    }
    labels
}
```

- [ ] **Step 4: Measure the outer footprint before each base/nested layout**

Add a `minimum_label_footprint` helper using the same monospace font as size painting:

```rust
pub fn minimum_label_footprint(
    ui: &egui::Ui,
    snapshot: &IndexSnapshot,
    root: NodeId,
    real_budget: usize,
) -> LabelFootprint {
    let mut sizes = snapshot.node(root).into_iter()
        .flat_map(|node| node.children.iter())
        .filter_map(|id| snapshot.node(*id).map(|node| node.allocated))
        .filter(|size| *size > 0)
        .collect::<Vec<_>>();
    sizes.sort_unstable_by(|left, right| right.cmp(left));
    let font = FontId::monospace(10.0);
    let max_text_width = footprint_labels(&sizes, real_budget).into_iter().fold(0.0_f32, |width, text| {
        width.max(ui.painter().layout_no_wrap(text, font.clone(), theme::TILE_MUTED).size().x)
    });
    LabelFootprint::new(
        max_text_width + 2.0 * (TILE_INSET + 5.0) + 2.0,
        14.0 + 2.0 * (TILE_INSET + 4.0) + 2.0,
    )
}
```

In `VoidspaceApp::workspace`, compute this before the base `layout` call and pass it as `ViewState.min_label`. In `treemap::show`, compute it again for `preview_root` after subtracting `PREVIEW_HEADER`, then pass it to the nested `ViewState`.

- [ ] **Step 5: Replace early-return labels with measured Large/Compact/SizeOnly tiers**

Define `LabelTier`, `LabelMeasurements`, and `choose_label_tier`. Measure the exact normal and compact size galleys with `Painter::layout_no_wrap`; derive the two-line heights from the actual font row heights, and use `inner = rect.shrink(TILE_INSET + 5.0)` for both tier selection and painting. Names may be ellipsized to the remaining row width, but size strings may never be truncated or dropped. Change `paint_tile` so it always paints a size for every emitted tile. The core match must be:

```rust
match choose_label_tier([inner.width(), inner.height()], measurements) {
    Some(LabelTier::Large) => {
        painter.text(name_pos, Align2::LEFT_TOP, clipped_name, FontId::proportional(14.0), name_color);
        painter.text(size_pos, Align2::LEFT_TOP, format_bytes(size), FontId::monospace(10.0), theme::TILE_MUTED);
    }
    Some(LabelTier::Compact) => {
        painter.text(name_pos, Align2::LEFT_TOP, compact_name, FontId::proportional(11.0), name_color);
        painter.text(compact_size_pos, Align2::LEFT_TOP, compact_bytes(size), FontId::monospace(9.0), theme::TILE_MUTED);
    }
    Some(LabelTier::SizeOnly) => {
        painter.text(inner.center(), Align2::CENTER_CENTER, compact_bytes(size), FontId::monospace(9.0), theme::TEXT);
    }
    None => {
        debug_assert!(false, "layout emitted a tile below its label footprint");
        painter.with_clip_rect(rect).text(
            rect.center(),
            Align2::CENTER_CENTER,
            compact_bytes(size),
            FontId::monospace(9.0),
            theme::TEXT,
        );
    }
}
```

Paint `PINNED` in the preview header and use lime for the secondary pinned outline while retaining orange only for `selected`.

When either the base or nested layout contains an `AggregateGroup` for its current root but no depth-one node because its canvas misses `min_label`, call the same helper with that layout's actual bounds and render a centered non-tile fallback instead of calling `paint_tile`:

```rust
fn paint_layout_overflow(
    painter: &egui::Painter,
    layout: &LayoutSnapshot,
    bounds: Rect,
) -> bool {
    let Some(group) = layout.aggregates.iter().find(|group| {
        group.parent_id == layout.root && group.depth == 1
    }) else { return false; };
    if layout.nodes.iter().any(|node| node.depth == 1) { return false; }
    painter.text(
        bounds.center(),
        Align2::CENTER_CENTER,
        format!("Not enough room · {}", format_bytes(group.size)),
        FontId::monospace(11.0),
        theme::TILE_MUTED,
    );
    true
}
```

- [ ] **Step 6: Run label and app tests, then commit**

Run:

```powershell
cargo test -p voidspace-app treemap::label_tests --lib
cargo test -p voidspace-app
```

Expected: all tests pass; every `ViewState` constructor includes `min_label`.

Commit:

```powershell
git add crates/voidspace-app/src/treemap.rs crates/voidspace-app/src/app.rs
git commit -m "feat: label every visible treemap tile"
```

### Task 5: Integrate actions, canonical breadcrumb, and exact aggregate details

**Files:**
- Modify: `crates/voidspace-app/src/treemap.rs`
- Modify: `crates/voidspace-app/src/app.rs`
- Modify: `crates/voidspace-app/tests/ui_state.rs`

- [ ] **Step 1: Migrate `ScanTab` to the new state boundary**

Replace:

```rust
selected: Option<NodeId>,
view_root: NodeId,
history: Vec<NodeId>,
show_other_for: Option<(NodeId, u32)>,
preview: treemap::PreviewState,
```

with:

```rust
treemap_state: TreemapState,
```

Initialize and reset with:

```rust
treemap_state: TreemapState::new(snapshot.root),
```

Use `tab.treemap_state.view_path.current()` wherever layout or inspector needs `view_root`, `tab.treemap_state.selected` for selection, and `PreviewState { pinned: tab.treemap_state.pinned }` for painting. Pass `tab.treemap_state.aggregate.as_ref()` as the new final argument to `treemap::show`; immediately after the call, apply its passive exact-membership validation before applying the optional action:

```rust
if !response.aggregate_still_valid {
    tab.treemap_state.aggregate = None;
}
if let Some(action) = response.action {
    tab.apply_treemap_action(action);
}
```

- [ ] **Step 2: Add a single action application path**

Add `ScanTab::apply_treemap_action`:

```rust
fn apply_treemap_action(&mut self, action: TreemapAction) {
    match action {
        TreemapAction::Zoom(target) => {
            if self.snapshot.node(target).is_some_and(|node| !node.children.is_empty()) {
                let root = self.snapshot.root;
                if self.treemap_state.view_path.rebuild(target, |id| {
                    self.snapshot.node(id).and_then(|node| node.parent)
                }).is_none() {
                    self.treemap_state.view_path = ViewPath::root(root);
                    self.errors.push("Cannot build breadcrumb path for zoom target".into());
                    return;
                }
                self.treemap_state.apply(TreemapAction::Zoom(target));
            }
        }
        other => self.treemap_state.apply(other),
    }
}
```

In the same compile-safe slice, replace `TreemapResponse` with:

```rust
pub struct TreemapResponse {
    pub action: Option<TreemapAction>,
    /// True when the currently open exact aggregate still exists in a layout rendered this frame.
    pub aggregate_still_valid: bool,
}
```

Extend `VisibleHit` and convert it to the already-tested pure `ActionHit`:

```rust
#[derive(Clone, Debug)]
struct VisibleHit {
    node_id: NodeId,
    rect: Rect,
    depth: u8,
    aggregated: bool,
    expandable: bool,
    name: String,
    formatted_size: String,
    aggregate_members: Vec<NodeId>,
}

impl VisibleHit {
    fn action_hit(&self) -> ActionHit {
        if self.aggregated {
            ActionHit::aggregate(self.node_id, self.depth, self.aggregate_members.clone())
        } else if self.depth > 1 {
            ActionHit::nested(self.node_id, self.expandable)
        } else if self.expandable {
            ActionHit::base_directory(self.node_id)
        } else {
            ActionHit::base_leaf(self.node_id)
        }
    }
}
```

Build one egui interaction overlay per hit with a stable ID and accessibility label:

```rust
let tile_response = ui.interact(
    hit.rect,
    ui.id().with((base_layout.root, hit.node_id, hit.depth, hit.aggregated)),
    Sense::click(),
);
tile_response.widget_info(|| egui::WidgetInfo::labeled(
    egui::WidgetType::Button,
    true,
    format!("{} · {}{}", hit.name, hit.formatted_size, if hit.expandable { " · expandable" } else { "" }),
));
```

Create base overlays first and nested overlays second, then resolve nested responses in reverse paint order before base responses so an inline child always wins over its enclosing tile. Resolve activation in this strict order across that z-ordered list: `double_clicked`, focused `Ctrl+Enter`, `clicked`/focused Enter/Space. Return the first action only. If no tile action exists and the canvas receives a single click or Escape, return `ClearPreview`.

Use capability-aware hover text: expandable real directories get `Click: inspect · Double-click: zoom`, leaves get `Click: inspect`, and aggregate tiles get `Click: inspect grouped items`.

Change `show` to receive `open_aggregate: Option<&AggregateSelection>`. Resolve aggregate hit members from each layout's `AggregateGroup` by `(parent_id, local depth)`; expose base hits at render depth `1` and nested hits at render depth `2`. Collect groups from both layouts, including groups with no drawable aggregate rectangle, and set:

```rust
fn aggregate_is_still_valid(
    open_aggregate: Option<&AggregateSelection>,
    rendered_aggregates: &[RenderedAggregate],
) -> bool {
    open_aggregate.is_none_or(|open| {
    rendered_aggregates.iter().any(|rendered| {
        rendered.parent == open.parent
            && rendered.depth == open.depth
            && rendered.members == open.members
    })
    })
}
```

This passive validity bit is not an input action; `TreemapResponse.action` still contains at most one activation. It closes exact `OTHER` details after a live boundary change, including nested aggregates owned by `treemap::show`.

Add a focused unit test with one exact match plus changed-members, changed-depth, and missing-group cases; only the exact ordered ID list may return `true`.

Delete the existing `clicked`, `double_clicked`, `aggregate_clicked`, and `pin_clicked` branches; the block above is their only replacement.

- [ ] **Step 3: Render the persistent navigation strip before allocating treemap bounds**

Add a helper that returns an optional breadcrumb target:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationIntent {
    None,
    Back,
    Jump(NodeId),
}

fn treemap_navigation(ui: &mut egui::Ui, tab: &ScanTab) -> NavigationIntent {
    let mut intent = NavigationIntent::None;
    ui.horizontal(|ui| {
        if ui.add_enabled(tab.treemap_state.view_path.as_slice().len() > 1, egui::Button::new("← BACK")).clicked() {
            intent = NavigationIntent::Back;
        }
        for (index, node_id) in tab.treemap_state.view_path.as_slice().iter().copied().enumerate() {
            if index > 0 { ui.label(egui::RichText::new("›").color(theme::MUTED)); }
            let label = tab.snapshot.node(node_id).map(|node| node.name.display_escaped()).unwrap_or_else(|| "?".into());
            if index + 1 == tab.treemap_state.view_path.as_slice().len() {
                ui.label(egui::RichText::new(label).color(theme::ORANGE).strong());
            } else if ui.link(label).clicked() {
                intent = NavigationIntent::Jump(node_id);
            }
        }
    });
    intent
}
```

Apply `Back`, breadcrumb `Jump`, and `Alt+Left` through one helper that truncates `view_path`, selects the resulting root, and clears pin/aggregate state:

```rust
fn apply_navigation(tab: &mut ScanTab, intent: NavigationIntent) {
    let target = match intent {
        NavigationIntent::None => None,
        NavigationIntent::Back => tab.treemap_state.view_path.back(),
        NavigationIntent::Jump(node_id) => tab.treemap_state.view_path.jump_to(node_id),
    };
    if let Some(target) = target {
        tab.treemap_state.selected = Some(target);
        tab.treemap_state.pinned = None;
        tab.treemap_state.aggregate = None;
    }
}
```

Consume the keyboard shortcut before the navigation strip is rendered:

```rust
if ui.ctx().input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)) {
    apply_navigation(tab, NavigationIntent::Back);
}
let navigation = treemap_navigation(ui, tab);
apply_navigation(tab, navigation);
```

Keep the inspector `ZOOM INTO` and `BACK` buttons, but route them through the same `TreemapAction::Zoom` and canonical `ViewPath::back` path so the two navigation surfaces cannot diverge.

- [ ] **Step 4: Render exact `OTHER` members and repair state after snapshot updates**

Replace `take(other_count)` guessing with exact IDs:

```rust
if let Some(group) = &tab.treemap_state.aggregate {
    for child_id in &group.members {
        if let Some(child) = tab.snapshot.node(*child_id) {
            ui.horizontal(|ui| {
                ui.label(child.name.display_escaped());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(treemap::format_bytes(child.allocated));
                });
            });
        }
    }
}
```

After assigning a fresh snapshot, call:

```rust
tab.treemap_state.repair(
    |id| tab.snapshot.node(id).is_some(),
    |id| tab.snapshot.node(id).and_then(|node| node.parent),
);
```

Do not guess the aggregate suffix again in `app.rs`: the `aggregate_still_valid` bit returned from the base and nested layouts in `treemap::show` performs the exact `(parent, render depth, ordered member IDs)` comparison.

After each new base layout, clear a pin that is no longer a depth-one, non-aggregate, expandable node in that layout:

```rust
let pin_is_eligible = tab.treemap_state.pinned.is_some_and(|pinned| {
    tab.layout.nodes.iter().any(|node| {
        node.depth == 1
            && !node.aggregated
            && node.node_id == pinned
            && node.rect.width() >= 150.0
            && node.rect.height() >= 100.0
            && tab.snapshot.node(pinned).is_some_and(|entry| !entry.children.is_empty())
    })
});
if tab.treemap_state.pinned.is_some() && !pin_is_eligible {
    tab.treemap_state.pinned = None;
}
```

- [ ] **Step 5: Add integration tests for pin → nested zoom → Back and direct breadcrumb**

Extend `ui_state.rs` with:

```rust
#[test]
fn pin_nested_zoom_back_and_breadcrumb_share_one_path() {
    let root = NodeId(1);
    let base = NodeId(2);
    let nested = NodeId(3);
    let mut state = TreemapState::new(root);
    state.apply(TreemapAction::ActivateBaseDirectory(base));
    state.apply(TreemapAction::ActivateNested(nested));
    state.view_path.rebuild(nested, |id| match id {
        NodeId(2) => Some(root),
        NodeId(3) => Some(base),
        _ => None,
    }).unwrap();
    state.apply(TreemapAction::Zoom(nested));

    assert_eq!(state.view_path.back(), Some(base));
    assert_eq!(state.view_path.jump_to(root), Some(root));
    assert_eq!(state.view_path.as_slice(), &[root]);
}
```

- [ ] **Step 6: Run app tests and commit**

Run:

```powershell
cargo test -p voidspace-app
```

Expected: all app lib/UI-state tests pass; inspector and canvas tests use the same state types.

Commit:

```powershell
git add crates/voidspace-app/src/treemap.rs crates/voidspace-app/src/app.rs crates/voidspace-app/tests/ui_state.rs
git commit -m "feat: add treemap breadcrumb navigation"
```

### Task 6: Update user guidance and run fast quality gates

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the interaction documentation**

Add this bullet block under `## Возможности`:

```markdown
- hover временно показывает содержимое крупной папки внутри её плитки;
- один клик закрепляет раскрытие, двойной клик разворачивает папку на весь treemap;
- `← BACK`, breadcrumb и `Alt+←` возвращают к родительской папке;
- каждая видимая плитка показывает размер, а неподписываемая мелочь собирается в `OTHER`.
```

- [ ] **Step 2: Run formatting, Clippy, and focused workspace tests**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy -p voidspace-layout -p voidspace-app --all-targets -- -D warnings
cargo test -p voidspace-layout
cargo test -p voidspace-app
```

Expected: every command exits `0`; no Clippy warnings; all new footprint/action/navigation tests pass.

- [ ] **Step 3: Review the owned diff and commit docs/final cleanup**

Run:

```powershell
git diff --check
git diff --stat
git status --short
```

Expected: no whitespace errors; only task-owned source/tests/docs plus the pre-existing untracked `artifacts/` directory.

Commit:

```powershell
git add README.md
git commit -m "docs: explain treemap drill-down controls"
```

### Task 7: Package and manually verify the exact release candidate

**Files:**
- Create: `scripts/install-local.ps1`
- Modify: `scripts/package.ps1`
- Generated: `dist/Voidspace-0.1.0-windows-x64/voidspace.exe`
- Generated: `dist/Voidspace-0.1.0-windows-x64.zip`
- Generated: `%LOCALAPPDATA%\Voidspace\voidspace.exe`
- Generated: `%USERPROFILE%\Desktop\Voidspace.lnk`

- [ ] **Step 1: Add a stable per-user install and Desktop shortcut updater**

Create `scripts/install-local.ps1` with a mandatory `SourceDir`. Resolve and validate that source, require `voidspace.exe` and `voidspace-elevated.exe`, and install into the fixed path `%LOCALAPPDATA%\Voidspace`. Stop only running Voidspace processes whose resolved `ExecutablePath` equals one of those two fixed installed executables; never stop by process name alone.

Copy each candidate to a same-directory `.new` file and atomically replace/move it into place. Recreate `%USERPROFILE%\Desktop\Voidspace.lnk` with Windows Script Host so its target and working directory are independent of this repository/worktree:

```powershell
param([Parameter(Mandatory)][string]$SourceDir)
$ErrorActionPreference = 'Stop'

$source = (Resolve-Path -LiteralPath $SourceDir).Path
$installDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Voidspace'
$installExe = Join-Path $installDir 'voidspace.exe'
$installedExecutables = @($installExe, (Join-Path $installDir 'voidspace-elevated.exe')) |
    ForEach-Object { [IO.Path]::GetFullPath($_) }
$desktop = [Environment]::GetFolderPath('Desktop')
$shortcutPath = Join-Path $desktop 'Voidspace.lnk'

$running = Get-CimInstance Win32_Process | Where-Object {
    if (-not $_.ExecutablePath) { return $false }
    $processPath = [IO.Path]::GetFullPath($_.ExecutablePath)
    return $installedExecutables -contains $processPath
}
$running | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
foreach ($name in @('voidspace.exe', 'voidspace-elevated.exe', 'README.md', 'LICENSE', 'SHA256SUMS.txt')) {
    $from = Join-Path $source $name
    if (-not (Test-Path -LiteralPath $from -PathType Leaf)) { throw "Missing packaged file: $name" }
    $to = Join-Path $installDir $name
    $next = "$to.new"
    Copy-Item -LiteralPath $from -Destination $next -Force
    if (Test-Path -LiteralPath $to) {
        [IO.File]::Replace($next, $to, $null)
    } else {
        Move-Item -LiteralPath $next -Destination $to
    }
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $installExe
$shortcut.WorkingDirectory = $installDir
$shortcut.IconLocation = "$installExe,0"
$shortcut.Save()
```

Read the shortcut back through the same COM API, compare its normalized `TargetPath`/`WorkingDirectory`, compare installed and packaged `voidspace.exe` SHA-256 values, then emit:

```text
VOIDSPACE_DESKTOP_OK C:\Users\user\Desktop\Voidspace.lnk C:\Users\user\AppData\Local\Voidspace\voidspace.exe <sha256>
```

At the end of `scripts/package.ps1`, invoke `install-local.ps1 -SourceDir $stage`; fail the package command if installation or read-back verification fails. This makes every successful local package run refresh the same Desktop entry to the exact candidate that passed formatting, Clippy, tests, release build, and smoke.

- [ ] **Step 2: Parse-check the PowerShell and commit the installer hook**

Run:

```powershell
[void][scriptblock]::Create((Get-Content -Raw -LiteralPath '.\scripts\install-local.ps1'))
[void][scriptblock]::Create((Get-Content -Raw -LiteralPath '.\scripts\package.ps1'))
git diff --check
```

Expected: both scripts parse without exception and the diff has no whitespace errors.

Commit:

```powershell
git add scripts/install-local.ps1 scripts/package.ps1
git commit -m "build: keep desktop shortcut on latest release"
```

- [ ] **Step 3: Close only worktree candidates that can lock build/package output**

Run the existing exact-path process check before packaging:

```powershell
$candidates = @(
    'C:\Users\user\.github\voidspace\.worktrees\voidspace-v1\dist\Voidspace-0.1.0-windows-x64\voidspace.exe',
    'C:\Users\user\.github\voidspace\.worktrees\voidspace-v1\target\release\voidspace.exe'
)
$matches = Get-CimInstance Win32_Process -Filter "Name='voidspace.exe'" |
    Where-Object { $candidates -contains $_.ExecutablePath }
$matches | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
```

Expected: only exact worktree build/package candidates are stopped; other processes are untouched. The installer handles only exact fixed-install paths later.

- [ ] **Step 4: Run the full package and local-install pipeline**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

Expected output contains all three markers:

```text
VOIDSPACE_SMOKE_OK
VOIDSPACE_DESKTOP_OK C:\Users\user\Desktop\Voidspace.lnk C:\Users\user\AppData\Local\Voidspace\voidspace.exe <sha256>
VOIDSPACE_PACKAGE_OK ...\Voidspace-0.1.0-windows-x64.zip <sha256>
```

- [ ] **Step 5: Verify the installed executable and Desktop shortcut are the exact candidate**

Run:

```powershell
$stageExe = (Resolve-Path '.\dist\Voidspace-0.1.0-windows-x64\voidspace.exe').Path
$installExe = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Voidspace\voidspace.exe'
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Voidspace.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
if ([IO.Path]::GetFullPath($shortcut.TargetPath) -ne [IO.Path]::GetFullPath($installExe)) { throw 'Desktop shortcut target mismatch' }
if ([IO.Path]::GetFullPath($shortcut.WorkingDirectory) -ne [IO.Path]::GetFullPath((Split-Path $installExe))) { throw 'Desktop shortcut working directory mismatch' }
if ((Get-FileHash $stageExe).Hash -ne (Get-FileHash $installExe).Hash) { throw 'Installed executable hash mismatch' }
[pscustomobject]@{
    Shortcut = $shortcutPath
    Target = $shortcut.TargetPath
    WorkingDirectory = $shortcut.WorkingDirectory
    Sha256 = (Get-FileHash $installExe).Hash.ToLowerInvariant()
} | Format-List
```

Expected: the shortcut targets `%LOCALAPPDATA%\Voidspace\voidspace.exe`, its working directory is `%LOCALAPPDATA%\Voidspace`, and installed/staged hashes are identical.

- [ ] **Step 6: Verify the installed UI against both observed cases**

Launch `Voidspace.lnk` from the Desktop (not a repo path), confirm UAC once, scan the volume containing the observed folders, and verify:

1. `Traum_v2.31`: hover previews children; one click shows persistent lime `PINNED` while orange selection remains distinct; double click fills the treemap canvas; `← BACK` returns to `books (FLIBUSTA)`.
2. Nested child: one click preserves the enclosing pin; double click builds a breadcrumb containing the skipped real parent; Back returns to that parent.
3. Google Drive area: every visible rectangle contains a normal or compact size; no blank micro-rectangles remain; their bytes/count appear in exact `OTHER` details.
4. Resize through widths above and below 900 px; labels do not overlap, breadcrumb stays visible, inspector/drawer still works.
5. Click the orange `TURBO ACTIVE` indicator; process/window count remains one.
6. Close and reopen the app from the same Desktop shortcut; Windows starts the installed executable whose SHA-256 was verified above.

- [ ] **Step 7: Capture release evidence and final commit status**

Run:

```powershell
git status --short
git log -8 --oneline
(Get-FileHash -Algorithm SHA256 '.\dist\Voidspace-0.1.0-windows-x64.zip').Hash.ToLowerInvariant()
(Get-FileHash -Algorithm SHA256 (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Voidspace\voidspace.exe')).Hash.ToLowerInvariant()
```

Expected: source work is committed; only pre-existing `artifacts/` is untracked; the archive SHA-256 matches `VOIDSPACE_PACKAGE_OK`, and the installed executable SHA-256 matches both the staged candidate and `VOIDSPACE_DESKTOP_OK`.

## Plan self-review

- Spec coverage: tasks cover hover/pin, cross-frame double-click rollback, nested direct zoom, canonical ancestry, Back/breadcrumb/Alt+Left, exact aggregate membership, saturating conservation, label footprint including suffix unit boundaries, all three label tiers, accessibility overlays, live repair, docs, package, stable per-user installation, verified Desktop shortcut refresh, and observed-case manual verification.
- Placeholder scan: the plan contains no unresolved markers, deferred implementation, or generic error-handling step; each source change has an explicit type/function/transition contract.
- Type consistency: `TreemapAction`, `AggregateSelection`, `ViewPath`, `TreemapState`, `LabelFootprint`, and `AggregateGroup` retain the same names and roles across all tasks.
