//! Streaming match location for oversized files (stage B, reference model
//! `core/src/search/hits/file.rs` + `headings/outline.rs`).
//!
//! The index postings already know WHICH page of a file carries a term
//! (`Index::pages_with_all_terms`); this module runs one sequential pass over
//! the file and checks only those listed pages, so a match near the end of a
//! multi-hundred-megabyte file costs a skip-read, not a full verify sweep.
//! `Cancel` is checked between pages: a superseded query stops reading
//! immediately.

use std::io;
use std::path::Path;

use crate::search::cancel::Cancel;
use crate::search::pages::PageStream;

/// Checks ONLY the listed page ordinals (ascending, from the index postings).
///
/// One sequential pass: pages before the first target are skipped without
/// checks, each target page is verified once, and the stream stops after the
/// last target. `check` returns `(count_in_page, snippet)` for a qualifying
/// page; the count is informational — result counts come from the index
/// postings, which cover the whole file. `Ok(None)` — nothing matched (or
/// cancelled).
pub(crate) fn check_pages(
    path: &Path,
    targets: &[u32],
    cancel: &Cancel,
    check: &mut dyn FnMut(&str) -> Option<(usize, String)>,
) -> io::Result<Option<(usize, String)>> {
    if targets.is_empty() {
        return Ok(None);
    }
    let mut stream = PageStream::open(path)?;
    let mut ordinal = 0u32;
    let mut next = 0usize;
    while let Some(page) = stream.next_page()? {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        if ordinal == targets[next] {
            next += 1;
            if let Some(hit) = check(&page) {
                return Ok(Some(hit));
            }
            if next >= targets.len() {
                return Ok(None);
            }
        }
        ordinal += 1;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("vibecodium_search_locate_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_a_match_beyond_the_first_megabyte() {
        let dir = tmp_dir("far");
        let path = dir.join("big.md");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"start marker\n").unwrap();
        while f.metadata().unwrap().len() < 2 * 1024 * 1024 {
            f.write_all(b"filler line without the term\n").unwrap();
        }
        f.write_all(b"the needle hides here\n").unwrap();
        drop(f);

        // Which page carries the needle (what the index postings provide).
        let target = {
            let mut stream = PageStream::open(&path).unwrap();
            let mut ordinal = 0u32;
            loop {
                match stream.next_page().unwrap() {
                    Some(page) if page.contains("needle") => break ordinal,
                    Some(_) => ordinal += 1,
                    None => panic!("needle page not found"),
                }
            }
        };

        let mut checks = 0usize;
        let hit = check_pages(&path, &[target], &Cancel::default(), &mut |page| {
            checks += 1;
            if page.contains("needle") {
                Some((1, page.to_string()))
            } else {
                None
            }
        })
        .unwrap();
        let (_, snippet) = hit.expect("match near the end must be located");
        assert!(snippet.contains("the needle hides here"));
        assert_eq!(
            checks, 1,
            "only the target page may be checked — earlier pages are skipped, not verified"
        );
    }

    /// Ordinals beyond the file's end stream through (no match), and empty
    /// targets read nothing at all.
    #[test]
    fn out_of_range_targets_and_empty_targets_match_nothing() {
        let dir = tmp_dir("none");
        let path = dir.join("big.md");
        let mut f = std::fs::File::create(&path).unwrap();
        while f.metadata().unwrap().len() < 600 * 1024 {
            f.write_all(b"nothing relevant\n").unwrap();
        }
        drop(f);

        let mut checks = 0usize;
        let hit = check_pages(&path, &[10_000], &Cancel::default(), &mut |_| {
            checks += 1;
            None
        })
        .unwrap();
        assert!(hit.is_none() && checks == 0, "ordinal beyond EOF never fires");

        let hit = check_pages(&path, &[], &Cancel::default(), &mut |_| {
            checks += 1;
            None
        })
        .unwrap();
        assert!(hit.is_none() && checks == 0, "empty targets read nothing");
    }

    #[test]
    fn cancelled_token_stops_the_stream() {
        let dir = tmp_dir("cancel");
        let path = dir.join("big.md");
        let mut f = std::fs::File::create(&path).unwrap();
        while f.metadata().unwrap().len() < 600 * 1024 {
            f.write_all(b"plenty of pages with the needle\n").unwrap();
        }
        drop(f);
        // Superseded before the first page: nothing may be reported.
        let generation = crate::search::cancel::Generation::default();
        let stale = generation.next();
        let _newer = generation.next();
        let hit = check_pages(&path, &[0], &stale, &mut |_| Some((1, "x".into()))).unwrap();
        assert!(hit.is_none(), "cancelled locate must not report a match");
    }
}
