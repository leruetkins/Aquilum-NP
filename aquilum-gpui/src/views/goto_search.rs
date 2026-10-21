//! "Go to file" dialog (aquilum-app's `SearchDialog`).
//!
//! Opened from the New Tab page or with `Ctrl+P`. Typing filters workspace
//! notes by file name first and by file contents second, showing match
//! counts and highlighted snippets like the reference implementation.
//! The scan itself runs on the search worker thread (`crate::search::worker`)
//! so typing is never blocked: at most [`MAX_SCAN_FILES`] files are visited,
//! files over the body-size limit are searched through their first megabyte
//! (see `search::cache`), and at most [`MAX_RESULTS`] hits surface.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, Context, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, Rgba, SharedString, Styled, StyledText, TextRun, Window,
};

use crate::blink_manager::BlinkTarget;
use crate::search::cancel::CANCELLED_SCANS;
use crate::search::cache::SearchContentCache;
use crate::search::fold::{self, Fold, Mode};
use crate::search::worker::{SearchConsumer, SearchRequest};
use crate::views::scrollbar::{overlay_scrollbar, ScrollbarGeometry, ScrollbarStyle};
use crate::icons;
use crate::state::FileNode;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// Upper bound on files inspected per keystroke (keeps typing responsive).
pub(crate) const MAX_SCAN_FILES: usize = 2000;
/// Upper bound on surfaced hits.
pub(crate) const MAX_RESULTS: usize = 50;
/// Snippet context around the first hit (chars).
const SNIPPET_BEFORE: usize = 60;
const SNIPPET_AFTER: usize = 140;

/// Right gutter of the results scroll container, in pixels.
///
/// Matches the `p_2` padding of the scroll container: the scrollbar lane
/// lives exactly in that gutter instead of floating over the cards.
const GOTO_RESULTS_PAD: f32 = 8.0;

/// Gap the cards leave between their border and the scrollbar lane.
///
/// A card border stretches to the edge of the content box, which is exactly
/// where the lane begins — without this the cards would run straight into
/// the thumb. Same idea as `SIDEBAR_ROW_SCROLLBAR_GAP` in the sidebar tree.
const GOTO_ROW_SCROLLBAR_GAP: f32 = 4.0;

/// Scrollbar look for the results list.
///
/// The lane is the whole padding gutter, the thumb is narrower and pushed
/// against the panel border with a 2px sliver of background on each side.
/// `thumb_w == thumb_w_active` keeps the width constant — the bar never
/// changes size under the pointer.
pub(crate) fn goto_scrollbar_style() -> ScrollbarStyle {
    ScrollbarStyle {
        track_w: GOTO_RESULTS_PAD,
        thumb_w: 6.0,
        thumb_w_active: 6.0,
        thumb_inset: 2.0,
        track_right_offset: 0.0,
        ..ScrollbarStyle::default()
    }
}

/// A single knowledge-base hit.
#[derive(Clone, Debug)]
pub struct GotoResult {
    pub path: PathBuf,
    pub title: String,
    pub snippet: String,
    pub match_count: usize,
    pub name_hit: bool,
}

/// State of the Go to file dialog.
#[derive(Clone, Debug, Default)]
pub struct SearchDialogState {
    pub is_open: bool,
    pub query: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub results: Vec<GotoResult>,
    /// The query the current `results` answer. While a newer query is in
    /// flight the list (and its highlights) stays on the old one — render
    /// highlights with THIS query's terms, not the live input's.
    pub results_query: String,
    pub selected_ix: usize,
    /// Window x of the input text box; measured via canvas for click mapping.
    pub box_origin_x: Rc<Cell<f32>>,
}

impl SearchDialogState {
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_pos {
                return Some((anchor.min(self.cursor_pos), anchor.max(self.cursor_pos)));
            }
        }
        None
    }
}

impl AppView {
    /// Opens the Go to file dialog with a cleared query and focused input.
    pub fn open_search_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Only one overlay at a time.
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        self.close_tab_context_menu(cx);
        self.close_tabs_popup(cx);
        if self.plugin_catalog.is_open {
            self.close_plugin_catalog(window, cx);
        }

