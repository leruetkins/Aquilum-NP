use super::classify::*;
use super::grammar::*;
use super::inline::*;
use super::list::*;
use super::quote::*;
use super::syntax_spans::*;
use super::visual::*;

/// One run of text painted inside a visual row, with the font traits the renderer
/// ends up using for it (the row's base traits merged with the span's own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintedSegment {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// True for inline code and for its delimiters, i.e. for everything the editor
    /// covers with the code chip. The editor paints the chip from these runs (the row
    /// backdrop rectangles in `views::app_view`) instead of from a text-run background, so
    /// that the selection rectangle — which is painted before the text — is not covered by
    /// it, and so that the chip and the selection are cut from the same columns.
    pub code: bool,
    /// Full `#tag` name if this segment is a markdown tag.
    pub tag: Option<String>,
    /// Line style and colours if this segment is a school-grammar member.
    pub grammar: Option<GrammarSpanStyle>,
}

/// The exact text a visual row paints, plus the source line char of every painted char.
#[derive(Debug, Clone)]
pub struct PaintedRow {
    pub segments: Vec<PaintedSegment>,
    /// `line_chars[k]` = line char index (chars from the start of the line) painted at
    /// column `k`. One extra trailing entry holds the line char just past the last
    /// painted char, so `line_chars.len() == columns() + 1`.
    ///
    /// Non-decreasing, but not strictly so: synthetic continuation padding repeats the
    /// first char of the wrapped content, and hidden inline markers leave gaps (the one
    /// exception being an inline-code delimiter, which keeps its cell — see
    /// `is_inline_code_delim`).
    pub line_chars: Vec<usize>,
}

/// An inline-code delimiter is a run of one or more backticks — the run length is what
/// closes the span, so `` `` `` is a delimiter in its own right. It is the one inline
/// marker that keeps its cell when it is not painted, so that the code chip has a
/// constant width and the selection rectangle does not jump when the delimiters are
/// revealed around the caret. Both the geometry (`row_painted_text`) and the renderer
/// (`views::app_view::render_syntax_spans`) must agree on this.
pub fn is_inline_code_delim(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b == b'`')
}

impl PaintedRow {
    /// Number of painted character cells in this row.
    pub fn columns(&self) -> usize {
        self.line_chars.len().saturating_sub(1)
    }

    /// Column ranges the row covers with the inline-code chip, merged and in order.
    ///
    /// One painted char owns one column, so the char offset of a segment inside the
    /// painted text is the column it starts at. Adjacent code segments merge, which is
    /// what turns a delimiter and the code it wraps into a single chip.
    pub fn code_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut col = 0usize;
        for segment in &self.segments {
            let n = segment.text.chars().count();
            if segment.code && n > 0 {
                match ranges.last_mut() {
                    Some(last) if last.1 == col => last.1 = col + n,
                    _ => ranges.push((col, col + n)),
                }
            }
            col += n;
        }
        ranges
    }

    /// Column ranges covered by grammar members, with their line style and colours.
    /// Adjacent segments of the same style merge, so an escape splitting the text
    /// does not break the line.
    pub fn grammar_ranges(&self) -> Vec<(usize, usize, GrammarSpanStyle)> {
        let mut ranges: Vec<(usize, usize, GrammarSpanStyle)> = Vec::new();
        let mut col = 0usize;
        for segment in &self.segments {
            let n = segment.text.chars().count();
            if let Some(style) = segment.grammar {
                if n > 0 {
                    match ranges.last_mut() {
                        Some(last) if last.1 == col && last.2 == style => last.1 = col + n,
                        _ => ranges.push((col, col + n, style)),
                    }
                }
            }
            col += n;
        }
        ranges
    }

    /// Column ranges of any `#tag` badges in this row, with their tag names.
    pub fn tag_ranges(&self) -> Vec<(usize, usize, String)> {
        let mut ranges: Vec<(usize, usize, String)> = Vec::new();
        let mut col = 0usize;
        for segment in &self.segments {
            let n = segment.text.chars().count();
            if let Some(ref tag) = segment.tag {
                if n > 0 {
                    ranges.push((col, col + n, tag.clone()));
                }
            }
            col += n;
        }
        ranges
    }
}

