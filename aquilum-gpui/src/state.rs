use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::document_buffer::{DocumentBuffer, DocumentId};
use crate::workspace::session_core::ops::EditEvent;
use crate::workspace::{EditorViewState, LeafId, LeafState, TabId, WorkspaceTree};

mod session;
mod window;
mod persist;
mod nav;
mod workspace;
mod fs;
mod close;
mod doc_lifecycle;
mod documents;
mod cloud_sync;

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
    /// A non-note file (an image attachment) was deleted or renamed on disk;
    /// the editor's attachment resolution caches must be dropped.
    AttachmentsTouched,
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
    /// Whether the window is kept above all other windows.
    #[serde(default)]
    pub always_on_top: bool,
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

/// Immutable snapshot of the vault state and the active file for synchronous plugin getters.
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
    /// True when the vault came from the command line (`--vault`). The session
    /// is loaded for that vault only and never written back, so the default
    /// (last used) vault stays untouched.
    pub vault_from_cli: bool,
    pub events: Arc<crate::events::EventBus>,
    pub commands: Arc<crate::commands::CommandRegistry>,
    pub ribbon: Arc<crate::ribbon::RibbonRegistry>,
    pub status_bar: Arc<crate::ribbon::StatusBarRegistry>,
    pub toolbars: Arc<crate::toolbar::ToolbarRegistry>,
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
    /// Dropbox synchronization service. It owns its worker thread and writes
    /// remote changes to disk; the fs watcher below turns them into normal
    /// reload/close events for the UI.
    pub sync: crate::sync::SyncService,
    /// Documents deleted on disk: first-seen time; a path missing for longer
    /// than the debounce window is confirmed as deleted (atomic saves of other
    /// programs make the file disappear briefly and it must be cancelled).
    pub pending_file_deletions: HashMap<PathBuf, std::time::Instant>,
}

const DEFAULT_WELCOME_NOTE: &str = "# Welcome to Vibecodium (GPUI Live Preview)\n\nYour fast, distraction-free markdown knowledge base with **true Obsidian Live Preview**!\n\n## Inline Live Preview Demo\n- Click any line or block to edit raw markdown inline\n- Move cursor away (or click another line) to see rendered styling\n- Regular text and code blocks are fully selectable across the entire page with `Ctrl+A`\n\n### Interactive Features\n- [x] Zero Electron / Chromium overhead (~40 MB RAM, 120 FPS)\n- [x] Full document continuous text selection\n- [x] Dedicated Obsidian-style code blocks with syntax styling\n- [ ] Auto-saving and instant search\n\n```rust\nfn main() {\n    println!(\"Hello from Vibecodium GPUI Live Preview!\");\n}\n```\n\n> Regular text and code blocks are cleanly organized with seamless Live Preview transitions.\n";

impl AppState {
    pub fn new() -> Self {
        Self::new_with_vault(None)
    }

    /// `vault_override` opens that vault for this run only: the previous
    /// session (last vault, tabs, layout) is neither restored nor overwritten.
    pub fn new_with_vault(vault_override: Option<PathBuf>) -> Self {
        let vault_override = match vault_override {
            Some(path) if path.is_dir() => Some(Self::normalize_path(&path)),
            Some(path) => {
                eprintln!("[vault] not a directory: {}", path.display());
                None
            }
            None => None,
        };
        let vault_from_cli = vault_override.is_some();
        let app_data_dir = app_data_dir();
        let _ = std::fs::create_dir_all(&app_data_dir);
        let settings = Arc::new(crate::settings::SettingsManager::new(&app_data_dir));
        let session = SessionState::load(&app_data_dir);
        let sync = crate::sync::SyncService::new(&app_data_dir);
        sync.start();

        let valid_tabs: Vec<PathBuf> = if vault_from_cli {
            Vec::new()
        } else {
            session
                .open_tabs
                .iter()
                .filter(|p| is_empty_tab_path(p) || p.exists() || p.to_string_lossy() == "Welcome.md")
                .cloned()
                .collect()
        };

        let mut bookmarks = session.bookmarks.clone();
        let mut recent_files = session.recent_files.clone();
        let session_vault = vault_override.as_ref().or(session.workspace_path.as_ref());
        if let Some(ws) = session_vault {
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
        let toolbars = Arc::new(crate::toolbar::ToolbarRegistry::new());
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
            vault_from_cli,
            events,
            commands,
            ribbon,
            status_bar,
            toolbars,
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
            sync,
            pending_file_deletions: HashMap::new(),
        };

        // Initialize active leaf in leaves map
        state.leaves.insert(
            state.workspace.active_leaf,
            LeafState::new(state.workspace.active_leaf),
        );

        // Determine workspace to open: the command-line vault wins.
        let mut ws_to_open = vault_override.clone();
        if ws_to_open.is_none() {
            if let Some(ref ws) = state.session.workspace_path {
                if ws.exists() {
                    ws_to_open = Some(ws.clone());
                }
            }
        }

        if let Some(ws) = ws_to_open {
            state.open_workspace_without_saving(ws);
        }

        // Restore workspace v2 session if present. A command-line vault starts
        // with a clean window: the saved layout belongs to another vault.
        let ws_v2_path = app_data_dir.join("workspace-v2.json");
        let mut restored_v2 = false;
        if !vault_from_cli {
            if let Ok(content) = std::fs::read_to_string(&ws_v2_path) {
                if let Ok(ws_v2) = crate::workspace::WorkspaceSessionV2::from_json(&content) {
                    state.restore_workspace_session_v2(ws_v2);
                    restored_v2 = true;
                }
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
            // Restore active note (or active empty tab). With a command-line
            // vault the session describes a different vault, so only the
            // vault's own first note is opened.
            let mut note_to_open = None;
            let mut empty_to_open = None;
            if !vault_from_cli {
                if let Some(ref active) = state.session.active_note {
                    if is_empty_tab_path(active) {
                        if state.open_tabs.contains(active) {
                            empty_to_open = Some(active.clone());
                        }
                    } else if active.exists() || active.to_string_lossy() == "Welcome.md" {
                        note_to_open = Some(active.clone());
                    }
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







}

#[cfg(test)]
mod tests;