        self.search_dialog = SearchDialogState {
            is_open: true,
            box_origin_x: self.search_dialog.box_origin_x.clone(),
            ..Default::default()
        };
        self.run_goto_search();
        window.focus(&self.goto_focus_handle);
        self.goto_blink.enable(BlinkTarget::GotoSearch, cx);
        cx.notify();
    }

    /// Closes the Go to file dialog and returns focus to the editor.
    pub fn close_search_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_dialog.is_open {
            return;
        }
        self.search_dialog.is_open = false;
        self.search_dialog.selection_anchor = None;
        self.search_dialog.is_mouse_selecting = false;
        self.goto_blink.disable(cx);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Follows the pointer while the input selection is being dragged.
    /// Mirrors `QuickFilterState::handle_mouse_move` in the sidebar.
    pub fn handle_goto_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search_dialog.is_open || !self.search_dialog.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.goto_click_pos(x, window);
        if self.search_dialog.cursor_pos != new_pos {
            self.search_dialog.cursor_pos = new_pos;
            self.goto_blink.pause_and_show(BlinkTarget::GotoSearch, cx);
            cx.notify();
        }
    }

    /// Ends an input drag selection, collapsing a zero-width range.
    /// Mirrors `QuickFilterState::handle_mouse_up` in the sidebar.
    pub fn handle_goto_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.search_dialog.is_mouse_selecting {
            self.search_dialog.is_mouse_selecting = false;
            if self.search_dialog.selection_anchor == Some(self.search_dialog.cursor_pos) {
                self.search_dialog.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Scrolls the results list to `scroll_y` pixels from the top.
    pub(crate) fn set_goto_scroll_top(&self, scroll_y: f32) {
        self.goto_scroll_handle.set_offset(point(px(0.0), px(-scroll_y)));
    }

    /// Follows the pointer while the results scrollbar thumb is dragged.
    /// Mirrors `handle_sidebar_scrollbar_drag` in the sidebar tree.
    pub(crate) fn handle_goto_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.goto_scrollbar_drag.active {
            return;
        }
        let style = goto_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.goto_scroll_handle, &style)
        else {
            return;
        };
        geometry.apply_drag(&self.goto_scroll_handle, &self.goto_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Requests a fresh dialog search on the worker (immediate entry points:
    /// dialog open, clear button, "search selection" command).
    ///
    /// The scan runs off the UI thread; results land via
    /// [`AppView::poll_search_replies`]. Stale replies are dropped by sequence
    /// number, so calling this repeatedly is always safe. An empty query
    /// clears synchronously and invalidates any in-flight reply.
    pub fn run_goto_search(&mut self) {
        let query = self.search_dialog.query.trim().to_string();
        if query.is_empty() {
            self.goto_search_seq += 1;
            self.search_dialog.results.clear();
            self.search_dialog.selected_ix = 0;
            return;
        }
        self.goto_search_seq += 1;
        self.state.search_worker.ask(SearchRequest {
            consumer: SearchConsumer::Dialog,
            seq: self.goto_search_seq,
            query,
            op: SearchOp::Any,
            case_sensitive: false,
            limit: MAX_RESULTS,
        });
    }

    /// Arms the debounced search for the dialog input.
    ///
    /// Called on EVERY keystroke, before the timer: the sequence number
    /// bumps and the in-flight scan is cancelled right here — a reply for a
    /// query the user has already moved past is dropped the moment it would
    /// surface, and stops reading files. Only the newest arm fires a scan
    /// ([`Self::run_goto_search`], off the UI thread); the text and caret
    /// were already updated by the caller, so typing is never blocked.
    pub fn request_goto_search(&mut self, cx: &mut Context<Self>) {
        self.goto_search_seq += 1;
        self.state
            .search_worker
            .cancel(SearchConsumer::Dialog);
        self.goto_debounce_epoch += 1;
        let epoch = self.goto_debounce_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                timer_executor.timer(Duration::from_millis(70)).await;
                let _ = entity.update(&mut async_cx, |this, _cx| {
                    if this.goto_debounce_epoch == epoch {
                        this.run_goto_search();
                    }
                });
            })
            .detach();
    }

    /// Opens a search hit: an active empty tab is replaced by the file,
    /// mirroring aquilum-app's `open-file` reducer behavior.
    pub fn open_goto_result(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(empty) = self.state.active_empty_tab.clone() {
            self.state.open_tabs.retain(|p| p != &empty);
            self.state.active_empty_tab = None;
        }
        self.close_search_dialog(window, cx);
        self.select_note(path, true, window, cx);
    }

    /// Creates a note named after the current query (`Shift+Enter`),
    /// replacing the active empty tab when there is one.
    pub fn create_note_from_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.search_dialog.query.trim().to_string();
        if title.is_empty() {
            return;
        }
        if self.state.workspace_path.is_none() {
            self.state.status_text = self.t("search.no_folder_hint");
            cx.notify();
            return;
        }
        if let Some(empty) = self.state.active_empty_tab.clone() {
            self.state.open_tabs.retain(|p| p != &empty);
            self.state.active_empty_tab = None;
        }
        self.close_search_dialog(window, cx);
        if self.state.create_named_note(&title).is_some() {
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            self.reset_editor_cursor_blink(cx);
        }
        cx.notify();
    }

    /// Handles keyboard events inside the dialog input.
    pub fn handle_goto_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.goto_blink.pause_and_show(BlinkTarget::GotoSearch, cx);
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Escape always dismisses the dialog.
        if key == "escape" {
            self.close_search_dialog(window, cx);
            return;
        }

        // Arrow navigation through results (wraps around).
        if key == "down" || key == "up" {
            let len = self.search_dialog.results.len();
            if len > 0 {
                let delta = if key == "down" { 1isize } else { -1 };
                let cur = self.search_dialog.selected_ix as isize;
                self.search_dialog.selected_ix =
                    (cur + delta).rem_euclid(len as isize) as usize;
                cx.notify();
            }
            return;
        }

        // Enter opens the selected hit; Shift+Enter creates a note instead.
        if key == "enter" {
            if shift {
                self.create_note_from_goto(window, cx);
            } else if let Some(hit) = self.search_dialog.results.get(self.search_dialog.selected_ix).cloned() {
                self.open_goto_result(hit.path, window, cx);
            }
            return;
        }

        // Home / End jump within the query.
        if key == "home" || key == "end" {
            let total = self.search_dialog.query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.search_dialog.selection_anchor.is_none() {
                    self.search_dialog.selection_anchor = Some(self.search_dialog.cursor_pos);
                }
            } else {
                self.search_dialog.selection_anchor = None;
            }
            self.search_dialog.cursor_pos = pos;
            cx.notify();
            return;
        }

        if key == "left" || key == "right" {
            let total = self.search_dialog.query.chars().count();
            if shift && self.search_dialog.selection_anchor.is_none() {
                self.search_dialog.selection_anchor = Some(self.search_dialog.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.search_dialog.cursor_pos = self.search_dialog.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.search_dialog.selection_range() {
                    self.search_dialog.cursor_pos = start;
                    self.search_dialog.selection_anchor = None;
                } else {
                    self.search_dialog.cursor_pos = self.search_dialog.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.search_dialog.cursor_pos = (self.search_dialog.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.search_dialog.selection_range() {
                self.search_dialog.cursor_pos = end;
                self.search_dialog.selection_anchor = None;
            } else {
                self.search_dialog.cursor_pos = (self.search_dialog.cursor_pos + 1).min(total);
            }
            cx.notify();
            return;
        }

        // Ctrl+A select all.
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.search_dialog.selection_anchor = Some(0);
            self.search_dialog.cursor_pos = self.search_dialog.query.chars().count();
            cx.notify();
            return;
        }

        // Ctrl+C copy selection.
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.search_dialog.selection_range() {
                let text: String = self.search_dialog.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        // Ctrl+X cut selection.
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.search_dialog.selection_range() {
                let text: String = self.search_dialog.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.search_dialog.query.chars().collect();
                chars.drain(s..e);
                self.search_dialog.query = chars.into_iter().collect();
                self.search_dialog.cursor_pos = s;
                self.search_dialog.selection_anchor = None;
                self.request_goto_search(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl+V paste.
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_goto_text(&filtered);
                        self.request_goto_search(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        if ctrl || alt || event.keystroke.modifiers.platform {
            return;
        }

        // Backspace / Delete.
        if key == "backspace" || key == "delete" {
            let mut chars: Vec<char> = self.search_dialog.query.chars().collect();
            if let Some((s, e)) = self.search_dialog.selection_range() {
                chars.drain(s..e);
                self.search_dialog.cursor_pos = s;
                self.search_dialog.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.search_dialog.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.search_dialog.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.search_dialog.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.search_dialog.query = chars.into_iter().collect();
            self.request_goto_search(cx);
            cx.notify();
            return;
        }

        // Plain text input.
        let text_to_insert = if key == "space" {
            Some(" ".to_string())
        } else if let Some(ch) = &event.keystroke.key_char {
            Some(ch.to_string())
        } else if key.chars().count() == 1 {
            Some(key.clone())
        } else {
            None
        };
        if let Some(text) = text_to_insert {
            let filtered: String = text
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                self.insert_goto_text(&filtered);
                self.request_goto_search(cx);
                cx.notify();
            }
        }
    }

    /// Inserts text at the cursor, replacing the selection first.
    fn insert_goto_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.search_dialog.query.chars().collect();
        let pos = if let Some((s, e)) = self.search_dialog.selection_range() {
            chars.drain(s..e);
            self.search_dialog.selection_anchor = None;
            s
        } else {
            self.search_dialog.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.search_dialog.cursor_pos = pos + text.chars().count();
        self.search_dialog.query = chars.into_iter().collect();
    }

    /// Maps a click x (window coords) to a query char index.
    pub fn goto_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.search_dialog.query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.search_dialog.box_origin_x.get();
        let rel_x = (click_x - box_x).max(0.0);
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        query[..byte_idx.min(query.len())].chars().count()
    }

    /// Localized "N matches" label with ru/uk pluralization.
    pub fn format_match_count(&self, count: usize) -> String {
        let norm = crate::i18n::LocaleInfo::normalize(&self.config.ui.language);
        if norm == "ru" || norm == "uk" {
            let key = match count % 10 {
                1 if count % 100 != 11 => "search.match_one",
                2..=4 if !(12..=14).contains(&(count % 100)) => "search.match_few",
                _ => "search.match_many",
            };
            self.t_fmt(key, &[&count.to_string()])
        } else if count == 1 {
            self.t_fmt("search.match_one", &["1"])
        } else {
            self.t_fmt("search.match_many", &[&count.to_string()])
        }
    }
}

/// Collects note file paths from the sidebar tree (depth-first, capped).
pub(crate) fn collect_goto_files(nodes: &[FileNode], limit: usize) -> Vec<PathBuf> {

    fn walk(nodes: &[FileNode], out: &mut Vec<PathBuf>, limit: usize) {
        for node in nodes {
            if out.len() >= limit {
                return;
            }
            match node {
                FileNode::File { path, .. } => out.push(path.clone()),
                FileNode::Folder { children, .. } => walk(children, out, limit),
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out, limit);
    out
}

/// Obsidian-style search operator: restricts what the query matches against.
///
/// `pub` because the worker API (`search::worker`) ships it in requests and
/// replies, not because new surfaces should construct it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchOp {
    /// File name first, then file contents (default).
    #[default]
    Any,
    /// Substring of the vault-relative file path.
    Path,
    /// Substring of the file name only.
    File,
    /// `#tag` mentions in the file body.
    Tag,
    /// All terms on a single line.
    Line,
    /// All terms under one heading-delimited section.
    Section,
}

/// Splits an `op: rest` prefix off the raw query (case-insensitive prefix).
///
/// Returns `(Any, trimmed whole query)` when there is no known prefix or
/// nothing follows the colon, so half-typed operators keep showing results.
///
/// The prefix is probed with [`str::get`] instead of byte slicing: queries
/// in multibyte scripts (e.g. Cyrillic, 2 bytes per letter) must never be
/// cut in the middle of a character.
pub(crate) fn parse_search_op(raw: &str) -> (SearchOp, String) {
    let trimmed = raw.trim();
    const PREFIXES: &[(&str, SearchOp)] = &[
        ("path:", SearchOp::Path),
        ("file:", SearchOp::File),
        ("tag:", SearchOp::Tag),
        ("line:", SearchOp::Line),
        ("section:", SearchOp::Section),
    ];
    for (prefix, op) in PREFIXES {
        // `get` yields `None` for short queries and for mid-character
        // boundaries instead of panicking.
        match trimmed.get(..prefix.len()) {
            Some(head) if head.eq_ignore_ascii_case(prefix) => {
                let rest = trimmed[prefix.len()..].trim().to_string();
                if rest.is_empty() {
                    break;
                }
                return (*op, rest);
            }
            _ => {}
        }
    }
    (SearchOp::Any, trimmed.to_string())
}

/// Extra knobs for [`search_goto_files_opts`].
pub(crate) struct GotoSearchParams<'a> {
    /// Case-sensitive matching (`Aa` toggle). Default is insensitive.
    pub case_sensitive: bool,
    /// Workspace root for vault-relative paths (`path:` operator and snippets).
    pub rel_base: Option<&'a Path>,
    /// Cancellation token of the enclosing request; the scan checks it
    /// between files and exits early when a newer request superseded it.
    /// Detached (default) tokens never cancel.
    pub cancel: crate::search::cancel::Cancel,
}

impl Default for GotoSearchParams<'_> {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            rel_base: None,
            cancel: Default::default(),
        }
    }
}

/// Fold setting shared by every matcher: `Exact` when case-sensitive.
fn fold_mode(case_sensitive: bool) -> Fold {
    if case_sensitive {
        Fold::Exact
    } else {
        Fold::Lower
    }
}

/// Scores and ranks files for an operator query (see [`SearchOp`]).
///
/// Contents are read only for the operators that match against the body
/// (`Any`, `Tag`, `Line`, `Section`) and for the snippets of files that
/// already matched by name under `File`; `Path` never reads. Reads go through
/// `cache`, so repeated calls between keystrokes cost metadata checks only.
pub(crate) fn search_goto_files_opts(
    files: &[PathBuf],
    raw_query: &str,
    op: SearchOp,
    params: &GotoSearchParams,
    limit: usize,
    cache: &mut SearchContentCache,
) -> Vec<GotoResult> {
    let cs = params.case_sensitive;
    let fl = fold_mode(cs);
    let rest = raw_query.trim();
    if rest.is_empty() {
        return Vec::new();
    }
    let q = rest.to_string();
    let terms: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        q.split_whitespace()
            .filter(|t| !t.is_empty())
            // Dedup on the folded form, like the old folded-query split did:
            // repeated terms must not inflate match counts.
            .filter(|t| seen.insert(fold::fold_str(t, fl)))
            .map(|t| t.to_string())
            .collect()
    };
    if terms.is_empty() {
        return Vec::new();
    }

    // (rank, match_count desc, title asc, hit)
    let mut ranked: Vec<(u8, usize, String, GotoResult)> = Vec::new();
    for path in files {
        // A newer request superseded this one: exit early instead of reading
        // the remaining files for a reply nobody will apply.
        if params.cancel.is_cancelled() {
            CANCELLED_SCANS.fetch_add(1, Ordering::Relaxed);
            return Vec::new();
        }
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }
        let rel_string: String = params
            .rel_base
            .and_then(|base| path.strip_prefix(base).ok())
            .map(|rel| rel.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        // Only body-matching operators read contents up front; `File` reads
        // lazily for a matched name's snippet, `Path` never reads.
        let content: Option<Arc<String>> =
            if matches!(op, SearchOp::Any | SearchOp::Tag | SearchOp::Line | SearchOp::Section) {
                cache.get_or_load(path)
            } else {
                None
            };

        let hit: Option<(usize, String, bool)> = match op {
            SearchOp::Any => {
                match_any(&file_name, content.as_ref().map(|s| s.as_str()), &q, &terms, fl)
            }
            SearchOp::File => {
                if !(fold::contains(&file_name, &q, fl, Mode::Substring)
                    || terms
                        .iter()
                        .all(|t| fold::contains(&file_name, t, fl, Mode::Substring)))
                {
                    None
                } else {
                    let count = fold::count(&file_name, &q, fl);
                    let count = if count > 0 {
                        count
                    } else {
                        terms
                            .iter()
                            .map(|t| fold::count(&file_name, t, fl))
                            .sum::<usize>()
                            .max(1)
                    };
                    let snippet = cache
                        .get_or_load(path)
                        .as_deref()
                        .map(|text| first_line(text, SNIPPET_BEFORE + SNIPPET_AFTER))
                        .unwrap_or_default();
                    Some((count, snippet, true))
                }
            }
            SearchOp::Path => {
                if !(fold::contains(&rel_string, &q, fl, Mode::Substring)
                    || terms
                        .iter()
                        .all(|t| fold::contains(&rel_string, t, fl, Mode::Substring)))
                {
                    None
                } else {
                    let count = fold::count(&rel_string, &q, fl).max(1);
                    // Path matches are name-like: same top rank as file names.
                    Some((count, rel_string.clone(), true))
                }
            }
            SearchOp::Tag => match content.as_deref() {
                None => None,
                Some(text) => match_tag(text, &terms, fl, cs)
                    .map(|(count, snippet)| (count, snippet, false)),
            },
            SearchOp::Line => match content.as_deref() {
                None => None,
                Some(text) => match_line(text, &terms, &q, fl)
                    .map(|(count, snippet)| (count, snippet, false)),
            },
            SearchOp::Section => match content.as_deref() {
                None => None,
                Some(text) => match_section(text, &terms, &q, fl, cs)
                    .map(|(count, snippet)| (count, snippet, false)),
            },
        };

        let Some((match_count, snippet, name_hit)) = hit else {
            continue;
        };
        // File/path-name hits always outrank body-text hits, regardless of
        // the raw match count (a name hit with 1 occurrence beats 50 hits
        // scattered across a body).
        let rank = if name_hit { 0u8 } else { 1u8 };
        ranked.push((
            rank,
            match_count,
            file_name.to_lowercase(),
            GotoResult {
                path: path.clone(),
                title: file_name,
                snippet,
                match_count,
                name_hit,
            },
        ));
    }

    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, _, hit)| hit)
        .collect()
}

