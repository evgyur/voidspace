use voidspace_model::NodeId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateSelection {
    pub parent: NodeId,
    pub depth: usize,
    pub members: Vec<NodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreemapAction {
    ActivateBaseDirectory(NodeId),
    ActivateBaseLeaf(NodeId),
    ActivateNested(NodeId),
    Zoom(NodeId),
    OpenAggregate(AggregateSelection),
    ZoomAggregate(AggregateSelection),
    ClearPreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewPath(Vec<NodeId>);

impl ViewPath {
    pub fn root(root: NodeId) -> Self {
        Self(vec![root])
    }

    pub fn from_ids(ids: Vec<NodeId>) -> Option<Self> {
        (!ids.is_empty()).then_some(Self(ids))
    }

    pub fn as_slice(&self) -> &[NodeId] {
        &self.0
    }

    pub fn current(&self) -> NodeId {
        *self.0.last().expect("view path is non-empty")
    }

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
            if reverse.contains(&current) {
                return None;
            }
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
    pub aggregate_views: Vec<AggregateSelection>,
}

impl TreemapState {
    pub fn new(root: NodeId) -> Self {
        Self {
            view_path: ViewPath::root(root),
            selected: None,
            pinned: None,
            aggregate: None,
            aggregate_views: Vec::new(),
        }
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
                self.aggregate_views.clear();
            }
            TreemapAction::OpenAggregate(selection) => {
                self.selected = Some(selection.parent);
                self.pinned = None;
                self.aggregate = Some(selection);
            }
            TreemapAction::ZoomAggregate(selection) => {
                self.selected = Some(selection.parent);
                self.pinned = None;
                self.aggregate = None;
                if self.aggregate_views.last() != Some(&selection) {
                    self.aggregate_views.push(selection);
                }
            }
            TreemapAction::ClearPreview => self.pinned = None,
        }
    }

    pub fn back(&mut self) -> Option<NodeId> {
        if self.aggregate_views.pop().is_some() {
            self.pinned = None;
            self.aggregate = None;
            let target = self
                .aggregate_views
                .last()
                .map_or_else(|| self.view_path.current(), |group| group.parent);
            self.selected = Some(target);
            return Some(target);
        }
        self.view_path.back()
    }

    pub fn jump_to(&mut self, target: NodeId) -> Option<NodeId> {
        self.aggregate_views.clear();
        self.view_path.jump_to(target)
    }

    pub fn repair(
        &mut self,
        mut exists: impl FnMut(NodeId) -> bool,
        mut parent_of: impl FnMut(NodeId) -> Option<NodeId>,
    ) {
        let invalid = self
            .view_path
            .as_slice()
            .windows(2)
            .position(|pair| !exists(pair[1]) || parent_of(pair[1]) != Some(pair[0]));
        if let Some(index) = invalid {
            self.view_path.0.truncate(index + 1);
        }
        let current = self.view_path.current();
        if self.selected.is_none_or(|id| !exists(id)) {
            self.selected = Some(current);
        }
        if self.pinned.is_some_and(|id| !exists(id)) {
            self.pinned = None;
        }
        if self
            .aggregate
            .as_ref()
            .is_some_and(|group| !valid_group(group, &mut exists, &mut parent_of))
        {
            self.aggregate = None;
        }
        self.aggregate_views
            .retain(|group| valid_group(group, &mut exists, &mut parent_of));
    }
}

fn valid_group(
    group: &AggregateSelection,
    exists: &mut impl FnMut(NodeId) -> bool,
    parent_of: &mut impl FnMut(NodeId) -> Option<NodeId>,
) -> bool {
    exists(group.parent)
        && !group.members.is_empty()
        && group
            .members
            .iter()
            .all(|id| exists(*id) && parent_of(*id) == Some(group.parent))
}
