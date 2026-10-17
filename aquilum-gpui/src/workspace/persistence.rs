use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::workspace::tree::{LeafId, SplitDirection, TabId};

pub const WORKSPACE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedView {
    pub anchor_line: usize,
    pub px_in_line: f32,
    pub cursor_line: usize,
    pub cursor_col: usize,
    #[serde(default)]
    pub scroll_top: f64,
    #[serde(default)]
    pub cursor_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedTab {
    pub id: TabId,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<PersistedView>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PersistedNode {
    Split {
        direction: SplitDirection,
        children: Vec<PersistedNode>,
        ratios: Vec<f32>,
    },
    Leaf {
        id: LeafId,
        #[serde(default)]
        active_tab_index: usize,
        #[serde(default)]
        tabs: Vec<PersistedTab>,
    },
}

impl PersistedNode {
    pub fn collect_leaf_ids(&self, out: &mut Vec<LeafId>) {
        match self {
            PersistedNode::Leaf { id, .. } => out.push(*id),
            PersistedNode::Split { children, .. } => {
                for c in children {
                    c.collect_leaf_ids(out);
                }
            }
        }
    }

    /// Validates and normalizes nodes according to §9.4
    pub fn validate_and_normalize(&mut self, seen_leaf_ids: &mut HashSet<LeafId>, seen_tab_ids: &mut HashSet<TabId>) {
        match self {
            PersistedNode::Leaf { id, active_tab_index, tabs } => {
                // Ensure unique LeafId
                if !seen_leaf_ids.insert(*id) {
                    *id = LeafId::new();
                    seen_leaf_ids.insert(*id);
                }
                // Ensure unique TabIds
                for tab in tabs.iter_mut() {
                    if !seen_tab_ids.insert(tab.id) {
                        tab.id = TabId::new();
                        seen_tab_ids.insert(tab.id);
                    }
                }
                // Clamp active_tab_index
                if tabs.is_empty() {
                    *active_tab_index = 0;
                } else if *active_tab_index >= tabs.len() {
                    *active_tab_index = tabs.len() - 1;
                }
            }
            PersistedNode::Split { children, ratios, .. } => {
                // Recursively validate children
                for child in children.iter_mut() {
                    child.validate_and_normalize(seen_leaf_ids, seen_tab_ids);
                }

                // Normalize ratios (§9.4)
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
                            *r = (*r / sum).max(0.05);
                        }
                        let re_sum: f32 = ratios.iter().copied().sum();
                        for r in ratios.iter_mut() {
                            *r /= re_sum;
                        }
                    } else {
                        let eq_ratio = 1.0 / (children.len() as f32);
                        *ratios = vec![eq_ratio; children.len()];
                    }
                }
            }
        }
    }

    /// Collapses single-child or empty splits (§9.4)
    pub fn collapse_splits(&mut self) {
        if let PersistedNode::Split { children, .. } = self {
            for child in children.iter_mut() {
                child.collapse_splits();
            }
            if children.len() == 1 {
                *self = children.remove(0);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionV2 {
    pub version: u32,
    pub active_leaf: LeafId,
    pub layout: PersistedNode,
}

impl WorkspaceSessionV2 {
    pub fn new_single_leaf(leaf_id: LeafId) -> Self {
        Self {
            version: WORKSPACE_SCHEMA_VERSION,
            active_leaf: leaf_id,
            layout: PersistedNode::Leaf {
                id: leaf_id,
                active_tab_index: 0,
                tabs: Vec::new(),
            },
        }
    }

    /// Validates whole session structure (§9.4)
    pub fn validate(&mut self) {
        self.layout.collapse_splits();
        let mut seen_leaves = HashSet::new();
        let mut seen_tabs = HashSet::new();
        self.layout.validate_and_normalize(&mut seen_leaves, &mut seen_tabs);

        let mut all_leaves = Vec::new();
        self.layout.collect_leaf_ids(&mut all_leaves);

        if all_leaves.is_empty() {
            let l = LeafId::new();
            *self = Self::new_single_leaf(l);
        } else if !all_leaves.contains(&self.active_leaf) {
            self.active_leaf = all_leaves[0];
        }
    }

    /// Loads workspace session from JSON string, handling migrations (§9.5) and fallbacks (§9.4)
    pub fn from_json(json: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct VersionProbe {
            version: Option<u32>,
        }

        let probe: VersionProbe = serde_json::from_str(json).map_err(|e| e.to_string())?;

        match probe.version {
            Some(2) => {
                let mut session: WorkspaceSessionV2 = serde_json::from_str(json).map_err(|e| e.to_string())?;
                session.validate();
                Ok(session)
            }
            Some(1) => {
                // Migrate v1 -> v2 (§9.5)
                Self::migrate_v1(json)
            }
            Some(v) => Err(format!("Unsupported workspace schema version: {}", v)),
            None => Err("Missing workspace version field".to_string()),
        }
    }

    /// Migrates v1 format to v2 according to §9.5
    fn migrate_v1(json: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct V1TabLeaf {
            #[serde(default)]
            id: Option<String>,
            #[serde(default)]
            tabs: Vec<String>,
            #[serde(default)]
            active_tab_index: usize,
            #[serde(default)]
            scroll: f64,
        }

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "lowercase")]
        enum V1Node {
            Split {
                direction: Option<String>,
                children: Vec<V1Node>,
                #[serde(default)]
                ratios: Vec<f32>,
            },
            Leaf(V1TabLeaf),
        }

        #[derive(Deserialize)]
        struct V1Session {
            active_leaf: Option<String>,
            layout: V1Node,
        }

        fn convert_node(v1: V1Node) -> PersistedNode {
            match v1 {
                V1Node::Leaf(leaf) => {
                    let id = leaf
                        .id
                        .and_then(|s| Uuid::parse_str(&s).ok())
                        .map(LeafId)
                        .unwrap_or_else(LeafId::new);

                    let mut tabs = Vec::new();
                    let approx_line = (leaf.scroll / 24.0).floor() as usize;

                    for (idx, p) in leaf.tabs.into_iter().enumerate() {
                        let view = if idx == leaf.active_tab_index {
                            Some(PersistedView {
                                anchor_line: approx_line,
                                px_in_line: (leaf.scroll % 24.0) as f32,
                                cursor_line: approx_line,
                                cursor_col: 0,
                                scroll_top: leaf.scroll as f64,
                                cursor_offset: 0,
                            })
                        } else {
                            None
                        };

                        tabs.push(PersistedTab {
                            id: TabId::new(),
                            kind: "document".to_string(),
                            path: Some(p),
                            pinned: false,
                            external: None,
                            view,
                        });
                    }

                    PersistedNode::Leaf {
                        id,
                        active_tab_index: leaf.active_tab_index,
                        tabs,
                    }
                }
                V1Node::Split { direction, children, ratios } => {
                    let dir = match direction.as_deref() {
                        Some("vertical") | Some("Column") | Some("column") => SplitDirection::Column,
                        _ => SplitDirection::Row,
                    };
                    PersistedNode::Split {
                        direction: dir,
                        children: children.into_iter().map(convert_node).collect(),
                        ratios,
                    }
                }
            }
        }

        let v1: V1Session = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let active = v1
            .active_leaf
            .and_then(|s| Uuid::parse_str(&s).ok())
            .map(LeafId)
            .unwrap_or_else(LeafId::new);

        let mut session = WorkspaceSessionV2 {
            version: WORKSPACE_SCHEMA_VERSION,
            active_leaf: active,
            layout: convert_node(v1.layout),
        };

        session.validate();
        Ok(session)
    }

    /// Atomic write to file via a temporary file in the same directory (§9.3)
    pub fn save_atomic(&self, target_path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pid = std::process::id();
        let tmp_name = format!(
            ".workspace.{}.tmp",
            pid
        );
        let tmp_path = match target_path.parent() {
            Some(p) => p.join(tmp_name),
            None => PathBuf::from(tmp_name),
        };

        std::fs::write(&tmp_path, json.as_bytes())?;
        if let Err(e) = std::fs::rename(&tmp_path, target_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_roundtrip_serialization() {
        let l1 = LeafId::new();
        let mut session = WorkspaceSessionV2::new_single_leaf(l1);

        if let PersistedNode::Leaf { tabs, .. } = &mut session.layout {
            tabs.push(PersistedTab {
                id: TabId::new(),
                kind: "document".to_string(),
                path: Some("Notes/Tasks.md".to_string()),
                pinned: false,
                external: None,
                view: Some(PersistedView {
                    anchor_line: 118,
                    px_in_line: 4.0,
                    cursor_line: 120,
                    cursor_col: 3,
                    scroll_top: 0.0,
                    cursor_offset: 0,
                }),
            });
        }

        let json = serde_json::to_string_pretty(&session).expect("Serialize should work");
        assert!(json.contains("\"version\": 2"));

        let loaded = WorkspaceSessionV2::from_json(&json).expect("Deserialize should work");
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.active_leaf, l1);
    }

    #[test]
    fn test_v1_migration_to_v2() {
        let v1_json = r#"{
            "version": 1,
            "active_leaf": "c2b5e1a4-0000-0000-0000-000000000001",
            "layout": {
                "type": "leaf",
                "id": "c2b5e1a4-0000-0000-0000-000000000001",
                "tabs": ["Notes/Hello.md", "Notes/World.md"],
                "active_tab_index": 0,
                "scroll": 120.0
            }
        }"#;

        let migrated = WorkspaceSessionV2::from_json(v1_json).expect("Migration should succeed");
        assert_eq!(migrated.version, 2);

        match migrated.layout {
            PersistedNode::Leaf { tabs, active_tab_index, .. } => {
                assert_eq!(active_tab_index, 0);
                assert_eq!(tabs.len(), 2);
                assert_eq!(tabs[0].path.as_deref(), Some("Notes/Hello.md"));
                assert_eq!(tabs[1].path.as_deref(), Some("Notes/World.md"));
                assert!(tabs[0].view.is_some());
                assert_eq!(tabs[0].view.as_ref().unwrap().anchor_line, 5); // 120 / 24 = 5
            }
            _ => panic!("Expected Leaf node"),
        }
    }

    #[test]
    fn test_validation_fixes_ratios_and_duplicates() {
        let l1 = LeafId::new();
        let mut session = WorkspaceSessionV2 {
            version: 2,
            active_leaf: l1,
            layout: PersistedNode::Split {
                direction: SplitDirection::Row,
                children: vec![
                    PersistedNode::Leaf {
                        id: l1,
                        active_tab_index: 999, // out of bounds
                        tabs: vec![],
                    },
                    PersistedNode::Leaf {
                        id: l1, // duplicate id!
                        active_tab_index: 0,
                        tabs: vec![],
                    },
                ],
                ratios: vec![0.1], // bad ratios length!
            },
        };

        session.validate();

        match &session.layout {
            PersistedNode::Split { children, ratios, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(ratios.len(), 2);
                assert!((ratios[0] - 0.5).abs() < 0.01);
                assert!((ratios[1] - 0.5).abs() < 0.01);

                // Leaf IDs must be different after deduplication
                if let (PersistedNode::Leaf { id: id1, .. }, PersistedNode::Leaf { id: id2, .. }) =
                    (&children[0], &children[1])
                {
                    assert_ne!(id1, id2);
                } else {
                    panic!("Expected leaves");
                }
            }
            _ => panic!("Expected Split node"),
        }
    }
}