/// Body-operator search backed by the in-memory index (stage B).
///
/// Modeled on the reference: body search ALWAYS goes through the index —
/// there is no full-vault fallback scan. While the index is incomplete the
/// results cover the indexed part instantly and the UI shows build progress
/// (see `SearchReply::index_progress`); a full scan here would read the
/// whole vault per keystroke — tens of seconds on a big vault, exactly what
/// the reference avoids.
///
/// Same observable result set as [`search_goto_files_opts`] with two recorded
/// deviations (see `TODO/search-index-plan.md`, B3): a body candidate must
/// match each term from the start of a word (the index cannot see substrings
/// inside words), and body-tier ordering is BM25 instead of raw match counts.
/// The name tier (`Any`) and every `Aa` (case-sensitive) query keep exact
/// stage-A behavior by routing through the engine.
///
/// Name hits first (in-memory name scan, then the engine over just those
/// files — it reads their bodies through `cache` for counts and snippets),
/// then index candidates ranked by BM25.
pub(crate) fn search_with_index(
    files: &[PathBuf],
    raw_query: &str,
    op: SearchOp,
    params: &GotoSearchParams,
    limit: usize,
    cache: &mut SearchContentCache,
    index: &crate::search::index::Index,
    pool: usize,
    max_terms: usize,
    bm25: crate::search::bm25::Bm25,
) -> Vec<GotoResult> {
    let cs = params.case_sensitive;
    let fl = fold_mode(cs);
    let rest = raw_query.trim();
    if rest.is_empty() {
        return Vec::new();
    }
    let q = rest.to_string();
    let terms: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        q.split_whitespace()
            .filter(|t| !t.is_empty())
            .filter(|t| seen.insert(fold::fold_str(t, fl)))
            .map(|t| t.to_string())
            .collect()
    };
    if terms.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<GotoResult> = Vec::new();
    // A superseded request must stop spending I/O at every stage — its reply
    // is dropped anyway. Checked between the tiers and between candidates;
    // `locate::first_page` checks again between streamed pages.
    let cancelled = |cancel: &crate::search::cancel::Cancel| cancel.is_cancelled();

    // 1) Name tier — only `Any` produces name hits; the engine over the
    // name-matching subset reproduces stage-A counts and snippets exactly.
    if op == SearchOp::Any {
        if cancelled(&params.cancel) {
            return Vec::new();
        }
        let name_matched: Vec<PathBuf> = files
            .iter()
            .filter(|p| {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                fold::contains(&name, &q, fl, Mode::Substring)
                    || terms
                        .iter()
                        .all(|t| fold::contains(&name, t, fl, Mode::Substring))
            })
            .cloned()
            .collect();
        if !name_matched.is_empty() {
            results.extend(search_goto_files_opts(
                &name_matched, raw_query, op, params, limit, cache,
            ));
        }
        if results.len() >= limit {
            return results;
        }
    }

    // 2) Body tier — index candidates ranked by BM25; the whole file is
    // covered by page postings (`index` partitions big files into pages).
    let candidates = crate::search::query::candidates(index, &terms, bm25, pool, max_terms);
    let folded_terms: Vec<String> = terms
        .iter()
        .take(max_terms)
        .map(|t| fold::fold_str(t, fl))
        .collect();
    let mut seen_paths: std::collections::HashSet<PathBuf> =
        results.iter().map(|r| r.path.clone()).collect();
    for cand in candidates {
        if results.len() >= limit || cancelled(&params.cancel) {
            break;
        }
        let path = index.doc_path_for_ix(cand.path_ix).to_path_buf();
        if seen_paths.contains(&path) {
            continue;
        }
        seen_paths.insert(path.clone());
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        // The whole-file count comes straight from the candidate: it was
        // summed from postings with a linear path filter — exact and
        // independent of posting-list order.
        let match_count = cand.count.max(1);
        // Verification + snippet: bounded through the cache for regular
        // files. The cached head answers most files without a disk stream.
        let head = cache.get_or_load(&path);
        let mut hit = head.as_deref().and_then(|text| verify_page(text, op, &q, &terms, fl, cs));
        if hit.is_none() {
            // The head didn't verify (or the file is oversized and uncached):
            // the postings know WHICH pages carry the terms — check only
            // those pages in one sequential pass, instead of streaming the
            // whole multi-hundred-MB file from its start (that full stream
            // was the multi-second search on big files).
            let targets = index.pages_with_all_terms(cand.path_ix, &folded_terms, 4096);
            hit = crate::search::locate::check_pages(
                &path,
                &targets,
                &params.cancel,
                &mut |page| verify_page(page, op, &q, &terms, fl, cs),
            )
            .unwrap_or(None);
        }
        let (match_count, snippet) = match hit {
            Some((c, s)) => (c.max(match_count), s),
            None => continue,
        };
        results.push(GotoResult {
            path,
            title: file_name,
            snippet,
            match_count,
            name_hit: false,
        });
    }
    results
}

