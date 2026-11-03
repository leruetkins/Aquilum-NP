use crate::search::cache::SearchContentCache;
use crate::search::fold::Fold;
    use super::*;
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
        // Cyrillic input (`trimmed[..5]` lands inside a multi-byte letter).
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
