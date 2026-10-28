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
    /// Replace was pressed while the list was stale: the reply runs it once
    /// the ranges for the current document arrive, instead of dropping the
    /// click.
    pub pending_replace: bool,
    /// Replace All pass in progress: bytes decided so far and the document
    /// length, so the counter can show a percentage instead of a match count.
    pub replace_all: Option<(usize, usize)>,
    /// Sequence of the latest Replace All request; a reply with an older
    /// number is dropped instead of being applied to the document.
    pub replace_all_seq: u64,
    /// Cancels the running Replace All pass between slices when the query
    /// changes or the bar closes.
    pub replace_cancel: Arc<AtomicBool>,
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
            pending_replace: false,
            replace_all: None,
            replace_all_seq: 0,
            replace_cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Stops a Replace All pass, if one is running: nothing of it will be
    /// applied and its background work stops at the next slice.
    pub fn cancel_replace_all(&mut self) {
        self.replace_all_seq = self.replace_all_seq.wrapping_add(1);
        self.replace_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.replace_all = None;
    }

    /// Counter label next to the query: `current/total`, `0/0` when empty, or
    /// the progress percentage while Replace All is rewriting the document.
    pub fn counter(&self) -> String {
        counter_label(
            self.active,
            self.matches.len(),
            self.truncated,
            self.replace_all,
        )
    }
}

fn counter_label(
    active: usize,
    total: usize,
    truncated: bool,
    replace_all: Option<(usize, usize)>,
) -> String {
    if let Some((done, total_bytes)) = replace_all {
        let percent = if total_bytes == 0 {
            100
        } else {
            done.saturating_mul(100) / total_bytes
        };
        return format!("{percent}%");
    }
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

/// One decided slice of [`stream_rope`].
struct WindowPass<'a> {
    /// Byte offset of `window[0]` in the rope.
    base: usize,
    window: &'a str,
    /// Length of the part of `window` whose text this pass settled.
    owned: usize,
    /// Matches starting in `window[..owned]`, in ascending order.
    hits: &'a [Range<usize>],
}

/// Streams the rope through a bounded window, handing every settled slice to
/// `on_pass` (which returns `false` to stop early).
///
/// The window always starts where the previous pass stopped, so the matches
/// form the same greedy non-overlapping sequence as scanning the whole text at
/// once. A match that crosses the cut is settled whole and the next pass
/// resumes after it; cutting exactly at the slice boundary instead would
/// rescan the tail of that match and report it a second time.
fn stream_rope(
    rope: &crop::Rope,
    query: &str,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
    mut on_pass: impl FnMut(WindowPass<'_>) -> bool,
) -> bool {
    if query.is_empty() || limit == 0 || rope.byte_len() == 0 {
        return true;
    }
    let overlap = query
        .chars()
        .count()
        .saturating_mul(12)
        .saturating_add(8)
        .min(SCAN_CHUNK);

    // Window starting at absolute `base`; everything before `cut` is settled
    // by this pass, everything after it is the context a crossing match needs.
    let mut window = String::new();
    let mut base = 0usize;
    let mut chunks = rope.chunks();
    let mut exhausted = false;
    let mut accepted: Vec<Range<usize>> = Vec::new();
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
            return true;
        }
        if cancelled() {
            return false;
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
            limit,
        );
        accepted.clear();
        let mut owned = cut;
        for hit in hits {
            // Hits that start in the context are owned by the next pass.
            if hit.start >= cut {
                break;
            }
            owned = owned.max(hit.end);
            accepted.push(hit);
        }
        if !on_pass(WindowPass {
            base,
            window: &window,
            owned,
            hits: &accepted,
        }) {
            return true;
        }
        if exhausted {
            return true;
        }
        let drain = owned.min(window.len());
        window.drain(..drain);
        base += drain;
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
    stream_rope(rope, query, limit, cancelled, |pass| {
        for hit in pass.hits {
            found.push(pass.base + hit.start..pass.base + hit.end);
            if found.len() >= limit {
                return false;
            }
        }
        true
    });
    found
}