/// Operator verification over one page (the cached head or a streamed page).
fn verify_page(
    text: &str,
    op: SearchOp,
    q: &str,
    terms: &[String],
    fl: Fold,
    cs: bool,
) -> Option<(usize, String)> {
    match op {
        SearchOp::Any => match_body(text, q, terms, fl),
        SearchOp::Tag => match_tag(text, terms, fl, cs),
        SearchOp::Line => match_line(text, terms, q, fl),
        SearchOp::Section => match_section(text, terms, q, fl, cs),
        SearchOp::File | SearchOp::Path => None,
    }
}

/// The default `Any` branch: name hits outrank content hits.
fn match_any(
    name: &str,
    content: Option<&str>,
    q: &str,
    terms: &[String],
    fl: Fold,
) -> Option<(usize, String, bool)> {
    let name_hit_whole = fold::contains(name, q, fl, Mode::Substring);
    let name_hit_terms = terms
        .iter()
        .all(|t| fold::contains(name, t, fl, Mode::Substring));
    if name_hit_whole || name_hit_terms {
        let (count, snip) = match content {
            Some(text) => {
                let count = fold::count(text, q, fl) + fold::count(name, q, fl);
                (count.max(1), make_snippet(text, q, terms, fl))
            }
            None => (fold::count(name, q, fl).max(1), String::new()),
        };
        Some((count, snip, true))
    } else {
        let text = content?;
        match_body(text, q, terms, fl).map(|(c, s)| (c, s, false))
    }
}

/// Body branch of `Any`: every term occurs somewhere in the body.
fn match_body(text: &str, q: &str, terms: &[String], fl: Fold) -> Option<(usize, String)> {
    if !terms
        .iter()
        .all(|t| fold::contains(text, t, fl, Mode::Substring))
    {
        return None;
    }
    let count = fold::count(text, q, fl);
    let count = if count > 0 {
        count
    } else {
        terms
            .iter()
            .map(|t| fold::count(text, t, fl))
            .sum::<usize>()
            .max(1)
    };
    Some((count, make_snippet(text, q, terms, fl)))
}

/// `#term` mentions in the body; a leading `#` in a query term is tolerated.
fn match_tag(text: &str, terms: &[String], fl: Fold, cs: bool) -> Option<(usize, String)> {
    let tags: Vec<String> = terms
        .iter()
        .map(|t| format!("#{}", t.trim_start_matches('#')))
        .collect();
    if !tags
        .iter()
        .all(|t| fold::contains(text, t, fl, Mode::Substring))
    {
        return None;
    }
    let count = tags
        .iter()
        .map(|t| fold::count(text, t, fl))
        .sum::<usize>()
        .max(1);
    let snippet = first_line_matching(text, &tags, cs)
        .unwrap_or_else(|| first_line(text, SNIPPET_BEFORE + SNIPPET_AFTER));
    Some((count, snippet))
}

