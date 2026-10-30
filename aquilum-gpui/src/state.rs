use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::document_buffer::{DocumentBuffer, DocumentId};
use crate::workspace::session_core::ops::EditEvent;
use crate::workspace::{EditorViewState, LeafId, LeafState, TabId, WorkspaceTree};

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| resolve_app_data_dir(&d))
        .unwrap_or_else(|| PathBuf::from("./data"))
}

pub fn resolve_app_data_dir(data_dir: &Path) -> PathBuf {
    let vibe = data_dir.join("Vibecodium");
    if vibe.exists() {
        return vibe;
    }
    let old_aquilum = data_dir.join("com.dmitriy.aquilum-app");
    let old_vibe = data_dir.join("com.dmitriy.vibecodium");
    let legacy = if old_aquilum.exists() {
        Some(old_aquilum)
    } else if old_vibe.exists() {
        Some(old_vibe)
    } else {
        None
    };
    if let Some(old) = legacy {
        if std::fs::rename(&old, &vibe).is_ok() {
            return vibe;
        }
        return old;
    }
    vibe
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsChangeEvent {
    Deleted(PathBuf),
    Renamed { old_path: PathBuf, new_path: PathBuf },
    Modified(PathBuf),
    /// Watcher error or removed folder: reconcile with the disk (tree reload,
    /// cache invalidation, open-path check).
    Rescan,
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
    #[serde(default)]
    pub source_mode: bool,
}

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
    /// Whether the find bar's replace row was expanded when the app closed.
    #[serde(default)]
    pub find_replace_open: bool,
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
    /// Bookmarks matching Obsidian format.
    #[serde(default)]
    pub bookmarks: crate::views::sidebar::bookmarks::BookmarksData,
    /// Recent files matching recent-files-obsidian plugin.
    #[serde(default)]
    pub recent_files: crate::views::sidebar::recent_files::RecentFilesData,
    /// Sort order of the recent files sidebar list.
    #[serde(default)]
    pub recent_files_sort_order: crate::views::sidebar::recent_files::RecentFilesSortOrder,
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

pub use crate::workspace::session_core::undo::TextChange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteEditEvent {
    pub edit_line: usize,
    pub at: usize,
    pub old_len: usize,
    pub new_len: usize,
    pub line_diff: isize,
}

/// Неизменяемый снимок состояния vault и активного файла для синхронных геттеров плагинов.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub version: u64,
    pub language: String,
    pub active_file: Option<crate::vault::FileInfo>,
    pub files: Arc<[crate::vault::FileInfo]>,
    /// Active editor buffer for the plugins' synchronous `editor.getValue()`
    /// and friends. Rope clone is O(1); `None` when no document is open.
    pub active_doc: Option<ActiveDocSnapshot>,
    /// Command palette entries for `app.commands` (id, name, hotkey, owner).
    pub commands: Arc<[CommandInfo]>,
    /// Registered plugin manifests for `app.plugins` (id, name, version, description).
    pub plugins: Arc<[PluginInfo]>,
}

/// Command palette entry exposed through `app.commands` to plugins.
#[derive(Debug, Clone, Default)]
pub struct CommandInfo {
    pub id: String,
    pub name: String,
    pub hotkey: Option<String>,
    pub owner: Option<String>,
}

/// Registered plugin entry exposed through `app.plugins` to plugins.
#[derive(Debug, Clone, Default)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
}

/// Cursor/selection of the active document in (line, ch) character coordinates,
/// matching Obsidian's `EditorPosition`.
#[derive(Debug, Clone, Default)]
pub struct ActiveDocSnapshot {
    pub rope: crop::Rope,
    /// Cursor position (head of the selection).
    pub cursor: (usize, usize),
    /// Selection anchor, absent for a bare caret.
    pub anchor: Option<(usize, usize)>,
}


/// Metadata for a document tracked by the workspace.
/// Stored even when the document buffer's content is evicted from memory.
#[derive(Clone, Debug)]
pub struct DocumentMetadata {
    pub id: DocumentId,
    pub path: PathBuf,
    pub title: String,
    pub is_dirty: bool,
    pub last_accessed: Instant,
}

pub const DEFAULT_MAX_INACTIVE_DOCS: usize = 4;
pub const MAX_INACTIVE_DOC_BYTES: usize = 1024 * 1024 * 1024; // 1 GB

pub struct AppState {
    pub settings: Arc<crate::settings::SettingsManager>,
    pub workspace_path: Option<PathBuf>,
    pub workspace_name: String,
    pub vault: Option<crate::vault::VaultCore>,
    pub events: Arc<crate::events::EventBus>,
    pub commands: Arc<crate::commands::CommandRegistry>,
    pub ribbon: Arc<crate::ribbon::RibbonRegistry>,
    pub status_bar: Arc<crate::ribbon::StatusBarRegistry>,
    pub notifications: Arc<crate::views::notification::NotificationRegistry>,
    pub code_block_processors: Arc<crate::plugins::code_block::CodeBlockProcessorRegistry>,
    pub plugin_host: Arc<crate::plugins::bridge::PluginHostState>,
    pub snapshot: Arc<arc_swap::ArcSwap<Snapshot>>,
    /// Registered plugin manifests, updated by the plugin bridge when a plugin
    /// registers or unregisters; carried into the snapshot for `app.plugins`.
    pub snapshot_plugins: Arc<arc_swap::ArcSwap<std::sync::Arc<[PluginInfo]>>>,
    pub file_tree: Vec<FileNode>,
    pub expanded_folders: HashSet<PathBuf>,
    pub search_query: String,
    pub open_tabs: Vec<PathBuf>,
    /// Active empty-tab sentinel, if the current tab has no file on disk.
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
    /// Metadata for all open documents (survives memory eviction)
    pub doc_meta: HashMap<DocumentId, DocumentMetadata>,
    /// Maximum number of inactive documents kept in memory (LRU limit)
    pub max_inactive_docs: usize,
    /// Maximum total bytes of inactive documents kept in memory (LRU limit)
    pub max_inactive_doc_bytes: usize,
    /// Multi-document workspace tree
    pub workspace: WorkspaceTree,
    /// Leaves state keyed by LeafId
    pub leaves: HashMap<LeafId, LeafState>,
    /// Editor view states keyed by TabId
    pub views: HashMap<TabId, EditorViewState>,
    /// Event-driven filesystem watcher for the workspace root. Lives here —
    /// not in any window — so every split pane and (future) window shares one
    /// watcher per AppState and cannot see duplicated events.
    pub fs_watcher: crate::fs_watcher::Watcher,
    /// Receiving end of coalesced watcher batches. `None` while no watcher is
    /// attached (no workspace).
    pub fs_events_rx: Option<std::sync::mpsc::Receiver<crate::fs_watcher::FsBatch>>,
    /// Bookmarks hierarchy matching Obsidian format.
    pub bookmarks: crate::views::sidebar::bookmarks::BookmarksData,
    /// Recent files matching recent-files-obsidian plugin.
    pub recent_files: crate::views::sidebar::recent_files::RecentFilesData,
    /// Background search scanner: owns the body cache and the file list, so
    /// the UI thread never touches the filesystem during a search. One
    /// worker per AppState — the "Go to file" dialog and the sidebar panel
    /// both talk to it (reply ordering is per consumer via sequence numbers).
    pub search_worker: crate::search::worker::SearchWorker,
    /// Documents deleted on disk: first-seen time; a path missing for longer
    /// than the debounce window is confirmed as deleted (atomic saves of other
    /// programs make the file disappear briefly and it must be cancelled).
    pub pending_file_deletions: HashMap<PathBuf, std::time::Instant>,
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

        let mut bookmarks = session.bookmarks.clone();
        let mut recent_files = session.recent_files.clone();
        if let Some(ref ws) = session.workspace_path {
            let vault_bm = ws.join(".vault").join("bookmarks.json");
            let obs_bm = ws.join(".obsidian").join("bookmarks.json");
            let bm_path = if vault_bm.exists() {
                Some(vault_bm)
            } else if obs_bm.exists() {
                Some(obs_bm)
            } else {
                None
            };
            if let Some(p) = bm_path {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(data) = serde_json::from_str::<crate::views::sidebar::bookmarks::BookmarksData>(&content) {
                        bookmarks = data;
                    }
                }
            }

