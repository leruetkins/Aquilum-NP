//! Indexed full-text search for the mobile front end.
//!
//! Kotlin owns the IO (a plain directory and a SAF tree are read through the
//! same repository) and streams one note at a time into this session; Rust
//! owns the shared desktop engine: `tokenize` folds terms into title/path/body
//! frequencies, `index` keeps the postings and `query` ranks paths with
//! BM25F. Nothing is persisted — the session is rebuilt from fingerprints
//! when a vault opens, and only files whose `(mtime, len)` changed are
//! re-tokenized afterwards.
//!
//! One page per file is enough here: the caller already holds the whole text,
//! so there is no streaming win. Big notes therefore rank with a whole-file
//! body norm instead of the desktop's per-page norm.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::search::bm25::Bm25;
use crate::search::fold::{self, Fold};
use crate::search::index::Index;

/// Same cap as the desktop query path, so a long query cannot sweep the
/// dictionary.
const MAX_TERMS: usize = 8;
/// Candidate pool multiplier: rank a little deeper than the display limit so
/// candidates that fail verification do not leave the list short.
const POOL_FACTOR: usize = 4;
/// Snippet length in characters, matching the scan the app used before.
const SNIPPET_CHARS: usize = 160;
/// One note can produce a lot of matches; the UI groups them by file but never
/// needs an unbounded list for one huge note.
const MAX_MATCHES_PER_FILE: usize = 200;
/// Characters of context kept on each side of the first match.
const SNIPPET_CONTEXT_CHARS: usize = 100;

/// One matching line with the context shown in the search list.
#[derive(Debug)]
pub struct LocatedMatch {
    /// 1-based line number.
    pub line: usize,
    /// Char offset of the first match inside the raw line.
    pub column: usize,
    /// Length of the first match in chars.
    pub length: usize,
    /// Context around the first match, with `…` at clipped edges.
    pub snippet: String,
    /// Match ranges in char offsets relative to [`Self::snippet`].
    pub ranges: Vec<(usize, usize)>,
}

struct Session {
    index: Index,
    /// `(mtime_ms, len)` per indexed path; `None` means never indexed.
    fingerprints: HashMap<String, (u64, u64)>,
    ready: bool,
}

fn session() -> &'static Mutex<Session> {
    static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();
    SESSION.get_or_init(|| {
        Mutex::new(Session {
            index: Index::empty(None),
            fingerprints: HashMap::new(),
            ready: false,
        })
    })
}

/// Drops everything. Called when a vault opens or closes.
pub fn reset() {
    if let Ok(mut session) = session().lock() {
        *session = Session {
            index: Index::empty(None),
            fingerprints: HashMap::new(),
            ready: false,
        };
    }
}

fn title_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Adds or refreshes one note. Returns `true` when the file was tokenized.
/// An unchanged fingerprint is skipped without touching the index.
pub fn add(path: &str, text: &str, mtime: u64, len: u64) -> bool {
    let Ok(mut session) = session().lock() else {
        return false;
    };
    let stamp = (mtime, len);
    if session.fingerprints.get(path) == Some(&stamp) {
        return false;
    }
    let path_ref = Path::new(path);
    session.index.clear_path(path_ref);
    let path_ix = session.index.ensure_path(path_ref);
    session
        .index
        .add_page(path_ix, &title_of(path), path, text, 0);
    session.index.set_fingerprint(path_ref, stamp);
    session.fingerprints.insert(path.to_string(), stamp);
    true
}

/// Drops one file (deleted or renamed away).
pub fn remove(path: &str) {
    let Ok(mut session) = session().lock() else {
        return;
    };
    let path_ref = Path::new(path);
    session.index.clear_path(path_ref);
    // `u64::MAX` is the on-disk "no stamp" marker, so the saved snapshot does
    // not resurrect the entry.
    session.index.set_fingerprint(path_ref, (u64::MAX, u64::MAX));
    session.fingerprints.remove(path);
}

/// Marks the index complete. Queries answer from it only after this, so a
/// half-built vault never produces "missing" search results.
pub fn finish() {
    if let Ok(mut session) = session().lock() {
        session.ready = true;
    }
}

pub fn is_ready() -> bool {
    session().lock().map(|s| s.ready).unwrap_or(false)
}

pub fn file_count() -> usize {
    session().lock().map(|s| s.fingerprints.len()).unwrap_or(0)
}

/// Replaces the session with a persisted snapshot (the desktop `VBIX`
/// format). Returns the stamps the caller should keep for incremental
/// refreshes, or `None` when the file is missing, truncated or written by
/// another analyzer version — the caller then rebuilds from scratch.
pub fn load(file: &str) -> Option<Vec<(String, u64, u64)>> {
    let index = Index::load(Path::new(file))?;
    let mut fingerprints: HashMap<String, (u64, u64)> = HashMap::new();
    for path in index.stored_paths() {
        if let Some(stamp) = index.stored_fingerprint(path) {
            fingerprints.insert(path.to_string_lossy().into_owned(), stamp);
        }
    }
    let entries = fingerprints
        .iter()
        .map(|(path, stamp)| (path.clone(), stamp.0, stamp.1))
        .collect();
    let Ok(mut session) = session().lock() else {
        return None;
    };
    *session = Session {
        index,
        fingerprints,
        ready: true,
    };
    Some(entries)
}

