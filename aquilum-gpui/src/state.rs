use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::markdown_doc::{MarkdownDocument, CursorState};

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| {
            let vibe = d.join("com.dmitriy.vibecodium");
            if !vibe.exists() {
                let old = d.join("com.dmitriy.aquilum-app");
                if old.exists() {
                    return old;
                }
            }
            vibe
        })
        .unwrap_or_else(|| PathBuf::from("./data"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

impl WindowState {
    pub fn load(dir: &Path) -> Self {
        Self::load_file(&dir.join("window-state.json"))
    }

    pub fn load_file(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<WindowState>(&content) {
                return state;
            }
        }
        Self::default()
    }

    pub fn save(&self, dir: &Path) {
        self.save_file(&dir.join("window-state.json"))
    }

    pub fn save_file(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn to_validated_bounds(&self, cx: &gpui::App, default_w: f32, default_h: f32) -> gpui::WindowBounds {
        use gpui::{px, size, Bounds, Point};
        let w = if self.width >= 300.0 { self.width } else { default_w };
        let h = if self.height >= 200.0 { self.height } else { default_h };
        let sz = size(px(w), px(h));

        let has_valid_coords = match (self.x, self.y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => true,
            _ => false,
        };

        let bounds = if has_valid_coords {
            Bounds {
                origin: Point {
                    x: px(self.x.unwrap()),
                    y: px(self.y.unwrap()),
                },
                size: sz,
            }
        } else {
            Bounds::centered(None, sz, cx)
        };

        if self.maximized {
            gpui::WindowBounds::Maximized(bounds)
        } else {
            gpui::WindowBounds::Windowed(bounds)
        }
    }

    pub fn to_window_bounds(&self, cx: &gpui::App) -> gpui::WindowBounds {
        use gpui::{px, size, Bounds, Point};
        let w = if self.width >= 300.0 { self.width } else { 1200.0 };
        let h = if self.height >= 200.0 { self.height } else { 800.0 };
        let sz = size(px(w), px(h));

        let has_valid_coords = match (self.x, self.y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => true,
            _ => false,
        };

        let bounds = if has_valid_coords {
            Bounds {
                origin: Point {
                    x: px(self.x.unwrap()),
                    y: px(self.y.unwrap()),
                },
                size: sz,
            }
        } else {
            Bounds::centered(None, sz, cx)
        };

        if self.maximized {
            gpui::WindowBounds::Maximized(bounds)
        } else {
            gpui::WindowBounds::Windowed(bounds)
        }
    }
}

fn default_true() -> bool {
    true
}

/// Prefix for virtual "empty tab" paths (like `NewTab` in aquilum-app).
///
/// An empty tab has no file on disk: it shows the New Tab page with
/// `Create note / Go to file / Close` actions instead of an editor.
/// The suffix is a unique id so several empty tabs can coexist.
pub const EMPTY_TAB_PREFIX: &str = "__new_tab__";

/// Builds a fresh unique empty-tab sentinel path.
pub fn empty_tab_path() -> PathBuf {
    PathBuf::from(format!("{}{}", EMPTY_TAB_PREFIX, uuid::Uuid::new_v4()))
}

/// Whether `path` is an empty-tab sentinel rather than a real file.
pub fn is_empty_tab_path(path: &Path) -> bool {
    path.to_string_lossy().starts_with(EMPTY_TAB_PREFIX)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSettings {
    pub cursor_offset: usize,
    #[serde(default)]
    pub selection_anchor: Option<usize>,
    #[serde(default)]
    pub scroll_top: f32,
    #[serde(default = "default_true")]
    pub edit_mode: bool,
    #[serde(default = "default_true")]
    pub word_wrap: bool,
}

impl Default for NoteSettings {
    fn default() -> Self {
        Self {
            cursor_offset: 0,
            selection_anchor: None,
            scroll_top: 0.0,
            edit_mode: true,
            word_wrap: true,
        }
    }
}

fn default_ribbon_hidden() -> Vec<String> {
    vec![
        "ribbon-quick-switcher".to_string(),
        "ribbon-graph".to_string(),
        "ribbon-canvas".to_string(),
        "ribbon-template".to_string(),
        "ribbon-drawing".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub workspace_path: Option<PathBuf>,
    #[serde(default)]
    pub open_tabs: Vec<PathBuf>,
    #[serde(default)]
    pub active_note: Option<PathBuf>,
    #[serde(default)]
    pub cursor_positions: HashMap<PathBuf, usize>,
    #[serde(default)]
    pub note_states: HashMap<PathBuf, NoteSettings>,
    #[serde(default)]
    pub expanded_folders: HashSet<PathBuf>,
    #[serde(default)]
    pub sidebar_scroll_top: f32,
    /// Left sidebar width in pixels (clamped to 160..=600 on restore).
    #[serde(default)]
    pub sidebar_width: f32,
    /// Open sidebar panel (file tree vs search). Defaults to files.
    #[serde(default)]
    pub sidebar_mode: crate::views::sidebar::SidebarMode,
    /// Known vaults for the switcher dialog, most recent first.
    #[serde(default)]
    pub known_vaults: Vec<PathBuf>,
    /// Hidden ribbon action IDs.
    #[serde(default = "default_ribbon_hidden")]
    pub ribbon_hidden: Vec<String>,
    /// Custom ribbon action order (ordered list of action IDs).
    #[serde(default)]
    pub ribbon_order: Vec<String>,
    /// Whether the ribbon rail itself is visible.
    #[serde(default = "default_true")]
    pub ribbon_visible: bool,
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
            known_vaults: Vec::new(),
            ribbon_hidden: default_ribbon_hidden(),
            ribbon_order: Vec::new(),
            ribbon_visible: true,
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

#[derive(Debug, Clone)]
pub enum FileNode {
    Folder {
        name: String,
        path: PathBuf,
        children: Vec<FileNode>,
    },
    File {
        name: String,
        path: PathBuf,
    },
}

impl FileNode {
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } => name,
            Self::File { name, .. } => name,
        }
    }

    /// Recursively checks if this node or any descendant matches the case-insensitive query.
    pub fn matches_query(&self, query_lower: &str) -> bool {
        if query_lower.is_empty() {
            return true;
        }
        match self {
            Self::File { name, .. } => name.to_lowercase().contains(query_lower),
            Self::Folder { name, children, .. } => {
                if name.to_lowercase().contains(query_lower) {
                    return true;
                }
                children.iter().any(|c| c.matches_query(query_lower))
            }
        }
    }
}

pub struct OpenedNote {
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    #[allow(dead_code)]
    pub version: u64,
    pub cursor_offset: usize,
    pub selection_anchor: Option<usize>,
    pub is_dirty: bool,
    pub undo_stack: Vec<(String, usize)>,
    pub redo_stack: Vec<(String, usize)>,

    // Markdown Document model
    pub doc: MarkdownDocument,
    pub cursor: CursorState,

    // Cached visual layout structures
    pub cached_lines: Arc<Vec<String>>,
    pub cached_line_starts: Arc<Vec<usize>>,
    pub cached_visual_rows: Arc<Vec<crate::markdown::VisualRow>>,
    pub cached_code_states: Arc<Vec<Option<crate::markdown::CodeBlockState>>>,
    pub cached_height_index: Arc<crate::markdown::VisualRowHeightIndex>,
    pub cached_wrap_cols: usize,
    pub cached_word_wrap: bool,
    pub visual_cache_dirty: bool,
}

impl OpenedNote {
    pub fn new(path: PathBuf, title: String, content: String, version: u64) -> Self {
        let doc = MarkdownDocument::new(content.clone());
        let mut cursor = CursorState::default();
        doc.update_cursor(&mut cursor, 0);

        Self {
            path,
            title,
            content,
            version,
            cursor_offset: 0,
            selection_anchor: None,
            is_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            doc,
            cursor,
            cached_lines: Arc::new(Vec::new()),
            cached_line_starts: Arc::new(Vec::new()),
            cached_visual_rows: Arc::new(Vec::new()),
            cached_code_states: Arc::new(Vec::new()),
            cached_height_index: Arc::new(crate::markdown::VisualRowHeightIndex::default()),
            cached_wrap_cols: 0,
            cached_word_wrap: true,
            visual_cache_dirty: true,
        }
    }

    pub fn sync_cursor(&mut self) {
        self.doc.update_cursor(&mut self.cursor, self.cursor_offset);
    }

    pub fn rebuild_visual_cache(&mut self, word_wrap: bool, wrap_cols: usize) {
        self.rebuild_visual_cache_with_headings(word_wrap, wrap_cols, None);
    }

    pub fn rebuild_visual_cache_with_headings(
        &mut self,
        word_wrap: bool,
        wrap_cols: usize,
        headings: Option<&crate::settings::HeadingsSettings>,
    ) {
        crate::perf::PERF.parse_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let lines: Vec<String> = self.content.split('\n').map(String::from).collect();
        crate::perf::PERF.span_rebuilds.fetch_add(lines.len() as u64, std::sync::atomic::Ordering::Relaxed);
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let visual_rows = crate::markdown::build_visual_rows(&line_refs, word_wrap, wrap_cols);
        let mut line_starts: Vec<usize> = Vec::with_capacity(lines.len());
        let mut running_offset = 0;
        for line in &lines {
            line_starts.push(running_offset);
            running_offset += line.len() + 1;
        }
        let code_states = crate::markdown::analyze_code_blocks(&line_refs);
        let height_index = crate::markdown::VisualRowHeightIndex::from_rows(&lines, &visual_rows, headings);

        self.cached_lines = Arc::new(lines);
        self.cached_line_starts = Arc::new(line_starts);
        self.cached_visual_rows = Arc::new(visual_rows);
        self.cached_code_states = Arc::new(code_states);
        self.cached_height_index = Arc::new(height_index);
        self.cached_word_wrap = word_wrap;
        self.cached_wrap_cols = wrap_cols;
        self.visual_cache_dirty = false;
    }

    pub fn ensure_visual_cache(&mut self, word_wrap: bool, wrap_cols: usize) {
        self.ensure_visual_cache_with_headings(word_wrap, wrap_cols, None);
    }

    pub fn ensure_visual_cache_with_headings(
        &mut self,
        word_wrap: bool,
        wrap_cols: usize,
        headings: Option<&crate::settings::HeadingsSettings>,
    ) {
        if self.visual_cache_dirty
            || self.cached_lines.is_empty()
            || self.cached_word_wrap != word_wrap
            || self.cached_wrap_cols != wrap_cols
        {
            self.rebuild_visual_cache_with_headings(word_wrap, wrap_cols, headings);
        }
    }

    pub fn snapshot_undo(&mut self) {
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push((self.content.clone(), self.cursor_offset));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((prev_content, prev_cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = prev_content;
            self.version += 1;
            self.doc = MarkdownDocument::new(self.content.clone());
            self.cursor_offset = prev_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
            self.visual_cache_dirty = true;
            self.sync_cursor();
        }
    }

    pub fn redo(&mut self) {
        if let Some((next_content, next_cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.content.clone(), self.cursor_offset));
            self.content = next_content;
            self.version += 1;
            self.doc = MarkdownDocument::new(self.content.clone());
            self.cursor_offset = next_cursor.min(self.content.len());
            self.selection_anchor = None;
            self.is_dirty = true;
            self.visual_cache_dirty = true;
            self.sync_cursor();
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_offset {
                let mut start = anchor.min(self.cursor_offset).min(self.content.len());
                let mut end = anchor.max(self.cursor_offset).min(self.content.len());
                while start > 0 && !self.content.is_char_boundary(start) {
                    start -= 1;
                }
                while end > 0 && !self.content.is_char_boundary(end) {
                    end -= 1;
                }
                if start != end {
                    return Some(start..end);
                }
            }
        }
        None
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection_range()?;
        if self.content.is_char_boundary(range.start) && self.content.is_char_boundary(range.end) {
            Some(self.content[range].to_string())
        } else {
            None
        }
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some(range) = self.selection_range() {
            self.snapshot_undo();
            self.doc.apply_edit(range.clone(), "");
            self.content = self.doc.source.clone();
            self.version += 1;
            self.cursor_offset = range.start;
            self.selection_anchor = None;
            self.is_dirty = true;
            self.visual_cache_dirty = true;
            self.sync_cursor();
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_offset = self.content.len();
        self.sync_cursor();
    }

    pub fn word_range_at(&self, offset: usize) -> (usize, usize) {
        if self.content.is_empty() {
            return (0, 0);
        }
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let chars: Vec<(usize, char)> = self.content.char_indices().collect();
        if chars.is_empty() {
            return (0, 0);
        }
        let char_idx = chars.iter().position(|(idx, _)| *idx >= offset).unwrap_or(chars.len() - 1);
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let mut start_idx = char_idx;
        while start_idx > 0 && is_word_char(chars[start_idx - 1].1) {
            start_idx -= 1;
        }
        let mut end_idx = char_idx;
        while end_idx < chars.len() && is_word_char(chars[end_idx].1) {
            end_idx += 1;
        }
        if start_idx == end_idx {
            return (offset, offset);
        }
        let start_byte = chars[start_idx].0;
        let end_byte = if end_idx < chars.len() {
            chars[end_idx].0
        } else {
            self.content.len()
        };
        (start_byte, end_byte)
    }

    pub fn line_range_at(&self, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let start = self.content[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = self.content[offset..].find('\n').map(|i| offset + i + 1).unwrap_or(self.content.len());
        (start, end)
    }

    /// Возвращает (line_index, col_index) 0-indexed
    pub fn line_col(&self) -> (usize, usize) {
        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        if !self.visual_cache_dirty && !self.cached_line_starts.is_empty() && !self.cached_lines.is_empty() {
            let line_idx = match self.cached_line_starts.binary_search(&offset) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };
            if line_idx < self.cached_lines.len() {
                let line_start = self.cached_line_starts[line_idx];
                let line = &self.cached_lines[line_idx];
                let mut byte_in_line = offset.saturating_sub(line_start).min(line.len());
                while byte_in_line > 0 && !line.is_char_boundary(byte_in_line) {
                    byte_in_line -= 1;
                }
                let col_idx = line[..byte_in_line].chars().count();
                return (line_idx, col_idx);
            }
        }
        let prefix = &self.content[..offset];
        let line_idx = prefix.matches('\n').count();
        let last_newline = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let col_idx = prefix[last_newline..].chars().count();
        (line_idx, col_idx)
    }

    pub fn offset_for_line_col(&self, target_line: usize, target_col: usize) -> usize {
        if !self.visual_cache_dirty && !self.cached_line_starts.is_empty() && !self.cached_lines.is_empty() {
            if target_line < self.cached_lines.len() {
                let line_start = self.cached_line_starts[target_line];
                let line = &self.cached_lines[target_line];
                let actual_col = target_col.min(line.chars().count());
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                return line_start + byte_col;
            }
            return self.content.len();
        }
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = target_col.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                return offset + byte_col;
            }
            offset += line.len() + 1;
        }
        self.content.len()
    }

    pub fn insert_text(&mut self, text: &str) {
        self.snapshot_undo();
        let _ = self.delete_selection();
        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        self.doc.apply_edit(offset..offset, text);
        self.content = self.doc.source.clone();
        self.version += 1;
        self.cursor_offset = offset + text.len();
        self.selection_anchor = None;
        self.is_dirty = true;
        self.visual_cache_dirty = true;
        self.sync_cursor();
    }

    pub fn insert_newline_auto_indent(&mut self) {
        self.snapshot_undo();
        let _ = self.delete_selection();

        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &self.content[..offset];
        let last_line = prefix.split('\n').last().unwrap_or("");
        let indent_len = last_line.chars().take_while(|c| c.is_whitespace()).count();
        let indent: String = last_line.chars().take(indent_len).collect();

        let insert_str = format!("\n{}", indent);
        self.doc.apply_edit(offset..offset, &insert_str);
        self.content = self.doc.source.clone();
        self.version += 1;
        self.cursor_offset = offset + insert_str.len();
        self.selection_anchor = None;
        self.is_dirty = true;
        self.visual_cache_dirty = true;
        self.sync_cursor();
    }

    pub fn delete_backwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor_offset == 0 || self.content.is_empty() {
            return;
        }
        self.snapshot_undo();
        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        if offset == 0 {
            return;
        }
        let mut prev = offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.doc.apply_edit(prev..offset, "");
        self.content = self.doc.source.clone();
        self.version += 1;
        self.cursor_offset = prev;
        self.selection_anchor = None;
        self.is_dirty = true;
        self.visual_cache_dirty = true;
        self.sync_cursor();
    }

    pub fn delete_forwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        if offset >= self.content.len() {
            return;
        }
        self.snapshot_undo();
        let mut next = offset + 1;
        while next <= self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        self.doc.apply_edit(offset..next, "");
        self.content = self.doc.source.clone();
        self.version += 1;
        self.selection_anchor = None;
        self.is_dirty = true;
        self.visual_cache_dirty = true;
        self.sync_cursor();
    }

    pub fn move_cursor_left(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.start;
                self.selection_anchor = None;
                self.sync_cursor();
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset == 0 {
            return;
        }
        let mut prev = self.cursor_offset - 1;
        while prev > 0 && !self.content.is_char_boundary(prev) {
            prev -= 1;
        }
        self.cursor_offset = prev;
        if !shift {
            self.selection_anchor = None;
        }
        self.sync_cursor();
    }

    pub fn move_cursor_right(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.end;
                self.selection_anchor = None;
                self.sync_cursor();
                return;
            }
        }
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        if self.cursor_offset >= self.content.len() {
            return;
        }
        let mut next = self.cursor_offset + 1;
        while next <= self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        if next <= self.content.len() {
            self.cursor_offset = next;
        }
        if !shift {
            self.selection_anchor = None;
        }
        self.sync_cursor();
    }

    pub fn move_cursor_up(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        if line_idx == 0 {
            self.cursor_offset = 0;
            if !shift {
                self.selection_anchor = None;
            }
            self.sync_cursor();
            return;
        }

        let lines: Vec<&str> = self.content.split('\n').collect();
        let target_line = line_idx - 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                self.sync_cursor();
                return;
            }
            offset += line.len() + 1;
        }
        self.sync_cursor();
    }

    pub fn move_cursor_down(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, col_idx) = self.line_col();
        let lines: Vec<&str> = self.content.split('\n').collect();
        if line_idx + 1 >= lines.len() {
            self.cursor_offset = self.content.len();
            if !shift {
                self.selection_anchor = None;
            }
            self.sync_cursor();
            return;
        }

        let target_line = line_idx + 1;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let char_count = line.chars().count();
                let actual_col = col_idx.min(char_count);
                let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
                self.cursor_offset = offset + byte_col;
                if !shift {
                    self.selection_anchor = None;
                }
                self.sync_cursor();
                return;
            }
            offset += line.len() + 1;
        }
        self.sync_cursor();
    }

    pub fn set_cursor_to_line_start(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset;
                self.selection_anchor = None;
                self.sync_cursor();
                return;
            }
            offset += line.len() + 1;
        }
        self.sync_cursor();
    }

    pub fn set_cursor_to_line_end(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset + line.len();
                self.selection_anchor = None;
                self.sync_cursor();
                return;
            }
            offset += line.len() + 1;
        }
        self.sync_cursor();
    }

    #[allow(dead_code)]
    pub fn set_cursor_to_line_col(&mut self, target_line: usize, target_col: usize) {
        self.cursor_offset = self.offset_for_line_col(target_line, target_col);
        self.selection_anchor = None;
        self.sync_cursor();
    }

    pub fn home(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_start(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
        self.sync_cursor();
    }

    pub fn end(&mut self, shift: bool) {
        if shift && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor_offset);
        }
        let (line_idx, _) = self.line_col();
        self.set_cursor_to_line_end(line_idx);
        if !shift {
            self.selection_anchor = None;
        }
        self.sync_cursor();
    }
}

