use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use gpui::{Rgba, StyledText, TextRun};
use crate::search::cancel::CANCELLED_SCANS;
use crate::search::cache::SearchContentCache;
use crate::search::fold::{self, Fold, Mode};
use crate::theme::Theme;
use super::{GotoResult, SNIPPET_AFTER, SNIPPET_BEFORE};

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
pub(crate) fn fold_mode(case_sensitive: bool) -> Fold {
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
pub(crate) fn verify_page(
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
pub(crate) fn match_any(
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
pub(crate) fn match_body(text: &str, q: &str, terms: &[String], fl: Fold) -> Option<(usize, String)> {
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
pub(crate) fn match_tag(text: &str, terms: &[String], fl: Fold, cs: bool) -> Option<(usize, String)> {
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
pub(crate) fn match_line(text: &str, terms: &[String], q: &str, fl: Fold) -> Option<(usize, String)> {
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
pub(crate) fn match_section(
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
pub(crate) fn first_line_matching(text: &str, terms: &[String], case_sensitive: bool) -> Option<String> {
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
pub(crate) fn cap_line(line: &str, limit: usize) -> String {
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
pub(crate) fn is_heading_line(line: &str) -> bool {
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
pub(crate) fn split_sections(text: &str) -> Vec<(String, String)> {
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
pub(crate) fn make_snippet(text: &str, query: &str, terms: &[String], fl: Fold) -> String {
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
pub(crate) fn first_line(text: &str, limit: usize) -> String {
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

pub(crate) fn flatten_whitespace(s: &str) -> String {
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

/// Case-aware [`find_term_ranges`].
///
/// The folding matcher reports non-overlapping byte ranges in the original,
/// longest term winning on overlap.
pub(crate) fn find_term_ranges_cased(
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
