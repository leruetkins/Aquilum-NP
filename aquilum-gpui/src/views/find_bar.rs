//! State of the in-editor find/replace bar (Ctrl+F).
//!
//! The bar itself is painted by `AppView::render_find_bar`; this module owns
//! the query fields, the computed matches and the chunked scanner used by the
//! background search, so the state can be kept next to the other input state
//! on `AppView`.

use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gpui::FocusHandle;

use crate::document_buffer::DocumentId;
use crate::views::text_input::TextInput;

mod scan;
pub use scan::*;

/// Hard cap on reported matches, matching the original Aquilum find bar.
/// The counter gets a `+` suffix when the scan stopped at this cap.
pub const MAX_FIND_MATCHES: usize = 19_999;

/// Bytes scanned per background slice. Small enough to notice cancellation
/// within a frame, large enough that the per-slice fixed cost disappears.
const SCAN_CHUNK: usize = 256 * 1024;

/// Matching rules of the find bar beyond the query text: the `Aa` toggle and
/// the whole-word toggle. Kept as one value so a result can remember the exact
/// rules it was computed with.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct FindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

impl FindOptions {
    fn fold(self) -> crate::search::fold::Fold {
        if self.case_sensitive {
            crate::search::fold::Fold::Exact
        } else {
            crate::search::fold::Fold::Lower
        }
    }

    fn mode(self) -> crate::search::fold::Mode {
        if self.whole_word {
            crate::search::fold::Mode::WholeWord
        } else {
            crate::search::fold::Mode::Substring
        }
    }
}

pub struct FindBarState {
    pub is_open: bool,
    pub query: TextInput,
    pub replace: TextInput,
    /// Whether the second (replace) row is expanded.
    pub replace_open: bool,
    /// Query matching is case-sensitive (`Aa` toggle).
    pub case_sensitive: bool,
    /// Query matches only whole words.
    pub whole_word: bool,
    /// Non-overlapping byte ranges of the active document's current query.
    pub matches: Vec<Range<usize>>,
    /// Index of the selected match in `matches`; `0` when there is none.
    pub active: usize,
    /// Query `matches` were computed from, so navigation can notice an edit
    /// to the query field.
    pub matches_query: String,
    /// Toggles `matches` were computed with, so flipping one rescans.
    pub matches_options: FindOptions,
    /// Document version `matches` were computed from, so edits in the editor
    /// invalidate the list instead of making Replace write to a stale range.
    pub matches_version: u64,
    /// The scan hit `MAX_FIND_MATCHES` and the list is incomplete.
    pub truncated: bool,
    /// Sequence of the latest search request. A background reply with an
    /// older number is dropped instead of overwriting newer results.
    pub search_seq: u64,
    /// A background scan is in flight; typing never waits for it.
    pub search_running: bool,
    /// A newer query arrived while a scan was running; rerun on completion.
    pub search_queued: bool,
    /// Cancels the running scan between slices when a newer query arrives.
    pub cancel: Arc<AtomicBool>,
    /// Query of the newest request (not necessarily of the installed list),
    /// so a repeated Enter does not restart a scan already on its way.
    pub search_query: String,
    /// Document version of the newest request, paired with `search_query`.
    pub search_version: u64,
    /// Toggles of the newest request, paired with `search_query`.
    pub search_options: FindOptions,
    /// Navigation asked for while the list was stale; applied on the reply.
    pub pending_step: Option<isize>,
    /// Replacement just happened at this offset; the reply activates the
    /// first match at or after it.
    pub pending_after: Option<usize>,
    /// Replace was pressed while the list was stale: the reply runs it once
    /// the ranges for the current document arrive, instead of dropping the
    /// click.
    pub pending_replace: bool,
    /// Replace All pass in progress: bytes decided so far and the document
    /// length, so the replace field can show a percentage instead of nothing.
    pub replace_all: Option<(usize, usize)>,
    /// Sequence of the latest Replace All request; a reply with an older
    /// number is dropped instead of being applied to the document.
    pub replace_all_seq: u64,
    /// Cancels the running Replace All pass between slices when the query
    /// changes or the bar closes.
    pub replace_cancel: Arc<AtomicBool>,
    /// Document lines marked by "Mark all", sorted and without duplicates.
    /// They only describe `marked_doc` at `marked_version`; any edit or another
    /// document makes them stale instead of silently pointing at other lines.
    pub marked_lines: Vec<usize>,
    /// Document the marked lines belong to.
    pub marked_doc: Option<DocumentId>,
    /// Revision the marked lines were computed from.
    pub marked_version: u64,
    /// "Mark all" was pressed while the match list was stale; the reply marks
    /// the fresh list instead of dropping the click.
    pub pending_mark_all: bool,
}