            let plugin_rf = ws.join(".vault").join("plugins").join("recent-files-obsidian").join("data.json");
            let vault_rf = ws.join(".vault").join("recent-files.json");
            let rf_path = if plugin_rf.exists() {
                Some(plugin_rf)
            } else if vault_rf.exists() {
                Some(vault_rf)
            } else {
                None
            };
            if let Some(p) = rf_path {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(data) = serde_json::from_str::<crate::views::sidebar::recent_files::RecentFilesData>(&content) {
                        recent_files = data;
                    }
                }
            }
        }

        let events = Arc::new(crate::events::EventBus::new());
        let commands = Arc::new(crate::commands::CommandRegistry::new());
        let ribbon = Arc::new(crate::ribbon::RibbonRegistry::new());
        let status_bar = Arc::new(crate::ribbon::StatusBarRegistry::new());
        let notifications = Arc::new(crate::views::notification::NotificationRegistry::new());
        let code_block_processors = Arc::new(crate::plugins::code_block::CodeBlockProcessorRegistry::new());
        let plugin_host = Arc::new(crate::plugins::bridge::PluginHostState::new());
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::default()));
        let snapshot_plugins = Arc::new(arc_swap::ArcSwap::from_pointee(std::sync::Arc::from(Vec::new())));

        let mut state = Self {
            settings: Arc::clone(&settings),
            workspace_path: None,
            workspace_name: "No Workspace".to_owned(),
            vault: None,
            events,
            commands,
            ribbon,
            status_bar,
            notifications,
            code_block_processors,
            plugin_host,
            snapshot,
            snapshot_plugins,
            file_tree: Vec::new(),
            expanded_folders: session.expanded_folders.clone(),
            search_query: String::new(),
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
            doc_meta: HashMap::new(),
            max_inactive_docs: DEFAULT_MAX_INACTIVE_DOCS,
            max_inactive_doc_bytes: MAX_INACTIVE_DOC_BYTES,
            workspace: {
                let default_leaf_id = LeafId::new();
                WorkspaceTree::new_single_leaf(default_leaf_id)
            },
            leaves: HashMap::new(),
            views: HashMap::new(),
            fs_watcher: crate::fs_watcher::Watcher::new(),
            fs_events_rx: None,
            bookmarks,
            recent_files,
            search_worker: crate::search::worker::SearchWorker::spawn(
                None,
                settings.clone(),
            ),
            pending_file_deletions: HashMap::new(),
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
                        state.document_path(*id).map(|p| p.to_path_buf())
                    }
                    crate::workspace::TabContent::Empty => Some(empty_tab_path()),
                    _ => None,
                }).collect();

                if let Some(tab) = leaf.active_tab() {
                    match &tab.content {
                        crate::workspace::TabContent::Document { id } => {
                            if let Some(doc) = state.documents.get(id) {
                                let doc_path = doc.path.clone();
                                state.reveal_path_in_tree(&doc_path);
                                state.active_empty_tab = None;
                            }
                        }
                        crate::workspace::TabContent::Empty => {
                            state.active_empty_tab = Some(empty_tab_path());
                        }
                        crate::workspace::TabContent::Plugin { .. } => {
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
                let doc_id = state.get_or_open_document(&welcome_path, "Welcome to Vibecodium".to_owned(), DEFAULT_WELCOME_NOTE.to_owned());
                state.workspace.open_file(&mut state.leaves, doc_id, state.workspace.active_leaf, crate::workspace::OpenMode::Reuse);
                if let Some(leaf) = state.leaves.get(&state.workspace.active_leaf) {
                    if let Some(tab) = leaf.active_tab() {
                        state.views.entry(tab.id).or_insert_with(|| crate::workspace::EditorViewState::new(tab.id, doc_id));
                    }
                }
                state.session.active_note = Some(welcome_path);
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
        }).or_else(|| {
            self.doc_meta.iter().find_map(|(id, meta)| {
                if meta.path == path {
                    Some(*id)
                } else {
                    None
                }
            })
        })
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.leaves.get(&self.workspace.active_leaf).and_then(|l| l.active_tab()).map(|t| t.id)
    }

    pub fn active_tab(&self) -> Option<&crate::workspace::Tab> {
        self.leaves.get(&self.workspace.active_leaf).and_then(|l| l.active_tab())
    }

    pub fn active_doc_id(&self) -> Option<DocumentId> {
        self.active_tab().and_then(|t| match &t.content {
            crate::workspace::TabContent::Document { id } => Some(*id),
            _ => None,
        })
    }

    pub fn active_document(&self) -> Option<&DocumentBuffer> {
        let doc_id = self.active_doc_id()?;
        self.documents.get(&doc_id)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut DocumentBuffer> {
        let doc_id = self.active_doc_id()?;
        self.documents.get_mut(&doc_id)
    }

    pub fn active_view(&self) -> Option<&EditorViewState> {
        let tab_id = self.active_tab_id()?;
        self.views.get(&tab_id)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        let tab_id = self.active_tab_id()?;
        self.views.get_mut(&tab_id)
    }

    pub fn active_view_and_doc(&self) -> Option<(&EditorViewState, &DocumentBuffer)> {
        let tab_id = self.active_tab_id()?;
        let doc_id = self.active_doc_id()?;
        let view = self.views.get(&tab_id)?;
        let doc = self.documents.get(&doc_id)?;
        Some((view, doc))
    }

    pub fn active_view_and_doc_mut(&mut self) -> Option<(&mut EditorViewState, &mut DocumentBuffer)> {
        let tab_id = self.active_tab_id()?;
        let doc_id = self.active_doc_id()?;
        let view = self.views.get_mut(&tab_id)?;
        let doc = self.documents.get_mut(&doc_id)?;
        Some((view, doc))
    }

    pub fn dispatch_edit_event(&mut self, ev: &EditEvent, note_edit: &NoteEditEvent) {
        for (tab_id, view) in self.views.iter_mut() {
            if view.doc_id == ev.doc {
                if Some(*tab_id) == ev.origin {
                    if let Some(c) = ev.cursor_after {
                        view.cursor_offset = c;
                        view.selection_anchor = None;
                    }
                    // The view method that produced this event already manages
                    // its own cache invalidation; forcing a full rebuild here
                    // made every keystroke repaint the whole window.
                } else if let Some(doc) = self.documents.get(&ev.doc) {
                    view.apply_external_edit_detailed(doc, note_edit);
                } else {
                    view.visual_cache_dirty = true;
                }
            }
        }
        // The plugin snapshot carries the buffer text: an edit (keyboard, undo,
        // another plugin) invalidates every synchronous editor getter.
        self.publish_snapshot();
    }

    pub fn active_insert_text(&mut self, text: &str) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.insert_text(doc, text);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_replace_range(
        &mut self,
        byte_range: std::ops::Range<usize>,
        new_text: String,
        new_cursor: usize,
    ) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.replace_range(doc, byte_range, new_text, new_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    /// Flips the mark of a task checkbox on `line`, without moving the caret or
    /// dropping the selection.
    ///
    /// `mark` is the byte range of the single character between the brackets,
    /// relative to the line. Replacing one character with one character keeps
    /// every offset valid, so the click only has to repaint the edited row. The
    /// step lands in the document history like any other edit.
    pub fn active_toggle_task_mark(&mut self, line: usize, mark: std::ops::Range<usize>) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let line_start = doc.line_start_byte(line);
        let range = line_start + mark.start..line_start + mark.end;
        if range.end > doc.len() {
            return false;
        }
        let removed = doc.slice_to_string(range.clone());
        let inserted = if removed.trim().is_empty() { "x" } else { " " };
        let selection = view.selection();
        let (_, note_edit, edit_event) = doc.record_change_with_selection(
            range.start,
            removed,
            inserted.to_string(),
            selection,
            selection,
            // An external edit (`None`) is transformed by every view instead of
            // overwriting its caret: the click must not move the cursor.
            None,
        );
        self.dispatch_edit_event(&edit_event, &note_edit);
        true
    }

    pub fn active_toggle_raw_table(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let (cursor_line, _) = view.line_col(doc);
        let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
        if let Some(t) = tables.at(cursor_line) {
            view.cursor_line = cursor_line;
            view.toggle_raw_table_at(t.first_line);
            true
        } else {
            false
        }
    }

    pub fn active_toggle_raw_table_at(&mut self, first_line: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };

        let (cursor_line, _) = view.line_col(doc);
        view.cursor_line = cursor_line;
        view.toggle_raw_table_at(first_line);
        true
    }

    pub fn active_apply_table_line_edit(
        &mut self,
        edit: &crate::table::LineEdit,
        new_cursor_line_col: Option<(usize, usize)>,
    ) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };

        if edit.new_lines.is_empty() {
            view.raw_tables.remove(&edit.first_line);
        }
        let line_diff = edit.new_lines.len() as isize - edit.old_line_count as isize;
        if line_diff != 0 {
            view.shift_raw_tables(edit.first_line + 1, line_diff);
        }

        let total_lines = doc.line_index.lines();
        let start_byte = doc.line_start_byte(edit.first_line);
        let end_byte = doc.line_start_byte(edit.first_line + edit.old_line_count);
        let mut new_text = edit.new_lines.join("\n");
        if !edit.new_lines.is_empty() && edit.first_line + edit.old_line_count < total_lines {
            new_text.push('\n');
        }
        let target_cursor = if let Some((target_line, target_col)) = new_cursor_line_col {
            if target_line >= edit.first_line && target_line < edit.first_line + edit.new_lines.len() {
                let rel_line = target_line - edit.first_line;
                let mut offset_in_new = 0;
                for i in 0..rel_line {
                    offset_in_new += edit.new_lines[i].len() + 1;
                }
                let byte_col: usize = edit.new_lines[rel_line]
                    .chars()
                    .take(target_col)
                    .map(|c| c.len_utf8())
                    .sum();
                start_byte + offset_in_new + byte_col
            } else {
                start_byte
            }
        } else {
            // Preserve existing cursor location
            let old_cursor = view.cursor_offset;
            let (cur_line, cur_col) = view.line_col(doc);
            if cur_line < edit.first_line {
                // Cursor was before the table: position unchanged
                old_cursor
            } else if cur_line >= edit.first_line + edit.old_line_count {
                // Cursor was after the table: shift by byte delta of replacement
                let old_table_bytes = end_byte.saturating_sub(start_byte);
                let new_table_bytes = new_text.len();
                let delta = new_table_bytes as isize - old_table_bytes as isize;
                (old_cursor as isize + delta).max(0) as usize
            } else {
                // Cursor was inside the table: keep it on the same line and column if possible
                let rel_line = cur_line - edit.first_line;
                if rel_line < edit.new_lines.len() {
                    let mut offset_in_new = 0;
                    for i in 0..rel_line {
                        offset_in_new += edit.new_lines[i].len() + 1;
                    }
                    let line_char_count = edit.new_lines[rel_line].chars().count();
                    let target_col = cur_col.min(line_char_count);
                    let byte_col: usize = edit.new_lines[rel_line]
                        .chars()
                        .take(target_col)
                        .map(|c| c.len_utf8())
                        .sum();
                    start_byte + offset_in_new + byte_col
                } else {
                    start_byte + new_text.len()
                }
            }
        };

        let (_, note_edit, edit_event) = view.replace_range(doc, start_byte..end_byte, new_text, target_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_cursor_line_col(&self) -> Option<(usize, usize)> {
        let (view, doc) = self.active_view_and_doc()?;
        Some(view.line_col(doc))
    }

    pub fn active_set_cursor_line_col(&mut self, line: usize, col: usize) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let offset = view.offset_for_line_col(doc, line, col);
        view.cursor_offset = offset;
        view.selection_anchor = None;
    }

    pub fn active_delete_backwards(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        if let Some((_, note_edit, edit_event)) = view.delete_backwards(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
        }
    }

    pub fn active_delete_forwards(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        if let Some((_, note_edit, edit_event)) = view.delete_forwards(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
        }
    }

    pub fn active_insert_newline_auto_indent(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.insert_newline_auto_indent(doc);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    pub fn active_delete_selection(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edit, edit_event)) = view.delete_selection(doc) {
            self.dispatch_edit_event(&edit_event, &note_edit);
            true
        } else {
            false
        }
    }

    pub fn active_replace_selection(&mut self, text: &str) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        let (_, note_edit, edit_event) = view.replace_selection(doc, text);
        self.dispatch_edit_event(&edit_event, &note_edit);
    }

    /// Full editor buffer state for the plugin protocol: text, cursor and
    /// selection as (line, ch) pairs with `ch` counted in characters, which is
    /// what Obsidian's `EditorPosition` uses.
    pub fn active_editor_buffer(&self) -> Option<(String, (usize, usize), Option<(usize, usize)>)> {
        let (view, doc) = self.active_view_and_doc()?;
        let head = view.line_col(doc);
        let anchor = view.selection_anchor.map(|a| {
            let anchor_line = doc.line_of_byte(a.min(doc.len()));
            let anchor_start = doc.line_start_byte(anchor_line).min(a);
            let anchor_ch = doc.slice_to_string(anchor_start..a).chars().count();
            (anchor_line, anchor_ch)
        });
        Some((doc.content(), head, anchor))
    }

    /// Replaces the whole document text, keeping the cursor clamped to the new end.
    pub fn active_set_buffer(&mut self, text: &str) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        let new_cursor = text.len();
        let (_, note_edit, edit_event) = view.replace_range(doc, 0..doc.len(), text.to_string(), new_cursor);
        self.dispatch_edit_event(&edit_event, &note_edit);
        true
    }

    /// Moves the cursor to a document byte offset, dropping the selection.
    pub fn active_set_cursor_offset(&mut self, offset: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        let offset = offset.min(doc.len());
        view.cursor_offset = offset;
        view.selection_anchor = None;
        view.visual_cache_dirty = true;
        true
    }

    /// Selects the byte range `anchor..head`; the cursor sits at `head`.
    pub fn active_set_selection_range(&mut self, anchor: usize, head: usize) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        view.cursor_offset = head.min(doc.len());
        view.selection_anchor = Some(anchor.min(doc.len()));
        view.visual_cache_dirty = true;
        true
    }

    pub fn active_undo(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edits, edit_event)) = view.undo(doc) {
            for note_edit in &note_edits {
                self.dispatch_edit_event(&edit_event, note_edit);
            }
            true
        } else {
            false
        }
    }

    pub fn active_redo(&mut self) -> bool {
        let Some(tab_id) = self.active_tab_id() else { return false; };
        let Some(doc_id) = self.active_doc_id() else { return false; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return false; };
        if let Some((_, note_edits, edit_event)) = view.redo(doc) {
            for note_edit in &note_edits {
                self.dispatch_edit_event(&edit_event, note_edit);
            }
            true
        } else {
            false
        }
    }

    pub fn active_select_all(&mut self) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.select_all(doc);
    }

    /// Read-only access to the active view and document for callers that need
    /// to compute coordinates (byte offsets from line/col) before mutating.
    pub fn with_active_doc_and_view<R>(&self, f: impl FnOnce(&DocumentBuffer, &EditorViewState) -> R) -> Option<R> {
        let (view, doc) = self.active_view_and_doc()?;
        Some(f(doc, view))
    }

    pub fn active_selected_text(&self) -> Option<String> {
        let (view, doc) = self.active_view_and_doc()?;
        view.selected_text(doc)
    }

    pub fn active_move_cursor_left(&mut self, shift: bool) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.move_cursor_left(doc, shift);
    }

    pub fn active_move_cursor_right(&mut self, shift: bool) {
        let Some(tab_id) = self.active_tab_id() else { return; };
        let Some(doc_id) = self.active_doc_id() else { return; };
        let (Some(view), Some(doc)) = (self.views.get_mut(&tab_id), self.documents.get_mut(&doc_id)) else { return; };
        view.move_cursor_right(doc, shift);
    }

    /// Returns the set of all DocumentIds that are currently visible
    /// as active tabs in ANY leaf/split (1, 4, 8, etc.).
    /// Visible documents are never evicted.
    pub fn visible_document_ids(&self) -> HashSet<DocumentId> {
        let mut visible = HashSet::new();
        for leaf in self.leaves.values() {
            if let Some(tab) = leaf.active_tab() {
                if let crate::workspace::TabContent::Document { id } = tab.content {
                    visible.insert(id);
                }
            }
        }
        visible
    }

    /// Update last access timestamp for a document in the LRU tracker.
    pub fn touch_document_accessed(&mut self, doc_id: DocumentId) {
        if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
            meta.last_accessed = Instant::now();
        }
    }

    /// Path of a document, whether currently loaded into memory or evicted.
    pub fn document_path(&self, id: DocumentId) -> Option<&Path> {
        self.documents
            .get(&id)
            .map(|d| d.path.as_path())
            .or_else(|| self.doc_meta.get(&id).map(|m| m.path.as_path()))
    }

    /// Title of a document, whether currently loaded into memory or evicted.
    pub fn document_title(&self, id: DocumentId) -> Option<&str> {
        self.documents
            .get(&id)
            .map(|d| d.title.as_str())
            .or_else(|| self.doc_meta.get(&id).map(|m| m.title.as_str()))
    }

    /// Dirty status of a document.
    pub fn is_document_dirty(&self, id: DocumentId) -> bool {
        self.documents
            .get(&id)
            .map(|d| d.is_dirty)
            .or_else(|| self.doc_meta.get(&id).map(|m| m.is_dirty))
            .unwrap_or(false)
    }

    /// Checks if a document is currently loaded in memory.
    pub fn is_document_loaded(&self, id: DocumentId) -> bool {
        self.documents.contains_key(&id)
    }

    /// Ensures that the given document is loaded in memory.
    /// If it was evicted or not yet read from disk, it is loaded on demand.
    pub fn ensure_document_loaded(&mut self, doc_id: DocumentId) -> bool {
        self.touch_document_accessed(doc_id);
        if self.documents.contains_key(&doc_id) {
            return true;
        }

        let Some(meta) = self.doc_meta.get(&doc_id) else {
            return false;
        };
        let path = meta.path.clone();
        let title = meta.title.clone();

        let buf = if path.to_string_lossy() == "Welcome.md" {
            DocumentBuffer::new(doc_id, path.clone(), title, DEFAULT_WELCOME_NOTE.to_owned(), 1)
        } else {
            match DocumentBuffer::from_file(doc_id, path.clone(), title.clone(), 1) {
                Ok(b) => b,
                Err(err) => {
                    eprintln!("Failed to read file on lazy load: {:?}: {}", path, err);
                    DocumentBuffer::new(doc_id, path.clone(), title, String::new(), 1)
                }
            }
        };

        self.documents.insert(doc_id, buf);
        let norm = Self::normalize_path(&path);
        self.by_path.insert(norm, doc_id);

        self.evict_inactive_documents_excluding(Some(doc_id));
        true
    }

    /// Evicts inactive document buffers from memory using LRU policy.
    /// Documents that are active in any visible leaf are NEVER evicted.
    /// If an inactive document is dirty, it is saved to disk before eviction.
    pub fn evict_inactive_documents_if_needed(&mut self) {
        self.evict_inactive_documents_excluding(None);
    }

    pub fn evict_inactive_documents_excluding(&mut self, excluded: Option<DocumentId>) {
        let visible = self.visible_document_ids();

        let mut inactive_candidates: Vec<DocumentId> = self
            .documents
            .keys()
            .filter(|id| !visible.contains(id) && Some(**id) != excluded)
            .copied()
            .collect();

        if inactive_candidates.is_empty() {
            return;
        }

        // Sort ascending by last_accessed: oldest accessed first
        inactive_candidates.sort_by_key(|id| {
            self.doc_meta
                .get(id)
                .map(|m| m.last_accessed)
                .unwrap_or_else(Instant::now)
        });

        let max_inactive = self.max_inactive_docs;
        let max_bytes = self.max_inactive_doc_bytes;
        let mut total_inactive_bytes: usize = inactive_candidates
            .iter()
            .filter_map(|id| self.documents.get(id))
            .map(|d| d.len())
            .sum();

        for doc_id in inactive_candidates {
            let inactive_count = self
                .documents
                .keys()
                .filter(|id| !visible.contains(id) && Some(**id) != excluded)
                .count();

            if inactive_count <= max_inactive && total_inactive_bytes <= max_bytes {
                break;
            }

            if let Some(mut doc) = self.documents.remove(&doc_id) {
                total_inactive_bytes = total_inactive_bytes.saturating_sub(doc.len());

                // If dirty, safely persist before eviction
                if doc.is_dirty {
                    if let Err(err) = crate::save_manager::safe_write_rope(&doc.path, doc.rope()) {
                        eprintln!(
                            "Failed to save dirty document before eviction: {:?}: {}",
                            doc.path, err
                        );
                    } else {
                        doc.is_dirty = false;
                    }
                }

                // Keep metadata up to date
                if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                    meta.is_dirty = doc.is_dirty;
                    meta.title = doc.title;
                }
            }
        }
    }

    pub fn get_or_open_document(&mut self, path: &Path, title: String, content: String) -> DocumentId {
        let norm = Self::normalize_path(path);
        if let Some(&doc_id) = self.by_path.get(&norm) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }
        if let Some(doc_id) = self.find_document_by_path(path) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }

        let doc_id = DocumentId::new();
        let buf = DocumentBuffer::new(doc_id, path.to_path_buf(), title.clone(), content, 1);
        self.documents.insert(doc_id, buf);
        self.by_path.insert(norm, doc_id);
        self.doc_meta.insert(
            doc_id,
            DocumentMetadata {
                id: doc_id,
                path: path.to_path_buf(),
                title,
                is_dirty: false,
                last_accessed: Instant::now(),
            },
        );
        self.evict_inactive_documents_excluding(Some(doc_id));
        doc_id
    }

    pub fn get_or_open_document_from_file(&mut self, path: &Path, title: String) -> DocumentId {
        let norm = Self::normalize_path(path);
        if let Some(&doc_id) = self.by_path.get(&norm) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }
        if let Some(doc_id) = self.find_document_by_path(path) {
            self.ensure_document_loaded(doc_id);
            self.touch_document_accessed(doc_id);
            return doc_id;
        }

        let doc_id = DocumentId::new();
        let buf = DocumentBuffer::from_file(doc_id, path.to_path_buf(), title.clone(), 1)
            .unwrap_or_else(|_| DocumentBuffer::new(doc_id, path.to_path_buf(), title.clone(), String::new(), 1));
        self.documents.insert(doc_id, buf);
        self.by_path.insert(norm, doc_id);
        self.doc_meta.insert(
            doc_id,
            DocumentMetadata {
                id: doc_id,
                path: path.to_path_buf(),
                title,
                is_dirty: false,
                last_accessed: Instant::now(),
            },
        );
        self.evict_inactive_documents_excluding(Some(doc_id));
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

        self.doc_meta.retain(|doc_id, meta| {
            if active_doc_ids.contains(doc_id) {
                true
            } else {
                removed_paths.push(meta.path.clone());
                false
            }
        });

        // 4. Clean up by_path and open_tabs
        for p in &removed_paths {
            let norm = Self::normalize_path(p);
            self.by_path.remove(&norm);
            self.open_tabs.retain(|x| x != p);
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
        self.invalidate_layout_caches();
        next
    }

    /// Captures a point-in-time snapshot of document memory allocations and open counts.
    pub fn memory_snapshot(&self) -> MemorySnapshot {
        let doc_bytes: usize = self.documents.values().map(|d| d.len()).sum();
        MemorySnapshot {
            document_count: self.documents.len(),
            document_bytes: doc_bytes,
            by_path_count: self.by_path.len(),
            view_count: self.views.len(),
            active_note_bytes: 0,
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
        let mut matching_doc_ids: Vec<DocumentId> = self
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

        for (id, meta) in &self.doc_meta {
            let norm_doc = Self::normalize_path(&meta.path);
            if (meta.path == path
                || norm_doc == norm_target
                || meta.path.starts_with(path)
                || norm_doc.starts_with(&norm_target))
                && !matching_doc_ids.contains(id)
            {
                matching_doc_ids.push(*id);
            }
        }

        let mut any_closed = false;
        let mut any_kept_dirty = false;

        for doc_id in matching_doc_ids {
            let doc_path = if let Some(p) = self.document_path(doc_id) {
                p.to_path_buf()
            } else {
                continue;
            };

            let is_doc_dirty = self.is_document_dirty(doc_id);

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
                        if !leaf.tabs.is_empty() {
                            if i < leaf.active_tab_index {
                                leaf.active_tab_index = leaf.active_tab_index.saturating_sub(1);
                            } else if leaf.active_tab_index >= leaf.tabs.len() {
                                leaf.active_tab_index = leaf.tabs.len() - 1;
                            }
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

            // Remove from document buffers, doc_meta and by_path mapping
            let removed_path = self.documents.remove(&doc_id).map(|d| d.path)
                .or_else(|| self.doc_meta.remove(&doc_id).map(|m| m.path));
            self.doc_meta.remove(&doc_id);

            if let Some(target_p) = removed_path {
                let norm = Self::normalize_path(&target_p);
                self.by_path.remove(&norm);
                self.by_path.remove(&Self::normalize_path(path));
                self.open_tabs.retain(|p| p != &target_p && p != path && p != &norm);
                self.session.open_tabs.retain(|p| p != &target_p && p != path && p != &norm);
                self.nav_history.retain(|p| p != &target_p && p != path && p != &norm);
                self.note_states.remove(&target_p);
                self.note_states.remove(path);

                if self.session.active_note.as_ref().map(|p| p == &target_p || p == path).unwrap_or(false) {
                    self.session.active_note = None;
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

            if self.session.active_note.as_ref().map(|p| p == path || Self::normalize_path(p) == norm_p).unwrap_or(false) {
                self.session.active_note = None;
                any_closed = true;
            }
        }

        // Also remove from recent files
        let ws = self.workspace_path.as_deref();
        if self.recent_files.remove_path_or_prefix(path, ws) {
            self.session.recent_files = self.recent_files.clone();
            any_closed = true;
        }

        self.cleanup_orphaned_documents_and_views();

        // If session active_note is None, try activating whatever tab is in active_leaf
        if self.session.active_note.is_none() {
            let active_leaf_id = self.workspace.active_leaf;
            if let Some(leaf) = self.leaves.get(&active_leaf_id) {
                if let Some(tab) = leaf.active_tab() {
                    match &tab.content {
                        crate::workspace::TabContent::Document { id } => {
                            if let Some(doc) = self.documents.get(id) {
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
        // The file set changed on disk: the plugin snapshot's file list (and the
        // vault cache behind it) must be rebuilt, or plugins keep seeing the
        // pre-change list. The rename case is covered by the same publish in
        // handle_file_renamed.
        self.invalidate_markdown_cache_and_tree();
        self.publish_snapshot();
        any_closed
    }

    /// Centralized handling when a file or directory has been renamed or moved on disk or by a plugin.
    ///
    /// Synchronously updates all matching documents, tabs across all leaves/windows,
    /// paths, titles, `open_tabs`, `session.open_tabs`, and `active_note`.
    pub fn handle_file_renamed(&mut self, old_path: &Path, new_path: &Path) -> bool {
        let norm_old = Self::normalize_path(old_path);
        let norm_new = Self::normalize_path(new_path);

        let mut matching_docs: Vec<(DocumentId, PathBuf)> = self
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

        for (id, meta) in &self.doc_meta {
            let norm_doc = Self::normalize_path(&meta.path);
            let target = if meta.path == old_path || norm_doc == norm_old {
                Some(new_path.to_path_buf())
            } else if let Ok(suffix) = meta.path.strip_prefix(old_path) {
                Some(new_path.join(suffix))
            } else if let Ok(suffix) = norm_doc.strip_prefix(&norm_old) {
                Some(norm_new.join(suffix))
            } else {
                None
            };
            if let Some(target_p) = target {
                if !matching_docs.iter().any(|(d, _)| d == id) {
                    matching_docs.push((*id, target_p));
                }
            }
        }

        let mut any_renamed = false;

        for (doc_id, target_new_path) in matching_docs {
            let new_title = target_new_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());

            let old_doc_path = if let Some(doc) = self.documents.get_mut(&doc_id) {
                let prev = std::mem::replace(&mut doc.path, target_new_path.clone());
                doc.title = new_title.clone();
                if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                    meta.path = target_new_path.clone();
                    meta.title = new_title.clone();
                }
                prev
            } else if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                let prev = std::mem::replace(&mut meta.path, target_new_path.clone());
                meta.title = new_title.clone();
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

            if self.session.active_note.as_ref().map(|p| p == &old_doc_path || Self::normalize_path(p) == norm_old_doc).unwrap_or(false) {
                self.session.active_note = Some(target_new_path);
            }

            any_renamed = true;
        }

        // Also check if open_tabs match old_path directly even if not in documents
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
        }

        // Also update recent files
        let ws = self.workspace_path.as_deref();
        if self.recent_files.rename_path_or_prefix(old_path, new_path, ws) {
            self.session.recent_files = self.recent_files.clone();
            any_renamed = true;
        }

        self.save_session_state();
        self.invalidate_markdown_cache_and_tree();
        self.publish_snapshot();
        any_renamed
    }

    /// Centralized handling when a file has been modified externally on disk.
    ///
    /// If the file is open and clean (not dirty), reloads content from disk.
    /// If the file has unsaved edits, keeps the user's edits and warns in status.
    /// A modified event for a *not open* note is how the watcher reports a
    /// note created outside the vault API (new-note button, external editor):
    /// the plugin file list must be rebuilt or plugins keep seeing the old set.
    pub fn handle_file_modified(&mut self, path: &Path) -> bool {
        let opt_doc_id = self.find_document_by_path(path);

        if opt_doc_id.is_none() {
            // Cheap: the vault cache is just an Option; the publish shares the
            // rebuilt list through an Arc. A write echo of a deleted-then-recreated
            // tab also lands here, which is the right moment to resync anyway.
            self.invalidate_markdown_cache_and_tree();
            self.publish_snapshot();
        }

        if let Some(doc_id) = opt_doc_id {
            let is_dirty = self.documents.get(&doc_id).map(|d| d.is_dirty).unwrap_or(false);

            // Dirty buffer: an external change normally only gets a status —
            // but the watcher also fires for our own background save (a plain
            // in-place write is indistinguishable from a foreign write by the
            // event alone). When the file content already equals the buffer,
            // this is our own write echo: reconcile the hash silently.
            if is_dirty {
                if let Some(actual) = crate::save_manager::hash_file(path) {
                    let buffer_matches = self.documents.get(&doc_id)
                        .map(|d| {
                            d.disk_hash.as_deref() == Some(actual.as_str())
                                || crate::save_manager::hash_rope(&d.rope) == actual
                        })
                        .unwrap_or(false);
                    if buffer_matches {
                        if let Some(doc) = self.documents.get_mut(&doc_id) {
                            doc.disk_hash = Some(actual);
                            doc.disk_stamp = crate::save_manager::file_stamp(path);
                        }
                        return false;
                    }
                }
                self.status_text = format!(
                    "Файл изменён внешним приложением (в редакторе есть несохранённые правки): {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                return false;
            }

            // Pull protocol (fs-watcher-plan §3): skip both our own write
            // echoes and pointless re-reads with two cheap checks before any
            // read. Unknown hash ("not reconciled yet") must not block a
            // reload — the on-disk content is authoritative for a clean doc.
            let known_hash = self.documents.get(&doc_id).and_then(|d| d.disk_hash.clone());
            let known_stamp = self.documents.get(&doc_id).and_then(|d| d.disk_stamp);
            if let Some(stamp) = crate::save_manager::file_stamp(path) {
                if Some(stamp) == known_stamp {
                    return false;
                }
            }
            if let (Some(known), Some(actual)) = (&known_hash, crate::save_manager::hash_file(path)) {
                if *known == actual {
                    // Same content, new stamp: remember it and skip the read.
                    if let Some(doc) = self.documents.get_mut(&doc_id) {
                        doc.disk_stamp = crate::save_manager::file_stamp(path);
                    }
                    return false;
                }
            }

            if let Ok(new_content) = std::fs::read_to_string(path) {
                if let Some(doc) = self.documents.get_mut(&doc_id) {
                    doc.reload(new_content);
                }
                for view in self.views.values_mut() {
                    if view.doc_id == doc_id {
                        view.visual_cache_dirty = true;
                    }
                }
                self.status_text = format!(
                    "Файл обновлён: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                self.invalidate_markdown_cache_and_tree();
                self.publish_snapshot();
                return true;
            }
        }
        false
    }

    /// Drains coalesced watcher events and applies the deletion debounce.
    ///
    /// The watcher (or a test, through the same channel) delivers
    /// `Deleted`-worthy paths; a path absent from disk for longer than
    /// `debounce_duration` is confirmed deleted. **No filesystem calls are
    /// made when nothing is pending** — the old per-document `path.exists()`
    /// polling on the UI thread is gone. The returned events are exactly what
    /// `poll_fs_changes` handles.
    pub fn check_fs_changes(
        &mut self,
        debounce_duration: Duration,
    ) -> Vec<FsChangeEvent> {
        let mut events = Vec::new();

        // 1. Drain every batch already coalesced by the delivery thread.
        // Deleting and renaming are acted on immediately; writes are fed
        // through the same missing-on-disk debounce, because an atomic save
        // (tmp → rename) can transiently make the target look gone.
        if let Some(rx) = &self.fs_events_rx {
            let mut deleted: Vec<PathBuf> = Vec::new();
            let mut renamed: Vec<(PathBuf, PathBuf)> = Vec::new();
            let mut modified: Vec<PathBuf> = Vec::new();
            let mut rescan = false;
            while let Ok(batch) = rx.try_recv() {
                deleted.extend(batch.deleted);
                renamed.extend(batch.renamed);
                modified.extend(batch.modified);
                rescan |= batch.rescan;
            }
            for (from, to) in renamed {
                events.push(FsChangeEvent::Renamed {
                    old_path: from,
                    new_path: to,
                });
            }
            // Deletions confirmed through the debounce below; writes of
            // existing files go straight to the modified path (the pull
            // protocol in `handle_file_modified` decides whether a reload is
            // needed).
            for path in modified {
                if path.exists() {
                    events.push(FsChangeEvent::Modified(path));
                }
            }
            let mut seen_missing: Vec<PathBuf> = Vec::new();
            for path in deleted {
                if !seen_missing.contains(&path) {
                    seen_missing.push(path.clone());
                    self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
                }
            }
            if rescan {
                events.push(FsChangeEvent::Rescan);
            }
        }

        // 2. Apply the deletion debounce to every pending path: a path that
        // reappeared (atomic save / rapid recreation) is cancelled even
        // before its debounce expires.
        let now_expired: Vec<PathBuf> = self
            .pending_file_deletions
            .iter()
            .filter(|(_, first_seen)| first_seen.elapsed() >= debounce_duration)
            .map(|(path, _)| path.clone())
            .collect();
        let still_pending: Vec<PathBuf> = self
            .pending_file_deletions
            .keys()
            .filter(|path| !now_expired.contains(path))
            .cloned()
            .collect();
        for path in still_pending {
            if path.exists() {
                self.pending_file_deletions.remove(&path);
            }
        }
        for path in now_expired {
            if path.exists() {
                self.pending_file_deletions.remove(&path);
            } else {
                self.pending_file_deletions.remove(&path);
                events.push(FsChangeEvent::Deleted(path));
            }
        }

        // 3. Clean up pending deletions for paths that are no longer open or in recent files.
        let mut tracked_paths: Vec<PathBuf> = self
            .documents
            .values()
            .map(|d| d.path.clone())
            .chain(self.doc_meta.values().map(|m| m.path.clone()))
            .collect();
        if let Some(ws) = &self.workspace_path {
            for item in &self.recent_files.recent_files {
                let p = Path::new(&item.path);
                let abs = if p.is_absolute() { p.to_path_buf() } else { ws.join(p) };
                tracked_paths.push(abs);
            }
        }
        self.pending_file_deletions.retain(|p, _| tracked_paths.contains(p));
        events
    }

    /// Request closing a tab in a leaf. Returns any effects (title commit, disk flush)
    /// that must be successfully executed before the tab can actually be closed.
    pub fn request_close_leaf_tab(&self, leaf_id: LeafId, tab_index: usize) -> Result<(TabId, crate::workspace::CloseRequest), String> {
        let leaf = self.leaves.get(&leaf_id).ok_or_else(|| "Unknown leaf".to_string())?;
        let tab = leaf.tabs.get(tab_index).ok_or_else(|| "Unknown tab index".to_string())?;
        let tab_id = tab.id;
        let mut effects = Vec::new();

        if let Some(view) = self.views.get(&tab_id) {
            if let Some(title_input) = &view.title_input {
                if let crate::workspace::TabContent::Document { id } = tab.content {
                    effects.push(crate::workspace::CloseEffect::CommitTitle {
                        tab_id,
                        doc_id: id,
                        new_title: title_input.text.clone(),
                    });
                }
            }
        }

        if let crate::workspace::TabContent::Document { id } = tab.content {
            let doc_id = id;
            let tab_count = self
                .leaves
                .values()
                .flat_map(|l| &l.tabs)
                .filter(|t| matches!(t.content, crate::workspace::TabContent::Document { id } if id == doc_id))
                .count();
            let is_last = tab_count <= 1;
            let is_dirty = self.is_document_dirty(doc_id);
            if is_last && is_dirty {
                effects.push(crate::workspace::CloseEffect::FlushDoc(doc_id));
            }
        }

        let req = if effects.is_empty() {
            crate::workspace::CloseRequest::Ready
        } else {
            crate::workspace::CloseRequest::NeedsEffects(effects)
        };
        Ok((tab_id, req))
    }

    /// Request closing a tab by its TabId.
    pub fn request_close_tab(&self, tab_id: TabId) -> Result<crate::workspace::CloseRequest, String> {
        for (leaf_id, leaf) in &self.leaves {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                let (_, req) = self.request_close_leaf_tab(*leaf_id, idx)?;
                return Ok(req);
            }
        }
        Err("Unknown tab".to_string())
    }

    /// Executes a single close effect with real IO.
    /// If renaming or saving fails, returns Err and leaves state intact.
    pub fn run_close_effect(&mut self, effect: &crate::workspace::CloseEffect) -> Result<(), String> {
        match effect {
            crate::workspace::CloseEffect::CommitTitle { tab_id, doc_id, new_title } => {
                let (current_path, current_title) = if let Some(doc) = self.documents.get(doc_id) {
                    (doc.path.clone(), doc.title.clone())
                } else if let Some(meta) = self.doc_meta.get(doc_id) {
                    (meta.path.clone(), meta.title.clone())
                } else {
                    return Err("Document not found".to_string());
                };
                let trimmed = new_title.trim();
                if !trimmed.is_empty() && trimmed != current_title {
                    self.rename_note(&current_path, trimmed)?;
                }
                if let Some(view) = self.views.get_mut(tab_id) {
                    view.title_input = None;
                }
                Ok(())
            }
            crate::workspace::CloseEffect::FlushDoc(doc_id) => {
                if self.save_document(*doc_id) {
                    Ok(())
                } else {
                    Err("Failed to save document".to_string())
                }
            }
        }
    }

    /// Performs the final step of closing a leaf tab after all effects have been satisfied.
    pub fn finish_close_leaf_tab(&mut self, leaf_id: LeafId, tab_index: usize) -> Result<Option<PathBuf>, String> {
        let (_tab_id, req) = self.request_close_leaf_tab(leaf_id, tab_index)?;
        if let crate::workspace::CloseRequest::NeedsEffects(_) = req {
            return Err("Pending effects must be executed before finish_close".to_string());
        }
        Ok(self.close_leaf_tab_direct(leaf_id, tab_index))
    }

    /// Performs the final step of closing a tab by TabId after all effects have been satisfied.
    pub fn finish_close_tab(&mut self, tab_id: TabId) -> Result<Option<PathBuf>, String> {
        let mut target = None;
        for (leaf_id, leaf) in &self.leaves {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                target = Some((*leaf_id, idx));
                break;
            }
        }
        let (leaf_id, idx) = target.ok_or_else(|| "Unknown tab".to_string())?;
        self.finish_close_leaf_tab(leaf_id, idx)
    }

    /// Updates the cached geometry bounds for all leaves based on the window size,
    /// sidebar widths, and current split ratios.
    pub fn update_leaf_bounds_from_layout(
        &mut self,
        win_w: f32,
        win_h: f32,
        left_sidebar_w: f32,
        right_sidebar_w: f32,
    ) {
        let workspace_w = (win_w - left_sidebar_w - right_sidebar_w).max(100.0);
        let workspace_h = (win_h - 66.0).max(100.0);
        let leaf_ids: Vec<LeafId> = self.leaves.keys().copied().collect();
        for lid in leaf_ids {
            let w_ratio = self.workspace.horizontal_width_ratio_of_leaf(lid);
            let h_ratio = self.workspace.vertical_height_ratio_of_leaf(lid);
            let w = (workspace_w * w_ratio).max(60.0);
            let h = (workspace_h * h_ratio).max(50.0);
            if let Some(leaf) = self.leaves.get_mut(&lid) {
                leaf.set_bounds(w, h);
            }
        }
    }

    /// Invalidates geometry and viewport caches across all leaves and views
    /// after layout structure changes (e.g. split or leaf close).
    pub fn invalidate_layout_caches(&mut self) {
        for leaf in self.leaves.values_mut() {
            leaf.bounds = None;
        }
        for view in self.views.values_mut() {
            view.measured_viewport_h = None;
        }
    }

    /// Selects a tab within a leaf by TabId, switching only pointers.
    pub fn select_tab(&mut self, leaf_id: LeafId, tab_id: TabId) -> Result<(), String> {
        let leaf = self.leaves.get_mut(&leaf_id).ok_or_else(|| "Unknown leaf".to_string())?;
        let pos = leaf.tabs.iter().position(|t| t.id == tab_id).ok_or_else(|| "Unknown tab".to_string())?;
        leaf.active_tab_index = pos;
        self.workspace.active_leaf = leaf_id;
        Ok(())
    }

    pub fn close_leaf_tab(&mut self, leaf_id: LeafId, tab_index: usize) -> Option<PathBuf> {
        if let Ok((_tab_id, req)) = self.request_close_leaf_tab(leaf_id, tab_index) {
            if let crate::workspace::CloseRequest::NeedsEffects(effects) = req {
                for eff in effects {
                    if let Err(e) = self.run_close_effect(&eff) {
                        self.status_text = format!("Close cancelled: {}", e);
                        return None;
                    }
                }
            }
        }
        self.close_leaf_tab_direct(leaf_id, tab_index)
    }

    pub fn close_leaf_tab_direct(&mut self, leaf_id: LeafId, tab_index: usize) -> Option<PathBuf> {
        let (removed_tab, leaf_is_empty) = if let Some(leaf) = self.leaves.get_mut(&leaf_id) {
            if tab_index < leaf.tabs.len() {
                let tab = leaf.tabs.remove(tab_index);
                self.views.remove(&tab.id);
                let empty = leaf.tabs.is_empty();
                if !empty {
                    if tab_index < leaf.active_tab_index {
                        leaf.active_tab_index -= 1;
                    } else if leaf.active_tab_index >= leaf.tabs.len() {
                        leaf.active_tab_index = leaf.tabs.len() - 1;
                    }
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
                    let doc_path = self.documents.remove(&doc_id).map(|d| d.path)
                        .or_else(|| self.doc_meta.remove(&doc_id).map(|m| m.path));
                    self.doc_meta.remove(&doc_id);
                    if let Some(doc_path) = doc_path {
                        let norm = Self::normalize_path(&doc_path);
                        self.by_path.remove(&norm);
                        self.open_tabs.retain(|p| p != &doc_path);
                        Some(doc_path)
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
                self.invalidate_layout_caches();
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
        self.attach_fs_watcher();
        // Search worker follows the vault: relative-path base and body cache.
        self.search_worker.reset(Some(path.clone()));
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
        let vault_bm = path.join(".vault").join("bookmarks.json");
        let obs_bm = path.join(".obsidian").join("bookmarks.json");
        let bm_path = if vault_bm.exists() {
            Some(vault_bm)
        } else if obs_bm.exists() {
            Some(obs_bm)
        } else {
            None
        };
        if let Some(p) = bm_path {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(data) = serde_json::from_str::<crate::views::sidebar::bookmarks::BookmarksData>(&content) {
                    self.bookmarks = data;
                }
            }
        }

        let plugin_rf = path.join(".vault").join("plugins").join("recent-files-obsidian").join("data.json");
        let vault_rf = path.join(".vault").join("recent-files.json");
        let rf_path = if plugin_rf.exists() {
            Some(plugin_rf)
        } else if vault_rf.exists() {
            Some(vault_rf)
        } else {
            None
        };
        if let Some(p) = rf_path {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(data) = serde_json::from_str::<crate::views::sidebar::recent_files::RecentFilesData>(&content) {
                    self.recent_files = data;
                }
            }
        }
        self.status_text = format!("Workspace: {}", self.workspace_name);

        self.events.emit_workspace(&crate::events::WorkspaceEvent::VaultOpen {
            path: path.to_string_lossy().to_string(),
        });
        if save {
            self.save_session_state();
        }
        self.publish_snapshot();
    }

    /// Starts or retargets the filesystem watcher for the current workspace
    /// root. Called on workspace open/switch; when the open fails or the
    /// workspace is absent, the watcher is simply left pointing at the old
    /// root until the next successful open.
    fn attach_fs_watcher(&mut self) {
        let Some(root) = self.workspace_path.clone() else {
            return;
        };
        if self.fs_watcher.watched_root().as_deref() == Some(root.as_path()) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        match self.fs_watcher.watch(&root, tx) {
            Ok(()) => self.fs_events_rx = Some(rx),
            Err(error) => {
                // Without a watcher the app still works; deletion tracking
                // falls back to nothing instead of the old 150 ms poll.
                self.fs_events_rx = None;
                self.status_text = format!("File watching unavailable: {error}");
            }
        }
    }

    /// Stops the filesystem watcher (workspace closed).
    pub fn detach_fs_watcher(&mut self) {
        self.fs_watcher.stop();
        self.fs_events_rx = None;
    }

    /// Applies a tree walked off the UI thread (see `AppView::reload_file_tree_async`).
    /// The only mutation is the assignment plus the status text.
    pub fn apply_file_tree(&mut self, tree: Vec<FileNode>) {
        self.file_tree = tree;
        self.status_text = format!("Vault: {}", self.workspace_name);
        self.sync_search_worker_files();
    }

    /// Pushes the current scan list to the search worker (up to
    /// [`MAX_SCAN_FILES`] paths, sidebar-tree order). Cheap enough to send
    /// on every tree application; the worker swaps its list wholesale.
    fn sync_search_worker_files(&mut self) {
        let files = crate::views::goto_search::collect_goto_files(
            &self.file_tree,
            crate::views::goto_search::MAX_SCAN_FILES,
        );
        self.search_worker.set_files(files);
    }

    /// Rescan reaction: forget the cached vault file list so the next
    /// snapshot rebuilds it. The tree itself is refreshed by
    /// `AppView::reload_file_tree_async`, which walks the vault on a
    /// background thread — a synchronous `reload_file_tree` on the UI thread
    /// stalled a frame on large vaults.
    pub fn invalidate_markdown_cache_and_tree(&mut self) {
        if let Some(vault) = &self.vault {
            vault.invalidate_markdown_cache();
        }
    }

    /// After a rescan, feeds every open document path into the deletion
    /// debounce: paths that truly disappeared will confirm as `Deleted` on a
    /// later tick (after the debounce window), present ones cancel out.
    pub fn recheck_open_paths_after_rescan(&mut self) {
        for path in self.documents.values().map(|d| d.path.clone()).collect::<Vec<_>>() {
            if !path.exists() {
                self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
            }
        }
        for path in self.doc_meta.values().map(|m| m.path.clone()).collect::<Vec<_>>() {
            if !self.documents.values().any(|d| d.path == path) && !path.exists() {
                self.pending_file_deletions.entry(path).or_insert_with(Instant::now);
            }
        }
    }

    /// Публикует актуальный иммутабельный снимок состояния vault и активного файла
    pub fn publish_snapshot(&self) {
        let active_doc = self.active_document();
        let active = active_doc.as_ref().and_then(|doc| {
            self.vault.as_ref().and_then(|v| {
                crate::vault::FileInfo::from_path(&v.root, &doc.path)
            })
        });
        let files: Arc<[crate::vault::FileInfo]> = self.vault
            .as_ref()
            .map(|v| v.list_markdown_files())
            .unwrap_or_default();

        let version = active_doc.as_ref().map(|d| d.version).unwrap_or(0);
        let language = crate::i18n::LocaleInfo::normalize(&self.settings.get_config().ui.language).to_string();
        let commands: std::sync::Arc<[CommandInfo]> = self
            .commands
            .all()
            .into_iter()
            .map(|c| CommandInfo { id: c.id, name: c.name, hotkey: c.hotkey, owner: c.owner })
            .collect();
        let active_doc_snapshot = active_doc.zip(self.active_view()).map(|(doc, view)| {
            let head = view.line_col(doc);
            let anchor = view.selection_anchor.map(|a| {
                let anchor_line = doc.line_of_byte(a.min(doc.len()));
                let anchor_start = doc.line_start_byte(anchor_line).min(a);
                let anchor_ch = doc.slice_to_string(anchor_start..a).chars().count();
                (anchor_line, anchor_ch)
            });
            ActiveDocSnapshot { rope: doc.rope().clone(), cursor: head, anchor }
        });
        self.snapshot.store(Arc::new(Snapshot {
            version,
            language,
            active_file: active,
            files,
            active_doc: active_doc_snapshot,
            commands,
            plugins: std::sync::Arc::from(self.snapshot_plugins.load().as_ref().to_vec()),
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
            self.sync_search_worker_files();
            return;
        };
        self.file_tree = Self::build_tree(ws);
        // Every UI path that mutates the vault (create/rename/delete from the
        // sidebar, plugin operations) converges here; the plugin file list must
        // follow or plugins keep seeing the pre-mutation set.
        if let Some(vault) = &self.vault {
            vault.invalidate_markdown_cache();
        }
        self.reload_files();
        self.sync_search_worker_files();
        self.status_text = format!("Vault: {}", self.workspace_name);
        self.publish_snapshot();
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
        if let Some((view, doc)) = self.active_view_and_doc_mut() {
            view.cursor_offset = doc.len();
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
        if let Some((view, doc)) = self.active_view_and_doc_mut() {
            view.cursor_offset = doc.len();
        }
        Some(candidate)
    }

    /// Opens (activates) an empty tab: no file on disk, the New Tab page
    /// is shown instead of an editor. The sentinel is added to `open_tabs`
    /// like a regular tab so it renders in the tab strip.
    pub fn open_empty_tab(&mut self) -> PathBuf {
        // Save current view's cursor and selection before switching
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((path, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(path.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(path.clone(), cursor);
            self.session.note_states.insert(path, entry.clone());
        }
        let path = empty_tab_path();
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
        self.active_empty_tab = Some(path.clone());
        self.session.active_note = Some(path.clone());
        self.save_session_state();
        path
    }

    /// Activates an already open empty tab.
    pub fn activate_empty_tab(&mut self, path: PathBuf) {
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((p, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(p.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(p.clone(), cursor);
            self.session.note_states.insert(p, entry.clone());
        }
        self.push_nav_history(path.clone());
        if !self.open_tabs.iter().any(|p| p == &path) {
            self.open_tabs.push(path.clone());
        }
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
            if self.session.active_note.as_deref() == Some(path) {
                self.activate_neighbor_after_close(pos);
            } else if self.active_empty_tab.as_deref() == Some(path) {
                self.active_empty_tab = None;
                self.activate_neighbor_after_close(pos);
            }
        } else if self.session.active_note.as_deref() == Some(path) {
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
                let doc_path = self.documents.remove(&doc_id).map(|d| d.path)
                    .or_else(|| self.doc_meta.remove(&doc_id).map(|m| m.path));
                self.doc_meta.remove(&doc_id);
                if let Some(doc_path) = doc_path {
                    let norm = Self::normalize_path(&doc_path);
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
                    self.active_empty_tab = Some(target.clone());
                    self.session.active_note = Some(target);
                } else {
                    self.active_empty_tab = None;
                    self.open_note(target);
                }
            }
            None => {
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
        let Some(doc) = self.active_document() else {
            return Vec::new();
        };
        if let Some(ref ws) = self.workspace_path {
            if let Ok(rel) = doc.path.strip_prefix(ws) {
                let mut parts = Vec::new();
                parts.push(self.workspace_name.clone());
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
        vec![doc.title.clone()]
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

    /// Records an opened note in the recent files list.
    pub fn record_recent_file(&mut self, path: &Path) {
        if is_empty_tab_path(path) || path.to_string_lossy() == "Welcome.md" {
            return;
        }
        let (rel_path, basename) = if let Some(ref ws) = self.workspace_path {
            let rel = path.strip_prefix(ws).unwrap_or(path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_str.clone());
            (rel_str, stem)
        } else {
            let p_str = path.to_string_lossy().replace('\\', "/");
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p_str.clone());
            (p_str, stem)
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.recent_files.add_file(&rel_path, &basename, now);
    }

    pub fn open_note_internal_at(&mut self, path: PathBuf, at_front: bool) {
        // Empty-tab sentinels carry no file: just activate the New Tab page.
        if is_empty_tab_path(&path) {
            self.activate_empty_tab(path);
            return;
        }
        self.record_recent_file(&path);
        self.reveal_path_in_tree(&path);

        // Save current view's cursor, selection before switching
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((p, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(p.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.cursor_positions.insert(p.clone(), cursor);
            self.session.note_states.insert(p, entry.clone());
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

        let doc_id = if let Some(doc_id) = self.find_document_by_path(&path) {
            self.status_text = format!("Switched: {}", path.file_name().unwrap_or_default().to_string_lossy());
            self.ensure_document_loaded(doc_id);
            doc_id
        } else {
            self.status_text = format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy());
            if path.to_string_lossy() == "Welcome.md" {
                self.get_or_open_document(&path, title.clone(), DEFAULT_WELCOME_NOTE.to_owned())
            } else {
                self.get_or_open_document_from_file(&path, title.clone())
            }
        };

        self.ensure_document_loaded(doc_id);
        if let Some(doc) = self.documents.get(&doc_id) {
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
            let active_tab_id = self.leaves.get(&self.workspace.active_leaf).and_then(|leaf| leaf.active_tab().map(|t| t.id));
            if let Some(tab_id) = active_tab_id {
                let view = self.views.entry(tab_id).or_insert_with(|| crate::workspace::EditorViewState::new(tab_id, doc_id));

                // Restore saved settings (cursor and selection) for this note
                if let Some(settings) = self.note_states.get(&path) {
                    let mut offset = settings.cursor_offset.min(doc.len());
                    while offset > 0 && !doc.is_char_boundary(offset) {
                        offset -= 1;
                    }
                    view.cursor_offset = offset;

                    view.selection_anchor = settings.selection_anchor.map(|anchor| {
                        let mut a = anchor.min(doc.len());
                        while a > 0 && !doc.is_char_boundary(a) {
                            a -= 1;
                        }
                        a
                    });
                }
            }
            self.active_empty_tab = None;
            self.session.active_note = Some(path.clone());
            self.save_session_state();

            let rel = self.vault.as_ref()
                .and_then(|v| path.strip_prefix(&v.root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            self.events.emit_workspace(&crate::events::WorkspaceEvent::FileOpen { path: rel });
            self.publish_snapshot();
        }
        self.evict_inactive_documents_if_needed();
    }

    pub fn snapshot_active_note_settings(&mut self, scroll_top: f32, edit_mode: bool, word_wrap: bool, source_mode: bool) {
        if let (Some(view), Some(doc)) = (self.active_view(), self.active_document()) {
            let path = doc.path.clone();
            let cursor_offset = view.cursor_offset;
            let selection_anchor = view.selection_anchor;
            let settings = NoteSettings {
                cursor_offset,
                selection_anchor,
                scroll_top,
                edit_mode,
                word_wrap,
                source_mode,
            };
            self.note_states.insert(path.clone(), settings.clone());
            self.session.note_states.insert(path.clone(), settings);
            self.session.cursor_positions.insert(path, cursor_offset);
        }
    }

    pub fn save_document(&mut self, doc_id: DocumentId) -> bool {
        if let Some(doc) = self.documents.get_mut(&doc_id) {
            if !doc.is_dirty {
                return true;
            }
            let expected_hash = doc.disk_hash.clone();
            let version = doc.version;
            match crate::save_manager::safe_write_rope_checked(&doc.path, doc.rope(), expected_hash.as_deref(), version) {
                crate::save_manager::CheckedWrite::Written(hash) => {
                    doc.disk_hash = Some(hash);
                    doc.disk_stamp = crate::save_manager::file_stamp(&doc.path);
                    doc.mark_clean();
                    if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                        meta.is_dirty = false;
                    }
                    // Search index follows the save (stage B5): reindex just
                    // this file; the worker skips paths not indexed yet.
                    self.search_worker.update_paths(vec![doc.path.clone()]);
                    self.status_text = format!("Saved: {}", doc.title);
                }
                crate::save_manager::CheckedWrite::Conflict { copy, disk_text } => {
                    // Two-sided conflict protocol: our version is preserved in
                    // `copy`, the buffer adopts the external content. No data
                    // loss on either side.
                    doc.reload(disk_text);
                    for view in self.views.values_mut() {
                        if view.doc_id == doc_id {
                            view.visual_cache_dirty = true;
                        }
                    }
                    self.status_text = format!(
                        "Файл изменён извне; ваша версия сохранена в {}",
                        copy.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
                crate::save_manager::CheckedWrite::Failed(err) => {
                    self.status_text = format!("Save error: {}", err);
                    return false;
                }
            }
            let rel_path = self.vault.as_ref()
                .and_then(|v| {
                    self.documents.get(&doc_id).map(|d| d.path.strip_prefix(&v.root).ok()).unwrap_or(None)
                })
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .or_else(|| {
                    self.documents.get(&doc_id).map(|d| d.path.to_string_lossy().into_owned())
                });
            if let Some(rel_path) = rel_path {
                self.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
            }
            self.publish_snapshot();
            return true;
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
        if let Some(doc_id) = self.active_doc_id() {
            self.save_document(doc_id);
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
                                        let doc_id = DocumentId::new();
                                        state.by_path.insert(norm, doc_id);
                                        state.doc_meta.insert(
                                            doc_id,
                                            DocumentMetadata {
                                                id: doc_id,
                                                path: path.clone(),
                                                title: title.clone(),
                                                is_dirty: false,
                                                last_accessed: Instant::now(),
                                            },
                                        );
                                        doc_id
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

        // Ensure active tabs in all leaves are loaded
        let visible_docs: Vec<DocumentId> = self.leaves.values()
            .filter_map(|l| l.active_tab())
            .filter_map(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => Some(*id),
                _ => None,
            })
            .collect();
        for id in visible_docs {
            self.ensure_document_loaded(id);
        }
        self.evict_inactive_documents_if_needed();
    }

    pub fn save_session_state(&mut self) {
        self.queue_session_state_save(true);
    }

    /// Serializes the session off the UI thread.
    ///
    /// Every save serializes two pretty-JSON files (session + workspace), and
    /// their size grows with `note_states` — serializing them on the UI thread
    /// stalled a frame after every typing pause. The mutation of the session
    /// fields stays synchronous; serialization and the two file writes run on
    /// the background executor.
    ///
    /// `sync` also runs the writes inline — used on the exit path, where the
    /// process may not outlive a spawned background job.
    pub fn queue_session_state_save(&mut self, sync: bool) {
        let active_info = self.active_view().and_then(|v| {
            self.active_document().map(|d| (d.path.clone(), v.cursor_offset, v.selection_anchor))
        });
        if let Some((path, cursor, anchor)) = active_info {
            let entry = self.note_states.entry(path.clone()).or_default();
            entry.cursor_offset = cursor;
            entry.selection_anchor = anchor;
            self.session.note_states.insert(path.clone(), entry.clone());
            self.session.cursor_positions.insert(path.clone(), cursor);
            self.session.active_note = Some(path);
            self.active_empty_tab = None;
        } else if let Some(ref empty) = self.active_empty_tab.clone() {
            self.session.active_note = Some(empty.clone());
        }
        self.session.open_tabs = self.open_tabs.clone();
        self.session.workspace_path = self.workspace_path.clone();
        self.session.expanded_folders = self.expanded_folders.clone();
        self.session.known_vaults = self.known_vaults.clone();

        let dir = app_data_dir();
        let session = self.session.clone();
        let ws_v2 = self.export_workspace_session_v2();

        let write_all = move || {
            session.save(&dir);
            let ws_v2_path = dir.join("workspace-v2.json");
            let _ = ws_v2.save_atomic(&ws_v2_path);
        };

        if sync {
            write_all();
            return;
        }

        // The spawned thread owns serialized inputs; nothing here touches
        // `self` or any GPUI entity, so an OS thread is safe (and cheaper than
        // routing through a UI context just to write two files). The exit path
        // uses `sync`, so a torn write from a dying process is not a concern.
        std::thread::Builder::new()
            .name("session-save".to_owned())
            .spawn(write_all)
            .expect("spawning the session-save thread cannot fail on a sane system");
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
        let opt_doc_id = self.find_document_by_path(old_path);
        if let Some(doc_id) = opt_doc_id {
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                if doc.is_dirty {
                    let _ = crate::save_manager::safe_write_rope(old_path, doc.rope());
                    doc.is_dirty = false;
                }
            }
        }

        // Rename on disk if the old file exists
        if old_path.exists() {
            std::fs::rename(old_path, &new_path)
                .map_err(|e| format!("Failed to rename file: {}", e))?;
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
        let norm_old = Self::normalize_path(old_path);
        let norm_new = Self::normalize_path(&new_path);
        let opt_doc_id = self.by_path.remove(old_path).or_else(|| self.by_path.remove(&norm_old));
        if let Some(doc_id) = opt_doc_id {
            self.by_path.insert(norm_new, doc_id);
            if let Some(doc) = self.documents.get_mut(&doc_id) {
                doc.path = new_path.clone();
                doc.title = trimmed.to_string();
            }
            if let Some(meta) = self.doc_meta.get_mut(&doc_id) {
                meta.path = new_path.clone();
                meta.title = trimmed.to_string();
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
    fn test_resolve_app_data_dir_migration() {
        let temp_base = std::env::temp_dir().join(format!("vibe_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_base).unwrap();

        // 1. Clean path (neither exists): resolves to Vibecodium
        let resolved = resolve_app_data_dir(&temp_base);
        assert_eq!(resolved, temp_base.join("Vibecodium"));

        // 2. Legacy com.dmitriy.aquilum-app exists: should auto-migrate (rename) to Vibecodium
        let legacy_dir = temp_base.join("com.dmitriy.aquilum-app");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("test.txt"), "hello").unwrap();

        let migrated = resolve_app_data_dir(&temp_base);
        assert_eq!(migrated, temp_base.join("Vibecodium"));
        assert!(migrated.exists());
        assert!(migrated.join("test.txt").exists());
        assert!(!legacy_dir.exists());

        // 3. Existing Vibecodium returns directly
        let second_call = resolve_app_data_dir(&temp_base);
        assert_eq!(second_call, temp_base.join("Vibecodium"));

        let _ = std::fs::remove_dir_all(temp_base);
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
        saved.find_replace_open = true;
        saved.known_vaults = vec![PathBuf::from("D:/vault-a"), PathBuf::from("D:/vault-b")];
        saved.save(&dir);

        let loaded = SessionState::load(&dir);
        assert_eq!(loaded.sidebar_width, 320.0);
        assert_eq!(
            loaded.sidebar_mode,
            crate::views::sidebar::SidebarMode::Search
        );
        assert!(loaded.find_replace_open);
        assert_eq!(
            loaded.known_vaults,
            vec![PathBuf::from("D:/vault-a"), PathBuf::from("D:/vault-b")]
        );

        // Session files written before these keys existed still load.
        std::fs::write(dir.join("session-state.json"), "{}").unwrap();
        let legacy = SessionState::load(&dir);
        assert_eq!(legacy.sidebar_width, 0.0);
        assert!(!legacy.find_replace_open);
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

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Initial cache build
        view.ensure_visual_cache(&mut doc, true, 80, 0);
        assert_eq!(view.window.lines.len(), 5001); // 5000 lines + trailing empty
        assert_eq!(doc.line_index.lines(), 5001);
        let initial_lines = Arc::clone(&view.window.lines);

        let t0 = std::time::Instant::now();
        const MOVES: usize = 1000;
        for _ in 0..MOVES {
            view.move_cursor_right(&doc, false);
            view.ensure_visual_cache(&mut doc, true, 80, 0);
        }
        for _ in 0..MOVES {
            view.move_cursor_left(&doc, false);
            view.ensure_visual_cache(&mut doc, true, 80, 0);
        }
        let elapsed = t0.elapsed();
        let per_move_us = elapsed.as_micros() as f64 / (MOVES * 2) as f64;

        println!(
            "2000 cursor moves on 5,000 lines: total {:?}, {:.2} µs/move",
            elapsed, per_move_us
        );

        // CRITICAL REQUIREMENTS:
        // 1. Cached lines Arc must NOT have been reallocated or rebuilt during cursor movements:
        assert!(Arc::ptr_eq(&view.window.lines, &initial_lines), "Cache must not be rebuilt during cursor moves");
        assert!(!view.visual_cache_dirty, "Cache must remain clean");
        // 2. Must be << 200 µs (0.2 ms)
        assert!(per_move_us < 200.0, "Per move latency should be under 200 µs, got {:.2} µs", per_move_us);
    }

    #[test]
    fn test_opened_note_incremental_text_edit() {
        let text = "# Title\n\nSome paragraph text here.\n".to_string();
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("edit.md"),
            "edit".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        view.ensure_visual_cache(&mut doc, true, 80, 0);
        assert!(!view.visual_cache_dirty);

        // Move cursor to inside paragraph: line 2 ("Some paragraph text here.")
        // "# Title\n\n" is 9 bytes. "Some " is 5 bytes. Offset 14 is right after "Some ".
        view.cursor_offset = 14;

        // Insert text incrementally
        view.insert_text(&mut doc, "awesome ");
        assert_eq!(doc.content(), "# Title\n\nSome awesome paragraph text here.\n");
        // Same-line, pipe-free insertion updates the window line in place:
        // no full rebuild is scheduled and the cache stays fresh.
        assert!(!view.visual_cache_dirty, "in-place edit must keep the cache fresh");
        assert_eq!(view.window.lines[2], "Some awesome paragraph text here.");

        // Rebuilding cache should reflect the new content
        view.ensure_visual_cache(&mut doc, true, 80, 0);
        assert!(!view.visual_cache_dirty);
        assert_eq!(view.window.lines[2], "Some awesome paragraph text here.");

        // Undo
        view.undo(&mut doc);
        assert_eq!(doc.content(), "# Title\n\nSome paragraph text here.\n");
        assert!(view.visual_cache_dirty);

        view.ensure_visual_cache(&mut doc, true, 80, 0);
        assert_eq!(view.window.lines[2], "Some paragraph text here.");

        // Redo
        view.redo(&mut doc);
        assert_eq!(doc.content(), "# Title\n\nSome awesome paragraph text here.\n");
    }

    #[test]
    fn test_sequential_edits_undo_per_character() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "Initial line.\n".to_string(),
            0,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        // 1. Sequential typing: "Hello " letter by letter
        for ch in ["H", "e", "l", "l", "o", " "] {
            view.insert_text(&mut doc, ch);
        }
        assert_eq!(doc.content(), "Hello Initial line.\n");
        // No coalescing: every keystroke is its own step (AkelPad detailed undo)
        assert_eq!(doc.history.undo_len(), 6);

        // 2. Undo letter by letter: each undo removes one character
        for expected in ["HelloInitial", "HellInitial", "HelInitial", "HeInitial", "HInitial", "Initial"].iter() {
            view.undo(&mut doc);
            assert_eq!(doc.content(), format!("{expected} line.\n"));
        }
        assert_eq!(doc.content(), "Initial line.\n");
        assert_eq!(view.cursor_offset, 0);

        // 3. Redo letter by letter
        for _ in 0..6 {
            view.redo(&mut doc);
        }
        assert_eq!(doc.content(), "Hello Initial line.\n");
        assert_eq!(view.cursor_offset, 6);

        // 4. Backspace 3 times: each erases exactly the character typed last,
        // so the mutual-exclusion rule annuls each (insert, delete) pair.
        view.delete_backwards(&mut doc);
        view.delete_backwards(&mut doc);
        view.delete_backwards(&mut doc);
        assert_eq!(doc.content(), "HelInitial line.\n");
        assert_eq!(view.cursor_offset, 3);
        assert_eq!(doc.history.undo_len(), 3, "three typed-and-erased pairs are annulled");

        // 5. Undo keeps walking the remaining typing steps character by character
        for expected in ["HeInitial line.\n", "HInitial line.\n", "Initial line.\n"].iter() {
            view.undo(&mut doc);
            assert_eq!(doc.content(), *expected);
        }
        assert_eq!(doc.content(), "Initial line.\n");

        // 6. Selection replace: select "Initial" and retype over it
        view.cursor_offset = 7;
        view.selection_anchor = Some(0);
        view.replace_selection(&mut doc, "Rewritten");
        assert_eq!(doc.content(), "Rewritten line.\n");

        view.undo(&mut doc);
        assert_eq!(doc.content(), "Initial line.\n");
        // Selection is restored: the replaced range "Initial" is selected again
        assert_eq!(view.selection_anchor, Some(0));
        assert_eq!(view.cursor_offset, 7);
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

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("prop.md"),
            "prop".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        view.ensure_visual_cache(&mut doc, true, 40, 0); // wrap at 40 cols
        let visual_rows = Arc::clone(&view.window.visual_rows);

        // Test every single possible cursor offset in the document!
        for offset in 1..=doc.content().len() {
            view.cursor_offset = offset;
            let (cursor_line, cursor_col) = view.line_col(&doc);
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
            let new_offset = view.offset_for_line_col(&doc, target_v.line_idx, target_char);

            view.cursor_offset = new_offset;
            let (new_line, new_col) = view.line_col(&doc);
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

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("prop_down.md"),
            "prop_down".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        view.ensure_visual_cache(&mut doc, true, 40, 0); // wrap at 40 cols
        let visual_rows = Arc::clone(&view.window.visual_rows);

        for offset in 0..doc.content().len() {
            view.cursor_offset = offset;
            let (cursor_line, cursor_col) = view.line_col(&doc);
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
            let new_offset = view.offset_for_line_col(&doc, target_v.line_idx, target_char);

            view.cursor_offset = new_offset;
            let (new_line, new_col) = view.line_col(&doc);
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

    /// A note far larger than one window must not make the scrollbar jitter
    /// as the window travels.
    ///
    /// `total_height` mixes an exact window with an O(1) estimate on either
    /// side. A naive rebuild recomputes the margins from the frozen estimate
    /// and lets the exact window height leak into the total — the total
    /// wobbles by the real-vs-estimate difference of whichever span the
    /// window just swapped, and the scrollbar thumb ticks with it on every
    /// move. A pure window move therefore *anchors*: mid-document, the total
    /// is preserved bit-for-bit and the swapped-span difference is absorbed
    /// into the margin estimate. Two legal exceptions may move the total,
    /// both one-shot corrections: a document edge (a margin is legitimately
    /// zero and the total settles onto the true height) and an anchor-chain
    /// break (the real heights accumulated by the chain outgrew the
    /// estimate-based total, so both margins fall back to the estimate).
    /// Checked on a document whose lines genuinely differ in height: between
    /// corrections the total must not move at all, corrections must be rare
    /// and bounded, and the total must never wobble back and forth.
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
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(doc_id, PathBuf::from("stable.md"), "stable".to_string(), text, 1);
        let mut view = EditorViewState::new(tab_id, doc_id);

        // The first build freezes the estimate basis; it sits at the top.
        view.ensure_visual_cache(&mut doc, true, 80, 0);
        assert_eq!(view.above_lines, 0, "the first window sits at the top");
        assert!(view.below_lines > 0, "the note must be windowed");
        let initial = view.total_height();
        assert!(initial > 0.0);
        let mut reference = initial;

        let mut worst = 0.0f64;
        let mut corrections = 0usize;
        let mut prev_total = initial;
        for anchor in (0..doc.line_index.lines()).step_by(997) {
            view.ensure_visual_cache(&mut doc, true, 80, anchor);
            assert!(
                view.window.covers_line(anchor),
                "the window must hold its anchor {anchor}"
            );
            let total = view.total_height();
            let at_edge = view.above_lines == 0 || view.below_lines == 0;
            if total != prev_total {
                corrections += 1;
                // A correction must be a bounded step, not a wobble.
                assert!(
                    (total - prev_total).abs() / initial < 0.05,
                    "correction at anchor {anchor} moved the total by {:.2}%",
                    (total - prev_total).abs() / initial * 100.0
                );
                reference = total;
            }
            prev_total = total;
            if !at_edge {
                assert_eq!(total, reference, "mid-document move at anchor {anchor} moved the total");
            }
            worst = worst.max((total - initial).abs() / initial);
        }
        assert!(
            corrections <= 3,
            "the total was corrected {corrections} times across one traversal"
        );
        assert!(
            worst < 0.05,
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
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(doc_id, PathBuf::from("caret.md"), "caret".to_string(), text, 1);
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.ensure_visual_cache(&mut doc, true, 80, 12_000);

        let rows = Arc::clone(&view.window.visual_rows);
        let start = view.window.start_line;
        let end = view.window.end_line;
        assert!(start > 0 && end < doc.line_index.lines(), "must be windowed");

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

        view.ensure_visual_cache(&mut doc, true, 80, outside);
        assert!(view.window.covers_line(outside));
        let rows = Arc::clone(&view.window.visual_rows);
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
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(doc_id, PathBuf::from("code.md"), "code".to_string(), text, 1);
        let mut view = EditorViewState::new(tab_id, doc_id);

        view.ensure_visual_cache(&mut doc, true, 0, 100_000);
        assert!(view.above_lines > 0, "the note must be windowed");

        let start = view.window.start_line;
        assert!(start > 2, "the window must start below the opening fence");
        assert!(
            doc.code_checkpoints.scanned_lines() > start,
            "the checkpoints must reach the window's first line"
        );

        // The state the window reports for its own first line is the checkpoint
        // state, not the default of a fresh scan.
        let state = view
            .window
            .code_state(start)
            .expect("the window's first line must be inside the code block");
        assert_eq!(state.kind, CodeBlockKind::InsideCode);
        assert_eq!(state.block_id, 0);

        // And so is the line of the first row actually painted.
        let first_line = view.window.visual_rows.first().unwrap().line_idx;
        assert_eq!(
            view.window.code_state(first_line).map(|s| s.kind),
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
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(doc_id, PathBuf::from("cp.md"), "cp".to_string(), text, 1);
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.ensure_visual_cache(&mut doc, true, 0, 15_000);
        let start = view.window.start_line;
        let scanned = doc.code_checkpoints.scanned_lines();
        assert!(
            scanned >= start + 1,
            "the scan must reach the window's first line"
        );

        // Type a character deep inside the block.
        let edit_line = 10_000usize;
        view.cursor_offset = doc.line_start_byte(edit_line) + 3;
        view.insert_text(&mut doc, "X");

        let kept =
            (edit_line / crate::code_scan::CHECKPOINT_LINES) * crate::code_scan::CHECKPOINT_LINES;
        assert_eq!(
            doc.code_checkpoints.scanned_lines(),
            kept,
            "the checkpoints must be truncated exactly at the edit's checkpoint"
        );
        assert!(kept > 0 && kept < scanned, "the prefix must survive");

        // And the window still resolves its code state after the edit.
        view.ensure_visual_cache(&mut doc, true, 0, 15_000);
        let start = view.window.start_line;
        assert_eq!(
            view.window.code_state(start).map(|s| s.kind),
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
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "# Заметка\n".to_string(),
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.ensure_visual_cache(&mut doc, true, 80, 0);

        // Move cursor to the end of the first line
        view.cursor_offset = doc.content().len();

        let russian_text = "Привет мир! Это тестовая строка на русском языке.";
        for ch in russian_text.chars() {
            let mut s = String::new();
            s.push(ch);
            view.insert_text(&mut doc, &s);
            // Same-line typing updates the window line in place: the cached
            // line must already match the document without a rebuild.
            assert!(!view.visual_cache_dirty);
            assert_eq!(view.window.lines[1], doc.line_text(1));

            // line_col() must stay consistent with the document either way
            let (line, col) = view.line_col(&doc);
            assert_eq!(line, 1);
            // col should match number of characters on line 1
            let expected_col = doc.content().split('\n').nth(1).unwrap().chars().count();
            assert_eq!(col, expected_col);
        }

        // Backspace test with multi-byte characters
        for _ in 0..10 {
            view.delete_backwards(&mut doc);
            let (line, col) = view.line_col(&doc);
            assert_eq!(line, 1);
            let expected_col = doc.content().split('\n').nth(1).unwrap().chars().count();
            assert_eq!(col, expected_col);
        }

        // Move cursor left and delete forwards
        view.cursor_offset = doc.content().len().saturating_sub(6); // inside Cyrillic text
        while !doc.content().is_char_boundary(view.cursor_offset) {
            view.cursor_offset -= 1;
        }
        view.delete_forwards(&mut doc);
        let _ = view.line_col(&doc);

        // Selection across multi-byte characters
        view.selection_anchor = Some(2); // inside "# Заметка"
        let range = view.selection_range(doc.content().len());
        assert!(range.is_some());
        let range = range.unwrap();
        assert!(doc.content().is_char_boundary(range.start));
        assert!(doc.content().is_char_boundary(range.end));
        let selected = view.selected_text(&doc);
        assert!(selected.is_some());

        // Test with exact template from create_note_in_folder: "# Без названия\n\n"
        let doc_id2 = DocumentId::new();
        let tab_id2 = TabId::new();
        let mut new_doc = DocumentBuffer::new(
            doc_id2,
            PathBuf::from("Без названия.md"),
            "Без названия".to_string(),
            "# Без названия\n\n".to_string(),
            1,
        );
        let mut new_view = EditorViewState::new(tab_id2, doc_id2);
        new_view.ensure_visual_cache(&mut new_doc, true, 80, 0);
        new_view.cursor_offset = new_doc.content().len();

        // Type text containing 'ё' and other Cyrillic characters
        let sample = "Привет, моё солнышко! Ещё одна заметка.";
        for ch in sample.chars() {
            let mut s = String::new();
            s.push(ch);
            new_view.insert_text(&mut new_doc, &s);
            let _ = new_view.line_col(&new_doc);
        }

        // Backspace every character one by one
        for _ in 0..sample.chars().count() {
            new_view.delete_backwards(&mut new_doc);
            let _ = new_view.line_col(&new_doc);
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
    fn test_document_buffer_version_increments() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "initial".to_string(),
            0,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);
        assert_eq!(doc.version, 0);

        view.cursor_offset = doc.content().len();
        view.insert_text(&mut doc, " text");
        assert_eq!(doc.version, 1);

        view.delete_backwards(&mut doc);
        assert_eq!(doc.version, 2);

        view.undo(&mut doc);
        assert_eq!(doc.version, 3);

        view.redo(&mut doc);
        assert_eq!(doc.version, 4);
    }

    #[test]
    fn test_typed_then_erased_character_leaves_no_history() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "ab".to_string(),
            0,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.cursor_offset = 2;

        // Type one character and erase it with backspace right away.
        view.insert_text(&mut doc, "c");
        assert_eq!(doc.content(), "abc");
        view.delete_backwards(&mut doc);
        assert_eq!(doc.content(), "ab");

        assert_eq!(doc.history.undo_len(), 0, "the pair is annulled");
        assert!(view.undo(&mut doc).is_none(), "undo has nothing to do");
        assert_eq!(doc.content(), "ab");
        assert!(view.redo(&mut doc).is_none(), "redo is invalidated too");

        // A deletion that does not exactly revert the last step is a real step.
        view.insert_text(&mut doc, "c");
        assert_eq!(doc.content(), "abc");
        view.cursor_offset = 0;
        view.delete_forwards(&mut doc);
        assert_eq!(doc.content(), "bc");
        view.undo(&mut doc);
        assert_eq!(doc.content(), "abc");
    }

    #[test]
    fn test_undo_and_redo_restore_selection() {
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            "one two three".to_string(),
            0,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);

        // Select "two" and retype it.
        view.cursor_offset = 7;
        view.selection_anchor = Some(4);
        view.replace_selection(&mut doc, "TW");
        assert_eq!(doc.content(), "one TW three");

        // Undo restores both the text and the selection over it.
        view.undo(&mut doc);
        assert_eq!(doc.content(), "one two three");
        assert_eq!(view.selection_anchor, Some(4));
        assert_eq!(view.cursor_offset, 7);

        // Redo restores the post-edit state as well.
        view.redo(&mut doc);
        assert_eq!(doc.content(), "one TW three");
        assert_eq!(view.selection_anchor, None);
        assert_eq!(view.cursor_offset, 6);
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
        let settings = Arc::new(crate::settings::SettingsManager::new(&app_data_dir()));

        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let default_leaf_id = LeafId::new();
        let doc = DocumentBuffer::new(
            doc_id,
            temp_dir.join("Note1.md"),
            "Note1".into(),
            "content 1".into(),
            42,
        );
        let view = EditorViewState::new(tab_id, doc_id);
        let mut leaf = LeafState::new(default_leaf_id);
        leaf.tabs.push(crate::workspace::Tab {
            id: tab_id,
            content: crate::workspace::TabContent::Document { id: doc_id },
            pinned: false,
        });
        leaf.active_tab_index = 0;

        let mut documents = HashMap::new();
        let mut by_path = HashMap::new();
        by_path.insert(doc.path.clone(), doc_id);
        documents.insert(doc_id, doc);

        let mut views = HashMap::new();
        views.insert(tab_id, view);

        let mut leaves = HashMap::new();
        leaves.insert(default_leaf_id, leaf);

        let state = AppState {
            settings: Arc::new(crate::settings::SettingsManager::new(&temp_dir)),
            workspace_path: Some(temp_dir.clone()),
            workspace_name: "test".into(),
            vault: Some(vault),
            events,
            commands,
            ribbon: Arc::new(crate::ribbon::RibbonRegistry::new()),
            status_bar: Arc::new(crate::ribbon::StatusBarRegistry::new()),
            notifications: Arc::new(crate::views::notification::NotificationRegistry::new()),
            code_block_processors: Arc::new(crate::plugins::code_block::CodeBlockProcessorRegistry::new()),
            plugin_host: Arc::new(crate::plugins::bridge::PluginHostState::new()),
            snapshot: snapshot.clone(),
            snapshot_plugins: Arc::new(arc_swap::ArcSwap::from_pointee(std::sync::Arc::from(Vec::new()))),
            file_tree: Vec::new(),
            expanded_folders: HashSet::new(),
            search_query: String::new(),
            open_tabs: Vec::new(),
            active_empty_tab: None,
            nav_history: Vec::new(),
            nav_history_index: 0,
            status_text: String::new(),
            session: SessionState::default(),
            note_states: HashMap::new(),
            known_vaults: Vec::new(),
            documents,
            by_path,
            doc_meta: HashMap::new(),
            max_inactive_docs: DEFAULT_MAX_INACTIVE_DOCS,
            max_inactive_doc_bytes: MAX_INACTIVE_DOC_BYTES,
            workspace: WorkspaceTree::new_single_leaf(default_leaf_id),
            leaves,
            views,
            fs_watcher: crate::fs_watcher::Watcher::new(),
            fs_events_rx: None,
            bookmarks: crate::views::sidebar::bookmarks::BookmarksData::default(),
            recent_files: crate::views::sidebar::recent_files::RecentFilesData::default(),
            search_worker: crate::search::worker::SearchWorker::spawn(
                None,
                settings.clone(),
            ),
            pending_file_deletions: HashMap::new(),
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
        let doc_id = DocumentId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            String::new(),
            1,
        );
        doc.reload("hello  world\nтест".to_string());
        assert_eq!(doc.text_stats(), TextStats::Exact { words: 3, chars: 17 });

        // The cache must return the same value without recomputing.
        let first = doc.stats_cache.get();
        let _ = doc.text_stats();
        assert_eq!(doc.stats_cache.get(), first);

        // Editing the text invalidates the cache.
        let mut text = doc.content().to_string();
        text.push_str(" ещё");
        doc.reload(text);
        assert_eq!(doc.text_stats(), TextStats::Exact { words: 4, chars: 21 });
    }

    #[test]
    fn text_stats_large_skips_exact_count() {
        let doc_id = DocumentId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("test.md"),
            "test".to_string(),
            String::new(),
            1,
        );
        doc.reload("a ".repeat(TEXT_STATS_EXACT_LIMIT)); // 4 MB > limit
        assert!(matches!(doc.text_stats(), TextStats::Large { .. }));
    }

    #[test]
    fn test_doc_y_of_line_monotonic_and_covers_entire_document() {
        let mut text = String::new();
        for i in 0..40_000 {
            text.push_str(&format!("line {i}\n"));
        }
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("big.md"),
            "big".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);
        view.ensure_visual_cache(&mut doc, false, 0, 20_000);

        let total_lines = doc.line_index.lines();
        assert_eq!(total_lines, 40_001);

        let y_start = view.doc_y_of_line(total_lines, 0);
        let y_before = view.doc_y_of_line(total_lines, 100);
        let y_window_start = view.doc_y_of_line(total_lines, view.window.start_line);
        let y_middle = view.doc_y_of_line(total_lines, 20_000);
        let y_window_end = view.doc_y_of_line(total_lines, view.window.end_line);
        let y_after = view.doc_y_of_line(total_lines, 35_000);
        let y_end = view.doc_y_of_line(total_lines, 40_000);

        assert_eq!(y_start, 0.0);
        assert!(y_before > y_start);
        assert!(y_window_start >= y_before);
        assert!(y_middle > y_window_start);
        assert!(y_window_end > y_middle);
        assert!(y_after > y_window_end);
        assert!(y_end > y_after);
        assert!(y_end <= view.total_height());
    }

    #[test]
    fn test_typing_in_large_file_does_not_shift_window_or_above_h() {
        let mut text = String::new();
        for i in 0..50_000 {
            text.push_str(&format!("line {i}\n"));
        }
        let doc_id = DocumentId::new();
        let tab_id = TabId::new();
        let mut doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("big.md"),
            "big".to_string(),
            text,
            1,
        );
        let mut view = EditorViewState::new(tab_id, doc_id);
        let initial_anchor = 25_000;
        view.ensure_visual_cache(&mut doc, false, 0, initial_anchor);

        let initial_start = view.window.start_line;
        let initial_above_lines = view.above_lines;
        let initial_above_h = view.above_h;

        // Position cursor at line 25,100
        let offset = view.offset_for_line_col(&doc, 25_100, 4);
        view.cursor_offset = offset;

        // Type characters repeatedly with slight anchor perturbations within slack
        for i in 0..20 {
            view.insert_text(&mut doc, "a");
            let perturbed_anchor = initial_anchor + (i % 5);
            view.ensure_visual_cache(&mut doc, false, 0, perturbed_anchor);

            assert_eq!(
                view.window.start_line, initial_start,
                "window start_line must remain rock-solid during typing"
            );
            assert_eq!(
                view.above_lines, initial_above_lines,
                "above_lines must remain rock-solid during typing"
            );
            assert_eq!(
                view.above_h, initial_above_h,
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

        // 1. Text is owned by DocumentBuffer.content
        assert!(state.active_document().is_some());
        assert_eq!(state.active_document().unwrap().content(), "sample text content");
        let doc_id = state.find_document_by_path(&note_path).unwrap();
        let doc = state.documents.get(&doc_id).unwrap();
        assert_eq!(doc.content(), "sample text content");
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
        if let Some(doc) = state.active_document_mut() {
            doc.scan_code_ahead(100);
            assert!(doc.code_checkpoints.scanned_lines() > 0);
        }
        let note1_scanned = state.active_document().unwrap().code_checkpoints.scanned_lines();

        // 2. Open Note 2
        state.open_note(note2_path.clone());
        let doc2_id = state.find_document_by_path(&note2_path).unwrap();

        // Active note is now Note 2
        assert_eq!(state.active_document().unwrap().path, note2_path);
        // Note 1 in state.documents holds its text
        let doc1 = state.documents.get(&doc1_id).unwrap();
        assert_eq!(doc1.content(), content1);
        assert_eq!(doc1.line_index.lines(), 6);
        assert_eq!(doc1.code_checkpoints.scanned_lines(), note1_scanned);
        // Note 2 in state.documents also holds its text
        let doc2 = state.documents.get(&doc2_id).unwrap();
        assert_eq!(doc2.content(), content2);

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
        // While active, doc.content() in documents holds the content
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), content);
        assert_eq!(state.active_document().unwrap().content(), content);

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

        // Active note in leaf2 holds the content, buffer in state.documents also holds the content
        assert_eq!(state.active_document().unwrap().content(), content);
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), content);

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
    fn test_two_notes_close_second_tab() {
        let ws_path = std::env::temp_dir().join(format!("aquilum_test_close2_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note1_path = ws_path.join("Note1.md");
        let note2_path = ws_path.join("Note2.md");
        std::fs::write(&note1_path, "Content 1").unwrap();
        std::fs::write(&note2_path, "Content 2").unwrap();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());
        state.open_note(note1_path.clone());
        let leaf = state.workspace.active_leaf;
        assert_eq!(state.leaves.get(&leaf).unwrap().tabs.len(), 1);
        assert_eq!(state.active_document().unwrap().path, note1_path);

        state.open_note(note2_path.clone());
        assert_eq!(state.leaves.get(&leaf).unwrap().tabs.len(), 2);
        assert_eq!(state.leaves.get(&leaf).unwrap().active_tab_index, 1);
        assert_eq!(state.active_document().unwrap().path, note2_path);

        let closed = state.close_leaf_tab(leaf, 1);
        assert_eq!(closed, Some(note2_path.clone()));
        assert_eq!(state.leaves.get(&leaf).unwrap().tabs.len(), 1);
        assert_eq!(state.leaves.get(&leaf).unwrap().active_tab_index, 0);
        assert_eq!(state.active_document().unwrap().path, note1_path);

        let doc1_id = state.find_document_by_path(&note1_path).unwrap();
        let doc1 = state.documents.get(&doc1_id).unwrap();
        println!("doc1.path: {:?}", doc1.path);
        println!("doc1.title: {:?}", doc1.title);
        println!("doc1.content: {:?}", doc1.content());

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
                    state.doc_meta.remove(&doc_id);
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
        assert!(state.active_document().is_none());
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
        if let Some(doc) = state.active_document() {
            assert_eq!(doc.path, new_path);
            assert_eq!(doc.title, "renamed");
        }

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_handle_file_modified_pull_protocol() {
        let ws_path = std::env::temp_dir().join(format!("vibe_test_pull_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("pull_doc.md");
        std::fs::write(&note_path, "disk v1").unwrap();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());
        state.open_note(note_path.clone());
        let doc_id = state.find_document_by_path(&note_path).unwrap();

        // Loading filled in the disk hash.
        assert!(state.documents.get(&doc_id).unwrap().disk_hash.is_some());

        // 1. Unchanged file: no reload, no stamp change.
        let stamp_before = state.documents.get(&doc_id).unwrap().disk_stamp;
        assert!(!state.handle_file_modified(&note_path), "unchanged file must not reload");
        assert_eq!(state.documents.get(&doc_id).unwrap().disk_stamp, stamp_before);

        // 2. External rewrite of a CLEAN doc: reload happens.
        std::fs::write(&note_path, "disk v2 (external)").unwrap();
        assert!(state.handle_file_modified(&note_path), "external change must reload");
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), "disk v2 (external)");

        // 3. Our own write echo (same content as the buffer): no reload.
        // Simulate a completed own save: hash matches the buffer content.
        let doc = state.documents.get_mut(&doc_id).unwrap();
        doc.disk_hash = Some(crate::save_manager::hash_rope(&doc.rope));
        doc.disk_stamp = crate::save_manager::file_stamp(&note_path);
        drop(doc);
        assert!(!state.handle_file_modified(&note_path), "own write echo must not reload");

        // 4. External change to a DIRTY doc: only a status, no reload.
        let doc = state.documents.get_mut(&doc_id).unwrap();
        doc.record_change_with_selection(0, String::new(), "typed ".to_string(), crate::workspace::session_core::undo::Selection::cursor_only(0), crate::workspace::session_core::undo::Selection::cursor_only(6), None);
        drop(doc);
        std::fs::write(&note_path, "disk v3 (external)").unwrap();
        assert!(!state.handle_file_modified(&note_path), "dirty buffer must not be reloaded");
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), "typed disk v2 (external)");

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_save_document_conflicts_instead_of_overwriting() {
        let ws_path = std::env::temp_dir().join(format!("vibe_test_conflict_save_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&ws_path);
        let note_path = ws_path.join("conflict_doc.md");
        std::fs::write(&note_path, "disk v1").unwrap();

        let mut state = AppState::new_empty();
        state.workspace_path = Some(ws_path.clone());
        state.open_note(note_path.clone());
        let doc_id = state.find_document_by_path(&note_path).unwrap();

        // The user types while another program rewrites the file.
        let doc = state.documents.get_mut(&doc_id).unwrap();
        doc.record_change_with_selection(0, String::new(), "typed ".to_string(), crate::workspace::session_core::undo::Selection::cursor_only(0), crate::workspace::session_core::undo::Selection::cursor_only(6), None);
        drop(doc);
        std::fs::write(&note_path, "disk v2 (external)").unwrap();

        assert!(state.save_document(doc_id));

        // The disk keeps the external version; nothing was lost.
        assert_eq!(std::fs::read_to_string(&note_path).unwrap(), "disk v2 (external)");
        // Our version lives in a conflict copy next to the note.
        let copies: Vec<_> = std::fs::read_dir(&ws_path).unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains("конфликт"))
            .collect();
        assert_eq!(copies.len(), 1, "exactly one conflict copy must be created");
        assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), "typed disk v1");
        // The buffer adopted the external content and is clean again.
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), "disk v2 (external)");
        assert!(!state.documents.get(&doc_id).unwrap().is_dirty);

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

        // Synthetic watcher channel: the test injects batches exactly as the
        // delivery thread would.
        let (tx, rx) = std::sync::mpsc::channel();
        state.fs_events_rx = Some(rx);
        let debounce = Duration::from_millis(200);

        // 1. Initial check with no watcher events: nothing pending, no calls.
        let evs = state.check_fs_changes(debounce);
        assert!(evs.is_empty());
        assert!(state.pending_file_deletions.is_empty());

        // 2. Simulate unlink (file deleted temporarily): a real watcher
        //    reports the Remove; the file is gone from disk.
        std::fs::remove_file(&note_path).unwrap();
        tx.send(crate::fs_watcher::FsBatch {
            deleted: vec![note_path.clone()],
            ..Default::default()
        }).unwrap();

        // 3. First check after deletion: queued into pending, no Deleted event yet
        let evs1 = state.check_fs_changes(debounce);
        assert!(evs1.is_empty(), "First tick must not emit Deleted due to debounce");
        assert!(state.pending_file_deletions.contains_key(&note_path));

        // 4. Simulate atomic save: file reappears before debounce expires!
        std::fs::write(&note_path, "# Atomic Test Replaced").unwrap();
        let evs2 = state.check_fs_changes(debounce);
        assert!(evs2.is_empty(), "Must not emit Deleted if file reappeared");
        assert!(!state.pending_file_deletions.contains_key(&note_path), "Pending deletion must be cancelled");

        // 5. Now delete for real and wait past debounce
        std::fs::remove_file(&note_path).unwrap();
        tx.send(crate::fs_watcher::FsBatch {
            deleted: vec![note_path.clone()],
            ..Default::default()
        }).unwrap();
        let _ = state.check_fs_changes(debounce);
        assert!(state.pending_file_deletions.contains_key(&note_path));

        // Sleep to exceed debounce window
        std::thread::sleep(Duration::from_millis(250));

        let evs3 = state.check_fs_changes(debounce);
        assert_eq!(evs3, vec![FsChangeEvent::Deleted(note_path)]);
        assert!(state.pending_file_deletions.is_empty());

        let _ = std::fs::remove_dir_all(&ws_path);
    }

    #[test]
    fn test_document_buffer_preserves_text_stats() {
        let stats = TextStats::Large { bytes: 354 * 1024 * 1024 };
        let doc_id = DocumentId::new();
        let doc = DocumentBuffer::new(
            doc_id,
            PathBuf::from("/test/file.md"),
            "file".to_string(),
            String::new(),
            1,
        );
        doc.stats_cache.set(Some((1, stats)));
        assert_eq!(doc.text_stats(), stats);

        let small_stats = TextStats::Exact { words: 123, chars: 456 };
        let doc_id2 = DocumentId::new();
        let small_doc = DocumentBuffer::new(
            doc_id2,
            PathBuf::from("/test/small.md"),
            "small".to_string(),
            String::new(),
            1,
        );
        small_doc.stats_cache.set(Some((1, small_stats)));
        assert_eq!(small_doc.text_stats(), small_stats);
    }

    #[test]
    fn test_stage2_two_phase_close_with_title_commit_and_flush() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_stage2_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("Note1.md");
        std::fs::write(&note_path, "Initial text").unwrap();

        let mut state = AppState::new();
        state.open_tabs.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.leaves.clear();
        let root_leaf = LeafId::new();
        state.leaves.insert(root_leaf, LeafState::new(root_leaf));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: root_leaf };
        state.workspace.active_leaf = root_leaf;
        state.workspace_path = Some(temp_dir.clone());

        state.open_note(note_path.clone());

        let leaf_id = state.workspace.active_leaf;
        let active_idx = state.leaves.get(&leaf_id).unwrap().active_tab_index;
        let tab_id = state.leaves.get(&leaf_id).unwrap().tabs[active_idx].id;
        let doc_id = state.find_document_by_path(&note_path).unwrap();

        // 1. Enter title edit mode
        if let Some(view) = state.views.get_mut(&tab_id) {
            view.title_input = Some(crate::workspace::TitleInput::new("RenamedNote".to_string()));
        }

        // 2. Make document dirty
        if let Some(doc) = state.documents.get_mut(&doc_id) {
            doc.record_change(0, String::new(), "Modified ".to_string(), 0, 9, None);
            assert!(doc.is_dirty);
        }

        // 3. Request close: should produce CommitTitle and FlushDoc effects
        let (req_tab_id, req) = state.request_close_leaf_tab(leaf_id, active_idx).unwrap();
        assert_eq!(req_tab_id, tab_id);
        match req {
            crate::workspace::CloseRequest::NeedsEffects(effects) => {
                assert_eq!(effects.len(), 2);
                assert!(matches!(effects[0], crate::workspace::CloseEffect::CommitTitle { .. }));
                assert!(matches!(effects[1], crate::workspace::CloseEffect::FlushDoc(_)));

                // Attempting finish_close before running effects must fail
                assert!(state.finish_close_leaf_tab(leaf_id, active_idx).is_err());

                // Run each effect
                for eff in &effects {
                    state.run_close_effect(eff).unwrap();
                }
            }
            crate::workspace::CloseRequest::Ready => panic!("Expected NeedsEffects"),
        }

        // 4. Now finish_close must succeed and document is clean on disk
        let closed = state.finish_close_leaf_tab(leaf_id, active_idx).unwrap();
        assert!(closed.is_some());
        assert!(!state.documents.contains_key(&doc_id));

        let renamed_path = temp_dir.join("RenamedNote.md");
        assert!(renamed_path.exists());
        let saved_text = std::fs::read_to_string(&renamed_path).unwrap();
        assert_eq!(saved_text, "Modified Initial text");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stage2_multi_tab_shared_doc_flush_only_on_last() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_stage2_multi_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("Shared.md");
        std::fs::write(&note_path, "Base content").unwrap();

        let mut state = AppState::new();
        state.open_tabs.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.leaves.clear();
        let root_leaf = LeafId::new();
        state.leaves.insert(root_leaf, LeafState::new(root_leaf));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: root_leaf };
        state.workspace.active_leaf = root_leaf;
        state.workspace_path = Some(temp_dir.clone());

        state.open_note(note_path.clone());

        let leaf1 = state.workspace.active_leaf;
        let doc_id = state.find_document_by_path(&note_path).unwrap();

        // Create second split with same document
        let leaf2 = LeafId::new();
        state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2);
        let mut l2 = LeafState::new(leaf2);
        let tab2 = crate::workspace::Tab::new_document(doc_id);
        let tab2_id = tab2.id;
        l2.tabs.push(tab2);
        l2.active_tab_index = 0;
        state.leaves.insert(leaf2, l2);
        state.views.insert(tab2_id, crate::workspace::EditorViewState::new(tab2_id, doc_id));

        // Make doc dirty
        if let Some(doc) = state.documents.get_mut(&doc_id) {
            doc.record_change(0, String::new(), "Dirty ".to_string(), 0, 6, None);
            assert!(doc.is_dirty);
        }

        // Closing tab in leaf1 is NOT the last tab, so FlushDoc must NOT be required
        let active1 = state.leaves.get(&leaf1).unwrap().active_tab_index;
        let (_, req1) = state.request_close_leaf_tab(leaf1, active1).unwrap();
        assert_eq!(req1, crate::workspace::CloseRequest::Ready);
        let closed1 = state.close_leaf_tab(leaf1, active1);
        assert_eq!(closed1, None, "Doc must NOT be unloaded while tab2 is open");
        assert!(state.documents.contains_key(&doc_id));
        assert!(state.documents.get(&doc_id).unwrap().is_dirty);

        // Closing tab in leaf2 IS the last tab: FlushDoc must be required and executed
        let active2 = state.leaves.get(&leaf2).unwrap().active_tab_index;
        let (_, req2) = state.request_close_leaf_tab(leaf2, active2).unwrap();
        match req2 {
            crate::workspace::CloseRequest::NeedsEffects(effects) => {
                assert_eq!(effects.len(), 1);
                assert!(matches!(effects[0], crate::workspace::CloseEffect::FlushDoc(_)));
            }
            crate::workspace::CloseRequest::Ready => panic!("Expected FlushDoc for last dirty tab"),
        }

        let closed2 = state.close_leaf_tab(leaf2, active2);
        assert!(closed2.is_some());
        assert!(!state.documents.contains_key(&doc_id));

        let disk_content = std::fs::read_to_string(&note_path).unwrap();
        assert_eq!(disk_content, "Dirty Base content");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stage2_save_dirty_documents_flushes_all_dirty() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_stage2_dirty_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note1 = temp_dir.join("NoteA.md");
        let note2 = temp_dir.join("NoteB.md");
        std::fs::write(&note1, "A").unwrap();
        std::fs::write(&note2, "B").unwrap();

        let mut state = AppState::new();
        state.open_tabs.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.leaves.clear();
        let root_leaf = LeafId::new();
        state.leaves.insert(root_leaf, LeafState::new(root_leaf));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: root_leaf };
        state.workspace.active_leaf = root_leaf;
        state.workspace_path = Some(temp_dir.clone());

        state.open_note(note1.clone());
        state.open_note(note2.clone());

        let doc_id1 = state.find_document_by_path(&note1).unwrap();
        let doc_id2 = state.find_document_by_path(&note2).unwrap();

        state.documents.get_mut(&doc_id1).unwrap().record_change(0, String::new(), "Saved ".to_string(), 0, 6, None);
        state.documents.get_mut(&doc_id2).unwrap().record_change(0, String::new(), "Saved ".to_string(), 0, 6, None);

        assert!(state.documents.get(&doc_id1).unwrap().is_dirty);
        assert!(state.documents.get(&doc_id2).unwrap().is_dirty);

        state.save_dirty_documents();

        assert!(!state.documents.get(&doc_id1).unwrap().is_dirty);
        assert!(!state.documents.get(&doc_id2).unwrap().is_dirty);
        assert_eq!(std::fs::read_to_string(&note1).unwrap(), "Saved A");
        assert_eq!(std::fs::read_to_string(&note2).unwrap(), "Saved B");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_active_note_flushes_only_active_pane() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_active_save_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note1 = temp_dir.join("NoteA.md");
        let note2 = temp_dir.join("NoteB.md");
        std::fs::write(&note1, "A").unwrap();
        std::fs::write(&note2, "B").unwrap();

        let mut state = AppState::new();
        state.open_tabs.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.leaves.clear();
        let leaf1 = LeafId::new();
        state.leaves.insert(leaf1, LeafState::new(leaf1));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: leaf1 };
        state.workspace.active_leaf = leaf1;
        state.workspace_path = Some(temp_dir.clone());

        state.open_note(note1.clone());
        let doc1 = state.find_document_by_path(&note1).unwrap();

        // Second pane with its own document
        let leaf2 = LeafId::new();
        state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2);
        state.leaves.insert(leaf2, LeafState::new(leaf2));
        state.workspace.active_leaf = leaf2;
        state.open_note(note2.clone());
        let doc2 = state.find_document_by_path(&note2).unwrap();

        state.documents.get_mut(&doc1).unwrap().record_change(0, String::new(), "Saved ".to_string(), 0, 6, None);
        state.documents.get_mut(&doc2).unwrap().record_change(0, String::new(), "Saved ".to_string(), 0, 6, None);
        assert!(state.documents.get(&doc1).unwrap().is_dirty);
        assert!(state.documents.get(&doc2).unwrap().is_dirty);

        // Flush the active pane only: leaf1 is active, leaf2 must stay untouched
        state.workspace.active_leaf = leaf1;
        assert!(state.active_document().map(|d| d.is_dirty).unwrap_or(false));
        state.save_active_note();

        assert!(!state.documents.get(&doc1).unwrap().is_dirty);
        assert!(state.documents.get(&doc2).unwrap().is_dirty, "Other pane must keep its unsaved edits");
        assert_eq!(std::fs::read_to_string(&note1).unwrap(), "Saved A");
        assert_eq!(std::fs::read_to_string(&note2).unwrap(), "B");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stage3_multi_split_viewport_stability_when_editing_in_other_split() {
        let temp_dir = std::env::temp_dir().join(format!("vibe_stage3_vp_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("Doc.md");

        let mut content = String::new();
        for i in 0..500 {
            content.push_str(&format!("Line {i}\n"));
        }
        std::fs::write(&note_path, &content).unwrap();

        let mut state = AppState::new();
        state.open_tabs.clear();
        state.documents.clear();
        state.by_path.clear();
        state.views.clear();
        state.leaves.clear();
        let leaf1 = LeafId::new();
        state.leaves.insert(leaf1, LeafState::new(leaf1));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: leaf1 };
        state.workspace.active_leaf = leaf1;
        state.workspace_path = Some(temp_dir.clone());

        // Open note in leaf 1
        state.open_note(note_path.clone());
        let doc_id = state.find_document_by_path(&note_path).unwrap();
        let tab1_id = state.leaves.get(&leaf1).unwrap().active_tab().unwrap().id;

        // Create second split (leaf 2) with the same document
        let leaf2 = LeafId::new();
        state.workspace.split_leaf(leaf1, crate::workspace::SplitDirection::Row, leaf2);
        let mut l2 = LeafState::new(leaf2);
        let tab2 = crate::workspace::Tab::new_document(doc_id);
        let tab2_id = tab2.id;
        l2.tabs.push(tab2);
        l2.active_tab_index = 0;
        state.leaves.insert(leaf2, l2);

        let mut view2 = crate::workspace::EditorViewState::new(tab2_id, doc_id);
        // Anchor view 2 at line 200
        view2.viewport.offset = 200;
        view2.viewport.px_in_line = 4.0;
        {
            let doc = state.documents.get_mut(&doc_id).unwrap();
            view2.ensure_visual_cache(doc, false, 0, 200);
        }
        let initial_local_scroll2 = view2.scroll_top - view2.above_h;
        state.views.insert(tab2_id, view2);

        // Active split is leaf 1. Place cursor at start of line 0 and insert 5 new lines
        state.workspace.active_leaf = leaf1;
        if let Some(v1) = state.views.get_mut(&tab1_id) {
            v1.cursor_offset = 0;
        }

        state.active_insert_text("New1\nNew2\nNew3\nNew4\nNew5\n");

        // Split 2 must have had its anchor line shifted by exactly 5 lines (200 -> 205)
        let view2_after = state.views.get(&tab2_id).unwrap();
        assert_eq!(view2_after.viewport.offset, 205, "Anchor line in Split 2 must shift by 5 lines");

        // Split 2's local scroll (visible text position) must remain 100% UNCHANGED (zero jump!)
        let local_scroll2_after = view2_after.scroll_top - view2_after.above_h;
        assert_eq!(
            local_scroll2_after, initial_local_scroll2,
            "Visible text position in Split 2 must NOT jump when editing in Split 1 above it"
        );

        // Undo in Split 1
        assert!(state.active_undo());

        // Split 2 must have its anchor restored to 200
        let view2_after_undo = state.views.get(&tab2_id).unwrap();
        assert_eq!(view2_after_undo.viewport.offset, 200, "Anchor line in Split 2 must restore to 200 after undo");
        let local_scroll2_after_undo = view2_after_undo.scroll_top - view2_after_undo.above_h;
        assert_eq!(local_scroll2_after_undo, initial_local_scroll2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stage3_splitter_drag_updates_leaf_bounds_without_invalidating_unrelated() {
        let mut state = AppState::new();
        state.leaves.clear();

        let l1 = LeafId::new();
        let l2 = LeafId::new();
        let l3 = LeafId::new();

        state.leaves.insert(l1, LeafState::new(l1));
        state.leaves.insert(l2, LeafState::new(l2));
        state.leaves.insert(l3, LeafState::new(l3));

        // Layout: Split(Row, [l1, Split(Column, [l2, l3])])
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: l1 };
        state.workspace.split_leaf(l1, crate::workspace::SplitDirection::Row, l2);
        state.workspace.split_leaf(l2, crate::workspace::SplitDirection::Column, l3);

        // Update bounds for 1200x800 window
        state.update_leaf_bounds_from_layout(1200.0, 800.0, 44.0, 0.0);

        let b1 = state.leaves.get(&l1).unwrap().bounds.unwrap();
        let b2 = state.leaves.get(&l2).unwrap().bounds.unwrap();
        let b3 = state.leaves.get(&l3).unwrap().bounds.unwrap();

        assert!(b1.w > 500.0);
        assert!(b2.w > 500.0);
        assert!(b3.w > 500.0);
        assert!((b2.h - b3.h).abs() < 5.0, "Equal column split height");

        // Drag column divider between l2 and l3: l1 ratios and bounds remain untouched
        state.workspace.set_split_ratio_at(&[1], 0, 0.7);
        state.update_leaf_bounds_from_layout(1200.0, 800.0, 44.0, 0.0);

        let b1_new = state.leaves.get(&l1).unwrap().bounds.unwrap();
        let b2_new = state.leaves.get(&l2).unwrap().bounds.unwrap();
        let b3_new = state.leaves.get(&l3).unwrap().bounds.unwrap();

        assert_eq!(b1_new, b1, "Leaf 1 bounds must remain unaffected when dragging l2-l3 divider");
        assert!(b2_new.h > b3_new.h, "Leaf 2 height must increase relative to Leaf 3");
    }

    #[test]
    fn test_closing_downward_split_restores_full_height() {
        let mut state = AppState::new();
        let l1 = LeafId::new();
        let l2 = LeafId::new();

        state.leaves.insert(l1, LeafState::new(l1));
        state.leaves.insert(l2, LeafState::new(l2));
        state.workspace.root = crate::workspace::WorkspaceNode::Leaf { id: l1 };
        state.workspace.active_leaf = l1;

        // Split vertically (Column: top l1, bottom l2)
        state.workspace.split_leaf(l1, crate::workspace::SplitDirection::Column, l2);
        assert_eq!(state.workspace.leaves().len(), 2);
        assert!((state.workspace.vertical_height_ratio_of_leaf(l1) - 0.5).abs() < 0.001);
        assert!((state.workspace.vertical_height_ratio_of_leaf(l2) - 0.5).abs() < 0.001);

        // Update layout bounds for a 1200x800 window
        state.update_leaf_bounds_from_layout(1200.0, 800.0, 44.0, 0.0);
        let b1 = state.leaves.get(&l1).unwrap().bounds.unwrap();
        let b2 = state.leaves.get(&l2).unwrap().bounds.unwrap();
        assert!((b1.h - 367.0).abs() < 5.0, "Initial half height for top leaf");
        assert!((b2.h - 367.0).abs() < 5.0, "Initial half height for bottom leaf");

        // Close bottom split (l2)
        let next_active = state.close_leaf(l2);
        assert_eq!(next_active, Some(l1));
        assert_eq!(state.workspace.leaves().len(), 1);

        // Verify root collapsed to Leaf(l1) and ratio is 1.0 (full height)
        match &state.workspace.root {
            crate::workspace::WorkspaceNode::Leaf { id } => assert_eq!(*id, l1),
            _ => panic!("Root split must collapse to single leaf"),
        }
        assert!((state.workspace.vertical_height_ratio_of_leaf(l1) - 1.0).abs() < 0.001, "Top leaf must expand to 100% vertical ratio");

        // Verify layout caches were invalidated so stale bounds don't freeze the viewport
        assert!(state.leaves.get(&l1).unwrap().bounds.is_none(), "Stale bounds must be invalidated");
    }

    #[test]
    fn test_inactive_documents_lru_eviction() {
        let temp_dir = std::env::temp_dir().join(format!("aquilum_lru_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mut paths = Vec::new();
        for i in 1..=6 {
            let p = temp_dir.join(format!("note_{}.md", i));
            std::fs::write(&p, format!("Content of note {}", i)).unwrap();
            paths.push(p);
        }

        let mut state = AppState::new();
        state.documents.clear();
        state.doc_meta.clear();
        state.by_path.clear();
        state.max_inactive_docs = 10;
        let leaf_id = state.workspace.active_leaf;

        // Open 6 notes in sequence in the same leaf
        let mut doc_ids = Vec::new();
        for (i, p) in paths.iter().enumerate() {
            let doc_id = state.get_or_open_document(p, format!("Note {}", i + 1), format!("Content of note {}", i + 1));
            doc_ids.push(doc_id);
            if i == 0 {
                if let Some(leaf) = state.leaves.get_mut(&leaf_id) {
                    leaf.tabs.clear();
                    leaf.tabs.push(crate::workspace::Tab::new_document(doc_id));
                    leaf.active_tab_index = 0;
                }
            } else {
                if let Some(leaf) = state.leaves.get_mut(&leaf_id) {
                    leaf.tabs.push(crate::workspace::Tab::new_document(doc_id));
                    leaf.active_tab_index = leaf.tabs.len() - 1;
                }
            }
            // Emulate slight time delay so access times are distinct
            std::thread::sleep(std::time::Duration::from_millis(10));
            state.touch_document_accessed(doc_id);
        }

        // Active tab is note 6 (doc_ids[5]). Notes 1..5 are inactive.
        assert_eq!(state.documents.len(), 6);
        assert_eq!(state.doc_meta.len(), 6);

        // Set max_inactive_docs to 2
        state.max_inactive_docs = 2;
        state.evict_inactive_documents_if_needed();

        // Visible note 6 must still be loaded.
        assert!(state.is_document_loaded(doc_ids[5]));

        // Most recent inactive notes 4 and 5 must still be loaded.
        assert!(state.is_document_loaded(doc_ids[4]));
        assert!(state.is_document_loaded(doc_ids[3]));

        // Oldest inactive notes 1, 2, 3 must be evicted from RAM!
        assert!(!state.is_document_loaded(doc_ids[0]));
        assert!(!state.is_document_loaded(doc_ids[1]));
        assert!(!state.is_document_loaded(doc_ids[2]));

        // But metadata (title, path) for evicted notes is still 100% available
        assert_eq!(state.document_title(doc_ids[0]), Some("Note 1"));
        assert_eq!(state.document_path(doc_ids[0]), Some(paths[0].as_path()));

        // Lazy reload note 1 on demand
        assert!(state.ensure_document_loaded(doc_ids[0]));
        assert!(state.is_document_loaded(doc_ids[0]));
        assert_eq!(state.documents.get(&doc_ids[0]).unwrap().content(), "Content of note 1");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_visible_tabs_across_more_than_4_leaves_never_evicted() {
        let temp_dir = std::env::temp_dir().join(format!("aquilum_lru_splits_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mut state = AppState::new();
        state.documents.clear();
        state.doc_meta.clear();
        state.by_path.clear();
        state.leaves.clear();

        // Create 6 leaves (splits), each having 1 visible active document tab
        let mut visible_doc_ids = Vec::new();
        for i in 1..=6 {
            let p = temp_dir.join(format!("visible_note_{}.md", i));
            std::fs::write(&p, format!("Visible content {}", i)).unwrap();
            let doc_id = state.get_or_open_document(&p, format!("Visible {}", i), format!("Visible content {}", i));
            visible_doc_ids.push(doc_id);

            let leaf_id = crate::workspace::LeafId::new();
            let mut leaf = crate::workspace::LeafState::new(leaf_id);
            leaf.tabs.push(crate::workspace::Tab::new_document(doc_id));
            leaf.active_tab_index = 0;
            state.leaves.insert(leaf_id, leaf);
        }

        // Add 5 inactive tabs hidden in the first leaf
        let mut inactive_doc_ids = Vec::new();
        for i in 1..=5 {
            let p = temp_dir.join(format!("inactive_note_{}.md", i));
            std::fs::write(&p, format!("Inactive content {}", i)).unwrap();
            let doc_id = state.get_or_open_document(&p, format!("Inactive {}", i), format!("Inactive content {}", i));
            inactive_doc_ids.push(doc_id);

            let first_leaf_id = *state.leaves.keys().next().unwrap();
            state.leaves.get_mut(&first_leaf_id).unwrap().tabs.push(crate::workspace::Tab::new_document(doc_id));
        }

        // Limit inactive docs to 2
        state.max_inactive_docs = 2;
        state.evict_inactive_documents_if_needed();

        // ALL 6 visible tabs across the 6 leaves must NEVER be evicted!
        for (i, doc_id) in visible_doc_ids.iter().enumerate() {
            assert!(
                state.is_document_loaded(*doc_id),
                "Visible document {} across >4 leaves must never be evicted",
                i + 1
            );
        }

        // Only up to 2 inactive documents are kept loaded
        let loaded_inactive_count = inactive_doc_ids.iter().filter(|id| state.is_document_loaded(**id)).count();
        assert_eq!(loaded_inactive_count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dirty_inactive_document_auto_saved_before_eviction() {
        let temp_dir = std::env::temp_dir().join(format!("aquilum_lru_dirty_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let p1 = temp_dir.join("dirty_note.md");
        let p2 = temp_dir.join("active_note.md");
        std::fs::write(&p1, "Initial dirty file").unwrap();
        std::fs::write(&p2, "Active note content").unwrap();

        let mut state = AppState::new();
        state.documents.clear();
        state.doc_meta.clear();
        state.by_path.clear();
        let leaf_id = state.workspace.active_leaf;

        let doc1 = state.get_or_open_document(&p1, "Dirty Note".to_string(), "Initial dirty file".to_string());
        // Edit doc1 so it is dirty
        if let Some(buf) = state.documents.get_mut(&doc1) {
            let removed = buf.content();
            buf.record_change(0, removed, "Modified unsaved text!".to_string(), 0, 0, None);
            assert!(buf.is_dirty);
        }
        if let Some(meta) = state.doc_meta.get_mut(&doc1) {
            meta.is_dirty = true;
        }

        // Now open active doc2
        let doc2 = state.get_or_open_document(&p2, "Active Note".to_string(), "Active note content".to_string());
        if let Some(leaf) = state.leaves.get_mut(&leaf_id) {
            leaf.tabs.clear();
            leaf.tabs.push(crate::workspace::Tab::new_document(doc1));
            leaf.tabs.push(crate::workspace::Tab::new_document(doc2));
            leaf.active_tab_index = 1; // doc2 is active, doc1 is inactive
        }

        // Set max_inactive_docs to 0 so doc1 must be evicted
        state.max_inactive_docs = 0;
        state.evict_inactive_documents_if_needed();

        // doc1 should have been evicted from RAM
        assert!(!state.is_document_loaded(doc1));

        // Disk file must have received the flushed changes!
        let disk_content = std::fs::read_to_string(&p1).unwrap();
        assert_eq!(disk_content, "Modified unsaved text!");

        // Metadata is clean
        assert!(!state.is_document_dirty(doc1));

        // Reload doc1
        assert!(state.ensure_document_loaded(doc1));
        assert_eq!(state.documents.get(&doc1).unwrap().content(), "Modified unsaved text!");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_large_document_opens_on_first_try_without_premature_eviction() {
        let temp_dir = std::env::temp_dir().join(format!("aquilum_lru_large_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let p = temp_dir.join("large_note.md");
        // Create a note with 70MB (exceeding MAX_INACTIVE_DOC_BYTES of 64MB)
        let large_string = "A".repeat(70 * 1024 * 1024);
        std::fs::write(&p, &large_string).unwrap();

        let mut state = AppState::new();
        state.max_inactive_doc_bytes = 64 * 1024 * 1024;
        state.documents.clear();
        state.doc_meta.clear();
        state.by_path.clear();
        if let Some(leaf) = state.leaves.get_mut(&state.workspace.active_leaf) {
            leaf.tabs.clear();
        }

        // Open the large note on the first attempt
        state.open_note_internal_at(p.clone(), false);
        let doc_id = state.find_document_by_path(&p).expect("Document should be found");

        // Verify the document is loaded in memory and NOT immediately evicted
        assert!(state.is_document_loaded(doc_id), "Large document must remain loaded when opened");
        assert_eq!(state.documents.get(&doc_id).unwrap().len(), 70 * 1024 * 1024);

        // Verify the active leaf has this tab
        let leaf_id = state.workspace.active_leaf;
        let leaf = state.leaves.get(&leaf_id).unwrap();
        assert_eq!(leaf.tabs.len(), 1);
        match &leaf.tabs[0].content {
            crate::workspace::TabContent::Document { id } => assert_eq!(*id, doc_id),
            _ => panic!("Expected document tab"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_closing_multiple_tabs_with_lru_eviction_and_order() {
        let temp_dir = std::env::temp_dir().join(format!("aquilum_close_lru_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let mut paths = Vec::new();
        for i in 1..=7 {
            let p = temp_dir.join(format!("Note_{}.md", i));
            std::fs::write(&p, format!("Content of Note {}", i)).unwrap();
            paths.push(p);
        }

        let mut state = AppState::new();
        state.max_inactive_docs = 4;
        state.documents.clear();
        state.doc_meta.clear();
        state.by_path.clear();
        if let Some(leaf) = state.leaves.get_mut(&state.workspace.active_leaf) {
            leaf.tabs.clear();
        }

        // Open 7 notes in order
        let mut doc_ids = Vec::new();
        for p in &paths {
            state.open_note_internal_at(p.clone(), false);
            let doc_id = state.find_document_by_path(p).unwrap();
            doc_ids.push(doc_id);
            state.touch_document_accessed(doc_id);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let leaf_id = state.workspace.active_leaf;
        assert_eq!(state.leaves.get(&leaf_id).unwrap().tabs.len(), 7);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 6); // Note 7 is active

        // Notes 1 and 2 should have been evicted by LRU (max 4 inactive docs: notes 3, 4, 5, 6)
        assert!(!state.is_document_loaded(doc_ids[0]));
        assert!(!state.is_document_loaded(doc_ids[1]));

        // Close active tab 6 (Note 7) -> active tab index should become 5 (Note 6)
        state.close_leaf_tab(leaf_id, 6);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 5);

        // Close active tab 5 (Note 6) -> active tab index should become 4 (Note 5)
        state.close_leaf_tab(leaf_id, 5);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 4);

        // Close active tab 4 (Note 5) -> active tab index should become 3 (Note 4)
        state.close_leaf_tab(leaf_id, 4);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 3);

        // Close active tab 3 (Note 4) -> active tab index should become 2 (Note 3)
        state.close_leaf_tab(leaf_id, 3);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 2);

        // Close active tab 2 (Note 3) -> active tab index should become 1 (Note 2)
        state.close_leaf_tab(leaf_id, 2);
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 1);

        // Now active tab is Note 2, which was evicted.
        let active_doc = state.active_doc_id().unwrap();
        assert_eq!(active_doc, doc_ids[1]);
        assert!(!state.is_document_loaded(active_doc));

        // When ensure_document_loaded is called (as select_leaf_tab does), it loads it cleanly!
        assert!(state.ensure_document_loaded(active_doc));
        assert!(state.is_document_loaded(active_doc));
        assert_eq!(state.active_document().unwrap().content(), "Content of Note 2");

        // Now test closing inactive tab to the left: close tab 0 (Note 1) while tab 1 (Note 2) is active
        state.close_leaf_tab(leaf_id, 0);
        // Active tab must shift left from index 1 to index 0, still pointing to Note 2!
        assert_eq!(state.leaves.get(&leaf_id).unwrap().active_tab_index, 0);
        assert_eq!(state.active_doc_id().unwrap(), doc_ids[1]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_table_edit_preserves_cursor_when_none() {
        let temp_dir = std::env::temp_dir().join(format!("test_tbl_cursor_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("note.md");
        let initial_text = "Before text\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nAfter text\n";
        std::fs::write(&note_path, initial_text).unwrap();

        let mut state = AppState::new_empty();
        state.open_note(note_path);

        // Put cursor in "After text" (line 6, col 3)
        let doc_id = state.active_doc_id().unwrap();
        let tab_id = state.active_tab_id().unwrap();
        let after_offset = state.documents.get(&doc_id).unwrap().line_start_byte(6) + 3;
        state.views.get_mut(&tab_id).unwrap().cursor_offset = after_offset;

        // Apply a table edit that inserts a row into the table (lines 2..=4)
        let edit = crate::table::LineEdit {
            first_line: 2,
            old_line_count: 3,
            new_lines: vec![
                "| a | b |".to_string(),
                "|---|---|".to_string(),
                "| 1 | 2 |".to_string(),
                "| 3 | 4 |".to_string(),
            ],
        };

        state.active_apply_table_line_edit(&edit, None);

        // Verify the cursor is still in "After text" on the correct line and col
        let (view, doc) = state.active_view_and_doc().unwrap();
        let (line, col) = view.line_col(doc);
        assert_eq!(line, 7, "Cursor line should shift from 6 to 7 due to 1 added row");
        assert_eq!(col, 3, "Cursor column should stay at 3");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_column_delete_undo_redo_restores_content_and_marks_dirty() {
        let temp_dir = std::env::temp_dir().join(format!("test_tbl_col_undo_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("note.md");
        let initial = "| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n\nAfter\n";
        std::fs::write(&note_path, initial).unwrap();

        let mut state = AppState::new_empty();
        state.open_note(note_path);
        let doc_id = state.active_doc_id().unwrap();

        let edit = crate::table::LineEdit {
            first_line: 0,
            old_line_count: 3,
            new_lines: vec![
                "| a | c |".to_string(),
                "|---|---|".to_string(),
                "| 1 | 3 |".to_string(),
            ],
        };
        state.active_apply_table_line_edit(&edit, None);
        let deleted = "| a | c |\n|---|---|\n| 1 | 3 |\n\nAfter\n";
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), deleted);

        // Emulate a completed flush, then undo: the document must become dirty
        // again and get a fresh save deadline.
        state.documents.get_mut(&doc_id).unwrap().mark_clean();
        assert!(state.active_undo());
        {
            let doc = state.documents.get(&doc_id).unwrap();
            assert_eq!(doc.content(), initial);
            assert!(doc.is_dirty);
            assert!(doc.first_dirty_time.is_some());
        }

        assert!(state.active_redo());
        assert_eq!(state.documents.get(&doc_id).unwrap().content(), deleted);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn toggling_a_task_mark_keeps_the_caret_and_is_undoable() {
        let temp_dir = std::env::temp_dir().join(format!("test_task_toggle_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let note_path = temp_dir.join("tasks.md");
        std::fs::write(&note_path, "- [ ] first\n- [x] second\n").unwrap();

        let mut state = AppState::new_empty();
        state.open_note(note_path);
        let doc_id = state.active_doc_id().unwrap();
        let tab_id = state.active_tab_id().unwrap();
        let line2 = state.documents.get(&doc_id).unwrap().line_start_byte(1);
        {
            let view = state.views.get_mut(&tab_id).unwrap();
            view.cursor_offset = line2 + 8;
            view.selection_anchor = None;
        }

        // The click knows only the mark's place in the line, as the renderer saw it.
        let mark = crate::markdown::task_prefix("- [x] second").unwrap().mark;
        assert!(state.active_toggle_task_mark(1, mark));
        assert_eq!(state.documents.get(&doc_id).unwrap().line_text(1), "- [ ] second");
        {
            let view = state.views.get(&tab_id).unwrap();
            assert_eq!(view.cursor_offset, line2 + 8, "the click must not move the caret");
            assert_eq!(view.selection_anchor, None);
        }

        // The toggle is one ordinary undo step.
        assert!(state.active_undo());
        assert_eq!(state.documents.get(&doc_id).unwrap().line_text(1), "- [x] second");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