/// Неизменяемый снимок состояния vault и активного файла для синхронных геттеров плагинов.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub version: u64,
    pub active_file: Option<crate::vault::FileInfo>,
    pub files: Arc<[crate::vault::FileInfo]>,
}

pub struct AppState {
    pub settings: Arc<crate::settings::SettingsManager>,
    pub workspace_path: Option<PathBuf>,
    pub workspace_name: String,
    pub vault: Option<crate::vault::VaultCore>,
    pub events: Arc<crate::events::EventBus>,
    pub commands: Arc<crate::commands::CommandRegistry>,
    pub ribbon: Arc<crate::ribbon::RibbonRegistry>,
    pub status_bar: Arc<crate::ribbon::StatusBarRegistry>,
    pub code_block_processors: Arc<crate::plugins::code_block::CodeBlockProcessorRegistry>,
    pub plugin_host: Arc<crate::plugins::bridge::PluginHostState>,
    pub snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
    pub file_tree: Vec<FileNode>,
    pub expanded_folders: HashSet<PathBuf>,
    pub search_query: String,
    pub active_note: Option<OpenedNote>,
    pub open_tabs: Vec<PathBuf>,
    /// Active empty-tab sentinel, if the current tab has no file on disk.
    /// Mutually exclusive with `active_note`: exactly one of them is `Some`.
    pub active_empty_tab: Option<PathBuf>,
    pub nav_history: Vec<PathBuf>,
    pub nav_history_index: usize,
    pub status_text: String,
    pub session: SessionState,
    pub note_states: HashMap<PathBuf, NoteSettings>,
    /// Known vaults for the switcher dialog, most recent first.
    pub known_vaults: Vec<PathBuf>,
}

