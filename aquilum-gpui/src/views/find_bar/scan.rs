use std::ops::Range;
use super::{FindOptions, SCAN_CHUNK};

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