impl FindBarState {
    pub fn new(query_handle: FocusHandle, replace_handle: FocusHandle) -> Self {
        Self {
            is_open: false,
            query: TextInput::new(query_handle),
            replace: TextInput::new(replace_handle),
            replace_open: false,
            case_sensitive: false,
            whole_word: false,
            matches: Vec::new(),
            active: 0,
            matches_query: String::new(),
            matches_options: FindOptions::default(),
            matches_version: 0,
            truncated: false,
            search_seq: 0,
            search_running: false,
            search_queued: false,
            cancel: Arc::new(AtomicBool::new(false)),
            search_query: String::new(),
            search_version: 0,
            search_options: FindOptions::default(),
            pending_step: None,
            pending_after: None,
            pending_replace: false,
            replace_all: None,
            replace_all_seq: 0,
            replace_cancel: Arc::new(AtomicBool::new(false)),
            marked_lines: Vec::new(),
            marked_doc: None,
            marked_version: 0,
            pending_mark_all: false,
        }
    }

    /// Drops the marked lines. They describe a text revision that no longer
    /// exists (or a query that is no longer asked), so keeping them would put
    /// arrows next to unrelated lines.
    pub fn clear_marks(&mut self) {
        self.marked_lines.clear();
        self.marked_doc = None;
        self.marked_version = 0;
    }

    /// Whether there are marked lines to draw for `doc` at `version`.
    pub fn marks_valid_for(&self, doc: DocumentId, version: u64) -> bool {
        !self.marked_lines.is_empty()
            && self.marked_doc == Some(doc)
            && self.marked_version == version
    }

    /// Whether `line` of `doc` at `version` carries a "Mark all" marker.
    pub fn line_is_marked(&self, doc: DocumentId, version: u64, line: usize) -> bool {
        self.marks_valid_for(doc, version) && self.marked_lines.binary_search(&line).is_ok()
    }

    /// Stops a Replace All pass, if one is running: nothing of it will be
    /// applied and its background work stops at the next slice.
    pub fn cancel_replace_all(&mut self) {
        self.replace_all_seq = self.replace_all_seq.wrapping_add(1);
        self.replace_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.replace_all = None;
    }

    /// The `Aa` and whole-word toggles as one value, for comparing a result
    /// with the rules it was computed under.
    pub fn options(&self) -> FindOptions {
        FindOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        }
    }

    /// Counter label next to the query: `current/total`, `0/0` when empty.
    pub fn counter(&self) -> String {
        counter_label(self.active, self.matches.len(), self.truncated)
    }

    /// Progress of the running Replace All pass, shown at the right edge of
    /// the replace field. `None` when no pass is running, so the field keeps
    /// its normal look.
    pub fn replace_all_percent(&self) -> Option<String> {
        percent_label(self.replace_all)
    }
}

fn percent_label(replace_all: Option<(usize, usize)>) -> Option<String> {
    let (done, total) = replace_all?;
    let percent = if total == 0 {
        100
    } else {
        done.saturating_mul(100) / total
    };
    Some(format!("{percent}%"))
}

fn counter_label(active: usize, total: usize, truncated: bool) -> String {
    if total == 0 {
        return "0/0".to_string();
    }
    let current = active.min(total - 1) + 1;
    if truncated {
        format!("{current}/{total}+")
    } else {
        format!("{current}/{total}")
    }
}


#[cfg(test)]
mod tests;