const DEFAULT_WELCOME_NOTE: &str = "# Welcome to Vibecodium (GPUI Live Preview)\n\nYour fast, distraction-free markdown knowledge base with **true Obsidian Live Preview**!\n\n## Inline Live Preview Demo\n- Click any line or block to edit raw markdown inline\n- Move cursor away (or click another line) to see rendered styling\n- Regular text and code blocks are fully selectable across the entire page with `Ctrl+A`\n\n### Interactive Features\n- [x] Zero Electron / Chromium overhead (~40 MB RAM, 120 FPS)\n- [x] Full document continuous text selection\n- [x] Dedicated Obsidian-style code blocks with syntax styling\n- [ ] Auto-saving and instant search\n\n```rust\nfn main() {\n    println!(\"Hello from Vibecodium GPUI Live Preview!\");\n}\n```\n\n> Regular text and code blocks are cleanly organized with seamless Live Preview transitions.\n";

impl AppState {
    pub fn new() -> Self {
        let app_data_dir = app_data_dir();
        let _ = std::fs::create_dir_all(&app_data_dir);
        let settings = Arc::new(crate::settings::SettingsManager::new(&app_data_dir));
        let session = SessionState::load(&app_data_dir);

        let valid_tabs: Vec<PathBuf> = session
            .open_tabs
            .iter()
            .filter(|p| is_empty_tab_path(p) || p.exists() || p.to_string_lossy() == "Welcome.md")
            .cloned()
            .collect();

        let events = Arc::new(crate::events::EventBus::new());
        let commands = Arc::new(crate::commands::CommandRegistry::new());
        let ribbon = Arc::new(crate::ribbon::RibbonRegistry::new());
        let status_bar = Arc::new(crate::ribbon::StatusBarRegistry::new());
        let code_block_processors = Arc::new(crate::plugins::code_block::CodeBlockProcessorRegistry::new());
        let plugin_host = Arc::new(crate::plugins::bridge::PluginHostState::new());
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let mut state = Self {
            settings,
            workspace_path: None,
            workspace_name: "No Workspace".to_owned(),
            vault: None,
            events,
            commands,
            ribbon,
            status_bar,
            code_block_processors,
            plugin_host,
            snapshot,
            file_tree: Vec::new(),
            expanded_folders: session.expanded_folders.clone(),
            search_query: String::new(),
            active_note: None,
            open_tabs: valid_tabs,
            active_empty_tab: None,
            nav_history: Vec::new(),
            nav_history_index: 0,
            status_text: "GPUI Ready • 0 WebView Processes".to_owned(),
            note_states: session.note_states.clone(),
            known_vaults: session
                .known_vaults
                .iter()
                .filter(|p| p.is_dir())
                .take(20)
                .cloned()
                .collect(),
            session,
        };

        // Determine workspace to open
        let mut ws_to_open = None;
        if let Some(ref ws) = state.session.workspace_path {
            if ws.exists() {
                ws_to_open = Some(ws.clone());
            }
        }

        if let Some(ws) = ws_to_open {
            state.open_workspace(ws);
        }

        // Restore active note (or active empty tab)
        let mut note_to_open = None;
        let mut empty_to_open = None;
        if let Some(ref active) = state.session.active_note {
            if is_empty_tab_path(active) {
                if state.open_tabs.contains(active) {
                    empty_to_open = Some(active.clone());
                }
            } else if active.exists() || active.to_string_lossy() == "Welcome.md" {
                note_to_open = Some(active.clone());
            }
        }
        if note_to_open.is_none() && !state.open_tabs.is_empty() {
            note_to_open = Some(state.open_tabs.last().unwrap().clone());
        }
        if note_to_open.is_none() {
            note_to_open = Self::first_file_in_tree(&state.file_tree);
        }

        if let Some(path) = empty_to_open {
            state.active_empty_tab = Some(path);
        } else if let Some(path) = note_to_open {
            state.reveal_path_in_tree(&path);
            state.open_note(path);
        } else {
            let welcome_path = PathBuf::from("Welcome.md");
            state.open_tabs.push(welcome_path.clone());
            state.nav_history.push(welcome_path.clone());
            state.nav_history_index = 0;
            state.active_note = Some(OpenedNote::new(
                welcome_path,
                "Welcome to Vibecodium".to_owned(),
                DEFAULT_WELCOME_NOTE.to_owned(),
                0,
            ));
        }

        state
    }