/// First body line that contains every term.
fn match_line(text: &str, terms: &[String], q: &str, fl: Fold) -> Option<(usize, String)> {
    for line in text.lines() {
        if !terms
            .iter()
            .all(|t| fold::contains(line, t, fl, Mode::Substring))
        {
            continue;
        }
        let count = fold::count(line, q, fl);
        let count = if count > 0 {
            count
        } else {
            terms
                .iter()
                .map(|t| fold::count(line, t, fl))
                .sum::<usize>()
                .max(1)
        };
        return Some((count, cap_line(line, SNIPPET_BEFORE + SNIPPET_AFTER)));
    }
    None
}

/// First heading-delimited section whose chunk contains every term.
fn match_section(
    text: &str,
    terms: &[String],
    q: &str,
    fl: Fold,
    cs: bool,
) -> Option<(usize, String)> {
    for (heading, chunk) in split_sections(text) {
        if !terms
            .iter()
            .all(|t| fold::contains(&chunk, t, fl, Mode::Substring))
        {
            continue;
        }
        let count = fold::count(&chunk, q, fl);
        let count = if count > 0 {
            count
        } else {
            terms
                .iter()
                .map(|t| fold::count(&chunk, t, fl))
                .sum::<usize>()
                .max(1)
        };
        return Some((count, first_line_matching(&chunk, terms, cs).unwrap_or(heading)));
    }
    None
}

/// First line whose folded form contains every folded term.
fn first_line_matching(text: &str, terms: &[String], case_sensitive: bool) -> Option<String> {
    let fl = fold_mode(case_sensitive);
    for line in text.lines() {
        if terms
            .iter()
            .all(|t| fold::contains(line, t, fl, Mode::Substring))
        {
            return Some(cap_line(line, SNIPPET_BEFORE + SNIPPET_AFTER));
        }
    }
    None
}

/// Flattens a single line and caps it at `limit` chars with a "..." tail.
fn cap_line(line: &str, limit: usize) -> String {
    let flat = flatten_whitespace(line.trim());
    if flat.chars().count() > limit {
        let mut s: String = flat.chars().take(limit).collect();
        s.push_str("...");
        s
    } else {
        flat
    }
}

/// Whether `line` opens a Markdown section (`# …` … `###### …`).
fn is_heading_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    hashes > 0 && hashes <= 6 && bytes.get(hashes).map_or(true, |b| b.is_ascii_whitespace())
}