/// Result of [`replace_rope`].
pub struct ReplaceAllText {
    /// Document text with every match replaced.
    pub text: String,
    pub replacements: usize,
    /// Byte offset in `text` just after the first replacement, where the
    /// cursor lands like it does after a single replace.
    pub first_end: Option<usize>,
}

/// Builds the whole replaced document text in one streaming pass.
///
/// Matches and replacement text are produced slice by slice, so the document
/// is never copied as a whole and the match list is not materialised. There is
/// no match limit: Replace All replaces every occurrence, exactly like a scan
/// of the original text would. `on_progress` receives the number of bytes
/// settled so far. Returns `None` when cancelled or when the query is empty
/// (there is nothing to replace, and the text must not be thrown away).
pub fn replace_rope(
    rope: &crop::Rope,
    query: &str,
    replacement: &str,
    cancelled: &dyn Fn() -> bool,
    on_progress: &dyn Fn(usize),
) -> Option<ReplaceAllText> {
    if query.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(rope.byte_len());
    let mut replacements = 0usize;
    let mut first_end = None;
    // Start of the not-yet-emitted part of the current window; each pass
    // begins at the byte the previous one settled.
    let mut last = 0usize;
    let completed = stream_rope(rope, query, usize::MAX, cancelled, |pass| {
        for hit in pass.hits {
            out.push_str(&pass.window[last..hit.start]);
            out.push_str(replacement);
            last = hit.end;
            replacements += 1;
            if first_end.is_none() {
                first_end = Some(out.len());
            }
        }
        let owned_end = pass.owned.min(pass.window.len());
        if owned_end > last {
            out.push_str(&pass.window[last..owned_end]);
        }
        last = 0;
        on_progress(pass.base + owned_end);
        true
    });
    if !completed {
        return None;
    }
    Some(ReplaceAllText {
        text: out,
        replacements,
        first_end,
    })
}