fn push_verbatim(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src_start: usize,
    bold: bool,
    italic: bool,
    code: bool,
) {
    push_verbatim_tagged(segments, line_chars, text, src_start, bold, italic, code, None);
}

fn push_verbatim_tagged(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src_start: usize,
    bold: bool,
    italic: bool,
    code: bool,
    tag: Option<String>,
) {
    if text.is_empty() {
        return;
    }
    for i in 0..text.chars().count() {
        line_chars.push(src_start + i);
    }
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code, tag, grammar: None });
}

/// Padding the renderer synthesises for a wrapped row's continuation indent. Every
/// char resolves to `src`, so a click in the padding lands on a real character.
fn push_padding(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src: usize,
    bold: bool,
    italic: bool,
) {
    if text.is_empty() {
        return;
    }
    for _ in 0..text.chars().count() {
        line_chars.push(src);
    }
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code: false, tag: None, grammar: None });
}

/// Paints a grammar fragment's runs, mapping every painted char back to its source
/// char.
///
/// The fragment is already parsed into the markup a member may carry, so a bold word
/// inside it keeps its face and the line under the member stays unbroken: hidden
/// markers take no cell at all. The escape backslash never paints, even when revealed,
/// and the escaped char maps to its own source offset.
fn push_grammar_runs(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    content: &[VisibleSpan],
    src_start: usize,
    bold: bool,
    italic: bool,
    style: GrammarSpanStyle,
) {
    let mut src = src_start;
    for visible in content {
        let (text, is_bold, is_italic, is_code) = match &visible.span {
            SyntaxSpan::Marker(t) => {
                let n = t.chars().count();
                if t == "\\" || (!visible.marker_visible && !is_inline_code_delim(t)) {
                    src += n;
                    continue;
                }
                (t.as_str(), bold, italic, is_inline_code_delim(t))
            }
            SyntaxSpan::Text(t) => (t.as_str(), bold, italic, false),
            SyntaxSpan::Bold(t) => (t.as_str(), true, italic, false),
            SyntaxSpan::Italic(t) => (t.as_str(), bold, true, false),
            SyntaxSpan::BoldItalic(t) => (t.as_str(), true, true, false),
            SyntaxSpan::InlineCode(t) => (t.as_str(), bold, italic, true),
            SyntaxSpan::Strikethrough(t) => (t.as_str(), bold, italic, false),
            other => (span_text(other), bold, italic, false),
        };
        push_grammar_run(segments, line_chars, text, src, is_bold, is_italic, is_code, style);
        src += text.chars().count();
    }
}

fn push_grammar_run(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src_start: usize,
    bold: bool,
    italic: bool,
    code: bool,
    style: GrammarSpanStyle,
) {
    if text.is_empty() {
        return;
    }
    for i in 0..text.chars().count() {
        line_chars.push(src_start + i);
    }
    segments.push(PaintedSegment {
        text: text.to_string(),
        bold,
        italic,
        code,
        tag: None,
        grammar: Some(style),
    });
}

/// Number of chars in the first `byte_len` bytes of `line` (clamped to a char boundary).
pub(crate) fn head_char_count(line: &str, byte_len: usize) -> usize {
    let mut end = byte_len.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].chars().count()
}

