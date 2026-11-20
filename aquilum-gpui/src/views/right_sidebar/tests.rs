use super::*;

fn title_of(line: &str) -> Option<(usize, String, usize)> {
    parse_outline_line(line, 7).map(|item| (item.level, item.title, item.line))
}

#[test]
fn outline_parses_levels_and_titles() {
    assert_eq!(title_of("# Title"), Some((1, "Title".to_string(), 7)));
    assert_eq!(title_of("  ###   Sub  heading  "), Some((3, "Sub  heading".to_string(), 7)));
    assert_eq!(title_of("###### Deep"), Some((6, "Deep".to_string(), 7)));
}

#[test]
fn outline_strips_closing_hashes_but_keeps_text_sharps() {
    assert_eq!(title_of("## Notes ##").map(|t| t.1), Some("Notes".to_string()));
    assert_eq!(title_of("# C#").map(|t| t.1), Some("C#".to_string()));
}

#[test]
fn outline_ignores_non_headings() {
    assert_eq!(title_of("text #"), None);
    assert_eq!(title_of("#nospace"), None);
    assert_eq!(title_of("# "), None);
}

#[test]
fn outline_skips_fenced_code_blocks() {
    let doc = DocumentBuffer::new(
        DocumentId::new(),
        std::path::PathBuf::from("note.md"),
        "note".to_string(),
        "```\n# not a heading\n```\n# Real heading\n".to_string(),
        0,
    );
    let items = collect_outline(&doc);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Real heading");
    assert_eq!(items[0].line, 3);
}