    pub fn open_workspace(&mut self, path: PathBuf) {
        let vault = crate::vault::VaultCore::open(path.clone());
        let name = vault.name.clone();

        self.workspace_name = name;
        self.workspace_path = Some(path.clone());
        self.session.workspace_path = Some(path.clone());
        self.vault = Some(vault);
        // Remember the vault for the switcher dialog (most recent first).
        self.known_vaults.retain(|p| p != &path);
        self.known_vaults.insert(0, path.clone());
        while self.known_vaults.len() > 20 {
            self.known_vaults.pop();
        }
        self.session.known_vaults = self.known_vaults.clone();
        self.expanded_folders.insert(path.clone());
        self.open_tabs.retain(|p| p.starts_with(&path) || p.to_string_lossy() == "Welcome.md");
        if let Some(ref empty) = self.active_empty_tab {
            if !self.open_tabs.contains(empty) {
                self.active_empty_tab = None;
            }
        }
        self.reload_file_tree();
        self.status_text = format!("Workspace: {}", self.workspace_name);

        self.events.emit_workspace(&crate::events::WorkspaceEvent::VaultOpen {
            path: path.to_string_lossy().to_string(),
        });
        self.save_session_state();
        self.publish_snapshot();
    }

    /// Публикует актуальный иммутабельный снимок состояния vault и активного файла
    pub fn publish_snapshot(&self) {
        let active = self.active_note.as_ref().and_then(|note| {
            self.vault.as_ref().and_then(|v| {
                crate::vault::FileInfo::from_path(&v.root, &note.path)
            })
        });
        let files: Vec<crate::vault::FileInfo> = self.vault
            .as_ref()
            .map(|v| v.list_markdown_files())
            .unwrap_or_default();

        let version = self.active_note.as_ref().map(|n| n.version).unwrap_or(0);
        self.snapshot.store(Arc::new(Snapshot {
            version,
            active_file: active,
            files: files.into(),
        }));
    }

