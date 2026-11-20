use super::*;

fn document(markdown: &str) -> DocumentBuffer {
    DocumentBuffer::new(
        DocumentId::from_u64(1),
        PathBuf::from("note.md"),
        "note".to_string(),
        markdown.to_string(),
        1,
    )
}

#[test]
fn section_stops_at_same_or_higher_heading() {
    let doc = document("# One\nintro\n## Two\na\n### Three\nb\n## Four\nc\n");
    let section = find_heading_section(&doc, "Two").expect("section");
    assert_eq!(section.start_line, 2);
    assert_eq!(section.end_line, 6);
    assert_eq!(section.heading, "Two");
}

#[test]
fn section_ignores_headings_inside_fenced_code() {
    let doc = document("# One\n```\n# Not a heading\n```\n## Two\nbody\n");
    let section = find_heading_section(&doc, "Two").expect("section");
    assert_eq!((section.start_line, section.end_line), (4, 7));
}

#[test]
fn fragment_matches_case_and_github_slug() {
    let doc = document("## My Heading\ntext\n");
    assert!(find_heading_section(&doc, "my heading").is_some());
    assert!(find_heading_section(&doc, "My-Heading").is_some());
    assert!(find_heading_section(&doc, "missing").is_none());
}

#[test]
fn target_split_keeps_decoded_fragment() {
    assert_eq!(
        split_link_target("Люди/Сергей%20Кот.md#Заметки"),
        (
            "Люди/Сергей%20Кот.md".to_string(),
            Some("Заметки".to_string())
        )
    );
    assert_eq!(
        split_link_target("#Раздел"),
        (String::new(), Some("Раздел".to_string()))
    );
    assert_eq!(
        split_link_target("Note.md"),
        ("Note.md".to_string(), None)
    );
}

#[test]
fn heading_line_for_fragment_returns_heading_start() {
    let doc = document("intro\n## Two\ntext\n");
    assert_eq!(AppView::heading_line_for_fragment(&doc, "Two"), Some(1));
    assert_eq!(AppView::heading_line_for_fragment(&doc, "Missing"), None);
}

#[test]
fn fragment_accepts_extra_heading_hashes() {
    let doc = document("## Привет:\ntext\n");
    assert!(find_heading_section(&doc, "Привет:").is_some());
    assert!(find_heading_section(&doc, "#Привет:").is_some());
    assert!(find_heading_section(&doc, "##Привет:").is_some());
}
