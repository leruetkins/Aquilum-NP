use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::document_buffer::DocumentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LeafId(pub Uuid);

impl LeafId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_u64(val: u64) -> Self {
        Self(Uuid::from_u128(val as u128))
    }
}

impl Default for LeafId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TabId(pub Uuid);

impl TabId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_u64(val: u64) -> Self {
        Self(Uuid::from_u128(val as u128))
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LeafBounds {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeafState {
    pub id: LeafId,
    pub tabs: Vec<Tab>,
    pub active_tab_index: usize,
    #[serde(default)]
    pub bounds: Option<LeafBounds>,
}

impl LeafState {
    pub fn new(id: LeafId) -> Self {
        Self {
            id,
            tabs: Vec::new(),
            active_tab_index: 0,
            bounds: None,
        }
    }

    pub fn with_initial_tab(id: LeafId, tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![tab],
            active_tab_index: 0,
            bounds: None,
        }
    }

    pub fn set_bounds(&mut self, w: f32, h: f32) {
        self.bounds = Some(LeafBounds { w, h });
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
mod tests;