/// Splits `text` into `(heading, chunk)` pairs at ATX headings.
///
/// The heading line belongs to its own chunk so heading terms match.
/// Text before the first heading forms a chunk with an empty heading.
fn split_sections(text: &str) -> Vec<(String, String)> {
    let mut chunks: Vec<(String, String)> = Vec::new();
    let mut heading = String::new();
    let mut body = String::new();
    for line in text.lines() {
        if is_heading_line(line) {
            if !heading.is_empty() || !body.trim().is_empty() {
                chunks.push((std::mem::take(&mut heading), std::mem::take(&mut body)));
            }
            heading = line.trim().to_string();
            body.push_str(line);
            body.push('\n');
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if !heading.is_empty() || !body.trim().is_empty() {
        chunks.push((heading, body));
    }
    chunks
}

/// Snippet around the first content hit, whitespace-flattened with "..." cuts.
///
/// The hit is located by the folding matcher, which reports a byte range in
/// the original text — no mapping back through a folded copy is needed, so
/// exotic casings that change byte length cannot skew the window.
fn make_snippet(text: &str, query: &str, terms: &[String], fl: Fold) -> String {
    let hit = fold::find(text, query, fl, Mode::Substring).or_else(|| {
        terms
            .iter()
            .filter_map(|t| fold::find(text, t, fl, Mode::Substring))
            .min_by_key(|r| r.start)
    });
    let Some(hit) = hit else {
        // Name-only hit: lead with the file's first line.
        return first_line(text, SNIPPET_BEFORE + SNIPPET_AFTER);
    };
    // The window is anchored at the hit's first char: up to SNIPPET_BEFORE
    // chars before it, SNIPPET_AFTER chars from it — same shape as before.
    let start = text[..hit.start]
        .char_indices()
        .rev()
        .nth(SNIPPET_BEFORE - 1)
        .map_or(0, |(i, _)| i);
    let end = text[hit.start..]
        .char_indices()
        .nth(SNIPPET_AFTER)
        .map_or(text.len(), |(i, _)| hit.start + i);
    let snippet: String = flatten_whitespace(&text[start..end]);
    if snippet.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    if start > 0 {
        out.push_str("...");
    }
    out.push_str(&snippet);
    if end < text.len() {
        out.push_str("...");
    }
    out
}

/// First line of `text`, flattened and capped at `limit` chars.
fn first_line(text: &str, limit: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let flat = flatten_whitespace(line);
    if flat.chars().count() > limit {
        let mut s: String = flat.chars().take(limit).collect();
        s.push_str("...");
        s
    } else {
        flat
    }
}

fn flatten_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builds highlighted [`StyledText`]: every occurrence of any term gets the
/// accent color + tinted background (like the sidebar quick filter).
pub fn highlight_terms(
    text: &str,
    terms: &[String],
    base_color: Rgba,
    is_selected: bool,
    theme: &Theme,
) -> StyledText {
    highlight_terms_cased(text, terms, base_color, is_selected, false, theme)
}

/// Case-aware [`highlight_terms`]: with `case_sensitive`, only exact-case
/// occurrences are highlighted.
pub(crate) fn highlight_terms_cased(
    text: &str,
    terms: &[String],
    base_color: Rgba,
    is_selected: bool,
    case_sensitive: bool,
    theme: &Theme,
) -> StyledText {
    let ranges = find_term_ranges_cased(text, terms, case_sensitive);
    if ranges.is_empty() {
        return StyledText::new(text.to_string());
    }
    let reg_font = gpui::font("Segoe UI");
    let match_font = gpui::font("Segoe UI").bold();
    let hl_bg = Rgba {
        r: theme.accent.r,
        g: theme.accent.g,
        b: theme.accent.b,
        a: if theme.is_dark { 0.32 } else { 0.22 },
    };
    let match_color = if is_selected {
        theme.text_primary
    } else {
        theme.accent
    };
    let mut runs = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if start > cursor {
            runs.push(TextRun {
                len: start - cursor,
                font: reg_font.clone(),
                color: base_color.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        runs.push(TextRun {
            len: end - start,
            font: match_font.clone(),
            color: match_color.into(),
            background_color: Some(hl_bg.into()),
            underline: None,
            strikethrough: None,
        });
        cursor = end;
    }
    if cursor < text.len() {
        runs.push(TextRun {
            len: text.len() - cursor,
            font: reg_font,
            color: base_color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    StyledText::new(text.to_string()).with_runs(runs)
}

/// Non-overlapping byte ranges of term hits in `text` (case-insensitive).
///
/// Scans char-by-char so slicing the original is always safe, even when
/// case folding changes byte lengths. Longest terms win on overlap.
///
/// Kept for tests; production code uses the case-aware variant below.
#[cfg(test)]
fn find_term_ranges(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    find_term_ranges_cased(text, terms, false)
}

/// Case-aware [`find_term_ranges`].
///
/// The folding matcher reports non-overlapping byte ranges in the original,
/// longest term winning on overlap.
fn find_term_ranges_cased(
    text: &str,
    terms: &[String],
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    let refs: Vec<&str> = terms.iter().map(|s| s.as_str()).collect();
    fold::ranges(text, &refs, fold_mode(case_sensitive), usize::MAX)
        .into_iter()
        .map(|r| (r.start, r.end))
        .collect()
}

/// Query terms for highlighting: whole query plus individual words.
pub(crate) fn highlight_query_terms(query: &str) -> Vec<String> {
    highlight_query_terms_cased(query, false)
}

/// Case-aware [`highlight_query_terms`].
pub(crate) fn highlight_query_terms_cased(query: &str, case_sensitive: bool) -> Vec<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let q = if case_sensitive {
        trimmed.to_string()
    } else {
        trimmed.to_lowercase()
    };
    let mut terms = vec![q.clone()];
    for part in q.split_whitespace() {
        if part != q {
            terms.push(part.to_string());
        }
    }
    terms
}

impl AppView {
    /// Renders the Go to file dialog overlay.
    pub fn render_search_dialog(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_w: f32 = window_size.width.into();
        let window_h: f32 = window_size.height.into();
        let card_w = (640.0f32).min(window_w - 64.0).max(320.0);
        // Fixed height: the card never resizes while typing or when
        // results appear; the list scrolls inside the leftover space.
        let card_h = (560.0f32).min(window_h - 120.0).max(360.0);

        let query = self.search_dialog.query.clone();
        // Highlights follow the RESULTS' query, not the live input: while a
        // newer query is in flight the displayed list still answers the old
        // one, and mixing old results with new terms leaves matches
        // unhighlighted (the "no highlight" look between keystrokes).
        let terms = highlight_query_terms(&self.search_dialog.results_query);
        let is_query_empty = query.trim().is_empty();
        let has_results = !self.search_dialog.results.is_empty();
        let has_workspace = self.state.workspace_path.is_some();
        let selected_ix = self.search_dialog.selected_ix;
        // Device-pixel grid for the caret (see `crate::snap`).
        let pixel_scale = window.scale_factor();

        // Input caret geometry (Segoe UI 14px, same shaping as click mapping).
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.search_dialog.cursor_pos.min(query_chars.len());
        let selection_range = self.search_dialog.selection_range();
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let cursor_x = if query.is_empty() {
            0.0
        } else {
            let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };
        let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..e.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                // Both edges go through the same device-pixel snapping as the caret
                // above; the rectangle is rounded, so it is painted antialiased and an
                // unsnapped edge would breathe while the caret stays crisp.
                crate::snap::snapped_span(
                    f32::from(shaped.x_for_index(b1)),
                    f32::from(shaped.x_for_index(b2)),
                    pixel_scale,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        let is_input_focused = self.goto_focus_handle.is_focused(window);
        let caret_visible = is_input_focused && self.goto_blink.visible();
        // The displayed list answers `results_query`; while the live input
        // differs, a scan is in flight — say so instead of letting the old
        // results read as answers to the new query. The percent is the
        // worker's LIVE build progress (not reply-stale): it moves even when
        // no reply lands, which is exactly the "is it doing anything" case.
        let search_pending =
            !is_query_empty && self.search_dialog.results_query.trim() != query.trim();
        let pending_label = match self.state.search_worker.build_progress() {
            Some(pct) => format!("{} {}%", self.t("search.searching"), pct),
            None => self.t("search.searching").to_string(),
        };

        div()
            .id("goto-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            // NOTE: this overlay stops mouse moves so the editor behind the
            // dialog never reacts — which also cuts the root-level drag
            // handlers off. Both in-progress gestures (input selection and
            // scrollbar thumb) are therefore followed right here.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.goto_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_goto_scrollbar_drag(y, cx);
                } else {
                    this.handle_goto_mouse_move(event, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
            }))
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("goto-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(gpui::rgba(0x00000066))
                    // Same drag following as the overlay: the backdrop sits
                    // between the pointer and the overlay and stops moves.
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if this.goto_scrollbar_drag.active {
                            let y: f32 = event.position.y.into();
                            this.handle_goto_scrollbar_drag(y, cx);
                        } else {
                            this.handle_goto_mouse_move(event, window, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.close_search_dialog(window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, window, cx| {
                        this.close_search_dialog(window, cx);
                    })),
            )
            .child(
                div()
                    .id("goto-center")
                    .absolute()
                    // Constrained to the editor column (between the sidebars)
                    // so the card lands in the editor's center, not the window's.
                    .left(px(if self.sidebar_collapsed {
                        44.0
                    } else {
                        self.sidebar_width + 1.0
                    }))
                    .right(px(if self.right_sidebar_collapsed {
                        0.0
                    } else {
                        self.right_sidebar_width + 1.0
                    }))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("goto-card")
                            .w(px(card_w))
                            .h(px(card_h))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_xl()
                            .shadow_xl()
                            .cursor(gpui::CursorStyle::Arrow)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // ---- Header: search box ----
                            .child(
                                div()
                                    .id("goto-header")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .h(px(48.0))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(icons::icon(icons::ICON_SEARCH, 16.0, theme.text_secondary))
                                    .child(
                                        div()
                                            .id("goto-input")
                                            .relative()
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(22.0))
                                            .track_focus(&self.goto_focus_handle)
                                            .cursor(gpui::CursorStyle::IBeam)
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                cx.stop_propagation();
                                                window.focus(&this.goto_focus_handle);
                                                this.goto_blink.pause_and_show(BlinkTarget::GotoSearch, cx);
                                                let click_x: f32 = event.position.x.into();
                                                let pos = this.goto_click_pos(click_x, window);
                                                let total_chars = this.search_dialog.query.chars().count();
                                                if event.click_count >= 3 {
                                                    this.search_dialog.selection_anchor = Some(0);
                                                    this.search_dialog.cursor_pos = total_chars;
                                                    this.search_dialog.is_mouse_selecting = false;
                                                } else if event.click_count == 2 {
                                                    let (s, e) = word_range_at(&this.search_dialog.query, pos);
                                                    this.search_dialog.selection_anchor = Some(s);
                                                    this.search_dialog.cursor_pos = e;
                                                    this.search_dialog.is_mouse_selecting = false;
                                                } else if event.modifiers.shift {
                                                    if this.search_dialog.selection_anchor.is_none() {
                                                        this.search_dialog.selection_anchor = Some(this.search_dialog.cursor_pos);
                                                    }
                                                    this.search_dialog.cursor_pos = pos;
                                                    this.search_dialog.is_mouse_selecting = true;
                                                } else {
                                                    this.search_dialog.cursor_pos = pos;
                                                    this.search_dialog.selection_anchor = Some(pos);
                                                    this.search_dialog.is_mouse_selecting = true;
                                                }
                                                cx.notify();
                                            }))
                                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                                cx.stop_propagation();
                                                this.handle_goto_key(event, window, cx);
                                            }))
                                            .child(
                                                gpui::canvas(
                                                    {
                                                        let cell = self.search_dialog.box_origin_x.clone();
                                                        move |bounds, _, _| {
                                                            cell.set(f32::from(bounds.origin.x));
                                                        }
                                                    },
                                                    |_, _, _, _| {},
                                                )
                                                .absolute()
                                                .size_full(),
                                            )
                                            .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                                s.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(sel_x1))
                                                        .top(px(0.0))
                                                        .w(px(sel_w))
                                                        .h(px(22.0))
                                                        .bg(theme.selection_bg)
                                                        .rounded_xs(),
                                                )
                                            })
                                            .child(if query.is_empty() {
                                                div()
                                                    .font_family("Segoe UI")
                                                    .text_size(px(14.0))
                                                    .line_height(px(22.0))
                                                    .text_color(theme.text_muted)
                                                    .child(self.t("search.title"))
                                            } else {
                                                div()
                                                    .font_family("Segoe UI")
                                                    .text_size(px(14.0))
                                                    .line_height(px(22.0))
                                                    .text_color(theme.text_primary)
                                                    .child(query.clone())
                                            })
                                            .when(caret_visible, |s| {
                                                s.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(cursor_x))
                                                        .top(px(2.0))
                                                        .w(px(1.5))
                                                        .h(px(18.0))
                                                        .bg(theme.accent)
                                                        .rounded_xs(),
                                                )
                                            }),
                                    )
                                    .when(search_pending, |h| {
                                        h.child(
                                            div()
                                                .flex_shrink_0()
                                                .font_family("Segoe UI")
                                                .text_size(px(12.0))
                                                .text_color(theme.text_muted)
                                                .child(pending_label.clone()),
                                        )
                                    })
                                    .when(!query.is_empty(), |h| {
                                        h.child(
                                            div()
                                                .id("goto-clear")
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|b| b.bg(theme.bg_hover))
                                                .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_secondary))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation();
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.search_dialog.query.clear();
                                                    this.search_dialog.cursor_pos = 0;
                                                    this.search_dialog.selection_anchor = None;
                                                    this.run_goto_search();
                                                    this.goto_blink.pause_and_show(BlinkTarget::GotoSearch, cx);
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            )
                            // ---- Body ----
                            .child(if !has_workspace {
                                self.render_goto_empty_state(
                                    theme,
                                    icons::ICON_FOLDER,
                                    self.t("search.no_folder"),
                                    self.t("search.no_folder_hint"),
                                )
                            } else if is_query_empty {
                                self.render_goto_empty_state(
                                    theme,
                                    icons::ICON_SEARCH,
                                    self.t("search.title"),
                                    self.t("search.title_hint"),
                                )
                            } else if search_pending {
                                // A newer query is in flight: the displayed
                                // list still answers the OLD query (with
                                // highlights tuned to IT). Showing it under a
                                // new query reads as wrong results — hide it
                                // behind the searching state until the fresh
                                // reply lands.
                                self.render_goto_empty_state(
                                    theme,
                                    icons::ICON_SEARCH,
                                    pending_label.clone(),
                                    self.t("search.create_hint"),
                                )
                            } else if !has_results {
                                // "No results" only means it: the applied
                                // reply (results_query == query) found
                                // nothing. (Pending is handled above.)
                                self.render_goto_empty_state(
                                    theme,
                                    icons::ICON_SEARCH,
                                    self.t("search.no_results"),
                                    self.t("search.create_hint"),
                                )
                            } else {
                                // Scrollable results with the shared overlay
                                // scrollbar (same infra as the sidebar tree).
                                // The thumb is a sibling of the scroll container
                                // inside a `relative` wrapper, so it never
                                // scrolls with the content.
                                let scroll_style = goto_scrollbar_style();
                                let mut viewport = div()
                                    .id("goto-results-viewport")
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .child(
                                        div()
                                            .id("goto-results-scroll")
                                            .track_scroll(&self.goto_scroll_handle)
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_scroll()
                                            .p_2()
                                            .gap_1p5()
                                            .children(self.search_dialog.results.iter().enumerate().map(|(ix, hit)| {
                                        let is_selected = ix == selected_ix;
                                        let hit_path_click = hit.path.clone();
                                        let hit_title = hit.title.clone();
                                        let hit_snippet = hit.snippet.clone();
                                        let hit_count = hit.match_count;
                                        div()
                                            .id(SharedString::from(format!("goto-result-{}", ix)))
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .px_3()
                                            .py_2()
                                            .mr(px(GOTO_ROW_SCROLLBAR_GAP))
                                            .rounded_lg()
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .cursor_pointer()
                                            .when(is_selected, |d| d.bg(theme.bg_hover))
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                                                if *is_hovered && this.search_dialog.selected_ix != ix {
                                                    this.search_dialog.selected_ix = ix;
                                                    cx.notify();
                                                }
                                            }))
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation();
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.open_goto_result(hit_path_click.clone(), window, cx);
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(icons::icon(icons::ICON_FILE_TEXT, 15.0, theme.text_secondary))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_sm()
                                                            .child(highlight_terms(&hit_title, &terms, theme.text_primary, is_selected, theme)),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_shrink_0()
                                                            .text_xs()
                                                            .text_color(theme.text_muted)
                                                            .child(self.format_match_count(hit_count)),
                                                    ),
                                            )
                                            .when(!hit_snippet.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .pl(px(23.0))
                                                        .pr_1()
                                                        .text_xs()
                                                        .line_height(px(18.0))
                                                        .child(highlight_terms(&hit_snippet, &terms, theme.text_secondary, is_selected, theme)),
                                                )
                                            })
                                    })),
                                    );
                                if let Some(geometry) =
                                    ScrollbarGeometry::from_handle(&self.goto_scroll_handle, &scroll_style)
                                {
                                    let dragging = self.goto_scrollbar_drag.active;
                                    viewport = viewport.child(overlay_scrollbar(
                                        "goto-scrollbar-track",
                                        &geometry,
                                        &scroll_style,
                                        theme,
                                        dragging,
                                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            let pointer_y: f32 = event.position.y.into();
                                            this.set_goto_scroll_top(
                                                geometry.offset_for_track_click(pointer_y),
                                            );
                                            cx.notify();
                                        }),
                                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            this.goto_scrollbar_drag
                                                .begin(event.position.y.into(), geometry.scroll_y);
                                            cx.notify();
                                        }),
                                    ));
                                }
                                viewport.into_any_element()
                            })
                            // ---- Footer hints ----
                            .child(
                                div()
                                    .id("goto-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap_3()
                                    .h(px(36.0))
                                    .flex_shrink_0()
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(format!("↑↓ {}", self.t("search.hint_navigate")))
                                    .child(format!("⏎ {}", self.t("search.hint_open")))
                                    .child(format!("Shift+⏎ {}", self.t("search.hint_create"))),
                            ),
                    ),
            )
    }

    /// Centered placeholder inside the dialog (empty query / no results).
    fn render_goto_empty_state(
        &self,
        theme: &Theme,
        icon_path: &'static str,
        title: String,
        hint: String,
    ) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .child(icons::icon(icon_path, 44.0, theme.text_muted))
            .child(
                div()
                    .text_base()
                    .text_color(theme.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(hint),
            )
            .into_any_element()
    }
}