/// Writes the snapshot (atomic replace). `false` when the index is not usable
/// yet or the write failed; the in-memory index stays valid either way.
pub fn save(file: &str) -> bool {
    let Ok(session) = session().lock() else {
        return false;
    };
    if !session.ready {
        return false;
    }
    session.index.save(Path::new(file)).is_ok()
}

pub struct Hit {
    pub path: String,
    pub name: String,
    pub score: f32,
    pub count: usize,
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Query terms, deduped by their folded form (the desktop splits queries the
/// same way). Matching is always case-folded on mobile.
fn terms_of(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .filter(|term| seen.insert(fold::fold_str(term, Fold::Lower)))
        .map(|term| term.to_string())
        .collect()
}

/// Ranked candidate paths, best first. Empty while the index is not ready —
/// the caller falls back to its linear scan in that case.
pub fn query(query: &str, limit: usize) -> Vec<Hit> {
    if limit == 0 {
        return Vec::new();
    }
    let terms = terms_of(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let Ok(session) = session().lock() else {
        return Vec::new();
    };
    if !session.ready {
        return Vec::new();
    }
    let bm25 = Bm25::from_settings(&crate::settings::Bm25fParams::default());
    let pool = (limit * POOL_FACTOR).max(50);
    let candidates =
        crate::search::query::candidates(&session.index, &terms, bm25, pool, MAX_TERMS);
    candidates
        .into_iter()
        .take(limit)
        .map(|candidate| {
            let path = session
                .index
                .doc_path_for_ix(candidate.path_ix)
                .to_string_lossy()
                .into_owned();
            Hit {
                name: name_of(&path),
                path,
                score: candidate.score,
                count: candidate.count,
            }
        })
        .collect()
}

/// All lines carrying the query, with context ranges for the grouped UI.
///
/// Matching keeps the old mobile rule: when the query has several terms, a
/// line qualifies only if every term is present; if no such line exists, lines
/// with the exact phrase are used instead. Highlight ranges cover each term
/// occurrence (or the exact phrase), merged so one visual span is not painted
/// twice.
pub fn locate_all(query: &str, text: &str) -> Vec<LocatedMatch> {
    let terms = terms_of(query);
    let needle = query.trim();
    if terms.is_empty() && needle.is_empty() {
        return Vec::new();
    }

    let mut primary = Vec::new();
    let mut fallback = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let mut ranges = Vec::new();
        if terms.is_empty() {
            ranges = find_ranges(line, needle);
        } else {
            let mut all_terms = true;
            for term in &terms {
                let term_ranges = find_ranges(line, term);
                if term_ranges.is_empty() {
                    all_terms = false;
                    break;
                }
                ranges.extend(term_ranges);
            }
            if all_terms {
                ranges = merge_ranges(ranges);
            } else {
                ranges.clear();
            }
        }

        if !ranges.is_empty() {
            if primary.len() < MAX_MATCHES_PER_FILE {
                primary.push(located_match(line_number, line, &ranges));
            }
            if primary.len() >= MAX_MATCHES_PER_FILE {
                break;
            }
            continue;
        }

        if !needle.is_empty() && fallback.len() < MAX_MATCHES_PER_FILE {
            let exact = find_ranges(line, needle);
            if !exact.is_empty() {
                fallback.push(located_match(line_number, line, &exact));
            }
        }
    }

    if primary.is_empty() {
        fallback
    } else {
        primary
    }
}

/// First match only, kept for callers that still need the old single-hit API.
pub fn locate(query: &str, text: &str) -> (i32, String) {
    locate_all(query, text)
        .into_iter()
        .next()
        .map(|found| (found.line as i32, found.snippet))
        .unwrap_or((0, String::new()))
}

fn located_match(line_number: usize, line: &str, ranges: &[(usize, usize)]) -> LocatedMatch {
    let first = ranges[0];
    let (snippet, snippet_ranges) = snippet_for(line, ranges);
    LocatedMatch {
        line: line_number,
        column: first.0,
        length: first.1 - first.0,
        snippet,
        ranges: snippet_ranges,
    }
}

/// Context window around the first match, with ranges remapped into it.
fn snippet_for(line: &str, ranges: &[(usize, usize)]) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<char> = line.chars().collect();
    let first = ranges[0];
    let (start, end, clipped_start, clipped_end) = if chars.len() <= SNIPPET_CHARS {
        (0, chars.len(), false, false)
    } else {
        let start = first.0.saturating_sub(SNIPPET_CONTEXT_CHARS);
        let end = (first.1 + SNIPPET_CONTEXT_CHARS).min(chars.len());
        (start, end, start > 0, end < chars.len())
    };
    let mut snippet: String = chars[start..end].iter().collect();
    let shift = if clipped_start {
        snippet.insert(0, '…');
        1
    } else {
        0
    };
    if clipped_end {
        snippet.push('…');
    }
    let mapped = ranges
        .iter()
        .filter_map(|&(range_start, range_end)| {
            let range_start = range_start.max(start);
            let range_end = range_end.min(end);
            if range_end <= range_start {
                None
            } else {
                Some((range_start - start + shift, range_end - start + shift))
            }
        })
        .collect();
    (snippet, mapped)
}

/// Case-folded char offsets of every `needle` occurrence in `line`.
///
/// The shared engine returns byte ranges in the original string and already
/// handles lowercase expansions without shifting later offsets. Convert those
/// ranges to chars because the UI and the Kotlin selection use char columns.
fn find_ranges(line: &str, needle: &str) -> Vec<(usize, usize)> {
    fold::ranges(line, &[needle], Fold::Lower, MAX_MATCHES_PER_FILE)
        .into_iter()
        .map(|range| (chars_before(line, range.start), chars_before(line, range.end)))
        .collect()
}

fn chars_before(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session is a process-wide singleton, so tests must not interleave.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        test_guard()
    }

