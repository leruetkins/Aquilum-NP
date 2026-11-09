//! Mobile projection of the desktop markdown engine.
//!
//! These are the same source files the GPUI app classifies and highlights:
//! `classify`, `inline`, `syntax_spans`, `grammar`, `quote`, `list` and
//! `code_scan` are included by path, not reimplemented. `head_char_count` and
//! the GPUI/i18n shims in `lib.rs` are the only glue needed because the
//! mobile core never paints.
//!
//! `model()` flattens that logic into one JSON-friendly row per source line:
//! block kind, marker, content, inline spans with char offsets, code-fence
//! language and table cells. The Compose source view and preview both consume
//! this model, so their interpretation matches the desktop parser.

use serde::Serialize;

#[path = "../../../aquilum-gpui/src/markdown/code_scan.rs"]
pub mod code_scan;
#[path = "../../../aquilum-gpui/src/markdown/quote.rs"]
pub mod quote;
#[path = "../../../aquilum-gpui/src/markdown/list.rs"]
pub mod list;
#[path = "../../../aquilum-gpui/src/markdown/classify.rs"]
pub mod classify;
#[path = "../../../aquilum-gpui/src/markdown/grammar.rs"]
pub mod grammar;
#[path = "../../../aquilum-gpui/src/markdown/inline.rs"]
pub mod inline;
#[path = "../../../aquilum-gpui/src/markdown/syntax_spans.rs"]
pub mod syntax_spans;

pub use classify::*;
pub use code_scan::*;
pub use grammar::*;
pub use inline::*;
pub use list::*;
pub use quote::*;
pub use syntax_spans::*;

// The shared grammar/quote sources refer to `gpui::Rgba` and to the desktop
// color helper. Both exist as crate-local shims here; the mobile core never
// paints.
pub(crate) use crate::gpui;

