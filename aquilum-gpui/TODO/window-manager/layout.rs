//! Layout: pure geometry and tab lists, without documents.

use std::collections::HashMap;

use crate::ops::{LeafId, TabId};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
pub enum LayoutNode {
    Leaf(LeafId),
    Split { axis: Axis, ratio: f32, a: Box<LayoutNode>, b: Box<LayoutNode> },
}

#[derive(Clone, Debug, Default)]
pub struct LeafState {
    pub tabs: Vec<TabId>,
    pub active: Option<TabId>,
    /// The viewport height is taken from here, not globally.
    pub bounds: Rect,
}

pub struct WorkspaceLayout {
    pub root: LayoutNode,
    leaves: HashMap<LeafId, LeafState>,
    pub active_leaf: LeafId,
    next_leaf: u64,
}

impl WorkspaceLayout {
    pub fn new() -> Self {
        let first = LeafId(0);
        let mut leaves = HashMap::new();
        leaves.insert(first, LeafState { bounds: Rect { w: 1000.0, h: 600.0 }, ..Default::default() });
        Self { root: LayoutNode::Leaf(first), leaves, active_leaf: first, next_leaf: 1 }
    }

    pub fn leaf(&self, id: LeafId) -> Option<&LeafState> {
        self.leaves.get(&id)
    }
    pub fn leaf_mut(&mut self, id: LeafId) -> Option<&mut LeafState> {
        self.leaves.get_mut(&id)
    }
    pub fn leaf_of(&self, tab: TabId) -> Option<LeafId> {
        self.leaves.iter().find(|(_, l)| l.tabs.contains(&tab)).map(|(id, _)| *id)
    }

    /// Splits a leaf; the new leaf is empty and inherits the size.
    pub fn split_leaf(&mut self, leaf: LeafId, axis: Axis) -> Option<LeafId> {
        let bounds = self.leaves.get(&leaf)?.bounds;
        let new = LeafId(self.next_leaf);
        self.next_leaf += 1;
        if !split_node(&mut self.root, leaf, axis, new) {
            return None;
        }
        self.leaves.insert(new, LeafState { bounds, ..Default::default() });
        Some(new)
    }

    /// Removes a tab; the adjacent one becomes active (the next, otherwise the previous).
    pub fn remove_tab(&mut self, leaf: LeafId, tab: TabId) {
        if let Some(l) = self.leaves.get_mut(&leaf) {
            if let Some(idx) = l.tabs.iter().position(|&t| t == tab) {
                l.tabs.remove(idx);
                if l.active == Some(tab) {
                    l.active = l.tabs.get(idx).or_else(|| idx.checked_sub(1).and_then(|i| l.tabs.get(i))).copied();
                }
            }
        }
    }
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self::new()
    }
}

fn split_node(node: &mut LayoutNode, target: LeafId, axis: Axis, new: LeafId) -> bool {
    if matches!(node, LayoutNode::Leaf(id) if *id == target) {
        *node = LayoutNode::Split {
            axis,
            ratio: 0.5,
            a: Box::new(LayoutNode::Leaf(target)),
            b: Box::new(LayoutNode::Leaf(new)),
        };
        return true;
    }
    if let LayoutNode::Split { a, b, .. } = node {
        return split_node(a, target, axis, new) || split_node(b, target, axis, new);
    }
    false
}