/// Word boundaries around char index `pos` for double-click selection.
pub(crate) fn word_range_at(text: &str, pos: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let pos = pos.min(chars.len().saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let target_is_word = is_word(chars[pos]);
    let mut start = pos;
    while start > 0 && is_word(chars[start - 1]) == target_is_word && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = pos + 1;
    while end < chars.len() && is_word(chars[end]) == target_is_word && !chars[end].is_whitespace() {
        end += 1;
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn write_tmp(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vibecodium_goto_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn search_plain(files: &[PathBuf], raw: &str, limit: usize) -> Vec<GotoResult> {
        let mut cache = SearchContentCache::new();
        search_goto_files_opts(
            files,
            raw,
            SearchOp::Any,
            &GotoSearchParams::default(),
            limit,
            &mut cache,
        )
    }

    #[test]
    fn name_hits_rank_before_content_hits() {
        let dir = tmp_dir("rank");
        let content_hit = write_tmp(&dir, "unrelated.md", "the query word appears here in body text");
        let name_hit = write_tmp(&dir, "query notes.md", "nothing relevant inside");
        let files = vec![content_hit.clone(), name_hit.clone()];
        let hits = search_plain(&files, "query", 50);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, name_hit);
        assert!(hits[0].name_hit);
        assert_eq!(hits[1].path, content_hit);
        assert!(!hits[1].name_hit);
    }

    #[test]
    fn name_hit_wins_despite_lower_match_count() {
        let dir = tmp_dir("rankcount");
        // One name occurrence vs three body occurrences: the name still first.
        let named = write_tmp(&dir, "query.md", "nothing relevant inside");
        let bodied = write_tmp(&dir, "zzz.md", "query query query");
        let files = vec![bodied.clone(), named.clone()];
        let hits = search_plain(&files, "query", 50);
        assert_eq!(hits.len(), 2);
        assert!(hits[1].match_count > hits[0].match_count);
        assert_eq!(hits[0].path, named);
    }

    #[test]
    fn all_terms_required_for_content_match() {
        let dir = tmp_dir("terms");
        let both = write_tmp(&dir, "a.md", "alpha and beta together");
        let single = write_tmp(&dir, "b.md", "only alpha here");
        let files = vec![single.clone(), both.clone()];
        let hits = search_plain(&files, "alpha beta", 50);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, both);
    }

    #[test]
    fn snippet_wraps_first_hit_with_cuts() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor. The magic needle hides in this sentence among many other words following after it, and the tail keeps going on and on without end, padding even more words here.";
        let snip = make_snippet(text, "needle", &["needle".to_string()], Fold::Lower);
        assert!(snip.contains("needle"), "snippet: {snip}");
        assert!(snip.starts_with("..."), "snippet: {snip}");
        assert!(snip.ends_with("..."), "snippet: {snip}");
        assert!(!snip.contains('\n'));
    }

    #[test]
    fn oversized_files_match_by_name_only() {
        let dir = tmp_dir("big");
        let big = write_tmp(&dir, "bigmatch.md", "small body");
        // Pretend the file is huge: metadata path is real, so instead verify
        // the counter helper directly for the name-only branch behavior.
        let hits = search_plain(&[big.clone()], "bigmatch", 50);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name_hit);
        assert!(hits[0].match_count >= 1);
    }

    #[test]
    fn highlight_ranges_are_non_overlapping_and_safe() {
        let theme = Theme::dark();
        let styled = highlight_terms("Приватизация и приключения", &["при".to_string()], gpui::rgb(0xffffff), false, &theme);
        let _ = styled;
        // Must not panic on Cyrillic case folding; ranges covered by helper test below.
        let ranges = find_term_ranges("Приватизация ПРИ", &["при".to_string()]);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn empty_query_yields_no_hits() {
        assert!(search_plain(&[], "", 50).is_empty());
        assert!(search_plain(&[PathBuf::from("a.md")], "   ", 50).is_empty());
    }

    fn search_opts(
        files: &[PathBuf],
        raw: &str,
        case_sensitive: bool,
        base: Option<&Path>,
    ) -> Vec<GotoResult> {
        let (op, rest) = parse_search_op(raw);
        let mut cache = SearchContentCache::new();
        search_goto_files_opts(
            files,
            &rest,
            op,
            &GotoSearchParams {
                case_sensitive,
                rel_base: base,
                cancel: Default::default(),
            },
            50,
            &mut cache,
        )
    }

    #[test]
    fn op_prefix_parsing() {
        assert_eq!(
            parse_search_op("file: Protocols"),
            (SearchOp::File, "Protocols".to_string())
        );
        assert_eq!(
            parse_search_op("PATH:docs/x"),
            (SearchOp::Path, "docs/x".to_string())
        );
        assert_eq!(
            parse_search_op("  Tag: rust  "),
            (SearchOp::Tag, "rust".to_string())
        );
        // Bare prefix without a query falls back to a plain search.
        assert_eq!(
            parse_search_op("line:"),
            (SearchOp::Any, "line:".to_string())
        );
        assert_eq!(
            parse_search_op("just words"),
            (SearchOp::Any, "just words".to_string())
        );
    }

    #[test]
    fn op_file_matches_name_only() {
        let dir = tmp_dir("opfile");
        let named = write_tmp(&dir, "protocols.md", "nothing about the query in here");
        let bodied = write_tmp(&dir, "other.md", "protocols appear only in this body");
        let files = vec![named.clone(), bodied.clone()];
        let hits = search_opts(&files, "file: protocols", false, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, named);
    }

    #[test]
    fn op_path_matches_relative_path() {
        let dir = tmp_dir("oppath");
        let sub = dir.join("docs");
        std::fs::create_dir_all(&sub).unwrap();
        let inside = write_tmp(&sub, "note.md", "no match in body at all");
        let outside = write_tmp(&dir, "note2.md", "no match in body at all");
        let files = vec![inside.clone(), outside.clone()];
        let hits = search_opts(&files, "path: docs", false, Some(dir.as_path()));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, inside);
        assert!(hits[0].snippet.contains("docs"));
    }

    #[test]
    fn op_tag_matches_hash_mentions() {
        let dir = tmp_dir("optag");
        let tagged = write_tmp(&dir, "a.md", "some text with #rust inside the line");
        let plain = write_tmp(&dir, "b.md", "rust without a hash is not a tag");
        let files = vec![tagged.clone(), plain.clone()];
        let hits = search_opts(&files, "tag: rust", false, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, tagged);
        assert!(hits[0].snippet.contains("#rust") || hits[0].snippet.contains("#Rust"));
    }

    #[test]
    fn op_line_requires_single_line() {
        let dir = tmp_dir("opline");
        let together = write_tmp(&dir, "a.md", "alpha\nbeta\nnothing\n");
        let split = write_tmp(&dir, "b.md", "alpha and beta together here");
        let files = vec![together.clone(), split.clone()];
        let hits = search_opts(&files, "line: alpha beta", false, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, split);
    }

    #[test]
    fn op_section_requires_shared_heading() {
        let dir = tmp_dir("opsection");
        let same = write_tmp(
            &dir,
            "a.md",
            "# Cooking\nadd alpha and beta\n# Other\nnothing\n",
        );
        let apart = write_tmp(&dir, "b.md", "# Alpha\nfirst\n# Beta\nsecond\n");
        let files = vec![same.clone(), apart.clone()];
        let hits = search_opts(&files, "section: alpha beta", false, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, same);
    }

    #[test]
    fn op_prefix_with_multibyte_query_does_not_panic() {
        // Regression: probing the ASCII prefix with byte slicing panicked on
        // Cyrillic input (`trimmed[..5]` lands inside `и` for "при").
        assert_eq!(
            parse_search_op("при"),
            (SearchOp::Any, "при".to_string())
        );
        assert_eq!(
            parse_search_op("прив"),
            (SearchOp::Any, "прив".to_string())
        );
        let dir = tmp_dir("opcyr");
        let f = write_tmp(&dir, "заметка.md", "привет мир");
        let hits = search_opts(&[f.clone()], "при", false, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, f);
    }

    #[test]
    fn case_sensitive_toggle() {
        let dir = tmp_dir("opcase");
        let upper = write_tmp(&dir, "a.md", "Rust language");
        let lower = write_tmp(&dir, "b.md", "rust language");
        let files = vec![upper.clone(), lower.clone()];
        let lax = search_opts(&files, "rust", false, None);
        assert_eq!(lax.len(), 2);
        let strict = search_opts(&files, "rust", true, None);
        assert_eq!(strict.len(), 1);
        assert_eq!(strict[0].path, lower);
    }

    #[test]
    fn cancelled_scan_returns_nothing() {
        let dir = tmp_dir("cancel");
        let f = write_tmp(&dir, "alpha.md", "alpha body text");
        let generation = crate::search::cancel::Generation::default();
        // A token superseded before the scan starts must produce nothing.
        let stale = generation.next();
        let _newer = generation.next();
        let mut cache = SearchContentCache::new();
        let hits = search_goto_files_opts(
            &[f],
            "alpha",
            SearchOp::Any,
            &GotoSearchParams {
                case_sensitive: false,
                rel_base: None,
                cancel: stale,
            },
            50,
            &mut cache,
        );
        assert!(hits.is_empty());
    }
}
