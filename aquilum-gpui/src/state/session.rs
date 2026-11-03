use super::*;

impl Default for NoteSettings {
    fn default() -> Self {
        Self {
            cursor_offset: 0,
            selection_anchor: None,
            scroll_top: 0.0,
            edit_mode: true,
            word_wrap: true,
            source_mode: false,
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            workspace_path: None,
            open_tabs: Vec::new(),
            active_note: None,
            cursor_positions: HashMap::new(),
            note_states: HashMap::new(),
            expanded_folders: HashSet::new(),
            sidebar_scroll_top: 0.0,
            sidebar_width: 0.0,
            sidebar_mode: crate::views::sidebar::SidebarMode::default(),
            find_replace_open: false,
            known_vaults: Vec::new(),
            ribbon_hidden: default_ribbon_hidden(),
            ribbon_order: Vec::new(),
            ribbon_visible: true,
            bookmarks: crate::views::sidebar::bookmarks::BookmarksData::default(),
            recent_files: crate::views::sidebar::recent_files::RecentFilesData::default(),
            recent_files_sort_order: crate::views::sidebar::recent_files::RecentFilesSortOrder::default(),
        }
    }
}

impl SessionState {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("session-state.json");
        let mut state = if let Ok(content) = std::fs::read_to_string(&path) {
            serde_json::from_str::<SessionState>(&content).unwrap_or_default()
        } else {
            Self::default()
        };

        // Populate note_states from cursor_positions if note_states is empty
        for (p, &pos) in &state.cursor_positions {
            state.note_states.entry(p.clone()).or_insert_with(|| NoteSettings {
                cursor_offset: pos,
                selection_anchor: None,
                scroll_top: 0.0,
                edit_mode: true,
                word_wrap: true,
                source_mode: false,
            });
        }

        state
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join("session-state.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