/// The exact text a visual row paints, in paint order, together with the source line
/// char of every painted char.
///
/// This mirrors `render_live_styled_row` (and `render_syntax_spans`) branch for branch.
/// The editor's geometry is derived from it instead of from a `column * char_width`
/// cell model, so wide glyphs (emoji, CJK, VS16/ZWJ sequences) and the continuation
/// indents of wrapped rows are measured where they actually land.
///
/// NOTE: `render_live_styled_row` in `views/app_view.rs` must stay in sync with this.
pub fn row_painted_text(
    line: &str,
    classified: &LineKind<'_>,
    v: &VisualRow,
    is_active_line: bool,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
) -> PaintedRow {
    let mut segments: Vec<PaintedSegment> = Vec::new();
    let mut line_chars: Vec<usize> = Vec::new();

    // The renderer paints a row with no content as a single blank cell.
    if line.is_empty() || v.char_start >= v.char_end {
        push_padding(&mut segments, &mut line_chars, " ", v.char_end, false, false);
        line_chars.push(v.char_end);
        return PaintedRow { segments, line_chars };
    }

    // Base font traits of the row body: `render_syntax_spans` receives these as the
    // default weight/style for every span it flattens.
    let (base_bold, base_italic) = match classified {
        LineKind::Heading1(_) => (headings.h1.bold, headings.h1.italic),
        LineKind::Heading2(_) => (headings.h2.bold, headings.h2.italic),
        LineKind::Heading3(_) => (headings.h3.bold, headings.h3.italic),
        LineKind::Heading4(_) => (headings.h4.bold, headings.h4.italic),
        LineKind::Heading5(_) => (headings.h5.bold, headings.h5.italic),
        LineKind::Heading6(_) => (headings.h6.bold, headings.h6.italic),
        // A callout header is a label, not quoted prose: the renderer sets it bold and
        // upright, so its cells are measured upright too.
        LineKind::Quote(_) if callout_prefix(line).is_some() => (true, false),
        // Quoted prose is painted upright (`render_live_styled_row` gives its runs
        // the normal face). An italic base here would measure different advances
        // and shift the glyphs, the chip and the caret off their columns.
        LineKind::Quote(_) => (false, false),
        _ => (false, false),
    };

    // 1. Marker prefix, in paint order.
    match classified {
        // A heading marker is only painted on the active line, and only on its first
        // row. The renderer emits the literal marker string, so map it to the leading
        // line chars.
        LineKind::Heading1(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "# ", 0, base_bold, base_italic, false),
        LineKind::Heading2(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "## ", 0, base_bold, base_italic, false),
        LineKind::Heading3(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "### ", 0, base_bold, base_italic, false),
        LineKind::Heading4(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "#### ", 0, base_bold, base_italic, false),
        LineKind::Heading5(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "##### ", 0, base_bold, base_italic, false),
        LineKind::Heading6(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "###### ", 0, base_bold, base_italic, false),
        LineKind::Quote(_) => {
            let prefix = quote_prefix(line);
            let callout = callout_prefix(line);
            if v.sub_idx == 0 {
                // The markers stay in the grid whether or not they are shown: the
                // renderer hides them behind one bar per level and reveals the raw
                // text only on the line being edited. Both spellings are exactly
                // `prefix.chars` cells wide, so the wrap does not move.
                match (callout, prefix) {
                    // A callout spends its marker cells on its icon and, when it has
                    // no title of its own, on its type. A titled header is its own
                    // heading: the type name is not repeated, and the title follows
                    // the icon, like any other hidden inline marker.
                    (Some(c), Some(q)) if !is_active_line => {
                        push_verbatim(&mut segments, &mut line_chars, &" ".repeat(q.chars), 0, false, false, false);
                        push_padding(&mut segments, &mut line_chars, &" ".repeat(CALLOUT_ICON_CELLS), q.chars, false, false);
                        if c.title.is_empty() {
                            let label = callout_label_padded(&c, q.chars + CALLOUT_ICON_CELLS);
                            push_verbatim(&mut segments, &mut line_chars, &label, q.chars + CALLOUT_ICON_CELLS, true, false, false);
                        }
                    }
                    // Editing it: the raw marker, muted and upright like a heading's.
                    (Some(c), _) => push_verbatim(&mut segments, &mut line_chars, &line[..c.bytes], 0, false, false, false),
                    // The marker region is painted by its own upright div in the
                    // renderer, outside the italic prose wrapper, so it is measured
                    // upright here too — italic advances differ, and the content after
                    // it would otherwise start a fraction of a cell off the glyphs.
                    (None, Some(q)) if !is_active_line => {
                        push_verbatim(&mut segments, &mut line_chars, &" ".repeat(q.chars), 0, base_bold, false, false)
                    }
                    (None, Some(q)) => push_verbatim(&mut segments, &mut line_chars, &line[..q.bytes], 0, base_bold, false, false),
                    (None, None) => {}
                }
            } else {
                // A wrapped callout header continues under its title, i.e. past the
                // icon column, not under the quote bar alone.
                let icon_cells = if callout.is_some() { CALLOUT_ICON_CELLS } else { 0 };
                let pad = " ".repeat(prefix.map_or(0, |q| q.chars) + icon_cells);
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, base_bold, false);
            }
        }
        LineKind::Task { .. } => {
            if let Some(task) = task_prefix(line) {
                if v.sub_idx == 0 {
                    let show_raw = reveal.shows(0, task.chars);
                    let prefix = task_paint_prefix(line, &task, show_raw);
                    push_verbatim(&mut segments, &mut line_chars, &prefix, 0, true, base_italic, false);
                } else {
                    push_padding(&mut segments, &mut line_chars, &" ".repeat(task.chars), v.char_start, true, base_italic);
                }
            }
        }
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let raw_bullet = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { &trimmed[..2] } else { "" };
            let bullet = bullet_paint_text(raw_bullet, is_active_line);
            if v.sub_idx == 0 {
                push_verbatim(&mut segments, &mut line_chars, indent, 0, false, base_italic, false);
                push_verbatim(&mut segments, &mut line_chars, bullet, indent.chars().count(), true, base_italic, false);
            } else {
                let pad = format!("{}  ", indent);
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, false, base_italic);
            }
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let num_prefix_len = num.len() + 2;
            let num_part = if trimmed.len() >= num_prefix_len { &trimmed[..num_prefix_len] } else { "" };
            if v.sub_idx == 0 {
                push_verbatim(&mut segments, &mut line_chars, indent, 0, false, base_italic, false);
                push_verbatim(&mut segments, &mut line_chars, num_part, indent.chars().count(), true, base_italic, false);
            } else {
                let pad = " ".repeat(indent.chars().count() + num_part.chars().count());
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, false, base_italic);
            }
        }
        // A collapsed rule or fence paints a rule / a language label instead of the
        // line, but the geometry still follows the raw line so that a click resolves to
        // the character under the cursor.
        LineKind::HorizontalRule | LineKind::CodeFence { .. } => {
            push_verbatim(&mut segments, &mut line_chars, line, 0, false, false, false);
        }
        _ => {}
    }

    // 2. Content: the syntax-span slice the renderer paints for this row.
    match classified {
        // Handled as the prefix above.
        LineKind::HorizontalRule | LineKind::CodeFence { .. } => {}
        // Code rows paint the raw slice, with no inline-marker filtering.
        LineKind::CodeLine(_) => {
            let text: String = line
                .chars()
                .skip(v.char_start)
                .take(v.char_end.saturating_sub(v.char_start))
                .collect();
            push_verbatim(&mut segments, &mut line_chars, &text, v.char_start, base_bold, base_italic, false);
        }
        _ => {
            let (spans, content_offset) = line_inline_spans(line, classified);
            let s = v.char_start.saturating_sub(content_offset);
            let e = v.char_end.saturating_sub(content_offset);
            // The span slice starts at `char_start` chars into the line; skip the block
            // prefix the renderer paints separately.
            let mut src = content_offset + s;
            for visible in slice_syntax_spans_revealed(&spans, s, e, content_offset, reveal) {
                // A grammar fragment paints escape-resolved text, so its runs are
                // pushed separately from the plain span table.
                if let SyntaxSpan::Grammar { kind, line_color, text_color, text, content } = &visible.span {
                    let style = GrammarSpanStyle {
                        line: kind.line(),
                        line_color: *line_color,
                        text_color: *text_color,
                    };
                    push_grammar_runs(
                        &mut segments,
                        &mut line_chars,
                        content,
                        src,
                        base_bold,
                        base_italic,
                        style,
                    );
                    src += text.chars().count();
                    continue;
                }
                let (text, bold, italic, is_code, tag_opt) = match &visible.span {
                    // A marker is painted by the row's own font (or the code font for a
                    // delimiter), so it has to carry the row's base traits — italic
                    // advances differ from upright ones, and a revealed marker would
                    // otherwise shift the glyphs after it off their measured columns.
                    SyntaxSpan::Marker(t) => (t.as_str(), base_bold, base_italic, is_inline_code_delim(t), None),
                    SyntaxSpan::Text(t) => (t.as_str(), base_bold, base_italic, false, None),
                    SyntaxSpan::Bold(t) => (t.as_str(), true, base_italic, false, None),
                    SyntaxSpan::Italic(t) => (t.as_str(), base_bold, true, false, None),
                    SyntaxSpan::BoldItalic(t) => (t.as_str(), true, true, false, None),
                    SyntaxSpan::InlineCode(t) => (t.as_str(), base_bold, base_italic, true, None),
                    SyntaxSpan::Strikethrough(t)
                    | SyntaxSpan::Underline(t)
                    | SyntaxSpan::Highlight(t) => (t.as_str(), base_bold, base_italic, false, None),
                    SyntaxSpan::Tag(t) => (t.as_str(), base_bold, base_italic, false, Some(t.clone())),
                    SyntaxSpan::Link { text, .. } => (text.as_str(), base_bold, base_italic, false, None),
                    SyntaxSpan::Grammar { .. } => unreachable!("grammar spans are handled above"),
                };
                let n = text.chars().count();
                if n == 0 {
                    continue;
                }
                // An inline-code delimiter keeps its cell even when hidden, so the code
                // chip has a constant width and the selection rectangle does not jump
                // when the delimiters are revealed. The renderer paints that cell with
                // no ink at all, which is what reads as the chip's left/right padding.
                // Every other hidden marker collapses to nothing and leaves a gap in the
                // line chars.
                if !visible.marker_visible && !is_inline_code_delim(text) {
                    src += n;
                    continue;
                }
                push_verbatim_tagged(&mut segments, &mut line_chars, text, src, bold, italic, is_code, tag_opt);
                src += n;
            }
        }
    }

    if segments.is_empty() {
        // Nothing paintable, e.g. a row made of inline markers only.
        line_chars.clear();
        push_padding(&mut segments, &mut line_chars, " ", v.char_end, false, false);
        line_chars.push(v.char_end);
        return PaintedRow { segments, line_chars };
    }

    let terminal = if v.is_last_sub {
        v.char_end
    } else {
        line_chars.last().map(|&c| c + 1).unwrap_or(v.char_end)
    };
    line_chars.push(terminal);
    PaintedRow { segments, line_chars }
}

