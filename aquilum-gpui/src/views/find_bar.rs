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

use crate::views::text_input::TextInput;

/// Hard cap on reported matches, matching the original Aquilum find bar.
/// The counter gets a `+` suffix when the scan stopped at this cap.
pub const MAX_FIND_MATCHES: usize = 19_999;

/// Bytes scanned per background slice. Small enough to notice cancellation
/// within a frame, large enough that the per-slice fixed cost disappears.
const SCAN_CHUNK: usize = 256 * 1024;

pub struct FindBarState {
    pub is_open: bool,
    pub query: TextInput,
    pub replace: TextInput,
    /// Whether the second (replace) row is expanded.
    pub replace_open: bool,
    /// Non-overlapping byte ranges of the active document's current query.
    pub matches: Vec<Range<usize>>,
    /// Index of the selected match in `matches`; `0` when there is none.
    pub active: usize,
    /// Query `matches` were computed from, so navigation can notice an edit
    /// to the query field.
    pub matches_query: String,
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
    /// Navigation asked for while the list was stale; applied on the reply.
    pub pending_step: Option<isize>,
    /// Replacement just happened at this offset; the reply activates the
    /// first match at or after it.
    pub pending_after: Option<usize>,
}

impl FindBarState {
    pub fn new(query_handle: FocusHandle, replace_handle: FocusHandle) -> Self {
        Self {
            is_open: false,
            query: TextInput::new(query_handle),
            replace: TextInput::new(replace_handle),
            replace_open: false,
            matches: Vec::new(),
            active: 0,
            matches_query: String::new(),
            matches_version: 0,
            truncated: false,
            search_seq: 0,
            search_running: false,
            search_queued: false,
            cancel: Arc::new(AtomicBool::new(false)),
            search_query: String::new(),
            search_version: 0,
            pending_step: None,
            pending_after: None,
        }
    }

    /// Counter label next to the query: `current/total`, `0/0` when empty.
    pub fn counter(&self) -> String {
        let total = self.matches.len();
        if total == 0 {
            return "0/0".to_string();
        }
        let current = self.active.min(total - 1) + 1;
        if self.truncated {
            format!("{current}/{total}+")
        } else {
            format!("{current}/{total}")
        }
    }
}

/// Case-insensitive substring scan over a rope without copying it.
///
/// Same contract as [`crate::search::fold::ranges`] over the whole text, but
/// the text is streamed through a small window: rope chunks (a couple of KB
/// each) are appended to a buffer that is cut at a scan-chunk boundary and
/// dropped after it has been scanned. Memory stays around one chunk plus the
/// context overlap however large the document is — converting a big document
/// to a `String` would double its memory on every keystroke.
pub fn scan_rope(
    rope: &crop::Rope,
    query: &str,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    if query.is_empty() || limit == 0 || rope.byte_len() == 0 {
        return found;
    }
    let overlap = query
        .chars()
        .count()
        .saturating_mul(12)
        .saturating_add(8)
        .min(SCAN_CHUNK);

    // Window starting at absolute `base`; everything before `cut` is owned by
    // this pass, everything after it is the context a crossing match needs.
    let mut window = String::new();
    let mut base = 0usize;
    let mut chunks = rope.chunks();
    let mut exhausted = false;
    loop {
        while window.len() < SCAN_CHUNK + overlap {
            match chunks.next() {
                Some(chunk) => window.push_str(chunk),
                None => {
                    exhausted = true;
                    break;
                }
            }
        }
        if window.is_empty() {
            break;
        }
        if cancelled() {
            break;
        }
        let cut = if exhausted {
            window.len()
        } else {
            floor_boundary(&window, SCAN_CHUNK)
        };
        let hits = crate::search::fold::ranges(
            &window,
            &[query],
            crate::search::fold::Fold::Lower,
            limit - found.len(),
        );
        for hit in hits {
            // Hits that start in the context were owned by the previous pass.
            if hit.start >= cut {
                continue;
            }
            found.push(base + hit.start..base + hit.end);
            if found.len() >= limit {
                return found;
            }
        }
        if exhausted {
            break;
        }
        window.drain(..cut);
        base += cut;
    }
    found
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::fold::{self, Fold};

    fn whole_scan(hay: &str, query: &str) -> Vec<Range<usize>> {
        fold::ranges(hay, &[query], Fold::Lower, MAX_FIND_MATCHES)
    }

    /// The production scan, over a rope built from `text`.
    fn rope_scan(text: &str, query: &str) -> Vec<Range<usize>> {
        let rope = crop::Rope::from(text);
        scan_rope(&rope, query, MAX_FIND_MATCHES, &|| false)
    }

    #[test]
    fn chunked_scan_matches_whole_scan_across_boundaries() {
        // A filler longer than one slice, with hits placed deliberately near
        // and across the slice boundary.
        let mut text = "x".repeat(SCAN_CHUNK - 3);
        text.push_str("needle");
        text.push_str(&"y".repeat(20));
        text.push_str("NEEDLE");
        text.push_str(&"z".repeat(SCAN_CHUNK));
        text.push_str("nee");
        text.push_str("dle");
        let expected = whole_scan(&text, "needle");
        let got = rope_scan(&text, "needle");
        assert_eq!(got, expected);
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn chunked_scan_keeps_greedy_non_overlap() {
        let mut text = "a".repeat(SCAN_CHUNK - 1);
        text.push_str("aaaa");
        let expected = whole_scan(&text, "aa");
        let got = rope_scan(&text, "aa");
        assert_eq!(got, expected);
    }

    #[test]
    fn chunked_scan_respects_limit_and_cancellation() {
        let text = "ab ".repeat(SCAN_CHUNK);
        let rope = crop::Rope::from(text);
        let limited = scan_rope(&rope, "ab", 5, &|| false);
        assert_eq!(limited.len(), 5);

        let cancelled = scan_rope(&rope, "ab", MAX_FIND_MATCHES, &|| true);
        assert!(cancelled.is_empty());
    }

    #[test]
    fn chunked_scan_floors_overlap_start_to_char_boundary() {
        // Mixed 1/2/3-byte characters push the window cut inside a character;
        // it must be snapped backwards instead of slicing mid-character, which
        // is the case that used to panic while typing the first letter of a
        // Russian word.
        let mut text = "aЖ日".repeat(100_000);
        text.push_str("привет");
        let expected = whole_scan(&text, "п");
        let got = rope_scan(&text, "п");
        assert_eq!(got, expected);
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn chunked_scan_slices_on_char_boundaries() {
        // Multibyte characters straddle the chunk edge; slicing at a non
        // boundary would panic instead of returning matches.
        let mut text = "ж".repeat(SCAN_CHUNK / 2);
        text.push_str("иголка");
        text.push_str(&"я".repeat(SCAN_CHUNK / 2));
        let expected = whole_scan(&text, "иголка");
        let got = rope_scan(&text, "иголка");
        assert_eq!(got, expected);
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn rope_scan_matches_string_scan() {
        // The rope is streamed chunk by chunk and cut into windows; neither may
        // change the result compared with scanning one contiguous string.
        let mut text = "aЖ日".repeat(100_000);
        text.push_str("needle");
        text.push_str(&"я".repeat(SCAN_CHUNK));
        text.push_str("NEEDLE");
        text.push_str("nee");
        text.push_str("dle");
        let rope = crop::Rope::from(text.clone());
        for query in ["needle", "п", "日", "aЖ"] {
            let expected = whole_scan(&text, query);
            let got = scan_rope(&rope, query, MAX_FIND_MATCHES, &|| false);
            assert_eq!(got, expected, "query {query:?}");
        }
    }
}