    fn build(files: &[(&str, &str)]) {
        reset();
        for (path, text) in files {
            add(path, text, 1, text.len() as u64);
        }
        finish();
    }

    #[test]
    fn ranks_body_hits_and_locates_the_matching_line() {
        let _guard = lock();
        build(&[
            ("notes/alpha.md", "# Alpha\nnothing interesting\n"),
            ("notes/beta.md", "# Beta\nthe needle hides here\n"),
        ]);
        let hits = query("needle", 10);
        assert_eq!(hits.len(), 1, "only the note carrying the term ranks");
        assert_eq!(hits[0].path, "notes/beta.md");
        assert_eq!(hits[0].name, "beta.md");
        let (line, snippet) = locate("needle", "# Beta\nthe needle hides here\n");
        assert_eq!(line, 2);
        assert!(snippet.contains("needle"), "snippet: {snippet}");
    }

    #[test]
    fn locate_all_returns_every_line_and_every_range() {
        let _guard = lock();
        let text = "needle one\nnothing\ntwo needle needle\n";
        let found = locate_all("needle", text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].column, 0);
        assert_eq!(found[0].length, 6);
        assert_eq!(found[1].line, 3);
        assert_eq!(found[1].ranges.len(), 2);
        assert!(found[1].snippet.contains("needle"));
    }

    #[test]
    fn locate_all_requires_every_term_and_highlights_each() {
        let _guard = lock();
        let found = locate_all("rust runtime", "rust async runtime\nrust only\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].ranges.len(), 2);
    }

    #[test]
    fn every_term_must_match_and_prefixes_count() {
        let _guard = lock();
        build(&[
            ("a.md", "rust async runtime\n"),
            ("b.md", "rust only\n"),
        ]);
        // "runtime" only exists in a.md; "rust" in both.
        let hits = query("rust runtime", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
        // Prefix expansion matches word starts, like the desktop engine.
        let prefixed = query("runt", 10);
        assert_eq!(prefixed.len(), 1);
        assert_eq!(prefixed[0].path, "a.md");
        assert!(query("missingterm", 10).is_empty());
    }

    #[test]
    fn unchanged_fingerprints_are_skipped_and_removals_drop_postings() {
        let _guard = lock();
        reset();
        assert!(add("a.md", "alpha body", 7, 10));
        assert!(!add("a.md", "alpha body", 7, 10), "same stamp is a no-op");
        assert!(add("a.md", "alpha body changed", 8, 18), "mtime bump reindexes");
        finish();
        assert_eq!(query("changed", 10).len(), 1);
        remove("a.md");
        assert!(query("changed", 10).is_empty());
        assert!(!is_ready() || file_count() == 0);
    }

    #[test]
    fn queries_answer_nothing_before_finish() {
        let _guard = lock();
        reset();
        add("a.md", "alpha", 1, 5);
        assert!(query("alpha", 10).is_empty(), "partial index must not answer");
        finish();
        assert_eq!(query("alpha", 10).len(), 1);
    }

    #[test]
    fn snapshot_round_trips_and_keeps_stamps() {
        let _guard = lock();
        build(&[("a.md", "alpha beta")]);
        let dir = std::env::temp_dir().join(format!(
            "vibecodium_index_snapshot_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.vbix");
        let file = file.to_str().unwrap();

        assert!(save(file), "a finished index must persist");
        reset();
        assert!(!is_ready());

        let stamps = load(file).expect("the snapshot must load back");
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].0, "a.md");
        assert!(is_ready(), "a restored index answers immediately");
        assert_eq!(query("alpha", 10).len(), 1);

        // The restored stamp is unchanged: no re-read, no re-tokenize.
        assert!(!add("a.md", "alpha beta", stamps[0].1, stamps[0].2));
        assert!(query("beta", 10).len() == 1);
        // A new stamp reindexes the file and drops the old terms.
        assert!(add("a.md", "alpha gamma", stamps[0].1, stamps[0].2 + 1));
        assert_eq!(query("gamma", 10).len(), 1);
        assert!(query("beta", 10).is_empty());

        assert!(load("/definitely/missing/index.vbix").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