/// Cleans raw markdown link URL (removes title, quotes, angle brackets, adds scheme if needed for web links).
pub fn clean_link_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let url_part = if trimmed.starts_with('<') {
        if let Some(end_angle) = trimmed.find('>') {
            &trimmed[1..end_angle]
        } else {
            trimmed.trim_start_matches('<')
        }
    } else if let Some(idx) = trimmed.find(|c: char| c.is_whitespace()) {
        &trimmed[..idx]
    } else {
        trimmed
    };
    let url_part = url_part.trim_matches('"').trim_matches('\'');
    if url_part.is_empty() {
        return String::new();
    }
    let is_local = url_part.starts_with('.')
        || url_part.starts_with('/')
        || url_part.starts_with('\\')
        || url_part.contains('\\')
        || url_part.ends_with(".md")
        || url_part.ends_with(".markdown")
        || (url_part.len() >= 2 && url_part.as_bytes()[1] == b':');

    if !is_local && !url_part.contains("://") && !url_part.starts_with("mailto:") && !url_part.starts_with('#') {
        if url_part.contains('.') {
            return format!("https://{}", url_part);
        }
    }
    url_part.to_string()
}

/// Decodes percent-encoded characters (e.g. `%20` -> ` `, `%D0%9C` -> `U+041C`).
pub fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                if let (Some(v1), Some(v2)) = (hex_val(h1), hex_val(h2)) {
                    bytes.push((v1 << 4) | v2);
                    continue;
                }
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Strips inline markdown markers (bold, italic, strikethrough, inline code, links, highlights, underline),
/// returning the plain visible text.
pub fn strip_inline_markdown(mut s: &str) -> String {
    // 1. Handle image links ![alt](url) -> alt
    let mut intermediate = String::with_capacity(s.len());
    while !s.is_empty() {
        if s.starts_with("![") {
            if let Some(close_bracket) = s[2..].find(']') {
                let after_bracket = &s[2 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        let alt = &s[2..2 + close_bracket];
                        intermediate.push_str(alt);
                        let total_consumed = 2 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            intermediate.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    // 2. Parse inline syntax spans
    let spans = parse_inline_syntax_spans(&intermediate);
    let mut out = String::with_capacity(intermediate.len());
    for span in spans {
        match span {
            SyntaxSpan::Marker(_) => {}
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => {
                out.push_str(&t);
            }
            SyntaxSpan::Link { text, .. } => {
                out.push_str(&text);
            }
            SyntaxSpan::Grammar { text, content, .. } => {
                if content.is_empty() {
                    out.push_str(&grammar_paint_text(&text));
                } else {
                    for inner in &content {
                        if !matches!(inner.span, SyntaxSpan::Marker(_)) {
                            out.push_str(&grammar_paint_text(span_text(&inner.span)));
                        }
                    }
                }
            }
        }
    }

    // 3. Clean up any unclosed boundary markers from partial selection (e.g. "**word", "word**", "[word", "word](url)")
    let mut cleaned = out;
    if cleaned.contains("[[") && !cleaned.contains("]]") {
        cleaned = cleaned.replace("[[", "");
    }
    if cleaned.contains('[') && !cleaned.contains(']') {
        cleaned = cleaned.replace('[', "");
    }
    if let Some(pos) = cleaned.find("](") {
        if !cleaned[..pos].contains('[') {
            if let Some(end_paren) = cleaned[pos..].find(')') {
                cleaned.replace_range(pos..pos + end_paren + 1, "");
            } else {
                cleaned.truncate(pos);
            }
        }
    }
    let trimmed = cleaned.trim_start_matches(|c| c == '*' || c == '_' || c == '~' || c == '=' || c == '`' || c == '[');
    let trimmed = trimmed.trim_end_matches(|c| c == '*' || c == '_' || c == '~' || c == '=' || c == '`' || c == ']');
    trimmed.to_string()
}

/// Strips markdown tags and formatting, returning plain rendered text as seen in preview mode.
pub fn strip_markdown_tags(input: &str) -> String {
    let mut result_lines = Vec::new();
    let mut in_code_block = false;

    for line in input.lines() {
        let trimmed = line.trim();
        // Check for code fence
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue; // Code fences are hidden in preview mode
        }

        if in_code_block {
            result_lines.push(line.to_string());
            continue;
        }

        // Table separator line like |---|---|
        if crate::table::is_separator(trimmed) {
            continue;
        }

        // Table row like | a | b |
        if (trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1) || crate::table::is_separator(trimmed) {
            let cells = crate::table::split_row(trimmed);
            let stripped_cells: Vec<String> = cells
                .into_iter()
                .map(|c| {
                    let cell_content = &trimmed[c.content.clone()];
                    strip_inline_markdown(cell_content)
                })
                .collect();
            result_lines.push(stripped_cells.join("\t"));
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            continue;
        }

        // Headings: # Heading
        let mut work_line = line;
        let trimmed_start = line.trim_start();
        let leading_spaces = &line[..line.len() - trimmed_start.len()];

        if let Some(rest) = trimmed_start.strip_prefix("###### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("##### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("#### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("## ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("# ") {
            work_line = rest;
        } else if let Some(c) = callout_prefix(trimmed_start) {
            // A callout header previews as its title, never as its marker.
            work_line = c.title;
        } else if let Some(q) = quote_prefix(trimmed_start) {
            work_line = &trimmed_start[q.bytes..];
        }

        let stripped = strip_inline_markdown(work_line);
        if leading_spaces.is_empty() || work_line != line {
            result_lines.push(stripped);
        } else {
            result_lines.push(format!("{}{}", leading_spaces, stripped));
        }
    }

    let mut res = result_lines.join("\n");
    if input.ends_with('\n') {
        res.push('\n');
    }
    res
}
