//! Index-level query: terms → ranked candidate paths (stage B).
//!
//! Modeled on the reference `core/src/search/query.rs` (ideas only): for
//! every query term the exact term plus its prefix expansions are unioned (a
//! term matches when it starts a word), candidates are the INTERSECTION over
//! all query terms (the "all terms" requirement), grouped per PATH and ranked
//! by BM25F. Match counts come from postings (whole file, all pages); whether
//! a candidate really matches the requested mode (one line / one section /
//! literal tag) is verified against the body by the caller.

use std::collections::{BTreeMap, HashSet};

use crate::search::bm25::Bm25;
use crate::search::fold::{self, Fold};
use crate::search::index::{doc_path_ix, Index};

/// Cap on postings unioned per query term (a one-letter prefix can otherwise
/// sweep the whole dictionary).
const MAX_POSTINGS_PER_TERM: usize = 4096;

/// One ranked candidate path.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path_ix: u32,
    pub score: f32,
    /// Whole-file body count summed from postings (exact, all pages).
    pub count: usize,
}

/// A posting snapshot owned by the candidate loop (no borrow of `Index`).
#[derive(Clone, Copy)]
struct PostingRef {
    doc: u32,
    counts: crate::search::tokenize::FieldCounts,
}

/// Ranked candidate paths, best first, at most `pool` entries.
///
/// Terms are folded here — the index stores folded tokens. A term with no
/// matches at all fails the AND and yields no candidates, like the engine's
/// all-terms requirement.
pub fn candidates(
    index: &Index,
    terms: &[String],
    bm25: Bm25,
    pool: usize,
    max_terms: usize,
) -> Vec<Candidate> {
    if pool == 0 {
        return Vec::new();
    }
    let folded: Vec<String> = terms
        .iter()
        .take(max_terms)
        .map(|t| fold::fold_str(t, Fold::Lower))
        .filter(|t| !t.is_empty())
        .collect();
    if folded.is_empty() || index.doc_count() == 0 {
        return Vec::new();
    }

    // Per query term: path → summed body counts over exact + prefix variants,
    // the path's page postings for BM25, and paths matching the exact term.
    let mut per_term: Vec<(BTreeMap<u32, usize>, Vec<PostingRef>, HashSet<u32>)> = Vec::new();
    for term in &folded {
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        let mut page_postings: Vec<PostingRef> = Vec::new();
        let mut exact: HashSet<u32> = HashSet::new();
        let add = |p: &crate::search::index::Posting,
                       counts: &mut BTreeMap<u32, usize>,
                       page_postings: &mut Vec<PostingRef>| {
            *counts.entry(doc_path_ix(p.doc)).or_insert(0) += p.counts[2] as usize;
            page_postings.push(PostingRef {
                doc: p.doc,
                counts: p.counts,
            });
        };
        // The prefix pass already includes the exact term (`range(prefix..)`
        // starts at it), so counts come from the prefix pass alone and the
        // exact pass only marks which paths matched the term itself.
        if let Some(postings) = index.postings(term) {
            for p in postings {
                exact.insert(doc_path_ix(p.doc));
            }
        }
        for p in index.postings_with_prefix(term, MAX_POSTINGS_PER_TERM) {
            add(&p, &mut counts, &mut page_postings);
        }
        if counts.is_empty() {
            return Vec::new();
        }
        per_term.push((counts, page_postings, exact));
    }

    // Intersect: only paths present in every term's candidate map survive.
    let mut paths: HashSet<u32> = per_term[0].0.keys().copied().collect();
    for (map, _, _) in per_term.iter().skip(1) {
        paths.retain(|p| map.contains_key(p));
    }
    if paths.is_empty() {
        return Vec::new();
    }

    let avg = {
        // Page-level averages: title/path average over first pages only
        // (they carry those fields), body average over all pages.
        let title_pages: Vec<u32> = index
            .first_page_lengths()
            .map(|l| l[0])
            .collect();
        let path_pages: Vec<u32> = index.first_page_lengths().map(|l| l[1]).collect();
        let body_avg = index.avg_body_len();
        let avg_n = |vals: &[u32]| vals.iter().map(|v| *v as f32).sum::<f32>() / vals.len().max(1) as f32;
        [avg_n(&title_pages), avg_n(&path_pages), body_avg]
    };
    let pages_total = index.doc_count();
    let mut out: Vec<Candidate> = paths
        .into_iter()
        .map(|path_ix| {
            let mut score = 0.0f32;
            let mut count = 0usize;
            for (map, page_postings, exact) in per_term.iter() {
                count += map[&path_ix];
                // A multi-page note is ranked by its densest page: score each
                // contributing page and take the best (the reference groups
                // per-document scores to a note the same way).
                let mut best = 0.0f32;
                for pr in page_postings.iter().filter(|p| doc_path_ix(p.doc) == path_ix) {
                    let lens = index.page_lengths(pr.doc).unwrap_or([0; 3]);
                    let s = bm25.score_term(
                        pr.counts,
                        lens,
                        avg,
                        pages_total,
                        page_postings.len(),
                        exact.contains(&path_ix),
                    );
                    if s > best {
                        best = s;
                    }
                }
                score += best;
            }
            Candidate {
                path_ix,
                score,
                count,
            }
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(pool);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::bm25::Bm25;
    use crate::search::index::Builder;
    use std::io::Write as _;
    use std::path::PathBuf;

    fn bm25() -> Bm25 {
        Bm25 {
            k1: 1.2,
            b_title: 0.3,
            b_body: 0.75,
            title_weight: 2.5,
        }
    }

    fn build(dir: &std::path::Path, files: &[(&str, &str)]) -> Index {
        let paths: Vec<PathBuf> = files
            .iter()
            .map(|(name, body)| {
                let p = dir.join(name);
                std::fs::File::create(&p)
                    .unwrap()
                    .write_all(body.as_bytes())
                    .unwrap();
                p
            })
            .collect();
        let mut b = Builder::new(paths, Some(dir.to_path_buf()));
        b.step(files.len());
        b.take_index()
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("vibecodium_search_query_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn candidate_for<'a>(index: &Index, got: &'a [Candidate], name: &str) -> Option<&'a Candidate> {
        got.iter()
            .find(|c| index.doc_path_for_ix(c.path_ix).ends_with(name))
    }

    #[test]
    fn candidates_require_all_terms() {
        let dir = tmp_dir("and");
        let index = build(&dir, &[("a.md", "alpha beta"), ("b.md", "only alpha")]);
        let got = candidates(
            &index,
            &["alpha".to_string(), "beta".to_string()],
            bm25(),
            10,
            8,
        );
        assert_eq!(got.len(), 1);
        assert!(candidate_for(&index, &got, "a.md").is_some());
        assert_eq!(got[0].count, 2, "alpha + beta occurrences");
    }

    #[test]
    fn candidates_expand_prefixes() {
        let dir = tmp_dir("prefix");
        let index = build(&dir, &[("alpha.md", "nothing relevant inside")]);
        // "alph" is not a stored term but prefixes to "alpha" (title/path).
        let got = candidates(&index, &["alph".to_string()], bm25(), 10, 8);
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn candidates_empty_when_a_term_is_missing() {
        let dir = tmp_dir("missing");
        let index = build(&dir, &[("a.md", "alpha")]);
        assert!(candidates(
            &index,
            &["alpha".to_string(), "zeta".to_string()],
            bm25(),
            10,
            8
        )
        .is_empty());
    }

    #[test]
    fn exact_match_outranks_prefix_only() {
        let dir = tmp_dir("exact");
        let index = build(&dir, &[("a.md", "alpha alpha alpha"), ("b.md", "alphabet soup")]);
        let got = candidates(&index, &["alpha".to_string()], bm25(), 10, 8);
        assert_eq!(got.len(), 2);
        assert!(
            got[0].score >= got[1].score,
            "exact doc must not rank below prefix-only doc"
        );
        assert_eq!(candidate_for(&index, &got, "a.md").unwrap().count, 3);    }

    /// Counts come from postings over ALL pages: a term appearing only deep
    /// in a multi-page file is counted exactly, no file reads.
    #[test]
    fn counts_sum_whole_file_across_pages() {
        let dir = tmp_dir("pages");
        let big = dir.join("big.md");
        let mut f = std::fs::File::create(&big).unwrap();
        f.write_all(b"start\n").unwrap();
        while f.metadata().unwrap().len() < 2 * 256 * 1024 {
            f.write_all(b"padding without terms\n").unwrap();
        }
        let mut expected = 0;
        for _ in 0..5 {
            f.write_all(b"rareterm\n").unwrap();
            expected += 1;
        }
        drop(f);
        let mut builder = Builder::new(vec![big.clone()], Some(dir.clone()));
        builder.step(1);
        let index = builder.take_index();
        let got = candidates(&index, &["rareterm".to_string()], bm25(), 10, 8);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].count, expected, "count must cover all pages");
        assert!(got[0].score > 0.0);
    }
}
