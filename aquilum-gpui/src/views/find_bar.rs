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
    options: FindOptions,
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
    let fold = options.fold();
    let mode = options.mode();

    // Window starting at absolute `base`; everything before `cut` is settled
    // by this pass, everything after it is the context a crossing match needs.
    let mut window = String::new();
    let mut base = 0usize;
    let mut chunks = rope.chunks();
    let mut exhausted = false;
    // Last character before `window`, so a whole-word match on its first byte
    // still sees the left neighbour the pass boundary cut away.
    let mut left_char: Option<char> = None;
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
        let hits = crate::search::fold::ranges_mode(&window, &[query], fold, mode, limit);
        accepted.clear();
        let mut owned = cut;
        for hit in hits {
            // Hits that start in the context are owned by the next pass.
            if hit.start >= cut {
                break;
            }
            // The window has no character before its first byte, so the
            // matcher took it for a word boundary; restore the real neighbour.
            let left_blocks = options.whole_word
                && hit.start == 0
                && left_char.is_some_and(|c| c.is_alphanumeric());
            // A rejected hit still ends the greedy sequence: resuming inside
            // it would report overlapping matches the full text scan does not.
            owned = owned.max(hit.end);
            if !left_blocks {
                accepted.push(hit);
            }
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
        left_char = window[..drain].chars().next_back();
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
    options: FindOptions,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    stream_rope(rope, query, options, limit, cancelled, |pass| {
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
    options: FindOptions,
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
    let completed = stream_rope(rope, query, options, usize::MAX, cancelled, |pass| {
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
    options: FindOptions,
    left_char: Option<char>,
    limit: usize,
) -> Vec<Range<usize>> {
    let mut hits = crate::search::fold::ranges_mode(
        window,
        &[query],
        options.fold(),
        options.mode(),
        limit,
    );
    // The slice starts mid-text, so the first byte has no left neighbour for
    // the word-boundary check; the character before `scan_start` is that
    // neighbour.
    let left_blocks = options.whole_word && left_char.is_some_and(|c| c.is_alphanumeric());
    if left_blocks && hits.first().is_some_and(|hit| hit.start == 0) {
        hits.remove(0);
    }
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

/// Replacement text for "Delete marked lines": the kept lines of `span`.
///
/// `span` starts exactly at the first marked line and ends exactly where the
/// line after the last marked line starts (or at the end of the document).
/// `first_line` is the absolute index of its first line, and `marked` holds
/// sorted absolute line indices. The second return value tells whether any
/// line survived; when none did and the span reaches the end of the document,
/// the caller also removes the newline that ended the previous line, so the
/// document does not keep a dangling empty last line.
pub fn delete_marked_lines_replacement(
    span: &str,
    first_line: usize,
    marked: &[usize],
    ends_document: bool,
) -> (String, bool) {
    let mut kept: Vec<&str> = Vec::new();
    for (offset, chunk) in span.split_inclusive('\n').enumerate() {
        let line = first_line + offset;
        if marked.binary_search(&line).is_err() {
            kept.push(chunk.strip_suffix('\n').unwrap_or(chunk));
        }
    }
    let any_kept = !kept.is_empty();
    let mut replacement = kept.join("\n");
    // The span ended just after the last removed line's newline, so the line
    // that followed it starts immediately: a newline must separate the kept
    // tail from it. At the end of the document there is nothing to separate.
    if any_kept && !ends_document {
        replacement.push('\n');
    }
    (replacement, any_kept)
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

    fn whole_scan_opts(hay: &str, query: &str, options: FindOptions) -> Vec<Range<usize>> {
        fold::ranges_mode(
            hay,
            &[query],
            options.fold(),
            options.mode(),
            MAX_FIND_MATCHES,
        )
    }

    /// The production scan, over a rope built from `text`.
    fn rope_scan(text: &str, query: &str) -> Vec<Range<usize>> {
        rope_scan_opts(text, query, FindOptions::default())
    }

    fn rope_scan_opts(text: &str, query: &str, options: FindOptions) -> Vec<Range<usize>> {
        let rope = crop::Rope::from(text);
        scan_rope(&rope, query, options, MAX_FIND_MATCHES, &|| false)
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
        let limited = scan_rope(&rope, "ab", FindOptions::default(), 5, &|| false);
        assert_eq!(limited.len(), 5);

        let cancelled =
            scan_rope(&rope, "ab", FindOptions::default(), MAX_FIND_MATCHES, &|| true);
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
    fn rope_scan_honours_case_sensitivity() {
        let text = "Needle needle NEEDLE needle";
        let exact = FindOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let hits = rope_scan_opts(text, "needle", exact);
        assert_eq!(hits.len(), 2);
        for range in &hits {
            assert_eq!(&text[range.clone()], "needle");
        }
        assert_eq!(
            rope_scan_opts(text, "needle", FindOptions::default()).len(),
            4
        );

        // The rule holds across a chunk boundary too: only the lowercase hit
        // of the second half survives.
        let mut long = "x".repeat(SCAN_CHUNK / 2);
        long.push_str("NEEDLE");
        long.push_str(&"y".repeat(SCAN_CHUNK));
        long.push_str("needle");
        let hits = rope_scan_opts(&long, "needle", exact);
        assert_eq!(hits.len(), 1);
        assert_eq!(&long[hits[0].clone()], "needle");
    }

    #[test]
    fn rope_scan_honours_whole_words_across_the_window_cut() {
        let words = FindOptions {
            whole_word: true,
            ..Default::default()
        };
        // The second window starts exactly with "cat", which the window alone
        // reads as a word start; the real neighbour before the cut is an `a`,
        // so a whole-text scan reports nothing and the stream must agree.
        let mut text = "a".repeat(SCAN_CHUNK);
        text.push_str("cat");
        text.push_str(" cat");
        text.push_str(&"z".repeat(64));
        let got = rope_scan_opts(&text, "cat", words);
        assert!(got.is_empty());
        assert_eq!(got, whole_scan_opts(&text, "cat", words));
        // Substring mode has no boundary rule, so the same text has two hits.
        assert_eq!(
            rope_scan_opts(&text, "cat", FindOptions::default()).len(),
            2
        );

        // A real word one byte into the second window is still found: the
        // left-context check must not swallow the rest of the window.
        let mut text = "a".repeat(SCAN_CHUNK);
        text.push_str(" cat ");
        text.push_str(&"z".repeat(64));
        assert_eq!(
            rope_scan_opts(&text, "cat", words),
            whole_scan_opts(&text, "cat", words)
        );
        assert_eq!(rope_scan_opts(&text, "cat", words).len(), 1);
    }

    #[test]
    fn streaming_replace_uses_the_matching_options() {
        let text = "Cat cat CAT scattered cat";
        let rope = crop::Rope::from(text);
        let options = FindOptions {
            case_sensitive: true,
            whole_word: true,
        };
        let got = replace_rope(&rope, "cat", "dog", options, &|| false, &|_| {})
            .expect("scan is not cancelled");
        assert_eq!(got.text, "Cat dog CAT scattered dog");
        assert_eq!(got.replacements, 2);
    }

    #[test]
    fn patch_after_replace_keeps_the_matching_options() {
        // The patch must look for the same thing the list was built with: with
        // whole words on, "cat" inside "scattered" is not a match.
        let text = "cat scattered cat";
        let rope = crop::Rope::from(text);
        let options = FindOptions {
            whole_word: true,
            ..Default::default()
        };
        let old = scan_rope(&rope, "cat", options, MAX_FIND_MATCHES, &|| false);
        assert_eq!(old.len(), 2);
        let replaced = old[0].clone();
        let new_text = "dog scattered cat".to_string();
        let patched = patch_matches(
            &old,
            &replaced,
            3,
            0,
            new_text.len(),
            &new_text,
            "cat",
            options,
            None,
            MAX_FIND_MATCHES,
        );
        assert_eq!(
            patched,
            scan_rope(
                &crop::Rope::from(new_text),
                "cat",
                options,
                MAX_FIND_MATCHES,
                &|| false
            )
        );
        assert_eq!(patched.len(), 1);
    }

    #[test]
    fn patched_slice_uses_the_character_before_it() {
        // The rescanned slice starts at "cat" inside "acat". On its own the
        // slice reads as a word start; the document says otherwise.
        let options = FindOptions {
            whole_word: true,
            ..Default::default()
        };
        let inside_a_word = patch_matches(
            &[],
            &(0..0),
            0,
            1,
            4,
            "cat",
            "cat",
            options,
            Some('a'),
            MAX_FIND_MATCHES,
        );
        assert!(inside_a_word.is_empty());
        let after_a_gap = patch_matches(
            &[],
            &(0..0),
            0,
            1,
            4,
            "cat",
            "cat",
            options,
            Some(' '),
            MAX_FIND_MATCHES,
        );
        assert_eq!(after_a_gap, vec![1..4]);
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
            let got = scan_rope(&rope, query, FindOptions::default(), MAX_FIND_MATCHES, &|| false);
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
            let got = replace_rope(
                &rope,
                query,
                replacement,
                FindOptions::default(),
                &|| false,
                &|_| {},
            )
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
        let got = replace_rope(
            &rope,
            "needle",
            "thread",
            FindOptions::default(),
            &|| false,
            &|done| progress.borrow_mut().push(done),
        )
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
        assert!(replace_rope(
            &rope,
            "needle",
            "x",
            FindOptions::default(),
            &|| true,
            &|_| {}
        )
        .is_none());
    }

    #[test]
    fn streaming_replace_never_empties_the_document_for_an_empty_query() {
        let rope = crop::Rope::from("text");
        assert!(replace_rope(
            &rope,
            "",
            "x",
            FindOptions::default(),
            &|| false,
            &|_| {}
        )
        .is_none());
    }

    #[test]
    fn counter_and_replace_all_percent() {
        assert_eq!(counter_label(0, 2, false), "1/2");
        assert_eq!(counter_label(0, 0, false), "0/0");
        assert_eq!(counter_label(0, MAX_FIND_MATCHES, true), "1/19999+");
        // The percentage belongs to the replace field, not to the counter.
        assert_eq!(percent_label(None), None);
        assert_eq!(percent_label(Some((25, 100))).as_deref(), Some("25%"));
        assert_eq!(percent_label(Some((100, 100))).as_deref(), Some("100%"));
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
        let left_char = new_text[..scan_start].chars().next_back();
        let patched = patch_matches(
            &old,
            &replaced,
            replacement.len(),
            scan_start,
            owned_end,
            &new_text[scan_start..scan_end],
            query,
            FindOptions::default(),
            left_char,
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
        // A mixed text is replaced and patched repeatedly; every reported range
        // must be real, cut on character boundaries and in ascending order.
        let text = "aXbXcXdXeXfXg".repeat(50);
        let (new_text, patched) = patched_after_replace(&text, "x", "yy", 10);
        assert!(patched.windows(2).all(|w| w[0].end <= w[1].start));
        for range in &patched {
            assert!(new_text.is_char_boundary(range.start));
            assert!(new_text.is_char_boundary(range.end));
            assert_eq!(&new_text[range.clone()].to_lowercase(), "x");
        }
    }

    /// Applies the production "Delete marked lines" edit to `text` so the
    /// results can be checked against plain strings.
    fn deleted(text: &str, marked: &[usize]) -> String {
        let index = crate::line_index::LineIndex::new(text);
        let total = index.lines();
        let first = *marked.first().expect("at least one marked line");
        let last = *marked.last().expect("at least one marked line");
        let span_start = index.start(text, first);
        let ends_document = last + 1 >= total;
        let span_end = if ends_document {
            text.len()
        } else {
            index.start(text, last + 1)
        };
        let (replacement, any_kept) = delete_marked_lines_replacement(
            &text[span_start..span_end],
            first,
            marked,
            ends_document,
        );
        // Nothing survived at the end of the document: the newline of the
        // previous line goes as well, or an empty last line would remain.
        let mut start = span_start;
        if !any_kept && ends_document && first > 0 {
            start -= 1;
        }
        let mut out = text.to_string();
        out.replace_range(start..span_end, &replacement);
        out
    }

    #[test]
    fn marked_lines_delete_a_single_middle_line() {
        assert_eq!(
            deleted("alpha\nbeta\ngamma\ndelta", &[1]),
            "alpha\ngamma\ndelta"
        );
    }

    #[test]
    fn marked_lines_delete_the_first_and_the_last_line() {
        assert_eq!(deleted("alpha\nbeta\ngamma", &[0]), "beta\ngamma");
        assert_eq!(deleted("alpha\nbeta\ngamma", &[2]), "alpha\nbeta");
        assert_eq!(deleted("alpha\nbeta\ngamma", &[0, 2]), "beta");
    }

    #[test]
    fn marked_lines_keep_unmarked_lines_between_them() {
        assert_eq!(
            deleted("a\nb\nc\nd\ne", &[0, 2, 4]),
            "b\nd",
        );
        // The kept tail is separated from the next line by exactly one newline.
        assert_eq!(deleted("a\nb\nc\nd", &[0, 2]), "b\nd");
    }

    #[test]
    fn marked_lines_delete_the_whole_document() {
        assert_eq!(deleted("a\nb\nc", &[0, 1, 2]), "");
        assert_eq!(deleted("only", &[0]), "");
    }

    #[test]
    fn marked_lines_keep_empty_lines_between_them() {
        // The blank line survives; the gap after it must not swallow the
        // previous line's newline when the span reaches the document end.
        assert_eq!(deleted("p\nq\n\nr", &[1, 3]), "p\n");
        assert_eq!(deleted("p\nq\n\nr", &[0, 2]), "q\nr");
    }

    #[test]
    fn marked_lines_delete_a_line_that_ends_with_a_newline() {
        // A document ending in a newline has an extra empty last line, so the
        // removed line is not the last one and keeps its trailing newline.
        assert_eq!(deleted("a\nb\nc\n", &[2]), "a\nb\n");
    }

    #[test]
    fn marked_lines_keep_windows_line_endings() {
        assert_eq!(deleted("a\r\nb\r\nc", &[1]), "a\r\nc");
    }
}