/// New match list after one replacement, without rescanning the document.
///
/// Matches ending before the rescanned slice are kept untouched, matches
/// starting after the replaced range move by the length difference, and the
/// matches inside the slice are found again. `window` is the text of
/// `scan_start..`, `owned_end` marks where its settled part ends (the rest is
/// context for a match that crosses that boundary).
pub fn patch_matches(
    old: &[Range<usize>],
    replaced: &Range<usize>,
    inserted_len: usize,
    scan_start: usize,
    owned_end: usize,
    window: &str,
    query: &str,
    limit: usize,
) -> Vec<Range<usize>> {
    let hits = crate::search::fold::ranges(
        window,
        &[query],
        crate::search::fold::Fold::Lower,
        limit,
    );
    let owned_len = owned_end.saturating_sub(scan_start);
    let delta = inserted_len as isize - (replaced.end - replaced.start) as isize;
    let mut patched = Vec::with_capacity(old.len());
    for range in old {
        if range.end <= scan_start {
            // Text before the slice is untouched.
            patched.push(range.clone());
        }
    }
    let mut scanned_end = scan_start;
    for hit in hits {
        // A hit starting in the context is left to the shifted tail.
        if hit.start >= owned_len {
            continue;
        }
        let end = scan_start + hit.end;
        scanned_end = scanned_end.max(end);
        patched.push(scan_start + hit.start..end);
    }
    for range in old {
        if range.start < replaced.end {
            // Overlapping the edit or inside the slice: found again above.
            continue;
        }
        let start = (range.start as isize + delta).max(0) as usize;
        let end = (range.end as isize + delta).max(0) as usize;
        // Not before the settled area, and never overlapping a match that
        // reaches past its end from inside.
        if start >= owned_end.max(scanned_end) {
            patched.push(start..end);
        }
    }
    patched
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

    #[test]
    fn chunked_scan_does_not_repeat_a_match_crossing_the_cut() {
        // A one-byte prefix shifts every 2-byte character, so the slice cut
        // lands inside one and a match starts before it and ends after it. The
        // match must be reported once, not again when the next window starts.
        let mut text = String::from("a");
        text.push_str(&"а".repeat(SCAN_CHUNK / 2 + 100));
        let expected = whole_scan(&text, "аа");
        let got = rope_scan(&text, "аа");
        assert_eq!(got, expected);
    }

    #[test]
    fn chunked_scan_progresses_to_the_end() {
        // Every slice settles some text; a window that ends inside a match
        // must not stall the loop.
        let text = "аааа".repeat(SCAN_CHUNK);
        let expected = whole_scan(&text, "аа");
        let got = rope_scan(&text, "аа");
        assert_eq!(got, expected);
    }

    /// Every match, without the highlight cap: Replace All has no limit.
    fn whole_scan_all(hay: &str, query: &str) -> Vec<Range<usize>> {
        fold::ranges(hay, &[query], Fold::Lower, usize::MAX)
    }

    fn naive_replace(text: &str, query: &str, replacement: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for range in whole_scan_all(text, query) {
            out.push_str(&text[last..range.start]);
            out.push_str(replacement);
            last = range.end;
        }
        out.push_str(&text[last..]);
        out
    }

    #[test]
    fn streaming_replace_matches_naive_replace() {
        let mut text = String::from("a");
        text.push_str(&"а".repeat(SCAN_CHUNK / 2 + 100));
        text.push_str("needle");
        text.push_str(&"я".repeat(SCAN_CHUNK));
        text.push_str("NEEDLEnee");
        text.push_str("dle");
        let rope = crop::Rope::from(text.clone());
        for (query, replacement) in [
            ("аа", "б"),
            ("needle", "X"),
            ("nee", ""),
            ("dle", "дле"),
            ("日", "z"),
            ("nothing-here", "y"),
        ] {
            let got = replace_rope(&rope, query, replacement, &|| false, &|_| {})
                .expect("scan is not cancelled");
            assert_eq!(
                got.text,
                naive_replace(&text, query, replacement),
                "query {query:?}"
            );
            assert_eq!(
                got.replacements,
                whole_scan_all(&text, query).len(),
                "query {query:?}"
            );
        }
    }

    #[test]
    fn streaming_replace_reports_progress_and_first_end() {
        let text = format!("prefix {} suffix", "needle ".repeat(SCAN_CHUNK / 4));
        let rope = crop::Rope::from(text.clone());
        let progress = std::cell::RefCell::new(Vec::new());
        let got = replace_rope(&rope, "needle", "thread", &|| false, &|done| {
            progress.borrow_mut().push(done)
        })
        .expect("scan is not cancelled");
        let progress = progress.into_inner();
        assert_eq!(got.replacements, whole_scan_all(&text, "needle").len());
        // Progress is monotonic and ends with the full document.
        assert!(progress.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(progress.last().copied(), Some(text.len()));
        assert_eq!(got.first_end, Some("prefix thread".len()));
    }

    #[test]
    fn streaming_replace_stops_when_cancelled() {
        let rope = crop::Rope::from("needle ".repeat(SCAN_CHUNK));
        assert!(replace_rope(&rope, "needle", "x", &|| true, &|_| {}).is_none());
    }

    #[test]
    fn streaming_replace_never_empties_the_document_for_an_empty_query() {
        let rope = crop::Rope::from("text");
        assert!(replace_rope(&rope, "", "x", &|| false, &|_| {}).is_none());
    }

    #[test]
    fn counter_shows_replace_all_progress() {
        assert_eq!(counter_label(0, 2, false, None), "1/2");
        assert_eq!(counter_label(0, 0, false, None), "0/0");
        assert_eq!(counter_label(0, MAX_FIND_MATCHES, true, None), "1/19999+");
        assert_eq!(counter_label(0, 2, false, Some((25, 100))), "25%");
        assert_eq!(counter_label(0, 2, false, Some((100, 100))), "100%");
    }

    fn floor_char(text: &str, mut index: usize) -> usize {
        while index > 0 && !text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn ceil_char(text: &str, mut index: usize) -> usize {
        let len = text.len();
        while index < len && !text.is_char_boundary(index) {
            index += 1;
        }
        index.min(len)
    }

    /// Replaces one match the way `AppView` does and returns the list the
    /// incremental patch produces.
    fn patched_after_replace(
        text: &str,
        query: &str,
        replacement: &str,
        index: usize,
    ) -> (String, Vec<Range<usize>>) {
        let old = rope_scan(text, query);
        let replaced = old[index].clone();
        let mut new_text = String::with_capacity(text.len());
        new_text.push_str(&text[..replaced.start]);
        new_text.push_str(replacement);
        new_text.push_str(&text[replaced.end..]);
        let margin = query.chars().count() * 12 + 8;
        let scan_start = floor_char(&new_text, replaced.start.saturating_sub(margin));
        let owned_end = ceil_char(
            &new_text,
            (replaced.start + replacement.len() + margin).min(new_text.len()),
        );
        let scan_end = ceil_char(&new_text, (owned_end + margin).min(new_text.len()));
        let patched = patch_matches(
            &old,
            &replaced,
            replacement.len(),
            scan_start,
            owned_end,
            &new_text[scan_start..scan_end],
            query,
            MAX_FIND_MATCHES,
        );
        (new_text, patched)
    }

    #[test]
    fn patch_after_replace_drops_the_replaced_match() {
        let text = "alpha foo beta foo gamma foo delta";
        let (new_text, patched) = patched_after_replace(text, "foo", "X", 0);
        assert_eq!(new_text, "alpha X beta foo gamma foo delta");
        assert_eq!(patched, rope_scan(&new_text, "foo"));
        assert_eq!(patched.len(), 2);
    }

    #[test]
    fn patch_after_replace_finds_matches_the_replacement_created() {
        // The replacement itself contains the query, so the edit adds matches
        // next to the one it removed.
        let text = "alpha foo beta foo gamma";
        let (new_text, patched) = patched_after_replace(text, "foo", "foofoo", 0);
        assert_eq!(new_text, "alpha foofoo beta foo gamma");
        assert_eq!(patched, rope_scan(&new_text, "foo"));
        assert_eq!(patched.len(), 3);
    }

    #[test]
    fn patch_after_replace_shifts_the_tail_by_the_length_delta() {
        let text = "one target two target three target".replace("target", "needle");
        for replacement in ["x", "much longer replacement"] {
            let (new_text, patched) = patched_after_replace(&text, "needle", replacement, 1);
            assert_eq!(patched, rope_scan(&new_text, "needle"));
            assert_eq!(patched.len(), 2);
            for range in &patched {
                assert!(new_text.is_char_boundary(range.start));
                assert_eq!(&new_text[range.clone()], "needle");
            }
        }
    }

    #[test]
    fn patch_after_replace_handles_multibyte_context() {
        let text = "привет иголка мир иголка конец иголка";
        let (new_text, patched) = patched_after_replace(text, "иголка", "стрелка", 0);
        assert_eq!(patched, rope_scan(&new_text, "иголка"));
        assert_eq!(patched.len(), 2);
        // Deleting a match entirely must not leave its range behind.
        let (new_text, patched) = patched_after_replace(text, "иголка", "", 2);
        assert_eq!(patched, rope_scan(&new_text, "иголка"));
        assert_eq!(patched.len(), 2);
    }

    #[test]
    fn patch_after_replace_keeps_ranges_disjoint_and_real() {
        let text = "aXbXcXdXeXfXg".repeat(50);
        let (new_text, patched) = patched_after_replace(&text, "x", "yy", 10);
        assert!(patched.windows(2).all(|w| w[0].end <= w[1].start));
        for range in &patched {
            assert!(new_text.is_char_boundary(range.start));
            assert!(new_text.is_char_boundary(range.end));
            assert_eq!(&new_text[range.clone()].to_lowercase(), "x");
        }
    }
}
