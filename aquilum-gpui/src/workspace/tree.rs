use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::document_buffer::DocumentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LeafId(pub Uuid);

impl LeafId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LeafId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub Uuid);

impl TabId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TabId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    /// Panels in a row, side-by-side (vertical divider between them)
    Row,
    /// Panels in a column, stacked vertically (horizontal divider between them)
    Column,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitDragState {
    pub path: Vec<usize>,
    pub direction: SplitDirection,
    pub divider_index: usize,
    pub start_pos: f32,
    pub span: f32,
    pub initial_ratio: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TabContent {
    Document { id: DocumentId },
    Empty,
    Plugin { view_id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub content: TabContent,
    pub pinned: bool,
}

impl Tab {
    pub fn new_document(doc_id: DocumentId) -> Self {
        Self {
            id: TabId::new(),
            content: TabContent::Document { id: doc_id },
            pinned: false,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            id: TabId::new(),
            content: TabContent::Empty,
            pinned: false,
        }
    }

    pub fn new_plugin(view_id: impl Into<String>) -> Self {
        Self {
            id: TabId::new(),
            content: TabContent::Plugin { view_id: view_id.into() },
            pinned: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeafState {
    pub id: LeafId,
    pub tabs: Vec<Tab>,
    pub active_tab_index: usize,
}

impl LeafState {
    pub fn new(id: LeafId) -> Self {
        Self {
            id,
            tabs: Vec::new(),
            active_tab_index: 0,
        }
    }

    pub fn with_initial_tab(id: LeafId, tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![tab],
            active_tab_index: 0,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn find_tab_by_doc(&self, doc: DocumentId) -> Option<(usize, &Tab)> {
        self.tabs.iter().enumerate().find(|(_, t)| {
            matches!(t.content, TabContent::Document { id } if id == doc)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WorkspaceNode {
    Split {
        direction: SplitDirection,
        children: Vec<WorkspaceNode>,
        ratios: Vec<f32>,
    },
    Leaf {
        id: LeafId,
    },
}

impl WorkspaceNode {
    pub fn leaves(&self) -> Vec<LeafId> {
        let mut result = Vec::new();
        self.collect_leaves(&mut result);
        result
    }

    fn collect_leaves(&self, out: &mut Vec<LeafId>) {
        match self {
            WorkspaceNode::Leaf { id } => out.push(*id),
            WorkspaceNode::Split { children, .. } => {
                for child in children {
                    child.collect_leaves(out);
                }
            }
        }
    }

    pub fn contains_leaf(&self, leaf_id: LeafId) -> bool {
        match self {
            WorkspaceNode::Leaf { id } => *id == leaf_id,
            WorkspaceNode::Split { children, .. } => {
                children.iter().any(|c| c.contains_leaf(leaf_id))
            }
        }
    }

    /// Recursively normalizes ratios to sum to 1.0.
    pub fn normalize_ratios(&mut self) {
        if let WorkspaceNode::Split { children, ratios, .. } = self {
            if children.is_empty() {
                return;
            }
            if ratios.len() != children.len() {
                let eq_ratio = 1.0 / (children.len() as f32);
                *ratios = vec![eq_ratio; children.len()];
            } else {
                let sum: f32 = ratios.iter().copied().sum();
                if sum > 0.0001 {
                    for r in ratios.iter_mut() {
                        *r /= sum;
                    }
                } else {
                    let eq_ratio = 1.0 / (children.len() as f32);
                    *ratios = vec![eq_ratio; children.len()];
                }
            }
            for child in children {
                child.normalize_ratios();
            }
        }
    }

    /// Recursively flattens any nested splits that share the same direction as their parent.
    /// For instance, Split(Row, [A, Split(Row, [B, C])]) becomes Split(Row, [A, B, C]).
    pub fn flatten_same_direction_splits(&mut self) {
        if let WorkspaceNode::Split { direction, children, ratios } = self {
            // First recursively flatten all children
            for child in children.iter_mut() {
                child.flatten_same_direction_splits();
            }

            let mut new_children = Vec::with_capacity(children.len());
            let mut new_ratios = Vec::with_capacity(ratios.len());

            for (idx, mut child) in children.drain(..).enumerate() {
                let parent_r = ratios.get(idx).copied().unwrap_or(1.0);
                match &mut child {
                    WorkspaceNode::Split { direction: child_dir, children: sub_children, ratios: sub_ratios }
                        if *child_dir == *direction =>
                    {
                        let sub_len = sub_children.len().max(1);
                        for (sub_idx, sub_child) in sub_children.drain(..).enumerate() {
                            let sub_r = sub_ratios.get(sub_idx).copied().unwrap_or(1.0 / sub_len as f32);
                            new_ratios.push(parent_r * sub_r);
                            new_children.push(sub_child);
                        }
                    }
                    _ => {
                        new_ratios.push(parent_r);
                        new_children.push(child);
                    }
                }
            }

            *children = new_children;
            *ratios = new_ratios;
        }
    }

    /// Top leaves: a leaf touches the top edge if across all Column ancestors,
    /// it resides in the first child (index 0). For Row ancestors, any child touches the top.
    pub fn top_leaves(&self) -> Vec<LeafId> {
        let mut result = Vec::new();
        self.collect_top_leaves(&mut result);
        result
    }

    fn collect_top_leaves(&self, out: &mut Vec<LeafId>) {
        match self {
            WorkspaceNode::Leaf { id } => out.push(*id),
            WorkspaceNode::Split { direction, children, .. } => {
                match direction {
                    SplitDirection::Row => {
                        for child in children {
                            child.collect_top_leaves(out);
                        }
                    }
                    SplitDirection::Column => {
                        // Only the first child in a column split touches the top edge
                        if let Some(first) = children.first() {
                            first.collect_top_leaves(out);
                        }
                    }
                }
            }
        }
    }

    pub fn collect_top_columns(
        &self,
        path: &[usize],
        current_ratio: f32,
        out: &mut Vec<TopColumn>,
    ) {
        match self {
            WorkspaceNode::Leaf { id } => {
                out.push(TopColumn {
                    leaf_id: *id,
                    ratio: current_ratio,
                    divider_after: None,
                });
            }
            WorkspaceNode::Split { direction, children, ratios } => {
                match direction {
                    SplitDirection::Column => {
                        if let Some(first) = children.first() {
                            let mut child_path = path.to_vec();
                            child_path.push(0);
                            first.collect_top_columns(&child_path, current_ratio, out);
                        }
                    }
                    SplitDirection::Row => {
                        let num_children = children.len();
                        for (idx, child) in children.iter().enumerate() {
                            let mut child_path = path.to_vec();
                            child_path.push(idx);
                            let r = ratios.get(idx).copied().unwrap_or(1.0 / num_children.max(1) as f32);
                            let before_len = out.len();
                            child.collect_top_columns(&child_path, current_ratio * r, out);
                            if idx + 1 < num_children && out.len() > before_len {
                                let last_idx = out.len() - 1;
                                out[last_idx].divider_after = Some(TopDividerInfo {
                                    path: path.to_vec(),
                                    divider_index: idx,
                                    initial_ratio: r,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn horizontal_span_of_leaf(&self, target: LeafId, start: f32, end: f32) -> Option<(f32, f32)> {
        match self {
            WorkspaceNode::Leaf { id } => {
                if *id == target {
                    Some((start, (end - start).max(0.01)))
                } else {
                    None
                }
            }
            WorkspaceNode::Split { direction, children, ratios } => {
                match direction {
                    SplitDirection::Column => {
                        for child in children {
                            if let Some(res) = child.horizontal_span_of_leaf(target, start, end) {
                                return Some(res);
                            }
                        }
                        None
                    }
                    SplitDirection::Row => {
                        let span = end - start;
                        let mut curr_start = start;
                        for (idx, child) in children.iter().enumerate() {
                            let r = ratios.get(idx).copied().unwrap_or(1.0 / children.len().max(1) as f32);
                            let child_end = curr_start + span * r;
                            if let Some(res) = child.horizontal_span_of_leaf(target, curr_start, child_end) {
                                return Some(res);
                            }
                            curr_start = child_end;
                        }
                        None
                    }
                }
            }
        }
    }

    pub fn horizontal_ratio_of_leaf(&self, target: LeafId, start: f32, end: f32) -> Option<f32> {
        self.horizontal_span_of_leaf(target, start, end).map(|(s, _)| s)
    }

    pub fn vertical_span_of_leaf(&self, target: LeafId, start: f32, end: f32) -> Option<(f32, f32)> {
        match self {
            WorkspaceNode::Leaf { id } => {
                if *id == target {
                    Some((start, (end - start).max(0.01)))
                } else {
                    None
                }
            }
            WorkspaceNode::Split { direction, children, ratios } => {
                match direction {
                    SplitDirection::Row => {
                        for child in children {
                            if let Some(res) = child.vertical_span_of_leaf(target, start, end) {
                                return Some(res);
                            }
                        }
                        None
                    }
                    SplitDirection::Column => {
                        let span = end - start;
                        let mut curr_start = start;
                        for (idx, child) in children.iter().enumerate() {
                            let r = ratios.get(idx).copied().unwrap_or(1.0 / children.len().max(1) as f32);
                            let child_end = curr_start + span * r;
                            if let Some(res) = child.vertical_span_of_leaf(target, curr_start, child_end) {
                                return Some(res);
                            }
                            curr_start = child_end;
                        }
                        None
                    }
                }
            }
        }
    }

    pub fn vertical_ratio_of_leaf(&self, target: LeafId, start: f32, end: f32) -> Option<f32> {
        self.vertical_span_of_leaf(target, start, end).map(|(s, _)| s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopColumn {
    pub leaf_id: LeafId,
    pub ratio: f32,
    pub divider_after: Option<TopDividerInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopDividerInfo {
    pub path: Vec<usize>,
    pub divider_index: usize,
    pub initial_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Normal click: open in active leaf, reusing existing tab if present in this leaf
    Reuse,
    /// Ctrl+Click / middle click: force a new tab in the active leaf even if duplicate
    ForceNewTab,
    /// Split right or down: if panel already exists in direction, reuse; else split
    OpenInDirection(SplitDirection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    FocusedExisting { leaf: LeafId, tab_index: usize },
    OpenedNewTab { leaf: LeafId, tab_index: usize, tab_id: TabId },
    SplitCreated { new_leaf: LeafId, tab_id: TabId },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceTree {
    pub root: WorkspaceNode,
    pub active_leaf: LeafId,
}

impl WorkspaceTree {
    pub fn new_single_leaf(leaf_id: LeafId) -> Self {
        Self {
            root: WorkspaceNode::Leaf { id: leaf_id },
            active_leaf: leaf_id,
        }
    }

    pub fn leaves(&self) -> Vec<LeafId> {
        self.root.leaves()
    }

    pub fn top_leaves(&self) -> Vec<LeafId> {
        self.root.top_leaves()
    }

    pub fn top_columns(&self) -> Vec<TopColumn> {
        let mut cols = Vec::new();
        self.root.collect_top_columns(&[], 1.0, &mut cols);
        cols
    }

    pub fn is_leaf_in_titlebar(&self, leaf_id: LeafId) -> bool {
        self.top_leaves().contains(&leaf_id)
    }

    pub fn horizontal_span_of_leaf(&self, leaf_id: LeafId) -> (f32, f32) {
        self.root.horizontal_span_of_leaf(leaf_id, 0.0, 1.0).unwrap_or((0.0, 1.0))
    }

    pub fn horizontal_ratio_of_leaf(&self, leaf_id: LeafId) -> f32 {
        self.horizontal_span_of_leaf(leaf_id).0
    }

    pub fn horizontal_width_ratio_of_leaf(&self, leaf_id: LeafId) -> f32 {
        self.horizontal_span_of_leaf(leaf_id).1
    }

    pub fn vertical_span_of_leaf(&self, leaf_id: LeafId) -> (f32, f32) {
        self.root.vertical_span_of_leaf(leaf_id, 0.0, 1.0).unwrap_or((0.0, 1.0))
    }

    pub fn vertical_ratio_of_leaf(&self, leaf_id: LeafId) -> f32 {
        self.vertical_span_of_leaf(leaf_id).0
    }

    pub fn vertical_height_ratio_of_leaf(&self, leaf_id: LeafId) -> f32 {
        self.vertical_span_of_leaf(leaf_id).1
    }

    pub fn contains_leaf(&self, leaf_id: LeafId) -> bool {
        self.root.contains_leaf(leaf_id)
    }

    /// Splits a target leaf into two panels along `direction`.
    pub fn split_leaf(
        &mut self,
        target: LeafId,
        direction: SplitDirection,
        new_leaf_id: LeafId,
    ) -> bool {
        let res = Self::split_leaf_node(&mut self.root, target, direction, new_leaf_id);
        if res {
            self.root.flatten_same_direction_splits();
            self.root.normalize_ratios();
        }
        res
    }

    fn split_leaf_node(
        node: &mut WorkspaceNode,
        target: LeafId,
        direction: SplitDirection,
        new_leaf_id: LeafId,
    ) -> bool {
        match node {
            WorkspaceNode::Leaf { id } if *id == target => {
                *node = WorkspaceNode::Split {
                    direction,
                    children: vec![
                        WorkspaceNode::Leaf { id: target },
                        WorkspaceNode::Leaf { id: new_leaf_id },
                    ],
                    ratios: vec![0.5, 0.5],
                };
                true
            }
            WorkspaceNode::Split { direction: parent_dir, children, ratios } => {
                if *parent_dir == direction {
                    let mut found_idx = None;
                    for (idx, child) in children.iter().enumerate() {
                        if let WorkspaceNode::Leaf { id } = child {
                            if *id == target {
                                found_idx = Some(idx);
                                break;
                            }
                        }
                    }
                    if let Some(idx) = found_idx {
                        let r = ratios.get(idx).copied().unwrap_or(1.0 / children.len().max(1) as f32);
                        let half = r / 2.0;
                        ratios[idx] = half;
                        ratios.insert(idx + 1, half);
                        children.insert(idx + 1, WorkspaceNode::Leaf { id: new_leaf_id });
                        return true;
                    }
                }
                for child in children.iter_mut() {
                    if Self::split_leaf_node(child, target, direction, new_leaf_id) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Removes a leaf from the tree. If parent split is left with 1 child, collapses it.
    /// Returns the new active leaf if active_leaf was closed.
    pub fn close_leaf(&mut self, target: LeafId) -> Option<LeafId> {
        let all_leaves = self.leaves();
        if all_leaves.len() <= 1 && all_leaves.contains(&target) {
            // Cannot close the last remaining leaf in the tree
            return None;
        }

        let next_active = if self.active_leaf == target {
            let idx = all_leaves.iter().position(|l| *l == target)?;
            if idx + 1 < all_leaves.len() {
                Some(all_leaves[idx + 1])
            } else if idx > 0 {
                Some(all_leaves[idx - 1])
            } else {
                None
            }
        } else {
            Some(self.active_leaf)
        };

        if Self::remove_leaf_from_node(&mut self.root, target) {
            Self::collapse_single_child_splits(&mut self.root);
            self.root.flatten_same_direction_splits();
            self.root.normalize_ratios();
            if let Some(na) = next_active {
                self.active_leaf = na;
            }
            Some(self.active_leaf)
        } else {
            None
        }
    }

    fn remove_leaf_from_node(node: &mut WorkspaceNode, target: LeafId) -> bool {
        match node {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { children, ratios, .. } => {
                let mut found_idx = None;
                for (idx, child) in children.iter_mut().enumerate() {
                    match child {
                        WorkspaceNode::Leaf { id } if *id == target => {
                            found_idx = Some(idx);
                            break;
                        }
                        WorkspaceNode::Split { .. } => {
                            if Self::remove_leaf_from_node(child, target) {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(idx) = found_idx {
                    children.remove(idx);
                    if idx < ratios.len() {
                        ratios.remove(idx);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    fn collapse_single_child_splits(node: &mut WorkspaceNode) {
        if let WorkspaceNode::Split { children, .. } = node {
            for child in children.iter_mut() {
                Self::collapse_single_child_splits(child);
            }
            if children.len() == 1 {
                *node = children.remove(0);
            }
        }
    }

    /// Sets the ratio at a given divider in a node resolved by tree indices path.
    pub fn set_split_ratio_at(&mut self, path: &[usize], divider_index: usize, ratio: f32) -> bool {
        let mut curr = &mut self.root;
        for &idx in path {
            match curr {
                WorkspaceNode::Split { children, .. } => {
                    if idx < children.len() {
                        curr = &mut children[idx];
                    } else {
                        return false;
                    }
                }
                WorkspaceNode::Leaf { .. } => return false,
            }
        }

        if let WorkspaceNode::Split { ratios, .. } = curr {
            if divider_index < ratios.len() && divider_index + 1 < ratios.len() {
                // Minimum size constraint per panel: 0.05
                let min_ratio = 0.05;
                let pair_sum = ratios[divider_index] + ratios[divider_index + 1];
                let clamped_left = ratio.clamp(min_ratio, pair_sum - min_ratio);
                let right = pair_sum - clamped_left;
                ratios[divider_index] = clamped_left;
                ratios[divider_index + 1] = right;
                return true;
            }
        }
        false
    }

    /// Implements §6.2 open policy table as a pure function.
    pub fn open_file(
        &mut self,
        leaves: &mut HashMap<LeafId, LeafState>,
        doc: DocumentId,
        target: LeafId,
        mode: OpenMode,
    ) -> OpenOutcome {
        match mode {
            OpenMode::Reuse => {
                // Always opens in active_leaf
                let leaf_id = self.active_leaf;
                let state = leaves.entry(leaf_id).or_insert_with(|| LeafState::new(leaf_id));

                // If already open in this leaf, switch to existing tab
                if let Some((idx, _)) = state.find_tab_by_doc(doc) {
                    state.active_tab_index = idx;
                    return OpenOutcome::FocusedExisting {
                        leaf: leaf_id,
                        tab_index: idx,
                    };
                }

                // If currently active tab in this leaf is Empty, replace it (unless pinned)
                let active_idx = state.active_tab_index;
                if let Some(active_tab) = state.tabs.get_mut(active_idx) {
                    if matches!(active_tab.content, TabContent::Empty) && !active_tab.pinned {
                        let tab_id = active_tab.id;
                        active_tab.content = TabContent::Document { id: doc };
                        return OpenOutcome::OpenedNewTab {
                            leaf: leaf_id,
                            tab_index: active_idx,
                            tab_id,
                        };
                    }
                }

                // Otherwise append new tab
                let new_tab = Tab::new_document(doc);
                let tab_id = new_tab.id;
                state.tabs.push(new_tab);
                state.active_tab_index = state.tabs.len() - 1;
                OpenOutcome::OpenedNewTab {
                    leaf: leaf_id,
                    tab_index: state.active_tab_index,
                    tab_id,
                }
            }
            OpenMode::ForceNewTab => {
                let leaf_id = self.active_leaf;
                let state = leaves.entry(leaf_id).or_insert_with(|| LeafState::new(leaf_id));
                let new_tab = Tab::new_document(doc);
                let tab_id = new_tab.id;
                state.tabs.push(new_tab);
                state.active_tab_index = state.tabs.len() - 1;
                OpenOutcome::OpenedNewTab {
                    leaf: leaf_id,
                    tab_index: state.active_tab_index,
                    tab_id,
                }
            }
            OpenMode::OpenInDirection(dir) => {
                // Create a split with a new leaf and open the document in it
                let new_leaf_id = LeafId::new();
                self.split_leaf(target, dir, new_leaf_id);
                let new_tab = Tab::new_document(doc);
                let tab_id = new_tab.id;
                let new_state = LeafState::with_initial_tab(new_leaf_id, new_tab);
                leaves.insert(new_leaf_id, new_state);
                self.active_leaf = new_leaf_id;
                OpenOutcome::SplitCreated {
                    new_leaf: new_leaf_id,
                    tab_id,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_leaf_lifecycle() {
        let l1 = LeafId::new();
        let tree = WorkspaceTree::new_single_leaf(l1);
        assert_eq!(tree.leaves(), vec![l1]);
        assert_eq!(tree.top_leaves(), vec![l1]);
        assert!(tree.is_leaf_in_titlebar(l1));
    }

    #[test]
    fn test_split_and_top_leaves_rules() {
        // 2x2 grid: Row[Column[L1, L3], Column[L2, L4]]
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let l3 = LeafId::new();
        let l4 = LeafId::new();

        let mut tree = WorkspaceTree::new_single_leaf(l1);
        assert!(tree.split_leaf(l1, SplitDirection::Row, l2));
        assert!(tree.split_leaf(l1, SplitDirection::Column, l3));
        assert!(tree.split_leaf(l2, SplitDirection::Column, l4));

        let leaves = tree.leaves();
        assert_eq!(leaves.len(), 4);
        assert!(leaves.contains(&l1));
        assert!(leaves.contains(&l2));
        assert!(leaves.contains(&l3));
        assert!(leaves.contains(&l4));

        // Top leaves: across all column ancestors, must be first child.
        // L1 is top in col1, L2 is top in col2. L3 and L4 are lower children.
        let top = tree.top_leaves();
        assert_eq!(top.len(), 2);
        assert!(top.contains(&l1));
        assert!(top.contains(&l2));
        assert!(!top.contains(&l3));
        assert!(!top.contains(&l4));
    }

    #[test]
    fn test_close_leaf_and_collapse() {
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);
        tree.split_leaf(l1, SplitDirection::Row, l2);
        assert_eq!(tree.leaves().len(), 2);

        // Close l2, tree collapses back to single leaf l1
        let active = tree.close_leaf(l2);
        assert_eq!(active, Some(l1));
        assert_eq!(tree.leaves(), vec![l1]);
        assert_eq!(tree.root, WorkspaceNode::Leaf { id: l1 });
    }

    #[test]
    fn test_open_file_modes_and_table_6_2() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);
        let mut leaves = HashMap::new();
        let doc_a = DocumentId::new();
        let doc_b = DocumentId::new();

        // 1. Open new file in empty leaf
        let res1 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::Reuse);
        match res1 {
            OpenOutcome::OpenedNewTab { leaf, tab_index, .. } => {
                assert_eq!(leaf, l1);
                assert_eq!(tab_index, 0);
            }
            _ => panic!("Expected OpenedNewTab"),
        }

        // 2. Open doc_a again with Reuse -> switches to existing tab
        let res2 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::Reuse);
        assert_eq!(res2, OpenOutcome::FocusedExisting { leaf: l1, tab_index: 0 });

        // 3. Open doc_a with ForceNewTab -> creates duplicate tab in same leaf
        let res3 = tree.open_file(&mut leaves, doc_a, l1, OpenMode::ForceNewTab);
        match res3 {
            OpenOutcome::OpenedNewTab { leaf, tab_index, .. } => {
                assert_eq!(leaf, l1);
                assert_eq!(tab_index, 1);
            }
            _ => panic!("Expected OpenedNewTab duplicate"),
        }
        assert_eq!(leaves.get(&l1).unwrap().tabs.len(), 2);

        // 4. Open doc_b with OpenInDirection(Row) -> creates split
        let res4 = tree.open_file(&mut leaves, doc_b, l1, OpenMode::OpenInDirection(SplitDirection::Row));
        match res4 {
            OpenOutcome::SplitCreated { new_leaf, .. } => {
                assert_ne!(new_leaf, l1);
                assert_eq!(tree.leaves().len(), 2);
                assert_eq!(tree.active_leaf, new_leaf);
                assert_eq!(leaves.get(&new_leaf).unwrap().tabs.len(), 1);
            }
            _ => panic!("Expected SplitCreated"),
        }
    }

    #[test]
    fn test_top_columns_and_dividers() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // 1. Single leaf: 1 column, ratio 1.0, no divider
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].leaf_id, l1);
        assert!((cols[0].ratio - 1.0).abs() < 0.001);
        assert!(cols[0].divider_after.is_none());

        // 2. Row split: 2 leaves side by side
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.5).abs() < 0.001);
        assert!(cols[0].divider_after.is_some());
        let div = cols[0].divider_after.as_ref().unwrap();
        assert_eq!(div.divider_index, 0);
        assert!(cols[1].divider_after.is_none());

        // 3. 2x2 grid: split l1 down to l3, and split l2 down to l4
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l3);
        let l4 = LeafId::new();
        tree.split_leaf(l2, SplitDirection::Column, l4);

        // In 2x2 grid, top row is l1 and l2. l3 and l4 are bottom panels.
        let cols = tree.top_columns();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.5).abs() < 0.001);
        assert!(cols[0].divider_after.is_some());
        assert!(cols[1].divider_after.is_none());

        // Leaves in titlebar:
        assert!(tree.is_leaf_in_titlebar(l1));
        assert!(tree.is_leaf_in_titlebar(l2));
        assert!(!tree.is_leaf_in_titlebar(l3));
        assert!(!tree.is_leaf_in_titlebar(l4));
    }

    #[test]
    fn test_horizontal_span_and_width_ratios() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // Single leaf: span (0.0, 1.0)
        let (start, width) = tree.horizontal_span_of_leaf(l1);
        assert!((start - 0.0).abs() < 0.001);
        assert!((width - 1.0).abs() < 0.001);
        assert!((tree.horizontal_ratio_of_leaf(l1) - 0.0).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l1) - 1.0).abs() < 0.001);

        // Row split: l1 (left 50%) and l2 (right 50%)
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);
        let (s1, w1) = tree.horizontal_span_of_leaf(l1);
        let (s2, w2) = tree.horizontal_span_of_leaf(l2);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((s2 - 0.5).abs() < 0.001);
        assert!((w2 - 0.5).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l1) - 0.5).abs() < 0.001);
        assert!((tree.horizontal_width_ratio_of_leaf(l2) - 0.5).abs() < 0.001);

        // Column split on l1 (l1 top, l3 bottom): both keep 50% width and 0.0 start
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l3);
        let (s1, w1) = tree.horizontal_span_of_leaf(l1);
        let (s3, w3) = tree.horizontal_span_of_leaf(l3);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((s3 - 0.0).abs() < 0.001);
        assert!((w3 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_vertical_span_and_height_ratios() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        // Single leaf: span (0.0, 1.0)
        let (start, height) = tree.vertical_span_of_leaf(l1);
        assert!((start - 0.0).abs() < 0.001);
        assert!((height - 1.0).abs() < 0.001);
        assert!((tree.vertical_ratio_of_leaf(l1) - 0.0).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l1) - 1.0).abs() < 0.001);

        // Column split: l1 (top 50%) and l2 (bottom 50%)
        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Column, l2);
        let (s1, h1) = tree.vertical_span_of_leaf(l1);
        let (s2, h2) = tree.vertical_span_of_leaf(l2);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((h1 - 0.5).abs() < 0.001);
        assert!((s2 - 0.5).abs() < 0.001);
        assert!((h2 - 0.5).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l1) - 0.5).abs() < 0.001);
        assert!((tree.vertical_height_ratio_of_leaf(l2) - 0.5).abs() < 0.001);

        // Row split on l1 (l1 left, l3 right): both keep 50% height and 0.0 start
        let l3 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l3);
        let (s1, h1) = tree.vertical_span_of_leaf(l1);
        let (s3, h3) = tree.vertical_span_of_leaf(l3);
        assert!((s1 - 0.0).abs() < 0.001);
        assert!((h1 - 0.5).abs() < 0.001);
        assert!((s3 - 0.0).abs() < 0.001);
        assert!((h3 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_flattened_multi_row_splits() {
        let l1 = LeafId::new();
        let mut tree = WorkspaceTree::new_single_leaf(l1);

        let l2 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l2);

        let l3 = LeafId::new();
        tree.split_leaf(l2, SplitDirection::Row, l3);

        let cols = tree.top_columns();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].leaf_id, l1);
        assert_eq!(cols[1].leaf_id, l2);
        assert_eq!(cols[2].leaf_id, l3);
        assert!((cols[0].ratio - 0.5).abs() < 0.001);
        assert!((cols[1].ratio - 0.25).abs() < 0.001);
        assert!((cols[2].ratio - 0.25).abs() < 0.001);
        assert_eq!(cols[0].divider_after.as_ref().unwrap().divider_index, 0);
        assert_eq!(cols[1].divider_after.as_ref().unwrap().divider_index, 1);
        assert!(cols[2].divider_after.is_none());

        // Split l1 as well: creates 4 windows, l1 -> [l1, l4]
        let l4 = LeafId::new();
        tree.split_leaf(l1, SplitDirection::Row, l4);
        let cols4 = tree.top_columns();
        assert_eq!(cols4.len(), 4);
        assert_eq!(cols4[0].leaf_id, l1);
        assert_eq!(cols4[1].leaf_id, l4);
        assert_eq!(cols4[2].leaf_id, l2);
        assert_eq!(cols4[3].leaf_id, l3);
        assert!((cols4[0].ratio - 0.25).abs() < 0.001);
        assert!((cols4[1].ratio - 0.25).abs() < 0.001);
        assert!((cols4[2].ratio - 0.25).abs() < 0.001);
        assert!((cols4[3].ratio - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_flatten_arbitrary_nested_splits() {
        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let l3 = LeafId::new();
        let l4 = LeafId::new();

        // Construct manually nested tree: Row(l1, Row(l2, Row(l3, l4)))
        let mut root = WorkspaceNode::Split {
            direction: SplitDirection::Row,
            children: vec![
                WorkspaceNode::Leaf { id: l1 },
                WorkspaceNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        WorkspaceNode::Leaf { id: l2 },
                        WorkspaceNode::Split {
                            direction: SplitDirection::Row,
                            children: vec![
                                WorkspaceNode::Leaf { id: l3 },
                                WorkspaceNode::Leaf { id: l4 },
                            ],
                            ratios: vec![0.5, 0.5],
                        },
                    ],
                    ratios: vec![0.5, 0.5],
                },
            ],
            ratios: vec![0.5, 0.5],
        };

        root.flatten_same_direction_splits();
        root.normalize_ratios();

        match &root {
            WorkspaceNode::Split { direction, children, ratios } => {
                assert_eq!(*direction, SplitDirection::Row);
                assert_eq!(children.len(), 4);
                assert_eq!(ratios.len(), 4);
                assert!((ratios[0] - 0.5).abs() < 0.001);
                assert!((ratios[1] - 0.25).abs() < 0.001);
                assert!((ratios[2] - 0.125).abs() < 0.001);
                assert!((ratios[3] - 0.125).abs() < 0.001);
            }
            _ => panic!("Expected Split"),
        }
    }
}