pub(crate) fn parse_hex_color(hex: &str) -> Option<crate::gpui::Rgba> {
    let value = hex.trim().trim_start_matches('#');
    let parse = |text: &str| u8::from_str_radix(text, 16).ok();
    let (r, g, b, a) = match value.len() {
        3 => {
            let r = parse(&value[0..1])? * 17;
            let g = parse(&value[1..2])? * 17;
            let b = parse(&value[2..3])? * 17;
            (r, g, b, 255)
        }
        6 => (
            parse(&value[0..2])?,
            parse(&value[2..4])?,
            parse(&value[4..6])?,
            255,
        ),
        8 => (
            parse(&value[0..2])?,
            parse(&value[2..4])?,
            parse(&value[4..6])?,
            parse(&value[6..8])?,
        ),
        _ => return None,
    };
    Some(crate::gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct LineSpan {
    pub kind: String,
    #[serde(skip)]
    pub text: String,
    #[serde(skip)]
    pub url: Option<String>,
    /// Char offsets relative to `MarkdownLine::content`.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkdownLine {
    #[serde(skip)]
    pub raw: String,
    pub kind: String,
    #[serde(skip_serializing_if = "is_zero_u8")]
    pub level: u8,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub indent: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub marker: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_opening: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub prefix_chars: usize,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_spans"
    )]
    pub spans: Vec<LineSpan>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

/// Inline spans are the bulk of a large note's JSON. The shell only needs
/// `kind` and the two char offsets, so one compact string per line replaces an
/// array of objects: `"text:0:7;bold:7:13"`.
fn serialize_spans<S>(spans: &[LineSpan], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut out = String::new();
    for (index, span) in spans.iter().enumerate() {
        if index > 0 {
            out.push(';');
        }
        out.push_str(&span.kind);
        out.push(':');
        out.push_str(&span.start.to_string());
        out.push(':');
        out.push_str(&span.end.to_string());
    }
    serializer.serialize_str(&out)
}

pub fn model(text: &str) -> Vec<MarkdownLine> {
    let lines: Vec<&str> = text.lines().collect();
    let code_states = analyze_code_blocks(&lines);
    let mut result = Vec::with_capacity(lines.len());
    let mut current_language: Option<String> = None;
    let mut index = 0;

    while index < lines.len() {
        let raw = lines[index];
        let state = code_states[index].clone();

        if let Some(state) = &state {
            match state.kind {
                CodeBlockKind::OpeningFence => current_language = state.lang.clone(),
                CodeBlockKind::ClosingFence => current_language = None,
                CodeBlockKind::InsideCode => {}
            }
        }

        if state.is_none() && is_table_start(&lines, index) {
            result.push(table_line(raw, "table", split_cells(raw)));
            result.push(table_line(lines[index + 1], "table_separator", Vec::new()));
            index += 2;
            while index < lines.len() {
                let current = lines[index];
                if current.trim().is_empty() || !current.contains('|') {
                    break;
                }
                result.push(table_line(current, "table_row", split_cells(current)));
                index += 1;
            }
            continue;
        }

        let code_arg = state.as_ref().map(|state| (state.kind, state.lang.as_deref()));
        let kind = classify_line(raw, code_arg);
        result.push(line_model(raw, &kind, state.as_ref(), current_language.clone()));
        index += 1;
    }

    result
}

fn line_model(
    raw: &str,
    kind: &LineKind<'_>,
    state: Option<&CodeBlockState>,
    code_language: Option<String>,
) -> MarkdownLine {
    let leading = raw.chars().count() - raw.trim_start().chars().count();
    let (kind_name, level, indent, content, checked, num, is_opening, language): (
        &str,
        u8,
        usize,
        &str,
        Option<bool>,
        Option<String>,
        Option<bool>,
        Option<String>,
    ) = match kind {
        LineKind::Heading1(text) => ("heading", 1, leading, *text, None, None, None, None),
        LineKind::Heading2(text) => ("heading", 2, leading, *text, None, None, None, None),
        LineKind::Heading3(text) => ("heading", 3, leading, *text, None, None, None, None),
        LineKind::Heading4(text) => ("heading", 4, leading, *text, None, None, None, None),
        LineKind::Heading5(text) => ("heading", 5, leading, *text, None, None, None, None),
        LineKind::Heading6(text) => ("heading", 6, leading, *text, None, None, None, None),
        LineKind::Quote(text) => ("quote", 0, leading, *text, None, None, None, None),
        LineKind::Task { checked, text } => (
            "task",
            0,
            task_prefix(raw).map_or(leading, |task| task.indent_chars),
            *text,
            Some(*checked),
            None,
            None,
            None,
        ),
        LineKind::BulletList { indent, text } => {
            ("bullet", 0, *indent, *text, None, None, None, None)
        }
        LineKind::NumberedList { num, text } => (
            "numbered",
            0,
            leading,
            *text,
            None,
            Some((*num).to_string()),
            None,
            None,
        ),
        LineKind::HorizontalRule => ("rule", 0, leading, "", None, None, None, None),
        LineKind::CodeFence {
            is_opening,
            lang,
        } => (
            "code_fence",
            0,
            0,
            "",
            None,
            None,
            Some(*is_opening),
            lang.map(str::to_string),
        ),
        LineKind::CodeLine(text) => ("code", 0, 0, *text, None, None, None, code_language),
        LineKind::Paragraph(text) => ("paragraph", 0, leading, *text, None, None, None, None),
    };

    let marker = match kind {
        LineKind::Paragraph(_) | LineKind::CodeLine(_) | LineKind::HorizontalRule => String::new(),
        LineKind::CodeFence { .. } => raw.to_string(),
        _ => raw[..raw.len().saturating_sub(content.len())].to_string(),
    };

    let mut kind_name = kind_name;
    let mut target = None;
    let mut alt = None;
    if kind_name == "paragraph" {
        if let Some((image_alt, image_target)) = image_only(content) {
            kind_name = "image";
            alt = Some(image_alt);
            target = Some(image_target);
        }
    }

    let callout = if matches!(kind, LineKind::Quote(_)) {
        callout_prefix(raw).map(|callout| callout_label(&callout, "en"))
    } else {
        None
    };

    let (spans, prefix_chars) = if matches!(
        kind_name,
        "code" | "code_fence" | "rule" | "image" | "table" | "table_row" | "table_separator"
    ) {
        (Vec::new(), 0)
    } else {
        let (spans, prefix) = line_inline_spans(raw, kind);
        (spans_to_model(&spans), prefix)
    };

    MarkdownLine {
        raw: raw.to_string(),
        kind: kind_name.to_string(),
        level,
        indent,
        marker,
        content: content.to_string(),
        checked,
        num,
        language,
        is_opening,
        block_id: state.map(|state| state.block_id),
        callout,
        prefix_chars,
        spans,
        cells: Vec::new(),
        target,
        alt,
    }
}

fn spans_to_model(spans: &[SyntaxSpan]) -> Vec<LineSpan> {
    let mut result = Vec::new();
    let mut offset = 0usize;
    for span in spans {
        let (text, kind, url) = span_parts(span);
        let length = text.chars().count();
        if length > 0 {
            result.push(LineSpan {
                kind: kind.to_string(),
                text: text.to_string(),
                url,
                start: offset,
                end: offset + length,
            });
            offset += length;
        }
    }
    result
}

fn span_parts(span: &SyntaxSpan) -> (&str, &'static str, Option<String>) {
    match span {
        SyntaxSpan::Marker(text) => (text, "marker", None),
        SyntaxSpan::Text(text) => (text, "text", None),
        SyntaxSpan::Bold(text) => (text, "bold", None),
        SyntaxSpan::Italic(text) => (text, "italic", None),
        SyntaxSpan::BoldItalic(text) => (text, "bold_italic", None),
        SyntaxSpan::InlineCode(text) => (text, "inline_code", None),
        SyntaxSpan::Strikethrough(text) => (text, "strikethrough", None),
        SyntaxSpan::Link { text, url } => (text, "link", Some(url.clone())),
        SyntaxSpan::Underline(text) => (text, "underline", None),
        SyntaxSpan::Highlight(text) => (text, "highlight", None),
        SyntaxSpan::Tag(text) => (text, "tag", None),
        SyntaxSpan::Grammar { text, .. } => (text, "grammar", None),
    }
}

fn table_line(raw: &str, kind: &str, cells: Vec<String>) -> MarkdownLine {
    MarkdownLine {
        raw: raw.to_string(),
        kind: kind.to_string(),
        level: 0,
        indent: 0,
        marker: String::new(),
        content: raw.to_string(),
        checked: None,
        num: None,
        language: None,
        is_opening: None,
        block_id: None,
        callout: None,
        prefix_chars: 0,
        spans: Vec::new(),
        cells,
        target: None,
        alt: None,
    }
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    let header = lines[index].trim();
    if !header.contains('|') {
        return false;
    }
    is_table_separator(lines[index + 1].trim())
}

fn is_table_separator(line: &str) -> bool {
    if !line.contains('|') {
        return false;
    }
    let cells = split_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let trimmed = cell.trim();
            let trimmed = trimmed.trim_start_matches(':').trim_end_matches(':');
            trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-')
        })
}

