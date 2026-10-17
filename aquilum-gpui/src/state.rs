use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::document_buffer::{DocumentBuffer, DocumentId};
use crate::workspace::{EditorViewState, LeafId, LeafState, TabId, WorkspaceTree};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsChangeEvent {
    Deleted(PathBuf),
    Renamed { old_path: PathBuf, new_path: PathBuf },
    Modified(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub document_count: usize,
    pub document_bytes: usize,
    pub by_path_count: usize,
    pub view_count: usize,
    pub active_note_bytes: usize,
    pub open_tabs_count: usize,
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

/// Above this size the exact word/char count is not computed: two full passes
/// over hundreds of MB on the UI thread are a freeze by themselves.
pub const TEXT_STATS_EXACT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStats {
    Exact { words: usize, chars: usize },
    Large { bytes: usize },
}

pub fn compute_text_stats(text: &str) -> TextStats {
    let len = text.len();
    if len > TEXT_STATS_EXACT_LIMIT {
        TextStats::Large { bytes: len }
    } else {
        TextStats::Exact {
            words: text.split_whitespace().count(),
            chars: text.chars().count(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    /// Byte offset where the change occurred.
    pub at: usize,
    /// Text removed by this change (empty for pure insertion).
    pub removed: String,
    /// Text inserted by this change (empty for pure deletion).
    pub inserted: String,
}

#[derive(Debug, Clone)]
pub struct UndoStep {
    pub changes: Vec<TextChange>,
    pub cursor_before: usize,
    pub cursor_after: usize,
    pub time: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteEditEvent {
    pub edit_line: usize,
    pub at: usize,
    pub old_len: usize,
    pub new_len: usize,
    pub line_diff: isize,
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
    pub first_dirty_time: Option<std::time::Instant>,
    pub undo_stack: Vec<UndoStep>,
    pub redo_stack: Vec<UndoStep>,
    pub pending_edits: Vec<NoteEditEvent>,

    /// Line addressing for `content`, in O(1) memory per `LINE_BLOCK` lines.
    /// Kept in sync with `content` on every mutation; see `refresh_line_index`.
    pub line_index: crate::line_index::LineIndex,

    /// Fence state every [`crate::code_scan::CHECKPOINT_LINES`] lines.
    ///
    /// The window can open anywhere, so it needs the code-fence state at its
    /// first line rather than at line 0. These checkpoints make that a bounded
    /// lookup instead of a scan from the top of the file; see
    /// [`crate::code_scan`].
    pub code_checkpoints: crate::code_scan::CodeCheckpoints,

    /// The materialised slice of the document: lines, wrapped rows, code-fence
    /// states and row geometry, for a bounded range around the viewport. The
    /// renderer only ever draws a viewport, so this is all it needs — see
    /// [`crate::editor_window`].
    pub window: crate::editor_window::EditorWindow,
    /// Document lines before [`OpenedNote::window`], addressed by estimate only.
    pub above_lines: usize,
    /// Document lines after [`OpenedNote::window`], addressed by estimate only.
    pub below_lines: usize,
    /// Estimated height above the window, in logical pixels.
    pub above_h: f64,
    /// Estimated height below the window, in logical pixels.
    pub below_h: f64,
    /// Rows per line and row height the estimate above and below uses.
    ///
    /// Frozen when the window is built for a given wrap setting, so that
    /// `total_height` — and with it the scrollbar and `max_scroll` — does not
    /// drift as the window moves. Recomputing them from every new window would
    /// make the whole document jump whenever the density of the text changed.
    pub est_rows_per_line: f64,
    pub est_row_h: f64,
    pub visual_cache_dirty: bool,

    /// (version, content.len(), stats). Recomputed only when the text changes.
    pub stats_cache: Cell<Option<(u64, usize, TextStats)>>,
    pub view_shell_cursor_line_col: Option<(usize, usize)>,
    pub view_shell_selection_range: Option<Range<usize>>,
    pub view_shell_window_start_byte: Option<(usize, usize)>,
    pub view_shell_sel_lines: Option<(usize, usize)>,
}

impl OpenedNote {
    pub fn new(path: PathBuf, title: String, content: String, version: u64) -> Self {
        let line_index = crate::line_index::LineIndex::new(&content);
        Self {
            path,
            title,
            content,
            version,
            cursor_offset: 0,
            selection_anchor: None,
            is_dirty: false,
            first_dirty_time: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_edits: Vec::new(),
            line_index,
            code_checkpoints: crate::code_scan::CodeCheckpoints::new(),
            window: crate::editor_window::EditorWindow::default(),
            above_lines: 0,
            below_lines: 0,
            above_h: 0.0,
            below_h: 0.0,
            est_rows_per_line: 1.0,
            est_row_h: 24.0,
            visual_cache_dirty: true,
            stats_cache: Cell::new(None),
            view_shell_cursor_line_col: None,
            view_shell_selection_range: None,
            view_shell_window_start_byte: None,
            view_shell_sel_lines: None,
        }
    }

    pub fn new_view_shell(
        path: PathBuf,
        title: String,
        version: u64,
        cursor_offset: usize,
        selection_anchor: Option<usize>,
        is_dirty: bool,
        first_dirty_time: Option<std::time::Instant>,
        line_index: crate::line_index::LineIndex,
        window: crate::editor_window::EditorWindow,
        above_lines: usize,
        below_lines: usize,
        above_h: f64,
        below_h: f64,
        est_rows_per_line: f64,
        est_row_h: f64,
        stats: Option<TextStats>,
    ) -> Self {
        Self {
            path,
            title,
            content: String::new(),
            version,
            cursor_offset,
            selection_anchor,
            is_dirty,
            first_dirty_time,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_edits: Vec::new(),
            line_index,
            code_checkpoints: crate::code_scan::CodeCheckpoints::new(),
            window,
            above_lines,
            below_lines,
            above_h,
            below_h,
            est_rows_per_line,
            est_row_h,
            visual_cache_dirty: false,
            stats_cache: Cell::new(stats.map(|s| (version, 0, s))),
            view_shell_cursor_line_col: None,
            view_shell_selection_range: None,
            view_shell_window_start_byte: None,
            view_shell_sel_lines: None,
        }
    }

    pub fn with_view_shell_layout(
        mut self,
        cursor_line_col: (usize, usize),
        selection_range: Option<Range<usize>>,
        window_start_byte: (usize, usize),
        sel_lines: Option<(usize, usize)>,
    ) -> Self {
        self.view_shell_cursor_line_col = Some(cursor_line_col);
        self.view_shell_selection_range = selection_range;
        self.view_shell_window_start_byte = Some(window_start_byte);
        self.view_shell_sel_lines = sel_lines;
        self
    }

    /// Marks the note dirty, recording `first_dirty_time` if this is the start
    /// of an unwritten edit sequence.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
        if self.first_dirty_time.is_none() {
            self.first_dirty_time = Some(std::time::Instant::now());
        }
    }

    /// Clears the dirty state and resets `first_dirty_time`.
    pub fn clear_dirty(&mut self) {
        self.is_dirty = false;
        self.first_dirty_time = None;
    }

    /// Word/char statistics for the navigation bar. Cheap for large documents:
    /// above `TEXT_STATS_EXACT_LIMIT` it reports the byte size instead of
    /// running two full passes over the text on the UI thread.
    pub fn text_stats(&self) -> TextStats {
        let len = self.content.len();
        if let Some((v, l, s)) = self.stats_cache.get() {
            if v == self.version && (l == len || self.content.is_empty()) {
                return s;
            }
        }
        let stats = compute_text_stats(&self.content);
        self.stats_cache.set(Some((self.version, len, stats)));
        stats
    }

    pub fn rebuild_visual_cache(&mut self, word_wrap: bool, wrap_cols: usize) {
        self.rebuild_visual_cache_with_headings(word_wrap, wrap_cols, None, 0);
    }

    /// Materialise the window that holds `anchor_line`.
    ///
    /// This is the entire cost of opening a note: the `LineIndex` pass over the
    /// text (in [`OpenedNote::new`]) plus a split, a wrap, a code-fence scan and
    /// a measure of `MARGIN_LINES` lines on each side of the anchor — never of
    /// the document. A note small enough to fit in two margins is built whole,
    /// exactly as before.
    pub fn rebuild_visual_cache_with_headings(
        &mut self,
        word_wrap: bool,
        wrap_cols: usize,
        headings: Option<&crate::settings::HeadingsSettings>,
        anchor_line: usize,
    ) {
        crate::perf::PERF.parse_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // The estimate is only valid for the wrap setting it was measured with.
        let basis_stale = self.window.is_empty()
            || self.window.word_wrap != word_wrap
            || self.window.wrap_cols != wrap_cols;

        let lines_total = self.line_index.lines();
        let (start, end) = if lines_total <= 2 * crate::editor_window::MARGIN_LINES + 1 {
            (0, lines_total)
        } else if !self.window.is_empty()
            && !self.window.should_move_to(anchor_line, lines_total, word_wrap, wrap_cols)
        {
            let target_len = self
                .window
                .line_count()
                .max(2 * crate::editor_window::MARGIN_LINES + 1);
            let start = self.window.start_line.min(lines_total.saturating_sub(1));
            let end = (start + target_len).min(lines_total);
            (start, end)
        } else {
            crate::editor_window::EditorWindow::bounds_for(anchor_line, lines_total)
        };
        // The window can open anywhere, so it needs the fence state at its first
        // line. Extending the checkpoints is bounded by `start`, not by the
        // document: a note opened at the top scans a few thousand lines, and a
        // jump into the middle pays for the distance once and caches it.
        self.code_checkpoints
            .ensure(&self.content, &self.line_index, start + 1);
        let initial = self
            .code_checkpoints
            .state_at(&self.content, &self.line_index, start)
            .unwrap_or_default();
        let window = crate::editor_window::EditorWindow::build(
            &self.content,
            &self.line_index,
            start,
            end,
            word_wrap,
            wrap_cols,
            headings,
            initial,
        );
        crate::perf::PERF
            .span_rebuilds
            .fetch_add(window.line_count() as u64, std::sync::atomic::Ordering::Relaxed);

        self.above_lines = window.start_line;
        self.below_lines = self
            .line_index
            .lines()
            .saturating_sub(window.end_line);
        self.window = window;
        if basis_stale {
            self.refresh_estimate_basis();
        }
        self.recompute_estimate();
        self.visual_cache_dirty = false;
    }

    /// Rows-per-line and row-height of the current window.
    fn refresh_estimate_basis(&mut self) {
        let rows = self.window.visual_rows.len();
        let lines = self.window.line_count();
        if rows == 0 || lines == 0 {
            return;
        }
        self.est_rows_per_line = rows as f64 / lines as f64;
        self.est_row_h = self.window.total_height() / rows as f64;
    }

    /// Height of the estimated region on each side of the window.
    ///
    /// `above_lines + window + below_lines` is always the whole document, so
    /// `total_height` does not depend on where the window sits and the
    /// scrollbar stays put while the user scrolls.
    fn recompute_estimate(&mut self) {
        let rows_per_line = self.est_rows_per_line.max(f64::MIN_POSITIVE);
        let row_h = self.est_row_h;
        self.above_h = (self.above_lines as f64 * rows_per_line).round() * row_h;
        self.below_h = (self.below_lines as f64 * rows_per_line).round() * row_h;
    }

    pub fn ensure_visual_cache(&mut self, word_wrap: bool, wrap_cols: usize, anchor_line: usize) {
        self.ensure_visual_cache_with_headings(word_wrap, wrap_cols, None, anchor_line);
    }

    /// Make sure the window covers `anchor_line`, re-placing it if it does not.
    ///
    /// `anchor_line` is the document line at the top of the viewport: the window
    /// has to hold what is on screen, not what the caret is on.
    pub fn ensure_visual_cache_with_headings(
        &mut self,
        word_wrap: bool,
        wrap_cols: usize,
        headings: Option<&crate::settings::HeadingsSettings>,
        anchor_line: usize,
    ) {
        if self.visual_cache_dirty
            || self.window.should_move_to(
                anchor_line,
                self.line_index.lines(),
                word_wrap,
                wrap_cols,
            )
        {
            self.rebuild_visual_cache_with_headings(word_wrap, wrap_cols, headings, anchor_line);
        }
    }

    /// Total height of the document: the exact window plus the estimate on
    /// either side.
    pub fn total_height(&self) -> f64 {
        self.above_h + self.window.total_height() + self.below_h
    }

    /// Document Y of a window-local visual row index.
    pub fn doc_y_of_local_v_idx(&self, local_v_idx: usize) -> f64 {
        self.above_h + self.window.height_index.top_of(local_v_idx)
    }

    /// Document Y of a document line: exact inside the window, estimated outside it.
    pub fn doc_y_of_line(&self, line: usize) -> f64 {
        let lines_total = self.line_index.lines();
        if lines_total == 0 {
            return 0.0;
        }
        let line = line.min(lines_total.saturating_sub(1));

        // 1. Inside the window: exact row height from height_index
        if self.window.covers_line(line) {
            if let Some(v_idx) = self.window.first_row_of_line(line) {
                return self.above_h + self.window.height_index.top_of(v_idx);
            }
        }

        // 2. Above the window: estimated proportional height
        if line < self.window.start_line {
            if self.window.start_line == 0 || self.above_lines == 0 {
                return 0.0;
            }
            return (line as f64 / self.above_lines as f64) * self.above_h;
        }

        // 3. Below the window: estimated proportional height
        let window_bottom_y = self.above_h + self.window.total_height();
        if self.below_lines == 0 {
            return window_bottom_y;
        }
        let lines_below = line.saturating_sub(self.window.end_line);
        let fraction = (lines_below as f64 / self.below_lines as f64).clamp(0.0, 1.0);
        window_bottom_y + fraction * self.below_h
    }

    /// Window-local visual row index at a document Y.
    ///
    /// The window is placed to cover the viewport, so this is exact for every
    /// point the user can actually see; outside it clamps to the window edge.
    pub fn local_v_idx_at_doc_y(&self, doc_y: f64) -> usize {
        if self.window.visual_rows.is_empty() {
            return 0;
        }
        self.window.height_index.v_idx_at_y(doc_y - self.above_h)
    }

    /// Document line at a document Y: exact inside the window, proportional
    /// outside it.
    pub fn line_at_doc_y(&self, doc_y: f64) -> usize {
        let last_line = self.line_index.lines().saturating_sub(1);
        if self.window.visual_rows.is_empty() {
            // No window yet: one row per line is the only honest guess.
            let row_h = self.est_row_h.max(1.0);
            return ((doc_y.max(0.0) / row_h) as usize).min(last_line);
        }
        let rows_per_line = self.est_rows_per_line.max(f64::MIN_POSITIVE);
        let row_h = self.est_row_h.max(1.0);
        let local = doc_y - self.above_h;
        if local < 0.0 {
            let rows_above = (-local / row_h).ceil();
            let lines_above = (rows_above / rows_per_line).round() as usize;
            return self.window.start_line.saturating_sub(lines_above);
        }
        let window_h = self.window.total_height();
        if local >= window_h {
            let rows_below = (local - window_h) / row_h;
            let lines_below = (rows_below / rows_per_line).round() as usize;
            return self.window.end_line.saturating_add(lines_below).min(last_line);
        }
        let local_v = self.window.height_index.v_idx_at_y(local);
        self.window
            .visual_rows
            .get(local_v)
            .map(|v| v.line_idx)
            .unwrap_or(self.window.start_line)
    }

    pub fn record_change(&mut self, at: usize, removed: String, inserted: String, cursor_before: usize, cursor_after: usize) {
        if removed.is_empty() && inserted.is_empty() {
            return;
        }
        let edit_line = self.line_index.line_of(&self.content, at);
        let line_diff = (inserted.matches('\n').count() as isize) - (removed.matches('\n').count() as isize);
        self.pending_edits.push(NoteEditEvent {
            edit_line,
            at,
            old_len: removed.len(),
            new_len: inserted.len(),
            line_diff,
        });

        self.redo_stack.clear();
        let now = std::time::Instant::now();
        let change = TextChange { at, removed, inserted };

        if let Some(last_step) = self.undo_stack.last_mut() {
            if now.duration_since(last_step.time) < std::time::Duration::from_millis(1000)
                && last_step.changes.len() == 1
            {
                let last = &mut last_step.changes[0];
                // 1. Sequential typing without newlines
                if last.removed.is_empty()
                    && change.removed.is_empty()
                    && !change.inserted.contains('\n')
                    && last.at + last.inserted.len() == change.at
                {
                    last.inserted.push_str(&change.inserted);
                    last_step.cursor_after = cursor_after;
                    last_step.time = now;
                    return;
                }
                // 2. Sequential backspace without newlines
                if last.inserted.is_empty()
                    && change.inserted.is_empty()
                    && !change.removed.contains('\n')
                    && change.at + change.removed.len() == last.at
                {
                    last.removed.insert_str(0, &change.removed);
                    last.at = change.at;
                    last_step.cursor_after = cursor_after;
                    last_step.time = now;
                    return;
                }
            }
        }

        if self.undo_stack.len() >= 1000 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(UndoStep {
            changes: vec![change],
            cursor_before,
            cursor_after,
            time: now,
        });
    }

    pub fn snapshot_undo(&mut self) {
        // No-op: changes are now recorded incrementally as deltas in undo_stack
    }

    pub fn undo(&mut self) {
        if let Some(step) = self.undo_stack.pop() {
            for change in step.changes.iter().rev() {
                self.content.replace_range(change.at..change.at + change.inserted.len(), &change.removed);
                self.refresh_line_index_incremental(change.at, &change.inserted, &change.removed);
                let edit_line = self.line_index.line_of(&self.content, change.at);
                let line_diff = (change.removed.matches('\n').count() as isize) - (change.inserted.matches('\n').count() as isize);
                self.pending_edits.push(NoteEditEvent {
                    edit_line,
                    at: change.at,
                    old_len: change.inserted.len(),
                    new_len: change.removed.len(),
                    line_diff,
                });
            }
            self.version += 1;
            self.cursor_offset = step.cursor_before.min(self.content.len());
            self.selection_anchor = None;
            self.mark_dirty();
            self.visual_cache_dirty = true;
            self.redo_stack.push(step);
        }
    }

    pub fn redo(&mut self) {
        if let Some(step) = self.redo_stack.pop() {
            for change in step.changes.iter() {
                self.content.replace_range(change.at..change.at + change.removed.len(), &change.inserted);
                self.refresh_line_index_incremental(change.at, &change.removed, &change.inserted);
                let edit_line = self.line_index.line_of(&self.content, change.at);
                let line_diff = (change.inserted.matches('\n').count() as isize) - (change.removed.matches('\n').count() as isize);
                self.pending_edits.push(NoteEditEvent {
                    edit_line,
                    at: change.at,
                    old_len: change.removed.len(),
                    new_len: change.inserted.len(),
                    line_diff,
                });
            }
            self.version += 1;
            self.cursor_offset = step.cursor_after.min(self.content.len());
            self.selection_anchor = None;
            self.mark_dirty();
            self.visual_cache_dirty = true;
            self.undo_stack.push(step);
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        if let Some(ref r) = self.view_shell_selection_range {
            return Some(r.clone());
        }
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
            let cursor_before = self.cursor_offset;
            let removed = self.content[range.clone()].to_string();
            let at = range.start;
            self.content.replace_range(range.clone(), "");
            self.version += 1;
            self.refresh_line_index_incremental(at, &removed, "");
            self.cursor_offset = at;
            self.selection_anchor = None;
            self.mark_dirty();
            self.visual_cache_dirty = true;
            self.record_change(at, removed, String::new(), cursor_before, at);
            true
        } else {
            false
        }
    }

    pub fn replace_selection(&mut self, text: &str) -> bool {
        if let Some(range) = self.selection_range() {
            let cursor_before = self.cursor_offset;
            let removed = self.content[range.clone()].to_string();
            let at = range.start;
            let new_cursor = at + text.len();
            self.content.replace_range(range.clone(), text);
            self.version += 1;
            self.refresh_line_index_incremental(at, &removed, text);
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            self.mark_dirty();
            self.visual_cache_dirty = true;
            self.record_change(at, removed, text.to_string(), cursor_before, new_cursor);
            true
        } else {
            let mut offset = self.cursor_offset.min(self.content.len());
            while offset > 0 && !self.content.is_char_boundary(offset) {
                offset -= 1;
            }
            let cursor_before = self.cursor_offset;
            let at = offset;
            let new_cursor = at + text.len();
            self.content.replace_range(offset..offset, text);
            self.version += 1;
            self.refresh_line_index_incremental(at, "", text);
            self.cursor_offset = new_cursor;
            self.selection_anchor = None;
            self.mark_dirty();
            self.visual_cache_dirty = true;
            self.record_change(at, String::new(), text.to_string(), cursor_before, new_cursor);
            true
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor_offset = self.content.len();
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

    /// Rebuild [`OpenedNote::line_index`] from `content`, and drop the code
    /// checkpoints the edit invalidated.
    ///
    /// Called from every path that mutates the text. One `memchr` pass, no
    /// allocation per line — this is what replaced the old `Vec<String>` plus
    /// `Vec<usize>` pair on the caret path. Making it incremental (splice the
    /// checkpoints after the edit instead of rescanning) is stage 6 of
    /// `TODO/perf-plan-large-file-open.md`.
    ///
    /// `edited_at` is the byte offset the text changed at, or `None` when the
    /// whole text was replaced wholesale (undo/redo) and no prefix can be
    /// trusted. An edit at line `E` cannot change the fence state of any line
    /// above `E`, so the checkpoints above it survive and the rest are
    /// rescanned on demand — which keeps a keystroke in the middle of a 400 MB
    /// note from throwing away the whole scan.
    #[cfg(test)]
    fn refresh_line_index(&mut self, edited_at: Option<usize>) {
        self.line_index = crate::line_index::LineIndex::new(&self.content);
        match edited_at {
            Some(byte) => {
                let line = self.line_index.line_of(&self.content, byte);
                self.code_checkpoints.truncate_to(line);
            }
            None => self.code_checkpoints.reset(),
        }
    }

    fn refresh_line_index_incremental(&mut self, edit_byte: usize, removed: &str, inserted: &str) {
        self.line_index.splice(&self.content, edit_byte, removed, inserted);
        let line = self.line_index.line_of(&self.content, edit_byte);
        self.code_checkpoints.truncate_to(line);
    }

    /// Walk a little further ahead through the document's code fences.
    ///
    /// Called once per frame from the render path. The window only ever needs
    /// the state at its first line, and that is extended on demand — this is
    /// what makes a later jump to the end of a huge note free instead of a
    /// one-off scan of the whole distance. Bounded by `max_lines`, so it costs
    /// a fraction of a millisecond and never blocks a frame.
    pub fn scan_code_ahead(&mut self, max_lines: usize) -> usize {
        if self.code_checkpoints.is_complete(self.line_index.lines()) {
            return 0;
        }
        self.code_checkpoints
            .extend_slice(&self.content, &self.line_index, max_lines)
    }

    /// Возвращает (line_index, col_index) 0-indexed
    pub fn line_col(&self) -> (usize, usize) {
        if let Some(lc) = self.view_shell_cursor_line_col {
            return lc;
        }
        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let line_idx = self.line_index.line_of(&self.content, offset);
        // `min(offset)` guards against a stale index after an unindexed edit:
        // slicing must stay in bounds even if the two ever disagree.
        let line_start = self.line_index.start(&self.content, line_idx).min(offset);
        let col_idx = self.content[line_start..offset].chars().count();
        (line_idx, col_idx)
    }

    pub fn offset_for_line_col(&self, target_line: usize, target_col: usize) -> usize {
        if target_line >= self.line_index.lines() {
            return self.content.len();
        }
        let line_start = self.line_index.start(&self.content, target_line);
        let line_end = self
            .line_index
            .end(&self.content, target_line)
            .max(line_start)
            .min(self.content.len());
        let line = &self.content[line_start.min(self.content.len())..line_end];
        let actual_col = target_col.min(line.chars().count());
        let byte_col: usize = line.chars().take(actual_col).map(|c| c.len_utf8()).sum();
        line_start + byte_col
    }

    /// Byte offset where `line` starts.
    ///
    /// Replaces the old `cached_line_starts` lookup: that array held one offset
    /// per line (8 bytes × lines), this jumps to the nearest checkpoint and
    /// scans at most `LINE_BLOCK` lines with `memchr`.
    pub fn line_start_byte(&self, line: usize) -> usize {
        if let Some((w_start_line, w_start_byte)) = self.view_shell_window_start_byte {
            if line >= w_start_line && line < w_start_line + self.window.lines.len() {
                let offset_in_window: usize = self.window.lines.as_slice()[..line - w_start_line]
                    .iter()
                    .map(|l| l.len() + 1)
                    .sum();
                return w_start_byte + offset_in_window;
            }
        }
        self.line_index.start(&self.content, line)
    }

    /// Index of the line containing `byte`.
    ///
    /// Same contract as `cached_line_starts.binary_search(&byte)` with
    /// `Err(idx) => idx - 1`: the byte belongs to the last line starting at or
    /// before it.
    pub fn line_of_byte(&self, byte: usize) -> usize {
        if let Some(sel) = self.view_shell_selection_range.as_ref() {
            if let Some((s_line, e_line)) = self.view_shell_sel_lines {
                if byte == sel.start {
                    return s_line;
                }
                if byte == sel.end {
                    return e_line;
                }
            }
        }
        if let Some((w_start_line, w_start_byte)) = self.view_shell_window_start_byte {
            let mut at = w_start_byte;
            for (i, l) in self.window.lines.as_slice().iter().enumerate() {
                let next = at + l.len() + 1;
                if byte < next {
                    return w_start_line + i;
                }
                at = next;
            }
        }
        self.line_index.line_of(&self.content, byte)
    }

    pub fn insert_text(&mut self, text: &str) {
        self.replace_selection(text);
    }

    pub fn insert_newline_auto_indent(&mut self) {
        if self.selection_range().is_some() {
            self.delete_selection();
        }

        let mut offset = self.cursor_offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &self.content[..offset];
        let last_line = prefix.split('\n').last().unwrap_or("");
        let indent_len = last_line.chars().take_while(|c| c.is_whitespace()).count();
        let indent: String = last_line.chars().take(indent_len).collect();

        let insert_str = format!("\n{}", indent);
        let cursor_before = self.cursor_offset;
        let at = offset;
        let new_cursor = at + insert_str.len();

        self.content.replace_range(offset..offset, &insert_str);
        self.version += 1;
        self.refresh_line_index_incremental(at, "", &insert_str);
        self.cursor_offset = new_cursor;
        self.selection_anchor = None;
        self.mark_dirty();
        self.visual_cache_dirty = true;
        self.record_change(at, String::new(), insert_str, cursor_before, new_cursor);
    }

    pub fn delete_backwards(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.cursor_offset == 0 || self.content.is_empty() {
            return;
        }
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
        let cursor_before = self.cursor_offset;
        let removed = self.content[prev..offset].to_string();
        self.content.replace_range(prev..offset, "");
        self.version += 1;
        self.refresh_line_index_incremental(prev, &removed, "");
        self.cursor_offset = prev;
        self.selection_anchor = None;
        self.mark_dirty();
        self.visual_cache_dirty = true;
        self.record_change(prev, removed, String::new(), cursor_before, prev);
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
        let mut next = offset + 1;
        while next <= self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        let cursor_before = self.cursor_offset;
        let removed = self.content[offset..next].to_string();
        self.content.replace_range(offset..next, "");
        self.version += 1;
        self.refresh_line_index_incremental(offset, &removed, "");
        self.cursor_offset = offset;
        self.selection_anchor = None;
        self.mark_dirty();
        self.visual_cache_dirty = true;
        self.record_change(offset, removed, String::new(), cursor_before, offset);
    }

    pub fn move_cursor_left(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.start;
                self.selection_anchor = None;
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
    }

    pub fn move_cursor_right(&mut self, shift: bool) {
        if !shift && self.selection_anchor.is_some() {
            if let Some(range) = self.selection_range() {
                self.cursor_offset = range.end;
                self.selection_anchor = None;
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
                return;
            }
            offset += line.len() + 1;
        }
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
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_start(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset;
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    pub fn set_cursor_to_line_end(&mut self, target_line: usize) {
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                self.cursor_offset = offset + line.len();
                self.selection_anchor = None;
                return;
            }
            offset += line.len() + 1;
        }
    }

    #[allow(dead_code)]
    pub fn set_cursor_to_line_col(&mut self, target_line: usize, target_col: usize) {
        self.cursor_offset = self.offset_for_line_col(target_line, target_col);
        self.selection_anchor = None;
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
    }
}

/// Неизменяемый снимок состояния vault и активного файла для синхронных геттеров плагинов.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub version: u64,
    pub language: String,
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
    /// Multi-document workspace: document buffers keyed by DocumentId
    pub documents: HashMap<DocumentId, DocumentBuffer>,
    /// Map of normalized paths to DocumentId
    pub by_path: HashMap<PathBuf, DocumentId>,
    /// Multi-document workspace tree
    pub workspace: WorkspaceTree,
    /// Leaves state keyed by LeafId
    pub leaves: HashMap<LeafId, LeafState>,
    /// Editor view states keyed by TabId
    pub views: HashMap<TabId, EditorViewState>,
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
            documents: HashMap::new(),
            by_path: HashMap::new(),
            workspace: {
                let default_leaf_id = LeafId::new();
                WorkspaceTree::new_single_leaf(default_leaf_id)
            },
            leaves: HashMap::new(),
            views: HashMap::new(),
        };

        // Initialize active leaf in leaves map
        state.leaves.insert(
            state.workspace.active_leaf,
            LeafState::new(state.workspace.active_leaf),
        );

        // Determine workspace to open
        let mut ws_to_open = None;
        if let Some(ref ws) = state.session.workspace_path {
            if ws.exists() {
                ws_to_open = Some(ws.clone());
            }
        }

        if let Some(ws) = ws_to_open {
            state.open_workspace_without_saving(ws);
        }

        // Restore workspace v2 session if present
        let ws_v2_path = app_data_dir.join("workspace-v2.json");
        let mut restored_v2 = false;
        if let Ok(content) = std::fs::read_to_string(&ws_v2_path) {
            if let Ok(ws_v2) = crate::workspace::WorkspaceSessionV2::from_json(&content) {
                state.restore_workspace_session_v2(ws_v2);
                restored_v2 = true;
            }
        }

        if restored_v2 {
            if let Some(leaf) = state.leaves.get(&state.workspace.active_leaf) {
                state.open_tabs = leaf.tabs.iter().filter_map(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => {
                        state.documents.get(id).map(|d| d.path.clone())
                    }
                    crate::workspace::TabContent::Empty => Some(empty_tab_path()),
                    _ => None,
                }).collect();

                if let Some(tab) = leaf.active_tab() {
                    match &tab.content {
                        crate::workspace::TabContent::Document { id } => {
                            if let Some(doc) = state.documents.get_mut(id) {
                                let doc_path = doc.path.clone();
                                let doc_title = doc.title.clone();
                                let doc_dirty = doc.is_dirty;
                                let (c, li, cp, us, rs, fdt) = doc.take_for_active_note();
                                let line_index = if li.lines() > 0 && li.len() == c.len() {
                                    li
                                } else {
                                    crate::line_index::LineIndex::new(&c)
                                };
                                let mut opened = OpenedNote {
                                    path: doc_path,
                                    title: doc_title,
                                    content: c,
                                    version: 0,
                                    cursor_offset: 0,
                                    selection_anchor: None,
                                    is_dirty: doc_dirty,
                                    first_dirty_time: fdt,
                                    undo_stack: us,
                                    redo_stack: rs,
                                    pending_edits: Vec::new(),
                                    line_index,
                                    code_checkpoints: cp,
                                    window: crate::editor_window::EditorWindow::default(),
                                    above_lines: 0,
                                    below_lines: 0,
                                    above_h: 0.0,
                                    below_h: 0.0,
                                    est_rows_per_line: 1.0,
                                    est_row_h: 24.0,
                                    visual_cache_dirty: true,
                                    stats_cache: std::cell::Cell::new(None),
                                    view_shell_cursor_line_col: None,
                                    view_shell_selection_range: None,
                                    view_shell_window_start_byte: None,
                                    view_shell_sel_lines: None,
                                };
                                if let Some(v) = state.views.get(&tab.id) {
                                    opened.cursor_offset = v.cursor_offset.min(opened.content.len());
                                    opened.selection_anchor = v.selection_anchor.map(|a| a.min(opened.content.len()));
                                }
                                state.reveal_path_in_tree(&opened.path.clone());
                                state.active_note = Some(opened);
                                state.active_empty_tab = None;
                            }
                        }
                        crate::workspace::TabContent::Empty => {
                            state.active_note = None;
                            state.active_empty_tab = Some(empty_tab_path());
                        }
                        crate::workspace::TabContent::Plugin { .. } => {
                            state.active_note = None;
                            state.active_empty_tab = None;
                        }
                    }
                }
            }
        } else {
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
        }

        state.publish_snapshot();
        state
    }

    /// Creates a pristine, isolated AppState without any preloaded documents or tabs,
    /// suitable for unit tests and clean workspace resets.
    pub fn new_empty() -> Self {
        let mut state = Self::new();
        state.leaves.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.open_tabs.clear();
        state.active_note = None;
        state.active_empty_tab = None;
        let default_leaf_id = LeafId::new();
        state.workspace = WorkspaceTree::new_single_leaf(default_leaf_id);
        state.leaves.insert(default_leaf_id, LeafState::new(default_leaf_id));
        state
    }

    pub fn normalize_path(path: &Path) -> PathBuf {
        if let Ok(canon) = std::fs::canonicalize(path) {
            let s = canon.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                return PathBuf::from(stripped);
            }
            return canon;
        }
        let mut normalized = PathBuf::new();
        for comp in path.components() {
            normalized.push(comp.as_os_str());
        }
        normalized
    }

    pub fn find_document_by_path(&self, path: &Path) -> Option<DocumentId> {
        let norm = Self::normalize_path(path);
        self.by_path.get(&norm).copied().or_else(|| {
            self.documents.iter().find_map(|(id, doc)| {
                if doc.path == path {
                    Some(*id)
                } else {
                    None
                }
            })
        })
    }

    pub fn get_or_open_document(&mut self, path: &Path, title: String, content: String) -> DocumentId {
        let norm = Self::normalize_path(path);
        if let Some(&doc_id) = self.by_path.get(&norm) {
            return doc_id;
        }

        let doc_id = DocumentId::new();
        let buf = DocumentBuffer::new(doc_id, path.to_path_buf(), title, content, 1);
        self.documents.insert(doc_id, buf);
        self.by_path.insert(norm, doc_id);
        doc_id
    }

    /// Cleans up orphaned document buffers, path mappings, views, and active note
    /// after tabs have been closed, truncated, or removed.
    pub fn cleanup_orphaned_documents_and_views(&mut self) {
        // 1. Gather all active TabIds and DocumentIds across all leaves
        let mut active_tab_ids = HashSet::new();
        let mut active_doc_ids = HashSet::new();
        for leaf in self.leaves.values() {
            for tab in &leaf.tabs {
                active_tab_ids.insert(tab.id);
                if let crate::workspace::TabContent::Document { id } = &tab.content {
                    active_doc_ids.insert(*id);
                }
            }
        }

        // 2. Remove views for closed tabs
        self.views.retain(|tab_id, _| active_tab_ids.contains(tab_id));

        // 3. Remove documents no longer open in any leaf
        let mut removed_paths = Vec::new();
        self.documents.retain(|doc_id, doc| {
            if active_doc_ids.contains(doc_id) {
                true
            } else {
                removed_paths.push(doc.path.clone());
                false
            }
        });

        // 4. Clean up by_path, open_tabs, and active_note
        for p in &removed_paths {
            let norm = Self::normalize_path(p);
            self.by_path.remove(&norm);
            self.open_tabs.retain(|x| x != p);
            if self.active_note.as_ref().map(|n| &n.path == p).unwrap_or(false) {
                self.active_note = None;
            }
        }
    }

    /// Closes an entire leaf (split window) and unloads any orphaned documents and views.
    pub fn close_leaf(&mut self, leaf_id: LeafId) -> Option<LeafId> {
        if self.workspace.leaves().len() <= 1 {
            return None;
        }
        self.leaves.remove(&leaf_id);
        self.cleanup_orphaned_documents_and_views();
        let next = self.workspace.close_leaf(leaf_id);
        if let Some(na) = next {
            self.workspace.active_leaf = na;
        }
        next
    }

    /// Captures a point-in-time snapshot of document memory allocations and open counts.
    pub fn memory_snapshot(&self) -> MemorySnapshot {
        let doc_bytes: usize = self.documents.values().map(|d| d.content().len()).sum();
        let active_bytes: usize = self.active_note.as_ref().map(|n| n.content.len()).unwrap_or(0);
        MemorySnapshot {
            document_count: self.documents.len(),
            document_bytes: doc_bytes,
            by_path_count: self.by_path.len(),
            view_count: self.views.len(),
            active_note_bytes: active_bytes,
            open_tabs_count: self.open_tabs.len(),
        }
    }

    /// Centralized handling when a file or directory has been deleted on disk or by a plugin.
    ///
    /// Closes all matching tabs across all windows/leaves, clears document buffers,
    /// removes from `open_tabs`, `session.open_tabs`, `nav_history`, and unloads resources.
    /// If `keep_if_dirty` is true and the document has unsaved edits, it is kept open so the
    /// user has an opportunity to save or recover it.
    ///
    /// Returns `true` if any tabs/documents were closed.
    pub fn handle_file_deleted(&mut self, path: &Path, keep_if_dirty: bool) -> bool {
        let norm_target = Self::normalize_path(path);

        // Find all documents matching this path or starting with this folder
        let matching_doc_ids: Vec<DocumentId> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let norm_doc = Self::normalize_path(&doc.path);
                if doc.path == path
                    || norm_doc == norm_target
                    || doc.path.starts_with(path)
                    || norm_doc.starts_with(&norm_target)
                {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        let mut any_closed = false;
        let mut any_kept_dirty = false;

        for doc_id in matching_doc_ids {
            let doc_path = if let Some(doc) = self.documents.get(&doc_id) {
                doc.path.clone()
            } else {
                continue;
            };

            let is_doc_dirty = self.documents.get(&doc_id).map(|d| d.is_dirty).unwrap_or(false)
                || self.active_note.as_ref().map(|n| {
                    (n.path == doc_path || Self::normalize_path(&n.path) == Self::normalize_path(&doc_path))
                        && n.is_dirty
                }).unwrap_or(false);

            if keep_if_dirty && is_doc_dirty {
                self.status_text = format!(
                    "Файл удалён на диске, но сохранён в редакторе (несохранённые правки): {}",
                    doc_path.file_name().unwrap_or_default().to_string_lossy()
                );
                any_kept_dirty = true;
                continue;
            }

            // Close all tabs referencing this document in every leaf/window
            let mut empty_leaves = Vec::new();
            for (leaf_id, leaf) in self.leaves.iter_mut() {
                let mut i = 0;
                while i < leaf.tabs.len() {
                    let matches = match &leaf.tabs[i].content {
                        crate::workspace::TabContent::Document { id } => *id == doc_id,
                        _ => false,
                    };
                    if matches {
                        let tab = leaf.tabs.remove(i);
                        self.views.remove(&tab.id);
                        if leaf.active_tab_index >= leaf.tabs.len() && !leaf.tabs.is_empty() {
                            leaf.active_tab_index = leaf.tabs.len() - 1;
                        }
                    } else {
                        i += 1;
                    }
                }
                if leaf.tabs.is_empty() {
                    empty_leaves.push(*leaf_id);
                }
            }

            for leaf_id in empty_leaves {
                if self.workspace.leaves().len() > 1 {
                    self.workspace.close_leaf(leaf_id);
                    self.leaves.remove(&leaf_id);
                    if self.workspace.active_leaf == leaf_id {
                        if let Some(&first) = self.workspace.leaves().first() {
                            self.workspace.active_leaf = first;
                        }
                    }
                } else if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
                    leaf.tabs.push(crate::workspace::Tab::new_empty());
                    leaf.active_tab_index = 0;
                }
            }

            // Remove from document buffers and by_path mapping
            if let Some(doc) = self.documents.remove(&doc_id) {
                let norm = Self::normalize_path(&doc.path);
                self.by_path.remove(&norm);
                self.by_path.remove(&Self::normalize_path(path));
                self.open_tabs.retain(|p| p != &doc.path && p != path && p != &norm);
                self.session.open_tabs.retain(|p| p != &doc.path && p != path && p != &norm);
                self.nav_history.retain(|p| p != &doc.path && p != path && p != &norm);
                self.note_states.remove(&doc.path);
                self.note_states.remove(path);

                if self.session.active_note.as_ref().map(|p| p == &doc.path || p == path).unwrap_or(false) {
                    self.session.active_note = None;
                }
                if self.active_note.as_ref().map(|n| n.path == doc.path || n.path == path).unwrap_or(false) {
                    self.active_note = None;
                }
                any_closed = true;
            }
        }

        if !any_kept_dirty {
            // Also clean up any loose references in open_tabs/session if doc was not in documents map
            let norm_p = Self::normalize_path(path);
            self.open_tabs.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));
            self.session.open_tabs.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));
            self.nav_history.retain(|p| p != path && Self::normalize_path(p) != norm_p && !p.starts_with(path));

            if self.active_note.as_ref().map(|n| n.path == path || Self::normalize_path(&n.path) == norm_p).unwrap_or(false) {
                self.active_note = None;
                self.session.active_note = None;
                any_closed = true;
            }
        }

        self.cleanup_orphaned_documents_and_views();

        // If active_note is None, try activating whatever tab is in active_leaf
        if self.active_note.is_none() {
            let active_leaf_id = self.workspace.active_leaf;
            if let Some(leaf) = self.leaves.get(&active_leaf_id) {
                if let Some(tab) = leaf.active_tab() {
                    match &tab.content {
                        crate::workspace::TabContent::Document { id } => {
                            if let Some(doc) = self.documents.get(id) {
                                let opened = OpenedNote::new(
                                    doc.path.clone(),
                                    doc.title.clone(),
                                    doc.content().to_string(),
                                    doc.version,
                                );
                                self.active_note = Some(opened);
                                self.session.active_note = Some(doc.path.clone());
                            }
                        }
                        crate::workspace::TabContent::Empty => {
                            self.active_empty_tab = Some(empty_tab_path());
                            self.session.active_note = Some(empty_tab_path());
                        }
                        _ => {}
                    }
                }
            }
        }

        self.save_session_state();
        any_closed
    }

    /// Centralized handling when a file or directory has been renamed or moved on disk or by a plugin.
    ///
    /// Synchronously updates all matching documents, tabs across all leaves/windows,
    /// paths, titles, `open_tabs`, `session.open_tabs`, and `active_note`.
    pub fn handle_file_renamed(&mut self, old_path: &Path, new_path: &Path) -> bool {
        let norm_old = Self::normalize_path(old_path);
        let norm_new = Self::normalize_path(new_path);

        let matching_docs: Vec<(DocumentId, PathBuf)> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let norm_doc = Self::normalize_path(&doc.path);
                if doc.path == old_path || norm_doc == norm_old {
                    Some((*id, new_path.to_path_buf()))
                } else if let Ok(suffix) = doc.path.strip_prefix(old_path) {
                    Some((*id, new_path.join(suffix)))
                } else if let Ok(suffix) = norm_doc.strip_prefix(&norm_old) {
                    Some((*id, norm_new.join(suffix)))
                } else {
                    None
                }
            })
            .collect();

        let mut any_renamed = false;

        for (doc_id, target_new_path) in matching_docs {
            let new_title = target_new_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());

            let old_doc_path = if let Some(doc) = self.documents.get_mut(&doc_id) {
                let prev = std::mem::replace(&mut doc.path, target_new_path.clone());
                doc.title = new_title.clone();
                prev
            } else {
                continue;
            };

            // Update by_path index
            let norm_old_doc = Self::normalize_path(&old_doc_path);
            let norm_new_doc = Self::normalize_path(&target_new_path);
            self.by_path.remove(&norm_old_doc);
            self.by_path.insert(norm_new_doc, doc_id);


            // Update path lists
            for p in self.open_tabs.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }
            for p in self.session.open_tabs.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }
            for p in self.nav_history.iter_mut() {
                if *p == old_doc_path || Self::normalize_path(p) == norm_old_doc {
                    *p = target_new_path.clone();
                }
            }

            // Migrate note settings if present
            if let Some(settings) = self.note_states.remove(&old_doc_path) {
                self.note_states.insert(target_new_path.clone(), settings);
            }

            // Update active note if it pointed to the old path
            if let Some(ref mut note) = self.active_note {
                if note.path == old_doc_path || Self::normalize_path(&note.path) == norm_old_doc {
                    note.path = target_new_path.clone();
                    note.title = new_title.clone();
                }
            }
            if self.session.active_note.as_ref().map(|p| p == &old_doc_path || Self::normalize_path(p) == norm_old_doc).unwrap_or(false) {
                self.session.active_note = Some(target_new_path);
            }

            any_renamed = true;
        }

        // Also check if active_note or open_tabs match old_path directly even if not in documents
        if !any_renamed {
            for p in self.open_tabs.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
            for p in self.session.open_tabs.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
            for p in self.nav_history.iter_mut() {
                if *p == old_path || Self::normalize_path(p) == norm_old {
                    *p = new_path.to_path_buf();
                    any_renamed = true;
                }
            }
            if let Some(ref mut note) = self.active_note {
                if note.path == old_path || Self::normalize_path(&note.path) == norm_old {
                    note.path = new_path.to_path_buf();
                    note.title = new_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "untitled".to_string());
                    any_renamed = true;
                }
            }
        }

        self.save_session_state();
        any_renamed
    }

    /// Centralized handling when a file has been modified externally on disk.
    ///
    /// If the file is open and clean (not dirty), reloads content from disk.
    /// If the file has unsaved edits, keeps the user's edits and warns in status.
    pub fn handle_file_modified(&mut self, path: &Path) -> bool {
        let norm = Self::normalize_path(path);
        let opt_doc_id = self.find_document_by_path(path);

        if let Some(doc_id) = opt_doc_id {
            let is_dirty = self.documents.get(&doc_id).map(|d| d.is_dirty).unwrap_or(false)
                || self.active_note.as_ref().map(|n| {
                    (n.path == path || Self::normalize_path(&n.path) == norm) && n.is_dirty
                }).unwrap_or(false);

            if is_dirty {
                self.status_text = format!(
                    "Файл изменён внешним приложением (в редакторе есть несохранённые правки): {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                return false;
            }

            if let Ok(new_content) = std::fs::read_to_string(path) {
                if let Some(doc) = self.documents.get_mut(&doc_id) {
                    doc.reload(new_content.clone());
                }
                if let Some(ref mut note) = self.active_note {
                    if note.path == path || Self::normalize_path(&note.path) == norm {
                        note.content = new_content;
                        note.version += 1;
                        note.line_index = crate::line_index::LineIndex::new(&note.content);
                        note.rebuild_visual_cache(true, 80);
                    }
                }
                self.status_text = format!(
                    "Файл обновлён: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                return true;
            }
        }
        false
    }

    /// Checks for external filesystem changes on currently open documents.
    ///
    /// Handles atomic save debounce: if an open file is missing from disk, it is added
    /// to `pending_deletions`. Only if it remains missing after `debounce_duration` is it
    /// confirmed as deleted and returned in `FsChangeEvent::Deleted`. If it reappears
    /// within the debounce window, the pending deletion is cancelled.
    pub fn check_fs_changes(
        &mut self,
        pending_deletions: &mut HashMap<PathBuf, Instant>,
        debounce_duration: Duration,
    ) -> Vec<FsChangeEvent> {
        let mut events = Vec::new();
        let open_paths: Vec<PathBuf> = self.documents.values().map(|d| d.path.clone()).collect();

        for path in open_paths {
            if path.exists() {
                if pending_deletions.remove(&path).is_some() {
                    // Reappeared within debounce window: atomic save / rapid recreation!
                }
            } else {
                if let Some(first_seen) = pending_deletions.get(&path) {
                    if first_seen.elapsed() >= debounce_duration {
                        pending_deletions.remove(&path);
                        events.push(FsChangeEvent::Deleted(path));
                    }
                } else {
                    pending_deletions.insert(path, Instant::now());
                }
            }
        }

        // Clean up pending deletions for paths that are no longer open
        pending_deletions.retain(|p, _| self.find_document_by_path(p).is_some());

        events
    }

    pub fn close_leaf_tab(&mut self, leaf_id: LeafId, tab_index: usize) -> Option<PathBuf> {
        let (removed_tab, leaf_is_empty) = if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
            if tab_index < leaf.tabs.len() {
                let tab = leaf.tabs.remove(tab_index);
                self.views.remove(&tab.id);
                let empty = leaf.tabs.is_empty();
                if !empty && leaf.active_tab_index >= leaf.tabs.len() {
                    leaf.active_tab_index = leaf.tabs.len() - 1;
                }
                (Some(tab), empty)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };

        let Some(tab) = removed_tab else {
            return None;
        };

        let closed_doc_path = match &tab.content {
            crate::workspace::TabContent::Document { id } => {
                let doc_id = *id;
                let still_open = self.leaves.values().any(|leaf| {
                    leaf.tabs.iter().any(|t| match &t.content {
                        crate::workspace::TabContent::Document { id: other_id } => other_id == &doc_id,
                        _ => false,
                    })
                });

                if !still_open {
                    if let Some(doc) = self.documents.remove(&doc_id) {
                        let norm = Self::normalize_path(&doc.path);
                        self.by_path.remove(&norm);
                        self.open_tabs.retain(|p| p != &doc.path);
                        Some(doc.path)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            crate::workspace::TabContent::Empty => None,
            crate::workspace::TabContent::Plugin { .. } => None,
        };

        if leaf_is_empty {
            if self.workspace.leaves().len() > 1 {
                self.workspace.close_leaf(leaf_id);
                self.leaves.remove(&leaf_id);
                if self.workspace.active_leaf == leaf_id {
                    if let Some(&first) = self.workspace.leaves().first() {
                        self.workspace.active_leaf = first;
                    }
                }
            } else if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
                leaf.tabs.push(crate::workspace::Tab::new_empty());
                leaf.active_tab_index = 0;
            }
        }

        self.cleanup_orphaned_documents_and_views();

        closed_doc_path
    }

    pub fn open_workspace(&mut self, path: PathBuf) {
        self.open_workspace_internal(path, true);
    }

    pub fn open_workspace_without_saving(&mut self, path: PathBuf) {
        self.open_workspace_internal(path, false);
    }

    fn open_workspace_internal(&mut self, path: PathBuf, save: bool) {
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
        if save {
            self.save_session_state();
        }
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
        let language = crate::i18n::LocaleInfo::normalize(&self.settings.get_config().ui.language).to_string();
        self.snapshot.store(Arc::new(Snapshot {
            version,
            language,
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
        self.create_new_note_placed(false)
    }

    /// Same as [`create_new_note`], but the new tab is inserted first in the
    /// tab strip (used by the leading new-note button).
    pub fn create_new_note_at_front(&mut self) -> Option<PathBuf> {
        self.create_new_note_placed(true)
    }

    fn create_new_note_placed(&mut self, at_front: bool) -> Option<PathBuf> {
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
        if at_front {
            self.open_note_at_front(candidate.clone());
        } else {
            self.open_note(candidate.clone());
        }
        if let Some(ref mut note) = self.active_note {
            note.cursor_offset = note.content.len();
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
        let opt_doc_id = self.find_document_by_path(path);
        let is_empty = is_empty_tab_path(path) || path.to_string_lossy() == "newtab";
        let mut target = None;
        for (leaf_id, leaf) in &self.leaves {
            for (idx, tab) in leaf.tabs.iter().enumerate() {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if Some(*id) == opt_doc_id {
                            target = Some((*leaf_id, idx));
                            break;
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        if is_empty {
                            target = Some((*leaf_id, idx));
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if target.is_some() {
                break;
            }
        }
        if let Some((leaf_id, tab_idx)) = target {
            self.close_leaf_tab(leaf_id, tab_idx);
        }

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

        if let Some(doc_id) = opt_doc_id {
            let still_open = self.leaves.values().any(|leaf| {
                leaf.tabs.iter().any(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => id == &doc_id,
                    _ => false,
                })
            });
            if !still_open {
                if let Some(doc) = self.documents.remove(&doc_id) {
                    let norm = Self::normalize_path(&doc.path);
                    self.by_path.remove(&norm);
                }
            }
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

    /// Same as [`open_note`], but a new tab is inserted at the front of the
    /// tab strip instead of appended at the end.
    pub fn open_note_at_front(&mut self, path: PathBuf) {
        self.push_nav_history(path.clone());
        self.open_note_internal_at(path, true);
    }

    pub fn open_note_internal(&mut self, path: PathBuf) {
        self.open_note_internal_at(path, false);
    }

    pub fn open_note_internal_at(&mut self, path: PathBuf, at_front: bool) {
        // Empty-tab sentinels carry no file: just activate the New Tab page.
        if is_empty_tab_path(&path) {
            self.activate_empty_tab(path);
            return;
        }
        self.reveal_path_in_tree(&path);

        // Save current note's cursor, selection, and stash its state into DocumentBuffer before switching
        if let Some(mut current) = self.active_note.take() {
            let entry = self.note_states.entry(current.path.clone()).or_default();
            entry.cursor_offset = current.cursor_offset;
            entry.selection_anchor = current.selection_anchor;
            self.session.cursor_positions.insert(current.path.clone(), current.cursor_offset);
            self.session.note_states.insert(current.path.clone(), entry.clone());

            if let Some(doc_id) = self.find_document_by_path(&current.path) {
                if let Some(doc) = self.documents.get_mut(&doc_id) {
                    doc.stash_from_note(
                        std::mem::take(&mut current.content),
                        std::mem::take(&mut current.line_index),
                        std::mem::take(&mut current.code_checkpoints),
                        std::mem::take(&mut current.undo_stack),
                        std::mem::take(&mut current.redo_stack),
                        current.is_dirty,
                        current.first_dirty_time,
                    );
                }
            }
        }

        if !self.open_tabs.iter().any(|p| p == &path) {
            if at_front {
                self.open_tabs.insert(0, path.clone());
            } else {
                self.open_tabs.push(path.clone());
            }
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
        } else if let Some(doc_id) = self.find_document_by_path(&path) {
            // Already open in memory (e.g. in another leaf/window or previously loaded):
            // Take the buffer directly into OpenedNote to avoid duplicate memory and preserve unsaved edits.
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                let (content, line_index, code_checkpoints, undo_stack, redo_stack, first_dirty_time) = doc.take_for_active_note();
                let mut content = content;
                let mut line_index = line_index;
                if content.is_empty() {
                    if let Ok(disk_content) = std::fs::read_to_string(&path) {
                        line_index = crate::line_index::LineIndex::new(&disk_content);
                        content = disk_content;
                    }
                } else if line_index.lines() == 0 || line_index.len() != content.len() {
                    line_index = crate::line_index::LineIndex::new(&content);
                }
                let mut note = OpenedNote::new(
                    path.clone(),
                    doc.title.clone(),
                    content,
                    doc.version,
                );
                note.line_index = line_index;
                note.code_checkpoints = code_checkpoints;
                note.undo_stack = undo_stack;
                note.redo_stack = redo_stack;
                note.is_dirty = doc.is_dirty;
                note.first_dirty_time = first_dirty_time;
                opened_note = Some(note);
                self.status_text = format!("Switched: {}", path.file_name().unwrap_or_default().to_string_lossy());
            }
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
            let doc_id = self.get_or_open_document(&note.path, note.title.clone(), String::new());
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                doc.line_index = note.line_index.clone();
            }
            self.workspace.open_file(&mut self.leaves, doc_id, self.workspace.active_leaf, crate::workspace::OpenMode::Reuse);
            if at_front {
                if let Some(leaf) = self.leaves.get_mut(&self.workspace.active_leaf) {
                    if leaf.tabs.len() > 1 && leaf.active_tab_index != 0 {
                        let tab = leaf.tabs.remove(leaf.active_tab_index);
                        leaf.tabs.insert(0, tab);
                        leaf.active_tab_index = 0;
                    }
                }
            }
            if let Some(leaf) = self.leaves.get(&self.workspace.active_leaf) {
                if let Some(tab) = leaf.active_tab() {
                    self.views.entry(tab.id).or_insert_with(|| crate::workspace::EditorViewState::new(tab.id, doc_id));
                }
            }

            // Restore saved settings (cursor and selection) for this note
            if let Some(settings) = self.note_states.get(&path) {
                let mut offset = settings.cursor_offset.min(note.content.len());
                while offset > 0 && !note.content.is_char_boundary(offset) {
                    offset -= 1;
                }
                note.cursor_offset = offset;

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

    pub fn save_document(&mut self, doc_id: DocumentId) -> bool {
        if let Some(doc) = self.documents.get_mut(&doc_id) {
            if !doc.is_dirty {
                return true;
            }
            if let Err(err) = crate::save_manager::safe_write(&doc.path, doc.content()) {
                self.status_text = format!("Save error: {}", err);
                return false;
            } else {
                doc.mark_clean();
                self.status_text = format!("Saved: {}", doc.title);
                let rel_path = self.vault.as_ref()
                    .and_then(|v| doc.path.strip_prefix(&v.root).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|| doc.path.to_string_lossy().into_owned());
                self.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
                self.publish_snapshot();
                return true;
            }
        }
        false
    }

    pub fn save_dirty_documents(&mut self) {
        let dirty_ids: Vec<DocumentId> = self
            .documents
            .iter()
            .filter(|(_, doc)| doc.is_dirty)
            .map(|(id, _)| *id)
            .collect();

        for id in dirty_ids {
            self.save_document(id);
        }

        self.save_session_state();
    }

    pub fn save_active_note(&mut self) {
        let opt_doc_id = self.active_note.as_ref().and_then(|n| self.find_document_by_path(&n.path));
        if let Some(ref mut note) = self.active_note {
            if let Err(err) = crate::save_manager::safe_write(&note.path, &note.content) {
                self.status_text = format!("Save error: {}", err);
            } else {
                note.clear_dirty();
                self.status_text = format!("Saved: {}", note.title);

                if let Some(doc_id) = opt_doc_id {
                    if let Some(doc) = self.documents.get_mut(&doc_id) {
                        doc.mark_clean();
                    }
                }

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

    pub fn export_workspace_session_v2(&self) -> crate::workspace::WorkspaceSessionV2 {
        fn node_to_persisted(
            node: &crate::workspace::WorkspaceNode,
            leaves: &HashMap<LeafId, LeafState>,
            documents: &HashMap<DocumentId, crate::document_buffer::DocumentBuffer>,
            views: &HashMap<crate::workspace::TabId, crate::workspace::EditorViewState>,
        ) -> crate::workspace::PersistedNode {
            match node {
                crate::workspace::WorkspaceNode::Split { direction, children, ratios } => {
                    crate::workspace::PersistedNode::Split {
                        direction: *direction,
                        children: children.iter().map(|c| node_to_persisted(c, leaves, documents, views)).collect(),
                        ratios: ratios.clone(),
                    }
                }
                crate::workspace::WorkspaceNode::Leaf { id } => {
                    let (active_tab_index, tabs) = if let Some(leaf) = leaves.get(id) {
                        let persisted_tabs = leaf.tabs.iter().map(|t| {
                            let (kind, path) = match &t.content {
                                crate::workspace::TabContent::Document { id } => {
                                    let p = documents.get(id).map(|d| d.path.to_string_lossy().to_string());
                                    ("document".to_string(), p)
                                }
                                crate::workspace::TabContent::Empty => ("empty".to_string(), None),
                                crate::workspace::TabContent::Plugin { view_id } => ("plugin".to_string(), Some(view_id.clone())),
                            };
                            let view = views.get(&t.id).map(|v| crate::workspace::PersistedView {
                                anchor_line: v.viewport.offset,
                                px_in_line: v.viewport.px_in_line,
                                cursor_line: 0,
                                cursor_col: 0,
                                scroll_top: v.scroll_top,
                                cursor_offset: v.cursor_offset,
                            });
                            crate::workspace::PersistedTab {
                                id: t.id,
                                kind,
                                path,
                                pinned: t.pinned,
                                external: None,
                                view,
                            }
                        }).collect();
                        (leaf.active_tab_index, persisted_tabs)
                    } else {
                        (0, Vec::new())
                    };

                    crate::workspace::PersistedNode::Leaf {
                        id: *id,
                        active_tab_index,
                        tabs,
                    }
                }
            }
        }

        let layout = node_to_persisted(&self.workspace.root, &self.leaves, &self.documents, &self.views);
        let mut session = crate::workspace::WorkspaceSessionV2 {
            version: crate::workspace::WORKSPACE_SCHEMA_VERSION,
            active_leaf: self.workspace.active_leaf,
            layout,
        };
        session.validate();
        session
    }

    pub fn restore_workspace_session_v2(&mut self, session: crate::workspace::WorkspaceSessionV2) {
        fn persisted_to_node(
            persisted: crate::workspace::PersistedNode,
            leaves: &mut HashMap<LeafId, LeafState>,
            views: &mut HashMap<crate::workspace::TabId, crate::workspace::EditorViewState>,
            state: &mut AppState,
        ) -> crate::workspace::WorkspaceNode {
            match persisted {
                crate::workspace::PersistedNode::Split { direction, children, ratios } => {
                    crate::workspace::WorkspaceNode::Split {
                        direction,
                        children: children.into_iter().map(|c| persisted_to_node(c, leaves, views, state)).collect(),
                        ratios,
                    }
                }
                crate::workspace::PersistedNode::Leaf { id, active_tab_index, tabs } => {
                    let mut leaf_state = LeafState::new(id);
                    leaf_state.active_tab_index = active_tab_index;
                    for p_tab in tabs {
                        let content = match p_tab.kind.as_str() {
                            "document" => {
                                if let Some(path_str) = p_tab.path {
                                    let path = PathBuf::from(path_str);
                                    let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".to_string());
                                    let norm = AppState::normalize_path(&path);
                                    let doc_id = if let Some(&id) = state.by_path.get(&norm) {
                                        id
                                    } else {
                                        let text = std::fs::read_to_string(&path).unwrap_or_default();
                                        state.get_or_open_document(&path, title, text)
                                    };
                                    if let Some(pv) = p_tab.view {
                                        let mut ev = crate::workspace::EditorViewState::new(p_tab.id, doc_id);
                                        ev.viewport.offset = pv.anchor_line;
                                        ev.viewport.px_in_line = pv.px_in_line;
                                        ev.cursor_offset = pv.cursor_offset;
                                        ev.scroll_top = pv.scroll_top;
                                        views.insert(p_tab.id, ev);
                                    } else {
                                        views.insert(p_tab.id, crate::workspace::EditorViewState::new(p_tab.id, doc_id));
                                    }
                                    crate::workspace::TabContent::Document { id: doc_id }
                                } else {
                                    crate::workspace::TabContent::Empty
                                }
                            }
                            "plugin" => {
                                crate::workspace::TabContent::Plugin { view_id: p_tab.path.unwrap_or_default() }
                            }
                            _ => crate::workspace::TabContent::Empty,
                        };

                        leaf_state.tabs.push(crate::workspace::Tab {
                            id: p_tab.id,
                            content,
                            pinned: p_tab.pinned,
                        });
                    }

                    if leaf_state.tabs.is_empty() {
                        leaf_state.tabs.push(crate::workspace::Tab::new_empty());
                    }
                    if leaf_state.active_tab_index >= leaf_state.tabs.len() {
                        leaf_state.active_tab_index = leaf_state.tabs.len() - 1;
                    }

                    leaves.insert(id, leaf_state);
                    crate::workspace::WorkspaceNode::Leaf { id }
                }
            }
        }

        let mut new_leaves = HashMap::new();
        let mut new_views = HashMap::new();
        let root = persisted_to_node(session.layout, &mut new_leaves, &mut new_views, self);

        self.leaves = new_leaves;
        self.views = new_views;
        self.workspace.root = root;
        self.workspace.root.flatten_same_direction_splits();
        self.workspace.root.normalize_ratios();
        self.workspace.active_leaf = session.active_leaf;
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

        let ws_v2 = self.export_workspace_session_v2();
        let ws_v2_path = dir.join("workspace-v2.json");
        let _ = ws_v2.save_atomic(&ws_v2_path);
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

        // Update multi-document workspace state
        if let Some(doc_id) = self.by_path.remove(old_path) {
            self.by_path.insert(new_path.clone(), doc_id);
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                doc.path = new_path.clone();
                doc.title = trimmed.to_string();
            }
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
        note.ensure_visual_cache(true, 80, 0);
        assert_eq!(note.window.lines.len(), 5001); // 5000 lines + trailing empty
        assert_eq!(note.line_index.lines(), 5001);
        let initial_lines = Arc::clone(&note.window.lines);

        let t0 = std::time::Instant::now();
        const MOVES: usize = 1000;
        for _ in 0..MOVES {
            note.move_cursor_right(false);
            note.ensure_visual_cache(true, 80, 0);
        }
        for _ in 0..MOVES {
            note.move_cursor_left(false);
            note.ensure_visual_cache(true, 80, 0);
        }
        let elapsed = t0.elapsed();
        let per_move_us = elapsed.as_micros() as f64 / (MOVES * 2) as f64;

        println!(
            "2000 cursor moves on 5,000 lines: total {:?}, {:.2} µs/move",
            elapsed, per_move_us
        );

        // CRITICAL REQUIREMENTS:
        // 1. Cached lines Arc must NOT have been reallocated or rebuilt during cursor movements:
        assert!(Arc::ptr_eq(&note.window.lines, &initial_lines), "Cache must not be rebuilt during cursor moves");
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

        note.ensure_visual_cache(true, 80, 0);
        assert!(!note.visual_cache_dirty);

        // Move cursor to inside paragraph: line 2 ("Some paragraph text here.")
        // "# Title\n\n" is 9 bytes. "Some " is 5 bytes. Offset 14 is right after "Some ".
        note.cursor_offset = 14;

        // Insert text incrementally
        note.insert_text("awesome ");
        assert_eq!(note.content, "# Title\n\nSome awesome paragraph text here.\n");
        assert!(note.visual_cache_dirty, "Cache must be marked dirty after text insertion");

        // Rebuilding cache should reflect the new content
        note.ensure_visual_cache(true, 80, 0);
        assert!(!note.visual_cache_dirty);
        assert_eq!(note.window.lines[2], "Some awesome paragraph text here.");

        // Undo
        note.undo();
        assert_eq!(note.content, "# Title\n\nSome paragraph text here.\n");
        assert!(note.visual_cache_dirty);

        note.ensure_visual_cache(true, 80, 0);
        assert_eq!(note.window.lines[2], "Some paragraph text here.");

        // Redo
        note.redo();
        assert_eq!(note.content, "# Title\n\nSome awesome paragraph text here.\n");
    }

    #[test]
    fn test_delta_undo_stack_memory_and_sequential_edits() {
        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            "Initial line.\n".to_string(),
            0,
        );

        // 1. Sequential typing: "Hello " letter by letter
        for ch in ["H", "e", "l", "l", "o", " "] {
            note.insert_text(ch);
        }
        assert_eq!(note.content, "Hello Initial line.\n");
        // Merging sequential keystrokes: undo_stack should only have 1 step!
        assert_eq!(note.undo_stack.len(), 1);
        assert_eq!(note.undo_stack[0].changes[0].inserted, "Hello ");

        // 2. Undo the whole typed word
        note.undo();
        assert_eq!(note.content, "Initial line.\n");
        assert_eq!(note.cursor_offset, 0);

        // 3. Redo
        note.redo();
        assert_eq!(note.content, "Hello Initial line.\n");
        assert_eq!(note.cursor_offset, 6);

        // 4. Backspace 3 times
        note.delete_backwards();
        note.delete_backwards();
        note.delete_backwards();
        assert_eq!(note.content, "HelInitial line.\n");
        assert_eq!(note.cursor_offset, 3);
        // Sequential backspaces should be merged into 1 step
        assert_eq!(note.undo_stack.len(), 2);

        // 5. Undo backspaces
        note.undo();
        assert_eq!(note.content, "Hello Initial line.\n");
        assert_eq!(note.cursor_offset, 6);

        // 6. Selection replace
        note.cursor_offset = 6;
        note.selection_anchor = Some(0);
        note.replace_selection("Welcome to ");
        assert_eq!(note.content, "Welcome to Initial line.\n");

        note.undo();
        assert_eq!(note.content, "Hello Initial line.\n");
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

        note.ensure_visual_cache(true, 40, 0); // wrap at 40 cols
        let visual_rows = Arc::clone(&note.window.visual_rows);

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

        note.ensure_visual_cache(true, 40, 0); // wrap at 40 cols
        let visual_rows = Arc::clone(&note.window.visual_rows);

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

    /// A note far larger than one window must not make the scrollbar jump as
    /// the window travels.
    ///
    /// `total_height` mixes an exact window with an O(1) estimate on either
    /// side. The estimate basis (rows per line, row height) is frozen for a
    /// given wrap setting, so the total depends on the *number* of lines
    /// outside the window, not on which of them the window happens to hold.
    /// Checked here on a document whose lines genuinely differ in height.
    #[test]
    fn total_height_is_stable_while_the_window_travels() {
        let mut text = String::new();
        for i in 0..60_000usize {
            match (i / 500) % 3 {
                0 => text.push_str(&format!("short line {i}\n")),
                1 => text.push_str(&format!(
                    "a rather long paragraph line number {i} that certainly wraps \
                     once the editor is only eighty columns wide\n"
                )),
                _ => text.push_str(&format!("```\nfence body {i}\n```\n")),
            }
        }
        let mut note = OpenedNote::new(PathBuf::from("stable.md"), "stable".to_string(), text, 1);

        // The first build freezes the estimate basis; it sits at the top.
        note.ensure_visual_cache(true, 80, 0);
        assert_eq!(note.above_lines, 0, "the first window sits at the top");
        assert!(note.below_lines > 0, "the note must be windowed");
        let first = note.total_height();
        assert!(first > 0.0);

        let mut worst = 0.0f64;
        for anchor in (0..note.line_index.lines()).step_by(997) {
            note.ensure_visual_cache(true, 80, anchor);
            assert!(
                note.window.covers_line(anchor),
                "the window must hold its anchor {anchor}"
            );
            let total = note.total_height();
            worst = worst.max((total - first).abs() / first);
        }
        assert!(
            worst < 0.01,
            "document height drifted by {:.3}% across window moves",
            worst * 100.0
        );
    }

    /// The caret has to resolve on any line the window holds.
    ///
    /// `find_visual_row_for_cursor` binary-searches the window's rows, so every
    /// line inside the window must be present and must name itself. A line
    /// outside it clamps to the nearest edge instead of panicking, and the
    /// anchor built from that line re-places the window on it.
    #[test]
    fn cursor_resolves_on_every_line_of_the_window() {
        let mut text = String::new();
        for i in 0..20_000usize {
            if i % 7 == 0 {
                text.push_str(&format!(
                    "long wrapped line {i} with enough words to spill past the \
                     eighty column boundary several times over\n"
                ));
            } else {
                text.push_str(&format!("line {i}\n"));
            }
        }
        let mut note = OpenedNote::new(PathBuf::from("caret.md"), "caret".to_string(), text, 1);
        note.ensure_visual_cache(true, 80, 12_000);

        let rows = Arc::clone(&note.window.visual_rows);
        let start = note.window.start_line;
        let end = note.window.end_line;
        assert!(start > 0 && end < note.line_index.lines(), "must be windowed");

        for line in (start..end).step_by(13) {
            let (v_idx, _) = crate::markdown::find_visual_row_for_cursor(&rows, line, 0)
                .unwrap_or_else(|| panic!("line {line} has no row"));
            assert_eq!(rows[v_idx].line_idx, line, "row {v_idx} names the wrong line");
        }

        // A line above the window clamps to its first row rather than failing.
        let outside = 500usize;
        assert!(outside < start);
        let (v_idx, _) =
            crate::markdown::find_visual_row_for_cursor(&rows, outside, 0).expect("clamped row");
        assert_eq!(v_idx, 0, "a line above the window clamps to its first row");

        note.ensure_visual_cache(true, 80, outside);
        assert!(note.window.covers_line(outside));
        let rows = Arc::clone(&note.window.visual_rows);
        let (v_idx, _) = crate::markdown::find_visual_row_for_cursor(&rows, outside, 0)
            .expect("row after re-placing the window");
        assert_eq!(rows[v_idx].line_idx, outside);
    }

    /// Stage-5 readiness check: a code block that opened far above the window
    /// must still be recognised inside it.
    ///
    /// Before the checkpoints, the window's fence scan started from `false`,
    /// so the body of a block that began before the window was painted as plain
    /// text. Here the block opens at line 1 and the window sits 100k lines
    /// below it.
    #[test]
    fn a_window_in_the_middle_of_a_code_block_knows_it_is_inside_one() {
        use crate::markdown::CodeBlockKind;

        let mut text = String::from("intro\n```rust\n");
        for i in 0..120_000usize {
            text.push_str(&format!("let v{i} = {i};\n"));
        }
        text.push_str("```\ndone\n");
        let mut note = OpenedNote::new(PathBuf::from("code.md"), "code".to_string(), text, 1);

        note.ensure_visual_cache(true, 0, 100_000);
        assert!(note.above_lines > 0, "the note must be windowed");

        let start = note.window.start_line;
        assert!(start > 2, "the window must start below the opening fence");
        assert!(
            note.code_checkpoints.scanned_lines() > start,
            "the checkpoints must reach the window's first line"
        );

        // The state the window reports for its own first line is the checkpoint
        // state, not the default of a fresh scan.
        let state = note
            .window
            .code_state(start)
            .expect("the window's first line must be inside the code block");
        assert_eq!(state.kind, CodeBlockKind::InsideCode);
        assert_eq!(state.block_id, 0);

        // And so is the line of the first row actually painted.
        let first_line = note.window.visual_rows.first().unwrap().line_idx;
        assert_eq!(
            note.window.code_state(first_line).map(|s| s.kind),
            Some(CodeBlockKind::InsideCode)
        );
    }

    /// A keystroke must not throw away the whole checkpoint table: the prefix
    /// above the edit is still valid, and only the tail is rescanned.
    #[test]
    fn an_edit_keeps_the_code_checkpoints_above_it() {
        use crate::markdown::CodeBlockKind;

        let mut text = String::from("```\n");
        for i in 0..20_000usize {
            text.push_str(&format!("body {i}\n"));
        }
        text.push_str("```\n");
        let mut note = OpenedNote::new(PathBuf::from("cp.md"), "cp".to_string(), text, 1);
        note.ensure_visual_cache(true, 0, 15_000);
        let start = note.window.start_line;
        let scanned = note.code_checkpoints.scanned_lines();
        assert!(
            scanned >= start + 1,
            "the scan must reach the window's first line"
        );

        // Type a character deep inside the block.
        let edit_line = 10_000usize;
        note.cursor_offset = note.line_start_byte(edit_line) + 3;
        note.insert_text("X");

        let kept =
            (edit_line / crate::code_scan::CHECKPOINT_LINES) * crate::code_scan::CHECKPOINT_LINES;
        assert_eq!(
            note.code_checkpoints.scanned_lines(),
            kept,
            "the checkpoints must be truncated exactly at the edit's checkpoint"
        );
        assert!(kept > 0 && kept < scanned, "the prefix must survive");

        // And the window still resolves its code state after the edit.
        note.ensure_visual_cache(true, 0, 15_000);
        let start = note.window.start_line;
        assert_eq!(
            note.window.code_state(start).map(|s| s.kind),
            Some(CodeBlockKind::InsideCode)
        );
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
        note.ensure_visual_cache(true, 80, 0);

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
        new_note.ensure_visual_cache(true, 80, 0);
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
            documents: HashMap::new(),
            by_path: HashMap::new(),
            workspace: {
                let default_leaf_id = LeafId::new();
                WorkspaceTree::new_single_leaf(default_leaf_id)
            },
            leaves: HashMap::new(),
            views: HashMap::new(),
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

    #[test]
    fn text_stats_exact_for_small_and_cached() {
        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            String::new(),
            1,
        );
        note.content = "hello  world\nтест".to_string();
        note.version += 1;
        note.refresh_line_index(None);
        assert_eq!(note.text_stats(), TextStats::Exact { words: 3, chars: 17 });

        // The cache must return the same value without recomputing.
        let first = note.stats_cache.get();
        let _ = note.text_stats();
        assert_eq!(note.stats_cache.get(), first);

        // Editing the text invalidates the cache.
        note.content.push_str(" ещё");
        note.version += 1;
        note.refresh_line_index(None);
        assert_eq!(note.text_stats(), TextStats::Exact { words: 4, chars: 21 });
    }

    #[test]
    fn text_stats_large_skips_exact_count() {
        let mut note = OpenedNote::new(
            PathBuf::from("test.md"),
            "test".to_string(),
            String::new(),
            1,
        );
        note.content = "a ".repeat(TEXT_STATS_EXACT_LIMIT); // 4 MB > limit
        note.version += 1;
        note.refresh_line_index(None);
        assert!(matches!(note.text_stats(), TextStats::Large { .. }));
    }

    #[test]
    fn test_doc_y_of_line_monotonic_and_covers_entire_document() {
        let mut text = String::new();
        for i in 0..40_000 {
            text.push_str(&format!("line {i}\n"));
        }
        let mut note = OpenedNote::new(
            PathBuf::from("big.md"),
            "big".to_string(),
            text,
            1,
        );
        note.ensure_visual_cache(false, 0, 20_000);

        let total_lines = note.line_index.lines();
        assert_eq!(total_lines, 40_001);

        let y_start = note.doc_y_of_line(0);
        let y_before = note.doc_y_of_line(100);
        let y_window_start = note.doc_y_of_line(note.window.start_line);
        let y_middle = note.doc_y_of_line(20_000);
        let y_window_end = note.doc_y_of_line(note.window.end_line);
        let y_after = note.doc_y_of_line(35_000);
        let y_end = note.doc_y_of_line(40_000);

        assert_eq!(y_start, 0.0);
        assert!(y_before > y_start);
        assert!(y_window_start >= y_before);
        assert!(y_middle > y_window_start);
        assert!(y_window_end > y_middle);
        assert!(y_after > y_window_end);
        assert!(y_end > y_after);
        assert!(y_end <= note.total_height());
    }

    #[test]
    fn test_typing_in_large_file_does_not_shift_window_or_above_h() {
        let mut text = String::new();
        for i in 0..50_000 {
            text.push_str(&format!("line {i}\n"));
        }
        let mut note = OpenedNote::new(
            PathBuf::from("big.md"),
            "big".to_string(),
            text,
            1,
        );
        let initial_anchor = 25_000;
        note.ensure_visual_cache(false, 0, initial_anchor);

        let initial_start = note.window.start_line;
        let initial_above_lines = note.above_lines;
        let initial_above_h = note.above_h;

        // Position cursor at line 25,100
        let offset = note.offset_for_line_col(25_100, 4);
        note.cursor_offset = offset;

        // Type characters repeatedly with slight anchor perturbations within slack
        for i in 0..20 {
            note.insert_text("a");
            let perturbed_anchor = initial_anchor + (i % 5);
            note.ensure_visual_cache(false, 0, perturbed_anchor);

            assert_eq!(
                note.window.start_line, initial_start,
                "window start_line must remain rock-solid during typing"
            );
            assert_eq!(
                note.above_lines, initial_above_lines,
                "above_lines must remain rock-solid during typing"
            );
            assert_eq!(
                note.above_h, initial_above_h,
                "above_h must remain rock-solid during typing"
            );
        }
    }

    #[test]
    fn test_workspace_session_v2_export_and_restore_roundtrip() {
        let mut state = AppState::new();
        state.documents.clear();
        state.by_path.clear();
        state.leaves.clear();
        state.views.clear();
        let default_leaf_id = LeafId::new();
        state.workspace = crate::workspace::WorkspaceTree::new_single_leaf(default_leaf_id);
        state.leaves.insert(default_leaf_id, LeafState::new(default_leaf_id));
        let leaf1 = state.workspace.active_leaf;
        let doc1 = state.get_or_open_document(Path::new("test1.md"), "test1".to_string(), "hello world".to_string());
        let doc2 = state.get_or_open_document(Path::new("test2.md"), "test2".to_string(), "second doc".to_string());

        state.workspace.open_file(&mut state.leaves, doc1, leaf1, crate::workspace::OpenMode::Reuse);
        let leaf2 = LeafId::new();
        state.leaves.insert(leaf2, LeafState::new(leaf2));
        assert!(state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2));
        state.workspace.active_leaf = leaf2;
        state.workspace.open_file(&mut state.leaves, doc2, leaf2, crate::workspace::OpenMode::Reuse);

        // Add view state with anchor and scroll/cursor
        let tab1_id = state.leaves.get(&leaf1).unwrap().tabs[0].id;
        let mut v1 = crate::workspace::EditorViewState::new(tab1_id, doc1);
        v1.viewport.offset = 42;
        v1.viewport.px_in_line = 7.5;
        v1.scroll_top = 120.0;
        v1.cursor_offset = 5;
        state.views.insert(tab1_id, v1);

        // Add empty tab to leaf2
        state.leaves.get_mut(&leaf2).unwrap().tabs.push(crate::workspace::Tab::new_empty());

        let exported = state.export_workspace_session_v2();
        assert_eq!(exported.active_leaf, leaf2);

        let mut restored_state = AppState::new();
        restored_state.restore_workspace_session_v2(exported);

        assert_eq!(restored_state.workspace.active_leaf, leaf2);
        assert_eq!(restored_state.workspace.leaves().len(), 2);
        assert!(restored_state.leaves.contains_key(&leaf1));
        assert!(restored_state.leaves.contains_key(&leaf2));

        let restored_v1 = restored_state.views.get(&tab1_id).unwrap();
        assert_eq!(restored_v1.viewport.offset, 42);
        assert_eq!(restored_v1.viewport.px_in_line, 7.5);
        assert_eq!(restored_v1.scroll_top, 120.0);
        assert_eq!(restored_v1.cursor_offset, 5);

        let restored_leaf2 = restored_state.leaves.get(&leaf2).unwrap();
        assert_eq!(restored_leaf2.tabs.len(), 2);
        assert!(matches!(restored_leaf2.tabs[1].content, crate::workspace::TabContent::Empty));
    }

    #[test]
    fn test_open_note_at_front_places_tab_at_index_0() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_front_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note1 = ws_path.join("Note1.md");
        let note2 = ws_path.join("Note2.md");
        std::fs::write(&note1, "content 1").unwrap();
        std::fs::write(&note2, "content 2").unwrap();

        let mut state = AppState::new();
        state.workspace_path = Some(ws_path.clone());
        state.open_note(note1.clone());

        let active_leaf = state.workspace.active_leaf;
        let len_after_note1 = state.leaves.get(&active_leaf).unwrap().tabs.len();

        // Open note2 at front
        state.open_note_at_front(note2.clone());

        let leaf = state.leaves.get(&active_leaf).unwrap();
        assert_eq!(leaf.tabs.len(), len_after_note1 + 1);
        assert_eq!(leaf.active_tab_index, 0);

        let doc_id = match leaf.tabs[0].content {
            crate::workspace::TabContent::Document { id } => id,
            _ => panic!("Expected Document"),
        };
        let doc = state.documents.get(&doc_id).unwrap();
        assert_eq!(doc.path, note2);
        assert_eq!(state.open_tabs[0], note2);

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_open_and_close_tab_unloads_memory_and_avoids_duplication() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_mem_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("TestNote.md");
        std::fs::write(&note_path, "sample text content").unwrap();

        let mut state = AppState::new();
        state.workspace_path = Some(ws_path.clone());
        state.open_note(note_path.clone());

        // 1. Text is owned by active_note, not duplicated in DocumentBuffer.content
        assert!(state.active_note.is_some());
        assert_eq!(state.active_note.as_ref().unwrap().content, "sample text content");
        let doc_id = state.find_document_by_path(&note_path).unwrap();
        let doc = state.documents.get(&doc_id).unwrap();
        assert!(doc.content().is_empty());
        assert_eq!(doc.line_index.lines(), 1);

        // 2. Closing the tab unloads DocumentBuffer from documents and by_path
        let leaf_id = state.workspace.active_leaf;
        let active_tab_idx = state.leaves.get(&leaf_id).unwrap().active_tab_index;
        let closed_path = state.close_leaf_tab(leaf_id, active_tab_idx);
        assert_eq!(closed_path, Some(note_path.clone()));

        // DocumentBuffer is completely removed from memory
        assert!(!state.documents.contains_key(&doc_id));
        assert!(state.find_document_by_path(&note_path).is_none());
        assert!(!state.open_tabs.contains(&note_path));

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_tab_switch_preserves_checkpoints_and_zero_duplicate_memory() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_switch_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note1_path = ws_path.join("Note1.md");
        let note2_path = ws_path.join("Note2.md");

        let content1 = "line 1\n```rust\nfn main() {}\n```\nline 5\n".to_string();
        let content2 = "hello world\n".to_string();

        std::fs::write(&note1_path, &content1).unwrap();
        std::fs::write(&note2_path, &content2).unwrap();

        let mut state = AppState::new();
        state.workspace_path = Some(ws_path.clone());

        // 1. Open Note 1
        state.open_note(note1_path.clone());
        let doc1_id = state.find_document_by_path(&note1_path).unwrap();

        // Populate checkpoints in Note 1
        if let Some(ref mut note) = state.active_note {
            note.scan_code_ahead(100);
            assert!(note.code_checkpoints.scanned_lines() > 0);
        }
        let note1_scanned = state.active_note.as_ref().unwrap().code_checkpoints.scanned_lines();

        // 2. Open Note 2
        state.open_note(note2_path.clone());
        let doc2_id = state.find_document_by_path(&note2_path).unwrap();

        // Active note is now Note 2
        assert_eq!(state.active_note.as_ref().unwrap().path, note2_path);
        // Note 1 in state.documents must hold the stashed text, but Note 1 is NOT duplicated
        // (state.active_note holds Note 2)
        let doc1 = state.documents.get(&doc1_id).unwrap();
        assert_eq!(doc1.content(), content1);
        assert_eq!(doc1.line_index.lines(), 6);
        assert_eq!(doc1.code_checkpoints.scanned_lines(), note1_scanned);
        // Note 2 in state.documents has empty content because active_note holds it directly
        let doc2 = state.documents.get(&doc2_id).unwrap();
        assert!(doc2.content().is_empty());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_open_same_file_in_multiple_panes_reuses_single_buffer_and_zero_duplicate_memory() {
        let temp_dir = std::env::temp_dir();
        let ws_path = temp_dir.join(format!("test_multi_pane_sharing_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("SharedNote.md");
        let content = "Shared markdown content across multiple windows\nLine 2\n".to_string();
        std::fs::write(&note_path, &content).unwrap();

        let mut state = AppState::new();
        state.documents.clear();
        state.by_path.clear();
        state.leaves.clear();
        state.views.clear();
        let default_leaf_id = crate::workspace::LeafId::new();
        state.workspace = crate::workspace::WorkspaceTree::new_single_leaf(default_leaf_id);
        state.leaves.insert(default_leaf_id, crate::workspace::LeafState::new(default_leaf_id));
        state.workspace_path = Some(ws_path.clone());

        // Pane 1 is initial active_leaf
        let leaf1 = state.workspace.active_leaf;

        // 1. Open note in Pane 1
        state.open_note(note_path.clone());
        let doc_id = state.find_document_by_path(&note_path).unwrap();

        // Exactly one document buffer exists in state.documents
        assert_eq!(state.documents.len(), 1);
        // While active, doc.content() in documents is empty because active_note holds it
        assert!(state.documents.get(&doc_id).unwrap().content().is_empty());
        assert_eq!(state.active_note.as_ref().unwrap().content, content);

        // 2. Create Pane 2 (split leaf1 to the right)
        let leaf2 = crate::workspace::LeafId::new();
        assert!(state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2));
        state.workspace.active_leaf = leaf2;

        // 3. Open the SAME note in Pane 2 via open_note (simulating clicking in sidebar or search)
        state.open_note(note_path.clone());

        // Still exactly one document in state.documents (no new DocumentId, no duplicate buffer)
        assert_eq!(state.documents.len(), 1);
        let doc_id_after = state.find_document_by_path(&note_path).unwrap();
        assert_eq!(doc_id, doc_id_after);

        // Active note in leaf2 holds the content, buffer in state.documents is empty
        assert_eq!(state.active_note.as_ref().unwrap().content, content);
        assert!(state.documents.get(&doc_id).unwrap().content().is_empty());

        // Both leaves point to the SAME doc_id
        let l1_tab = state.leaves.get(&leaf1).unwrap().active_tab().unwrap();
        let l2_tab = state.leaves.get(&leaf2).unwrap().active_tab().unwrap();
        assert_eq!(l1_tab.content, crate::workspace::TabContent::Document { id: doc_id });
        assert_eq!(l2_tab.content, crate::workspace::TabContent::Document { id: doc_id });

        // 4. Close tab in Pane 2: note is still open in Pane 1, so document buffer is NOT dropped
        let closed_p = state.close_leaf_tab(leaf2, 0);
        assert_eq!(closed_p, None); // still open, returns None
        assert!(state.documents.contains_key(&doc_id));

        // 5. Close tab in Pane 1: now note is no longer open anywhere, document buffer is unloaded!
        let closed_p2 = state.close_leaf_tab(leaf1, 0);
        assert_eq!(closed_p2, Some(note_path));
        assert_eq!(state.documents.len(), 0);
        assert!(state.find_document_by_path(Path::new("SharedNote.md")).is_none());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_leaf_tab_isolation_and_close_all_tabs() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_isolation_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note1_path = ws_path.join("Note1.md");
        let note2_path = ws_path.join("Note2.md");
        let note3_path = ws_path.join("Note3.md");

        std::fs::write(&note1_path, "Content 1").unwrap();
        std::fs::write(&note2_path, "Content 2").unwrap();
        std::fs::write(&note3_path, "Content 3").unwrap();

        let mut state = AppState::new();
        state.workspace_path = Some(ws_path.clone());

        // Leaf 1: Note 1 and Note 2
        let leaf1 = state.workspace.active_leaf;
        state.leaves.get_mut(&leaf1).unwrap().tabs.clear();
        state.open_note(note1_path.clone());
        let doc1_id = state.find_document_by_path(&note1_path).unwrap();
        state.open_note(note2_path.clone());
        let doc2_id = state.find_document_by_path(&note2_path).unwrap();

        // Create Leaf 2 (split right): Note 2 and Note 3
        let leaf2 = crate::workspace::LeafId::new();
        state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2);
        state.leaves.insert(leaf2, crate::workspace::LeafState::new(leaf2));
        state.workspace.active_leaf = leaf2;
        state.open_note(note2_path.clone());
        state.open_note(note3_path.clone());
        let doc3_id = state.find_document_by_path(&note3_path).unwrap();

        // Verify Leaf 1 has 2 tabs, Leaf 2 has 2 tabs
        assert_eq!(state.leaves.get(&leaf1).unwrap().tabs.len(), 2);
        assert_eq!(state.leaves.get(&leaf2).unwrap().tabs.len(), 2);

        // Verify all 3 documents are in documents map
        assert!(state.documents.contains_key(&doc1_id));
        assert!(state.documents.contains_key(&doc2_id));
        assert!(state.documents.contains_key(&doc3_id));

        // Now simulate closing all tabs in Leaf 1:
        // doc1 is only in Leaf 1 -> should be removed.
        // doc2 is also in Leaf 2 -> should remain!
        if let Some(leaf) = state.leaves.get_mut(&leaf1) {
            let doc_ids: Vec<crate::document_buffer::DocumentId> = leaf
                .tabs
                .iter()
                .filter_map(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => Some(*id),
                    _ => None,
                })
                .collect();
            let tab_ids: Vec<crate::workspace::TabId> = leaf.tabs.iter().map(|t| t.id).collect();
            for tid in tab_ids {
                state.views.remove(&tid);
            }
            leaf.tabs.clear();
            leaf.tabs.push(crate::workspace::Tab::new_empty());
            leaf.active_tab_index = 0;

            for doc_id in doc_ids {
                let still_open = state.leaves.values().any(|l| {
                    l.tabs.iter().any(|t| match &t.content {
                        crate::workspace::TabContent::Document { id } => id == &doc_id,
                        _ => false,
                    })
                });
                if !still_open {
                    if let Some(doc) = state.documents.remove(&doc_id) {
                        let norm = AppState::normalize_path(&doc.path);
                        state.by_path.remove(&norm);
                        state.open_tabs.retain(|p| p != &doc.path);
                    }
                }
            }
        }

        // Leaf 1 now has 1 empty tab
        let l1 = state.leaves.get(&leaf1).unwrap();
        assert_eq!(l1.tabs.len(), 1);
        assert!(matches!(l1.tabs[0].content, crate::workspace::TabContent::Empty));

        // Leaf 2 tabs are 100% UNCHANGED and intact!
        let l2 = state.leaves.get(&leaf2).unwrap();
        assert_eq!(l2.tabs.len(), 2);

        // doc1 removed from state
        assert!(!state.documents.contains_key(&doc1_id));
        assert!(state.find_document_by_path(&note1_path).is_none());

        // doc2 and doc3 are still in state and still open in Leaf 2
        assert!(state.documents.contains_key(&doc2_id));
        assert!(state.documents.contains_key(&doc3_id));
        assert!(state.find_document_by_path(&note2_path).is_some());
        assert!(state.find_document_by_path(&note3_path).is_some());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_memory_snapshots_multi_window_sharing_and_complete_unloading() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_mem_snap_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_a_path = ws_path.join("NoteA.md");
        let note_b_path = ws_path.join("NoteB.md");

        // 10,000 bytes text for Note A, 25,000 bytes text for Note B
        let text_a = "A".repeat(10_000);
        let text_b = "B".repeat(25_000);
        std::fs::write(&note_a_path, &text_a).unwrap();
        std::fs::write(&note_b_path, &text_b).unwrap();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());

        // Snapshot 0: Baseline (no documents loaded)
        let snap0 = state.memory_snapshot();
        assert_eq!(snap0.document_count, 0);
        assert_eq!(snap0.document_bytes, 0);
        assert_eq!(snap0.by_path_count, 0);
        assert_eq!(snap0.view_count, 0);
        assert_eq!(snap0.active_note_bytes, 0);

        // 1. Open Note A in Window 1 (Pane 1)
        let leaf1 = state.workspace.active_leaf;
        state.open_note(note_a_path.clone());
        let snap1 = state.memory_snapshot();
        assert_eq!(snap1.document_count, 1);
        assert_eq!(snap1.by_path_count, 1);
        assert_eq!(snap1.document_bytes + snap1.active_note_bytes, 10_000);

        // 2. Split Window 1 into Window 2 and open the SAME Note A in Window 2
        let leaf2 = crate::workspace::LeafId::new();
        assert!(state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2));
        state.workspace.active_leaf = leaf2;
        state.open_note(note_a_path.clone());

        let snap2 = state.memory_snapshot();
        // Memory MUST NOT double: document_count remains 1, single shared buffer
        assert_eq!(snap2.document_count, 1, "Must reuse single document buffer for Note A");
        assert_eq!(snap2.by_path_count, 1);
        assert_eq!(snap2.document_bytes + snap2.active_note_bytes, 10_000, "Zero duplicate buffer memory!");

        // 3. Split Window 2 into Window 3 and open Note B in Window 3
        let leaf3 = crate::workspace::LeafId::new();
        assert!(state.workspace.split_leaf(leaf2, crate::workspace::SplitDirection::Column, leaf3));
        state.workspace.active_leaf = leaf3;
        state.open_note(note_b_path.clone());

        let snap3 = state.memory_snapshot();
        assert_eq!(snap3.document_count, 2);
        assert_eq!(snap3.by_path_count, 2);
        assert_eq!(snap3.document_bytes + snap3.active_note_bytes, 35_000);

        // 4. Close Window 3 (which holds Note B):
        // Note B is not open anywhere else -> completely unloaded from memory!
        let next_leaf = state.close_leaf(leaf3);
        assert!(next_leaf.is_some());

        let snap4 = state.memory_snapshot();
        assert_eq!(snap4.document_count, 1, "Note B must be completely unloaded");
        assert_eq!(snap4.by_path_count, 1);
        assert_eq!(snap4.document_bytes + snap4.active_note_bytes, 10_000, "Only Note A remains");
        assert!(state.find_document_by_path(&note_b_path).is_none());

        // 5. Close Window 2:
        // Window 2 holds Note A, but Note A is still open in Window 1 -> buffer preserved
        let next_leaf2 = state.close_leaf(leaf2);
        assert!(next_leaf2.is_some());

        let snap5 = state.memory_snapshot();
        assert_eq!(snap5.document_count, 1, "Note A is still open in Window 1");
        assert_eq!(snap5.by_path_count, 1);
        assert!(state.find_document_by_path(&note_a_path).is_some());

        // 6. Close the last tab in Window 1 (holding Note A):
        // Now Note A is completely closed everywhere -> fully unloaded from memory!
        state.close_leaf_tab(leaf1, 0);

        let snap6 = state.memory_snapshot();
        assert_eq!(snap6.document_count, 0, "All document buffers must be unloaded");
        assert_eq!(snap6.document_bytes, 0);
        assert_eq!(snap6.by_path_count, 0);
        assert_eq!(snap6.view_count, 0);
        assert_eq!(snap6.active_note_bytes, 0);
        assert_eq!(snap6, snap0, "Memory footprint must strictly return to initial baseline!");

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_memory_snapshots_close_other_tabs_and_close_to_right() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_close_multi_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let paths: Vec<PathBuf> = (0..5).map(|i| {
            let p = ws_path.join(format!("Note{}.md", i));
            std::fs::write(&p, format!("Content for note {}", i)).unwrap();
            p
        }).collect();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());
        let leaf = state.workspace.active_leaf;

        // Open all 5 notes
        for p in &paths {
            state.open_note(p.clone());
        }

        let snap_opened = state.memory_snapshot();
        assert_eq!(snap_opened.document_count, 5);
        assert_eq!(snap_opened.by_path_count, 5);

        // 1. Simulate close_tabs_to_right keeping tabs 0, 1, 2 (closing 3 and 4)
        if let Some(l) = state.leaves.get_mut(&leaf) {
            l.tabs.truncate(3);
        }
        state.cleanup_orphaned_documents_and_views();

        let snap_truncated = state.memory_snapshot();
        assert_eq!(snap_truncated.document_count, 3);
        assert_eq!(snap_truncated.by_path_count, 3);
        assert_eq!(snap_truncated.view_count, 3);
        assert!(state.find_document_by_path(&paths[3]).is_none());
        assert!(state.find_document_by_path(&paths[4]).is_none());

        // 2. Simulate close_other_tabs keeping only tab 1
        if let Some(l) = state.leaves.get_mut(&leaf) {
            let keep = l.tabs.remove(1);
            l.tabs.clear();
            l.tabs.push(keep);
            l.active_tab_index = 0;
        }
        state.cleanup_orphaned_documents_and_views();

        let snap_others = state.memory_snapshot();
        assert_eq!(snap_others.document_count, 1);
        assert_eq!(snap_others.by_path_count, 1);
        assert_eq!(snap_others.view_count, 1);

        // 3. Close the last tab
        state.close_leaf_tab(leaf, 0);
        let snap_final = state.memory_snapshot();
        assert_eq!(snap_final.document_count, 0);
        assert_eq!(snap_final.document_bytes, 0);
        assert_eq!(snap_final.by_path_count, 0);
        assert_eq!(snap_final.view_count, 0);
        assert_eq!(snap_final.active_note_bytes, 0);

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_memory_snapshots_session_restore_shared_zero_duplicate() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_session_restore_mem_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let shared_path = ws_path.join("Shared.md");
        let content = "Shared Content Across Multiple Windows".to_string();
        std::fs::write(&shared_path, &content).unwrap();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());
        let l1 = state.workspace.active_leaf;
        state.open_note(shared_path.clone());

        let l2 = crate::workspace::LeafId::new();
        state.workspace.split_leaf(l1, crate::workspace::SplitDirection::Row, l2);
        state.workspace.active_leaf = l2;
        state.open_note(shared_path.clone());

        // Export session
        let session = state.export_workspace_session_v2();

        // Restore into a fresh state instance
        let mut restored_state = AppState::new_empty();
        restored_state.workspace_path = Some(ws_path.clone());
        restored_state.restore_workspace_session_v2(session);

        let snap = restored_state.memory_snapshot();
        // Exactly one document buffer restored and shared across both windows
        assert_eq!(snap.document_count, 1);
        assert_eq!(snap.by_path_count, 1);
        assert_eq!(snap.document_bytes, content.len());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_handle_file_deleted_synchronizes_all_leaves_and_unloads_memory() {
        let mut state = AppState::new_empty();
        let ws_path = std::env::temp_dir().join(format!("vibe_test_delete_sync_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("sync_delete.md");
        let content = "# To Be Deleted\nContent to be removed.".to_string();
        std::fs::write(&note_path, &content).unwrap();

        state.workspace_path = Some(ws_path.clone());

        // Split into 2 leaves (windows)
        let l1 = state.workspace.active_leaf;
        let l2 = crate::workspace::LeafId::new();
        state.workspace.split_leaf(l1, crate::workspace::SplitDirection::Row, l2);
        state.leaves.insert(l2, LeafState::new(l2));

        // Open the same file in both windows
        state.workspace.active_leaf = l1;
        state.open_note(note_path.clone());
        state.workspace.active_leaf = l2;
        state.open_note(note_path.clone());

        let snap_before = state.memory_snapshot();
        assert_eq!(snap_before.document_count, 1);
        assert_eq!(snap_before.by_path_count, 1);

        // Delete the file using handle_file_deleted
        let closed = state.handle_file_deleted(&note_path, true);
        assert!(closed);

        let snap_after = state.memory_snapshot();
        // Document must be completely unloaded from memory
        assert_eq!(snap_after.document_count, 0);
        assert_eq!(snap_after.by_path_count, 0);
        assert_eq!(snap_after.document_bytes, 0);
        assert_eq!(snap_after.open_tabs_count, 0);
        assert!(state.active_note.is_none());
        assert!(!state.open_tabs.contains(&note_path));
        assert!(!state.session.open_tabs.contains(&note_path));
        assert!(!state.nav_history.contains(&note_path));

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_handle_file_deleted_preserves_dirty_file_when_configured() {
        let mut state = AppState::new_empty();
        let ws_path = std::env::temp_dir().join(format!("vibe_test_delete_dirty_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("dirty_note.md");
        let content = "# Unsaved edits\nPlease do not lose me!".to_string();
        std::fs::write(&note_path, &content).unwrap();

        state.workspace_path = Some(ws_path.clone());
        state.open_note(note_path.clone());

        // Mark document as dirty
        let doc_id = state.find_document_by_path(&note_path).unwrap();
        state.documents.get_mut(&doc_id).unwrap().is_dirty = true;
        if let Some(ref mut n) = state.active_note {
            n.is_dirty = true;
        }

        // When keep_if_dirty is true, should NOT close the tab
        let closed = state.handle_file_deleted(&note_path, true);
        assert!(!closed, "Dirty document must be preserved when keep_if_dirty is true");

        let snap = state.memory_snapshot();
        assert_eq!(snap.document_count, 1);
        assert_eq!(snap.by_path_count, 1);
        assert!(state.documents.contains_key(&doc_id));

        // When keep_if_dirty is false, should close anyway
        let force_closed = state.handle_file_deleted(&note_path, false);
        assert!(force_closed, "Document must be closed when keep_if_dirty is false");
        let snap_after = state.memory_snapshot();
        assert_eq!(snap_after.document_count, 0);

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_handle_file_renamed_updates_all_windows_and_tabs_synchronously() {
        let mut state = AppState::new_empty();
        let ws_path = std::env::temp_dir().join(format!("vibe_test_rename_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let old_path = ws_path.join("original.md");
        let new_path = ws_path.join("renamed.md");
        let content = "# Original Note\nHello world.".to_string();
        std::fs::write(&old_path, &content).unwrap();

        state.workspace_path = Some(ws_path.clone());

        // Split into 2 leaves
        let l1 = state.workspace.active_leaf;
        let l2 = crate::workspace::LeafId::new();
        state.workspace.split_leaf(l1, crate::workspace::SplitDirection::Row, l2);
        state.leaves.insert(l2, LeafState::new(l2));

        // Open original file in both leaves
        state.workspace.active_leaf = l1;
        state.open_note(old_path.clone());
        state.workspace.active_leaf = l2;
        state.open_note(old_path.clone());

        let doc_id = state.find_document_by_path(&old_path).unwrap();
        assert_eq!(state.documents.get(&doc_id).unwrap().title, "original");

        // Rename the file
        let renamed = state.handle_file_renamed(&old_path, &new_path);
        assert!(renamed);

        // Verify document path and title updated
        let doc = state.documents.get(&doc_id).unwrap();
        assert_eq!(doc.path, new_path);
        assert_eq!(doc.title, "renamed");

        // Verify by_path map updated
        assert!(state.find_document_by_path(&old_path).is_none());
        assert_eq!(state.find_document_by_path(&new_path), Some(doc_id));

        // Verify open_tabs and active_note updated
        assert!(state.open_tabs.contains(&new_path));
        assert!(!state.open_tabs.contains(&old_path));
        if let Some(ref note) = state.active_note {
            assert_eq!(note.path, new_path);
            assert_eq!(note.title, "renamed");
        }

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_check_fs_changes_atomic_save_debounce() {
        let mut state = AppState::new_empty();
        let ws_path = std::env::temp_dir().join(format!("vibe_test_atomic_save_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("atomic_doc.md");
        std::fs::write(&note_path, "# Atomic Test").unwrap();

        state.workspace_path = Some(ws_path.clone());
        state.open_note(note_path.clone());

        let mut pending = HashMap::new();
        let debounce = Duration::from_millis(200);

        // 1. Initial check: file exists -> no events
        let evs = state.check_fs_changes(&mut pending, debounce);
        assert!(evs.is_empty());
        assert!(pending.is_empty());

        // 2. Simulate unlink (file deleted temporarily)
        std::fs::remove_file(&note_path).unwrap();

        // 3. First check after deletion: queued into pending, no Deleted event yet
        let evs1 = state.check_fs_changes(&mut pending, debounce);
        assert!(evs1.is_empty(), "First tick must not emit Deleted due to debounce");
        assert!(pending.contains_key(&note_path));

        // 4. Simulate atomic save: file reappears before debounce expires!
        std::fs::write(&note_path, "# Atomic Test Replaced").unwrap();
        let evs2 = state.check_fs_changes(&mut pending, debounce);
        assert!(evs2.is_empty(), "Must not emit Deleted if file reappeared");
        assert!(!pending.contains_key(&note_path), "Pending deletion must be cancelled");

        // 5. Now delete for real and wait past debounce
        std::fs::remove_file(&note_path).unwrap();
        let _ = state.check_fs_changes(&mut pending, debounce);
        assert!(pending.contains_key(&note_path));

        // Sleep to exceed debounce window
        std::thread::sleep(Duration::from_millis(250));

        let evs3 = state.check_fs_changes(&mut pending, debounce);
        assert_eq!(evs3, vec![FsChangeEvent::Deleted(note_path)]);
        assert!(pending.is_empty());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_view_shell_preserves_text_stats() {
        let stats = TextStats::Large { bytes: 354 * 1024 * 1024 };
        let shell = OpenedNote::new_view_shell(
            PathBuf::from("/test/file.md"),
            "file".to_string(),
            1,
            0,
            None,
            false,
            None,
            crate::line_index::LineIndex::empty(),
            crate::editor_window::EditorWindow::default(),
            0,
            0,
            0.0,
            0.0,
            1.0,
            24.0,
            Some(stats),
        );

        assert_eq!(shell.text_stats(), stats);

        let small_stats = TextStats::Exact { words: 123, chars: 456 };
        let small_shell = OpenedNote::new_view_shell(
            PathBuf::from("/test/small.md"),
            "small".to_string(),
            1,
            0,
            None,
            false,
            None,
            crate::line_index::LineIndex::empty(),
            crate::editor_window::EditorWindow::default(),
            0,
            0,
            0.0,
            0.0,
            1.0,
            24.0,
            Some(small_stats),
        );

        assert_eq!(small_shell.text_stats(), small_stats);
    }
}