    pub fn build_tree(dir: &Path) -> Vec<FileNode> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut folders = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') || name == "node_modules" || name == "target" || name == ".git" {
                continue;
            }

            if path.is_dir() {
                let children = Self::build_tree(&path);
                folders.push(FileNode::Folder {
                    name,
                    path,
                    children,
                });
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("md")
                        || ext.eq_ignore_ascii_case("markdown")
                        || ext.eq_ignore_ascii_case("txt")
                    {
                        files.push(FileNode::File { name, path });
                    }
                }
            }
        }

        folders.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));
        files.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));

        let mut all = folders;
        all.extend(files);
        all
    }

    pub fn first_file_in_tree(nodes: &[FileNode]) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { path, .. } => return Some(path.clone()),
                FileNode::Folder { children, .. } => {
                    if let Some(p) = Self::first_file_in_tree(children) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn reload_file_tree(&mut self) {
        let Some(ws) = &self.workspace_path else {
            self.file_tree.clear();
            return;
        };
        self.file_tree = Self::build_tree(ws);
        self.reload_files();
        self.status_text = format!("Vault: {}", self.workspace_name);
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        if self.expanded_folders.contains(&path) {
            self.expanded_folders.remove(&path);
        } else {
            self.expanded_folders.insert(path);
        }
        self.save_session_state();
    }

    pub fn reveal_path_in_tree(&mut self, target: &Path) {
        let Some(ref ws) = self.workspace_path else { return; };
        let mut current = target.parent();
        while let Some(parent) = current {
            if parent.starts_with(ws) {
                self.expanded_folders.insert(parent.to_path_buf());
                if parent == ws {
                    break;
                }
                current = parent.parent();
            } else {
                break;
            }
        }
    }

    /// Default new-file stem in the current UI language.
    ///
    /// Used for every created note/canvas so the file name always matches
    /// the interface locale instead of a hardcoded language.
    pub fn untitled_base(&self) -> String {
        let lang = self.settings.get_config().ui.language;
        crate::i18n::t("common.untitled", &lang)
    }

    pub fn create_new_note(&mut self) -> Option<PathBuf> {
        let ws = self.workspace_path.clone()?;
        // Default file name comes from the UI locale ("Без названия" for
        // Russian, "Untitled" for English, ...), never hardcoded.
        let base = self.untitled_base();
        let mut candidate = ws.join(format!("{}.md", base));
        let mut counter = 1;
        while candidate.exists() {
            candidate = ws.join(format!("{} {}.md", base, counter));
            counter += 1;
        }
        let default_text = String::new();
        let _ = std::fs::write(&candidate, &default_text);
        self.reload_file_tree();
        self.open_note(candidate.clone());
        if let Some(ref mut note) = self.active_note {
            note.cursor_offset = note.content.len();
            note.sync_cursor();
        }
        Some(candidate)
    }

    /// Creates a new note with the given title as the file name.
    ///
    /// The title is sanitized (same rules as rename) and uniquified
    /// (`Title.md`, `Title 2.md`, ...). Used by "Shift+Enter — create"
    /// in the Go to file dialog.
    pub fn create_named_note(&mut self, title: &str) -> Option<PathBuf> {
        let ws = self.workspace_path.clone()?;
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return None;
        }
        let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        let sanitized: String = trimmed
            .chars()
            .filter(|c| !invalid_chars.contains(c) && !c.is_control())
            .collect();
        let base: String = sanitized.trim().chars().take(80).collect();
        if base.is_empty() {
            return None;
        }
        let mut candidate = ws.join(format!("{}.md", base));
        let mut counter = 2;
        while candidate.exists() {
            candidate = ws.join(format!("{} {}.md", base, counter));
            counter += 1;
        }
        let _ = std::fs::write(&candidate, String::new());
        self.reload_file_tree();
        self.open_note(candidate.clone());
        if let Some(ref mut note) = self.active_note {
            note.cursor_offset = note.content.len();
            note.sync_cursor();
        }
        Some(candidate)
    }

    /// Opens (activates) an empty tab: no file on disk, the New Tab page
    /// is shown instead of an editor. The sentinel is added to `open_tabs`
    /// like a regular tab so it renders in the tab strip.
    pub fn open_empty_tab(&mut self) -> PathBuf {
        // Save current note's cursor and selection before switching
        if let Some(ref current) = self.active_note {
            let entry = self.note_states.entry(current.path.clone()).or_default();
            entry.cursor_offset = current.cursor_offset;
            entry.selection_anchor = current.selection_anchor;
            self.session.cursor_positions.insert(current.path.clone(), current.cursor_offset);
            self.session.note_states.insert(current.path.clone(), entry.clone());
        }
        let path = empty_tab_path();
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        self.active_note = None;
        self.active_empty_tab = Some(path.clone());
        self.session.active_note = Some(path.clone());
        self.save_session_state();
        path
    }

    /// Activates an already open empty tab.
    pub fn activate_empty_tab(&mut self, path: PathBuf) {
        if let Some(ref current) = self.active_note {
            let entry = self.note_states.entry(current.path.clone()).or_default();
            entry.cursor_offset = current.cursor_offset;
            entry.selection_anchor = current.selection_anchor;
            self.session.cursor_positions.insert(current.path.clone(), current.cursor_offset);
            self.session.note_states.insert(current.path.clone(), entry.clone());
        }
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        self.active_note = None;
        self.active_empty_tab = Some(path.clone());
        self.session.active_note = Some(path);
        self.save_session_state();
    }

    pub fn close_tab(&mut self, path: &Path) {
        if let Some(pos) = self.open_tabs.iter().position(|p| p == path) {
            self.open_tabs.remove(pos);
            if self.active_note.as_ref().map(|n| &n.path == path).unwrap_or(false) {
                self.activate_neighbor_after_close(pos);
            } else if self.active_empty_tab.as_deref() == Some(path) {
                self.active_empty_tab = None;
                self.activate_neighbor_after_close(pos);
            }
        } else if self.active_note.as_ref().map(|n| &n.path == path).unwrap_or(false) {
            self.active_note = None;
            self.session.active_note = None;
        } else if self.active_empty_tab.as_deref() == Some(path) {
            self.active_empty_tab = None;
            self.session.active_note = None;
        }
        self.save_session_state();
    }

    /// After closing the active tab at `pos`, activates the tab that slid
    /// into its place (or the last one). Handles both real and empty tabs.
    fn activate_neighbor_after_close(&mut self, pos: usize) {
        let next_path = if pos < self.open_tabs.len() {
            Some(self.open_tabs[pos].clone())
        } else if !self.open_tabs.is_empty() {
            Some(self.open_tabs[self.open_tabs.len() - 1].clone())
        } else {
            None
        };
        match next_path {
            Some(target) => {
                if is_empty_tab_path(&target) {
                    self.active_note = None;
                    self.active_empty_tab = Some(target.clone());
                    self.session.active_note = Some(target);
                } else {
                    self.active_empty_tab = None;
                    self.open_note(target);
                }
            }
            None => {
                self.active_note = None;
                self.active_empty_tab = None;
                self.session.active_note = None;
            }
        }
    }

    pub fn reload_files(&mut self) {
        // file_tree is maintained by reload_file_tree()
    }

    pub fn push_nav_history(&mut self, path: PathBuf) {
        if let Some(current) = self.nav_history.get(self.nav_history_index) {
            if current == &path {
                return;
            }
        }
        if !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len() {
            self.nav_history.truncate(self.nav_history_index + 1);
        }
        self.nav_history.push(path);
        if self.nav_history.len() > 100 {
            self.nav_history.remove(0);
        }
        self.nav_history_index = self.nav_history.len().saturating_sub(1);
    }

    pub fn can_navigate_back(&self) -> bool {
        self.nav_history_index > 0 && !self.nav_history.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        !self.nav_history.is_empty() && self.nav_history_index + 1 < self.nav_history.len()
    }

    pub fn navigate_back(&mut self) -> Option<PathBuf> {
        if self.can_navigate_back() {
            self.nav_history_index -= 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn navigate_forward(&mut self) -> Option<PathBuf> {
        if self.can_navigate_forward() {
            self.nav_history_index += 1;
            let target = self.nav_history[self.nav_history_index].clone();
            self.open_note_internal(target.clone());
            Some(target)
        } else {
            None
        }
    }

    pub fn note_breadcrumbs(&self) -> Vec<String> {
        let Some(ref note) = self.active_note else {
            return Vec::new();
        };
        if let Some(ref ws) = self.workspace_path {
            if let Ok(rel) = note.path.strip_prefix(ws) {
                let mut parts = Vec::new();
                for comp in rel.components() {
                    let comp_str = comp.as_os_str().to_string_lossy().to_string();
                    if !comp_str.is_empty() {
                        parts.push(comp_str);
                    }
                }
                if let Some(last) = parts.last_mut() {
                    if let Some(stripped) = last.strip_suffix(".md") {
                        *last = stripped.to_string();
                    } else if let Some(stripped) = last.strip_suffix(".markdown") {
                        *last = stripped.to_string();
                    }
                }
                if !parts.is_empty() {
                    return parts;
                }
            }
        }
        vec![note.title.clone()]
    }

    pub fn open_note(&mut self, path: PathBuf) {
        self.push_nav_history(path.clone());
        self.open_note_internal(path);
    }

    pub fn open_note_internal(&mut self, path: PathBuf) {
        // Empty-tab sentinels carry no file: just activate the New Tab page.
        if is_empty_tab_path(&path) {
            self.activate_empty_tab(path);
            return;
        }
        self.reveal_path_in_tree(&path);

        // Save current note's cursor and selection before switching
        if let Some(ref current) = self.active_note {
            let entry = self.note_states.entry(current.path.clone()).or_default();
            entry.cursor_offset = current.cursor_offset;
            entry.selection_anchor = current.selection_anchor;
            self.session.cursor_positions.insert(current.path.clone(), current.cursor_offset);
            self.session.note_states.insert(current.path.clone(), entry.clone());
        }

        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        let fallback = self.untitled_base();
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or(fallback);

        let mut opened_note = None;
        if path.to_string_lossy() == "Welcome.md" {
            opened_note = Some(OpenedNote::new(
                path.clone(),
                "Welcome to Vibecodium".to_owned(),
                DEFAULT_WELCOME_NOTE.to_owned(),
                0,
            ));
        } else {
            if let Ok(content) = std::fs::read_to_string(&path) {
                opened_note = Some(OpenedNote::new(
                    path.clone(),
                    title.clone(),
                    content,
                    0,
                ));
                self.status_text = format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy());
            } else {
                eprintln!("Failed to read file: {:?}", path);
            }
        }

        if let Some(mut note) = opened_note {
            // Restore saved settings (cursor and selection) for this note
            if let Some(settings) = self.note_states.get(&path) {
                let mut offset = settings.cursor_offset.min(note.content.len());
                while offset > 0 && !note.content.is_char_boundary(offset) {
                    offset -= 1;
                }
                note.cursor_offset = offset;
                note.sync_cursor();

                note.selection_anchor = settings.selection_anchor.map(|anchor| {
                    let mut a = anchor.min(note.content.len());
                    while a > 0 && !note.content.is_char_boundary(a) {
                        a -= 1;
                    }
                    a
                });
            }
            self.active_note = Some(note);
            self.active_empty_tab = None;
            self.session.active_note = Some(path.clone());
            self.save_session_state();

            let rel = self.vault.as_ref()
                .and_then(|v| path.strip_prefix(&v.root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            self.events.emit_workspace(&crate::events::WorkspaceEvent::FileOpen { path: rel });
            self.publish_snapshot();
        }
    }

    pub fn snapshot_active_note_settings(&mut self, scroll_top: f32, edit_mode: bool, word_wrap: bool) {
        if let Some(ref note) = self.active_note {
            let settings = NoteSettings {
                cursor_offset: note.cursor_offset,
                selection_anchor: note.selection_anchor,
                scroll_top,
                edit_mode,
                word_wrap,
            };
            self.note_states.insert(note.path.clone(), settings.clone());
            self.session.note_states.insert(note.path.clone(), settings);
            self.session.cursor_positions.insert(note.path.clone(), note.cursor_offset);
        }
    }

    pub fn save_active_note(&mut self) {
        if let Some(ref mut note) = self.active_note {
            if let Err(err) = std::fs::write(&note.path, &note.content) {
                self.status_text = format!("Save error: {}", err);
            } else {
                note.is_dirty = false;
                self.status_text = format!("Saved: {}", note.title);

                let rel_path = self.vault.as_ref()
                    .and_then(|v| note.path.strip_prefix(&v.root).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|| note.path.to_string_lossy().into_owned());
                self.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
                self.publish_snapshot();
            }
        }
        self.save_session_state();
    }

    pub fn save_session_state(&mut self) {
        if let Some(ref note) = self.active_note {
            let entry = self.note_states.entry(note.path.clone()).or_default();
            entry.cursor_offset = note.cursor_offset;
            entry.selection_anchor = note.selection_anchor;
            self.session.note_states.insert(note.path.clone(), entry.clone());
            self.session.cursor_positions.insert(note.path.clone(), note.cursor_offset);
            self.session.active_note = Some(note.path.clone());
            self.active_empty_tab = None;
        } else if let Some(ref empty) = self.active_empty_tab.clone() {
            self.session.active_note = Some(empty.clone());
        }
        self.session.open_tabs = self.open_tabs.clone();
        self.session.workspace_path = self.workspace_path.clone();
        self.session.expanded_folders = self.expanded_folders.clone();
        self.session.known_vaults = self.known_vaults.clone();
        let dir = app_data_dir();
        self.session.save(&dir);
    }

    pub fn rename_note(&mut self, old_path: &Path, new_title: &str) -> Result<PathBuf, String> {
        let trimmed = new_title.trim();
        if trimmed.is_empty() {
            return Err("Note title cannot be empty".to_string());
        }

        let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if trimmed.chars().any(|c| invalid_chars.contains(&c) || c.is_control()) {
            return Err("Title contains invalid characters (< > : \" / \\ | ? *)".to_string());
        }

        let ext = old_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md");

        let parent = match old_path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => {
                if let Some(ref ws) = self.workspace_path {
                    ws.clone()
                } else {
                    PathBuf::from(".")
                }
            }
        };

        let new_filename = format!("{}.{}", trimmed, ext);
        let new_path = parent.join(&new_filename);

        if new_path == old_path {
            return Ok(old_path.to_path_buf());
        }

        if new_path.exists() {
            return Err(format!("File '{}' already exists", new_filename));
        }

        // If currently open and dirty, save it before rename
        if let Some(ref mut note) = self.active_note {
            if note.path == old_path && note.is_dirty {
                let _ = std::fs::write(old_path, &note.content);
                note.is_dirty = false;
            }
        }

        // Rename on disk if the old file exists
        if old_path.exists() {
            std::fs::rename(old_path, &new_path)
                .map_err(|e| format!("Failed to rename file: {}", e))?;
        }

        // Update active note if it is the renamed note
        if let Some(ref mut note) = self.active_note {
            if note.path == old_path {
                note.path = new_path.clone();
                note.title = trimmed.to_string();
            }
        }

        // Update open tabs
        for tab in &mut self.open_tabs {
            if tab == old_path {
                *tab = new_path.clone();
            }
        }

        // Update navigation history
        for nav in &mut self.nav_history {
            if nav == old_path {
                *nav = new_path.clone();
            }
        }

        // Update note states map
        if let Some(settings) = self.note_states.remove(old_path) {
            self.note_states.insert(new_path.clone(), settings);
        }

        // Update session
        self.session.open_tabs = self.open_tabs.clone();
        if self.session.active_note.as_deref() == Some(old_path) {
            self.session.active_note = Some(new_path.clone());
        }
        if let Some(cursor) = self.session.cursor_positions.remove(old_path) {
            self.session.cursor_positions.insert(new_path.clone(), cursor);
        }
        if let Some(st) = self.session.note_states.remove(old_path) {
            self.session.note_states.insert(new_path.clone(), st);
        }

        self.reload_file_tree();
        self.reload_files();
        self.reveal_path_in_tree(&new_path);
        self.save_session_state();
        self.status_text = format!("Renamed to: {}", trimmed);

        Ok(new_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tab_sentinel_roundtrip() {
        let a = empty_tab_path();
        let b = empty_tab_path();
        assert!(is_empty_tab_path(&a));
        assert!(is_empty_tab_path(&b));
        // Unique per tab so several empty tabs can coexist.
        assert_ne!(a, b);
        assert!(!is_empty_tab_path(Path::new("note.md")));
        assert!(!is_empty_tab_path(Path::new("Welcome.md")));
        assert!(!is_empty_tab_path(Path::new("")));
    }

    #[test]
    fn session_persists_sidebar_geometry() {
        let dir = std::env::temp_dir().join(format!(
            "vibecodium_session_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let mut saved = SessionState::default();
        saved.sidebar_width = 320.0;
        saved.sidebar_mode = crate::views::sidebar::SidebarMode::Search;
        saved.known_vaults = vec![PathBuf::from("D:/vault-a"), PathBuf::from("D:/vault-b")];
        saved.save(&dir);

        let loaded = SessionState::load(&dir);
        assert_eq!(loaded.sidebar_width, 320.0);
        assert_eq!(
            loaded.sidebar_mode,
            crate::views::sidebar::SidebarMode::Search
        );
        assert_eq!(
            loaded.known_vaults,
            vec![PathBuf::from("D:/vault-a"), PathBuf::from("D:/vault-b")]
        );

        // Session files written before these keys existed still load.
        std::fs::write(dir.join("session-state.json"), "{}").unwrap();
        let legacy = SessionState::load(&dir);
        assert_eq!(legacy.sidebar_width, 0.0);
        assert_eq!(
            legacy.sidebar_mode,
            crate::views::sidebar::SidebarMode::Files
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_opened_note_cursor_movement_zero_reparse() {
        let mut text = String::new();
        for i in 0..5000 {
            text.push_str(&format!("Line {} with **bold** text and `code`\n", i));
        }

        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            text,
            1,
        );

        // Initial cache build
        note.ensure_visual_cache(true, 80);
        assert_eq!(note.cached_lines.len(), 5001); // 5000 lines + trailing empty
        assert_eq!(note.cached_line_starts.len(), 5001);
        let initial_lines = Arc::clone(&note.cached_lines);

        let t0 = std::time::Instant::now();
        const MOVES: usize = 1000;
        for _ in 0..MOVES {
            note.move_cursor_right(false);
            note.ensure_visual_cache(true, 80);
        }
        for _ in 0..MOVES {
            note.move_cursor_left(false);
            note.ensure_visual_cache(true, 80);
        }
        let elapsed = t0.elapsed();
        let per_move_us = elapsed.as_micros() as f64 / (MOVES * 2) as f64;

        println!(
            "2000 cursor moves on 5,000 lines: total {:?}, {:.2} µs/move",
            elapsed, per_move_us
        );

        // CRITICAL REQUIREMENTS:
        // 1. Cached lines Arc must NOT have been reallocated or rebuilt during cursor movements:
        assert!(Arc::ptr_eq(&note.cached_lines, &initial_lines), "Cache must not be rebuilt during cursor moves");
        assert!(!note.visual_cache_dirty, "Cache must remain clean");
        // 2. Must be << 200 µs (0.2 ms)
        assert!(per_move_us < 200.0, "Per move latency should be under 200 µs, got {:.2} µs", per_move_us);
    }

    #[test]
    fn test_opened_note_incremental_text_edit() {
        let text = "# Title\n\nSome paragraph text here.\n".to_string();
        let mut note = OpenedNote::new(
            PathBuf::from("edit.md"),
            "edit".to_string(),
            text,
            1,
        );

        note.ensure_visual_cache(true, 80);
        assert!(!note.visual_cache_dirty);

        // Move cursor to inside paragraph: line 2 ("Some paragraph text here.")
        // "# Title\n\n" is 9 bytes. "Some " is 5 bytes. Offset 14 is right after "Some ".
        note.cursor_offset = 14;
        note.sync_cursor();

        // Insert text incrementally
        note.insert_text("awesome ");
        assert_eq!(note.content, "# Title\n\nSome awesome paragraph text here.\n");
        assert_eq!(note.doc.source, note.content);
        assert!(note.visual_cache_dirty, "Cache must be marked dirty after text insertion");

        // Rebuilding cache should reflect the new content
        note.ensure_visual_cache(true, 80);
        assert!(!note.visual_cache_dirty);
        assert_eq!(note.cached_lines[2], "Some awesome paragraph text here.");

        // Undo
        note.undo();
        assert_eq!(note.content, "# Title\n\nSome paragraph text here.\n");
        assert_eq!(note.doc.source, note.content);
        assert!(note.visual_cache_dirty);

        note.ensure_visual_cache(true, 80);
        assert_eq!(note.cached_lines[2], "Some paragraph text here.");

        // Redo
        note.redo();
        assert_eq!(note.content, "# Title\n\nSome awesome paragraph text here.\n");
        assert_eq!(note.doc.source, note.content);
    }

    #[test]
    fn test_property_move_cursor_up_always_decreases_v_idx() {
        let mut text = String::new();
        // Generate a document with empty lines, short lines, long lines (> 80 chars), and various text
        for i in 0..100 {
            if i % 5 == 0 {
                text.push_str("\n");
            } else if i % 5 == 1 {
                text.push_str("   \n");
            } else if i % 5 == 2 {
                text.push_str("Short\n");
            } else if i % 5 == 3 {
                text.push_str("This is a much longer line that definitely wraps around when word wrap is enabled at 40 or 80 columns in the editor view!\n");
            } else {
                text.push_str("Another medium line with some words and symbols.\n");
            }
        }

        let mut note = OpenedNote::new(
            PathBuf::from("prop.md"),
            "prop".to_string(),
            text,
            1,
        );

        note.ensure_visual_cache(true, 40); // wrap at 40 cols
        let visual_rows = Arc::clone(&note.cached_visual_rows);

        // Test every single possible cursor offset in the document!
        for offset in 1..=note.content.len() {
            note.cursor_offset = offset;
            let (cursor_line, cursor_col) = note.line_col();
            let (curr_v_idx, col_in_row) = match crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col) {
                Some(res) => res,
                None => continue,
            };

            if curr_v_idx == 0 {
                continue;
            }

            let target_v = &visual_rows[curr_v_idx - 1];
            let target_len = target_v.char_end.saturating_sub(target_v.char_start);
            let max_col = if target_v.is_last_sub {
                target_len
            } else {
                target_len.saturating_sub(1)
            };
            let target_col = col_in_row.min(max_col);
            let target_char = target_v.char_start + target_col;
            let new_offset = note.offset_for_line_col(target_v.line_idx, target_char);

            note.cursor_offset = new_offset;
            let (new_line, new_col) = note.line_col();
            let (new_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, new_line, new_col)
                .unwrap_or((0, 0));

            if new_v_idx >= curr_v_idx {
                panic!(
                    "CURSOR STUCK! orig_offset={}, curr_v_idx={}, target_v_idx={}, new_v_idx={}, \
                    cursor_line={}, cursor_col={}, col_in_row={}, target_len={}, target_char={}, new_offset={}, new_line={}, new_col={}",
                    offset, curr_v_idx, curr_v_idx - 1, new_v_idx,
                    cursor_line, cursor_col, col_in_row, target_len, target_char, new_offset, new_line, new_col
                );
            }
        }
    }

    #[test]
    fn test_property_move_cursor_down_always_increases_v_idx() {
        let mut text = String::new();
        for i in 0..100 {
            if i % 5 == 0 {
                text.push_str("\n");
            } else if i % 5 == 1 {
                text.push_str("   \n");
            } else if i % 5 == 2 {
                text.push_str("Short\n");
            } else if i % 5 == 3 {
                text.push_str("This is a much longer line that definitely wraps around when word wrap is enabled at 40 or 80 columns in the editor view!\n");
            } else {
                text.push_str("Another medium line with some words and symbols.\n");
            }
        }

        let mut note = OpenedNote::new(
            PathBuf::from("prop_down.md"),
            "prop_down".to_string(),
            text,
            1,
        );

        note.ensure_visual_cache(true, 40); // wrap at 40 cols
        let visual_rows = Arc::clone(&note.cached_visual_rows);

        for offset in 0..note.content.len() {
            note.cursor_offset = offset;
            let (cursor_line, cursor_col) = note.line_col();
            let (curr_v_idx, col_in_row) = match crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col) {
                Some(res) => res,
                None => continue,
            };

            if curr_v_idx + 1 >= visual_rows.len() {
                continue;
            }

            let target_v = &visual_rows[curr_v_idx + 1];
            let target_len = target_v.char_end.saturating_sub(target_v.char_start);
            let max_col = if target_v.is_last_sub {
                target_len
            } else {
                target_len.saturating_sub(1)
            };
            let target_col = col_in_row.min(max_col);
            let target_char = target_v.char_start + target_col;
            let new_offset = note.offset_for_line_col(target_v.line_idx, target_char);

            note.cursor_offset = new_offset;
            let (new_line, new_col) = note.line_col();
            let (new_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, new_line, new_col)
                .unwrap_or((0, 0));

            if new_v_idx <= curr_v_idx {
                panic!(
                    "CURSOR DOWN STUCK! orig_offset={}, curr_v_idx={}, target_v_idx={}, new_v_idx={}, \
                    cursor_line={}, cursor_col={}, col_in_row={}, target_len={}, target_char={}, new_offset={}, new_line={}, new_col={}",
                    offset, curr_v_idx, curr_v_idx + 1, new_v_idx,
                    cursor_line, cursor_col, col_in_row, target_len, target_char, new_offset, new_line, new_col
                );
            }
        }
    }

    #[test]
    fn test_file_node_matches_query() {
        let tree = FileNode::Folder {
            name: "Backend".to_string(),
            path: PathBuf::from("backend"),
            children: vec![
                FileNode::Folder {
                    name: "Services".to_string(),
                    path: PathBuf::from("backend/services"),
                    children: vec![
                        FileNode::File {
                            name: "auth_service.rs".to_string(),
                            path: PathBuf::from("backend/services/auth_service.rs"),
                        },
                    ],
                },
                FileNode::File {
                    name: "main.rs".to_string(),
                    path: PathBuf::from("backend/main.rs"),
                },
            ],
        };

        // Matching folder name
        assert!(tree.matches_query("backend"));
        assert!(tree.matches_query("back"));

        // Matching child folder name
        assert!(tree.matches_query("services"));

        // Matching nested file
        assert!(tree.matches_query("auth"));
        assert!(tree.matches_query("main"));

        // Non-matching
        assert!(!tree.matches_query("frontend"));
        assert!(!tree.matches_query("python"));

        // Empty query matches all
        assert!(tree.matches_query(""));
    }

    #[test]
    fn test_opened_note_utf8_cyrillic_typing_and_line_col() {
        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            "# Заметка\n".to_string(),
            1,
        );
        note.ensure_visual_cache(true, 80);

        // Move cursor to the end of the first line
        note.cursor_offset = note.content.len();

        let russian_text = "Привет мир! Это тестовая строка на русском языке.";
        for ch in russian_text.chars() {
            let mut s = String::new();
            s.push(ch);
            note.insert_text(&s);
            assert!(note.visual_cache_dirty);

            // Crucial: line_col() must not panic while visual_cache_dirty is true
            let (line, col) = note.line_col();
            assert_eq!(line, 1);
            // col should match number of characters on line 1
            let expected_col = note.content.split('\n').nth(1).unwrap().chars().count();
            assert_eq!(col, expected_col);
        }

        // Backspace test with multi-byte characters
        for _ in 0..10 {
            note.delete_backwards();
            let (line, col) = note.line_col();
            assert_eq!(line, 1);
            let expected_col = note.content.split('\n').nth(1).unwrap().chars().count();
            assert_eq!(col, expected_col);
        }

        // Move cursor left and delete forwards
        note.cursor_offset = note.content.len().saturating_sub(6); // inside Cyrillic text
        while !note.content.is_char_boundary(note.cursor_offset) {
            note.cursor_offset -= 1;
        }
        note.delete_forwards();
        let _ = note.line_col();

        // Selection across multi-byte characters
        note.selection_anchor = Some(2); // inside "# Заметка"
        let range = note.selection_range();
        assert!(range.is_some());
        let range = range.unwrap();
        assert!(note.content.is_char_boundary(range.start));
        assert!(note.content.is_char_boundary(range.end));
        let selected = note.selected_text();
        assert!(selected.is_some());

        // Test with exact template from create_note_in_folder: "# Без названия\n\n"
        let mut new_note = OpenedNote::new(
            PathBuf::from("Без названия.md"),
            "Без названия".to_string(),
            "# Без названия\n\n".to_string(),
            1,
        );
        new_note.ensure_visual_cache(true, 80);
        new_note.cursor_offset = new_note.content.len();

        // Type text containing 'ё' and other Cyrillic characters
        let sample = "Привет, моё солнышко! Ещё одна заметка.";
        for ch in sample.chars() {
            let mut s = String::new();
            s.push(ch);
            new_note.insert_text(&s);
            let _ = new_note.line_col();
        }

        // Backspace every character one by one
        for _ in 0..sample.chars().count() {
            new_note.delete_backwards();
            let _ = new_note.line_col();
        }
    }

    #[test]
    fn test_emoji_line_shaping_and_positions() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let text: gpui::SharedString = "# Нативные: 😀 🚀 ⚠️ ❤️ 👍 🎉 🐛 👨‍👩‍👧‍👦 🏳️‍🌈 RU".into();
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let run = gpui::TextRun {
                    len: text.len(),
                    font: gpui::font("Droid Sans Mono").bold(),
                    color: Default::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(text.clone(), gpui::px(22.0), &[run], None);
                println!("shaped line len in bytes: {}", shaped.len());
                println!("shaped total width: {:?}", shaped.width);
                println!("x at 0 (#): {:?}", shaped.x_for_index(0));
                println!("x at end (102, after RU): {:?}", shaped.x_for_index(102));
                println!("closest index for width: {:?}", shaped.closest_index_for_x(shaped.width));

                // Print x for all byte indices corresponding to chars
                let mut b_off = 0;
                for (c_idx, ch) in text.chars().enumerate() {
                    let x = shaped.x_for_index(b_off);
                    println!("c_idx #{:2} [{}] at byte {}: x = {:?}", c_idx, ch, b_off, x);
                    b_off += ch.len_utf8();
                }
                println!("FINAL at byte {}: x = {:?}", b_off, shaped.x_for_index(b_off));
                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    #[test]
    fn test_opened_note_version_increments() {
        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            "initial".to_string(),
            0,
        );
        assert_eq!(note.version, 0);

        note.insert_text(" text");
        assert_eq!(note.version, 1);

        note.delete_backwards();
        assert_eq!(note.version, 2);

        note.undo();
        assert_eq!(note.version, 3);

        note.redo();
        assert_eq!(note.version, 4);
    }

    #[test]
    fn test_app_state_snapshot_publish() {
        let temp_dir = std::env::temp_dir().join(format!("test_snapshot_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let vault = crate::vault::VaultCore::open(temp_dir.clone());
        let _ = vault.create_file("Note1.md", "content 1").unwrap();

        let events = Arc::new(crate::events::EventBus::new());
        let commands = Arc::new(crate::commands::CommandRegistry::new());
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));

        let mut note = OpenedNote::new(
            temp_dir.join("Note1.md"),
            "Note1".into(),
            "content 1".into(),
            42,
        );
        note.version = 42;

        let state = AppState {
            settings: Arc::new(crate::settings::SettingsManager::new(&temp_dir)),
            workspace_path: Some(temp_dir.clone()),
            workspace_name: "test".into(),
            vault: Some(vault),
            events,
            commands,
            ribbon: Arc::new(crate::ribbon::RibbonRegistry::new()),
            status_bar: Arc::new(crate::ribbon::StatusBarRegistry::new()),
            code_block_processors: Arc::new(crate::plugins::code_block::CodeBlockProcessorRegistry::new()),
            plugin_host: Arc::new(crate::plugins::bridge::PluginHostState::new()),
            snapshot: snapshot.clone(),
            file_tree: Vec::new(),
            expanded_folders: HashSet::new(),
            search_query: String::new(),
            active_note: Some(note),
            open_tabs: Vec::new(),
            active_empty_tab: None,
            nav_history: Vec::new(),
            nav_history_index: 0,
            status_text: String::new(),
            session: SessionState::default(),
            note_states: HashMap::new(),
            known_vaults: Vec::new(),
        };

        state.publish_snapshot();

        let snap = snapshot.load();
        assert_eq!(snap.version, 42);
        assert!(snap.active_file.is_some());
        assert_eq!(snap.active_file.as_ref().unwrap().path, "Note1.md");
        assert_eq!(snap.files.len(), 1);
        assert_eq!(snap.files[0].name, "Note1.md");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