fn split_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(|cell| cell.trim().to_string()).collect()
}

fn image_only(text: &str) -> Option<(String, String)> {
    let line = text.trim();
    if let Some(rest) = line.strip_prefix("![[") {
        if let Some(target) = rest.strip_suffix("]]") {
            let target = target.trim();
            if !target.is_empty() {
                return Some((target.to_string(), target.to_string()));
            }
        }
    }
    if let Some(rest) = line.strip_prefix("![") {
        if let Some(close) = rest.find("](") {
            let alt = rest[..close].to_string();
            let tail = &rest[close + 2..];
            if let Some(target) = tail.strip_suffix(')') {
                let target = target.trim();
                if !target.is_empty() {
                    return Some((alt, target.to_string()));
                }
            }
        }
    }
    None
}

/// Number of chars in the first `byte_len` bytes of `line` (clamped to a char boundary).
pub(crate) fn head_char_count(line: &str, byte_len: usize) -> usize {
    let mut end = byte_len.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_headings_and_inline_spans() {
        let lines = model("# Title\n\n**bold** and `code`\n\n> quote\n\n- [x] done\n");
        assert_eq!(lines[0].kind, "heading");
        assert_eq!(lines[0].level, 1);
        assert_eq!(lines[0].content, "Title");
        assert_eq!(lines[0].marker, "# ");

        let paragraph = &lines[2];
        assert_eq!(paragraph.kind, "paragraph");
        assert!(paragraph.spans.iter().any(|span| span.kind == "bold"));
        assert!(paragraph.spans.iter().any(|span| span.kind == "inline_code"));
        assert!(paragraph.spans.iter().any(|span| span.kind == "marker"));

        assert_eq!(lines[4].kind, "quote");
        assert_eq!(lines[4].content, "quote");

        assert_eq!(lines[6].kind, "task");
        assert_eq!(lines[6].checked, Some(true));
        assert_eq!(lines[6].content, "done");
        assert_eq!(lines[6].marker, "- [x] ");
    }

    #[test]
    fn code_fence_carries_language_to_lines() {
        let lines = model("```rust\nlet x = 1;\n```\n");
        assert_eq!(lines[0].kind, "code_fence");
        assert_eq!(lines[0].language.as_deref(), Some("rust"));
        assert_eq!(lines[0].is_opening, Some(true));
        assert_eq!(lines[1].kind, "code");
        assert_eq!(lines[1].language.as_deref(), Some("rust"));
        assert_eq!(lines[2].kind, "code_fence");
        assert_eq!(lines[2].is_opening, Some(false));
    }

    #[test]
    fn tables_and_images_are_projected() {
        let lines = model("| A | B |\n| --- | --- |\n| 1 | 2 |\n\n![](pic.png)\n");
        assert_eq!(lines[0].kind, "table");
        assert_eq!(lines[0].cells, vec!["A", "B"]);
        assert_eq!(lines[1].kind, "table_separator");
        assert_eq!(lines[2].kind, "table_row");
        assert_eq!(lines[2].cells, vec!["1", "2"]);
        assert_eq!(lines[4].kind, "image");
        assert_eq!(lines[4].target.as_deref(), Some("pic.png"));
    }

    #[test]
    fn supports_escapes_links_and_highlights() {
        let lines = model(r"\*not italic\* [link](https://x) ==mark== ~~gone~~");
        let spans = &lines[0].spans;
        assert!(spans.iter().any(|span| span.kind == "link"));
        assert!(spans.iter().any(|span| span.kind == "highlight"));
        assert!(spans.iter().any(|span| span.kind == "strikethrough"));
        assert!(!spans.iter().any(|span| span.kind == "italic"));
    }
}
