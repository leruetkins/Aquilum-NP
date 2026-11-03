use super::*;
use super::classify::marker_prefix_len;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineSpan {
    Text(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    InlineCode(String),
    Strikethrough(String),
    Link { label: String, url: String },
}

/// Number of backticks at the head of `s`, or 0 when it does not start with one.
///
/// A backtick string is a run of one *or more* backticks, and CommonMark closes a code
/// span with a run of **exactly** the same length. That is what makes
/// `` ``code with `backticks` `` `` a single span whose content holds literal backticks,
/// instead of an empty span followed by a lone one.
pub(crate) fn backtick_run_len(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b'`').count()
}

/// Byte offset of the first run of exactly `n` backticks in `s`, or `None` when there is
/// none. Backticks are ASCII, so the offset is a char boundary and safe to slice on; a
/// longer run does not match (`` `` `` is not closed by ``` ``` ```).
pub(crate) fn find_closing_backtick_run(s: &str, n: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - start == n {
            return Some(start);
        }
    }
    None
}

/// ASCII punctuation — the characters a backslash may escape (CommonMark). Anything else
/// after a backslash stays literal, backslash included.
pub(crate) fn is_escapable_punctuation(c: char) -> bool {
    matches!(c, '!'..='/' | ':'..='@' | '['..='`' | '{'..='~')
}

/// Byte length of a bare URL at the head of `s`, or `None` when there is none.
///
/// Same acceptance as the original editor: the URL starts with a scheme or `www.`,
/// runs up to whitespace, and trailing sentence punctuation (plus an unbalanced
/// closing paren) stays outside the link.
pub(crate) fn scan_bare_url(s: &str) -> Option<usize> {
    const PREFIXES: [&str; 3] = ["https://", "http://", "www."];
    if !PREFIXES.iter().any(|prefix| s.starts_with(prefix)) {
        return None;
    }
    let mut end = s
        .find(|c: char| c.is_whitespace() || c == '<')
        .unwrap_or(s.len());
    while let Some(last) = s[..end].chars().next_back() {
        let unbalanced_close = last == ')'
            && s[..end].matches(')').count() > s[..end].matches('(').count();
        if ".,:;!?\"'*_~".contains(last) || unbalanced_close {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    let host = s[..end]
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    (host.contains('.') && !host.starts_with('.')).then_some(end)
}

/// Autolink in angle brackets: `<https://example.com>` or `<mail@example.com>`.
/// Returns the shown text and the target to open.
pub(crate) fn scan_angle_autolink(s: &str) -> Option<(&str, String)> {
    let rest = s.strip_prefix('<')?;
    let inner = &rest[..rest.find('>')?];
    if inner.is_empty() || inner.chars().any(|c| c.is_whitespace() || c == '<') {
        return None;
    }
    let has_scheme = inner.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if has_scheme {
        return Some((inner, inner.to_string()));
    }
    let is_email = !inner.chars().any(|c| c == '/' || c == ':')
        && inner
            .split_once('@')
            .is_some_and(|(user, host)| !user.is_empty() && host.contains('.') && !host.starts_with('.'));
    is_email.then(|| (inner, format!("mailto:{inner}")))
}

pub fn parse_inline_spans(mut s: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Backslash escape: the escaped char is literal, so it must not open a span.
        if s.starts_with('\\') {
            if let Some(c) = s[1..].chars().next() {
                if is_escapable_punctuation(c) {
                    plain_accum.push(c);
                    s = &s[1 + c.len_utf8()..];
                    continue;
                }
            }
        }

        // Inline code: a run of N backticks, closed by the next run of exactly N.
        let run = backtick_run_len(s);
        if run > 0 {
            match find_closing_backtick_run(&s[run..], run) {
                Some(content_len) => {
                    if !plain_accum.is_empty() {
                        spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let code_content = &s[run..run + content_len];
                    spans.push(InlineSpan::InlineCode(code_content.to_string()));
                    s = &s[run + content_len + run..];
                    continue;
                }
                // No run of the same length: the backticks are literal text, and the
                // whole run is consumed so it cannot be re-read as a shorter delimiter.
                None => {
                    plain_accum.push_str(&s[..run]);
                    s = &s[run..];
                    continue;
                }
            }
        }

        // Bold italic: ***...***
        if s.starts_with("***") {
            if let Some(end_idx) = s[3..].find("***") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[3..3 + end_idx];
                spans.push(InlineSpan::BoldItalic(content.to_string()));
                s = &s[6 + end_idx..];
                continue;
            }
        }

        // Bold: **...**
        if s.starts_with("**") {
            if let Some(end_idx) = s[2..].find("**") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[2..2 + end_idx];
                spans.push(InlineSpan::Bold(content.to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Italic: *...* (excluding lists)
        if s.starts_with('*') {
            if let Some(end_idx) = s[1..].find('*') {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[1..1 + end_idx];
                spans.push(InlineSpan::Italic(content.to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if s.starts_with("~~") {
            if let Some(end_idx) = s[2..].find("~~") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[2..2 + end_idx];
                spans.push(InlineSpan::Strikethrough(content.to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // WikiLink: [[target]] or [[target|alias]]
        if s.starts_with("[[") {
            if let Some(close_idx) = s[2..].find("]]") {
                let inner = &s[2..2 + close_idx];
                if !inner.is_empty() && !inner.contains('\n') {
                    if !plain_accum.is_empty() {
                        spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let (target, label) = if let Some((t, a)) = inner.split_once('|') {
                        (t.trim(), a.trim())
                    } else {
                        (inner.trim(), inner.trim())
                    };
                    spans.push(InlineSpan::Link {
                        label: label.to_string(),
                        url: target.to_string(),
                    });
                    let total_consumed = 2 + close_idx + 2;
                    s = &s[total_consumed..];
                    continue;
                }
            }
        }

        // Link: [label](url)
        if s.starts_with('[') {
            if let Some(close_bracket) = s[1..].find(']') {
                let after_bracket = &s[1 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        if !plain_accum.is_empty() {
                            spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                        }
                        let label = &s[1..1 + close_bracket];
                        let url = &after_bracket[1..1 + close_paren];
                        spans.push(InlineSpan::Link {
                            label: label.to_string(),
                            url: url.to_string(),
                        });
                        let total_consumed = 1 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }

        // Ordinary character
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            plain_accum.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    if !plain_accum.is_empty() {
        spans.push(InlineSpan::Text(plain_accum));
    }

    spans
}

/// Which syntax markers of a line are shown right now.
///
/// The line layout is computed from the raw source, so revealing markers never changes how
/// many visual rows a line occupies or how tall the document is. Hidden markers simply
/// collapse out of the painted text (the one exception is an inline-code delimiter, which
/// keeps its cell so the code chip does not jump). `Ranges` holds absolute char offsets of
/// the inline elements (markers included) revealed around the caret or the selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum InlineReveal {
    #[default]
    None,
    All,
    Ranges(Vec<(usize, usize)>),
}

impl InlineReveal {
    /// Whether a marker at `[start, end)` of the raw line is visible.
    pub fn shows(&self, start: usize, end: usize) -> bool {
        match self {
            InlineReveal::None => false,
            InlineReveal::All => true,
            InlineReveal::Ranges(ranges) => ranges.iter().any(|&(a, b)| start < b && a < end),
        }
    }
}

/// A syntax span together with the visibility of the marker it came from.
/// `marker_visible` is always true for non-marker spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleSpan {
    pub span: SyntaxSpan,
    pub marker_visible: bool,
}

pub(crate) fn span_text(span: &SyntaxSpan) -> &str {
    match span {
        SyntaxSpan::Marker(t)
        | SyntaxSpan::Text(t)
        | SyntaxSpan::Bold(t)
        | SyntaxSpan::Italic(t)
        | SyntaxSpan::BoldItalic(t)
        | SyntaxSpan::InlineCode(t)
        | SyntaxSpan::Strikethrough(t)
        | SyntaxSpan::Underline(t)
        | SyntaxSpan::Highlight(t)
        | SyntaxSpan::Tag(t) => t,
        SyntaxSpan::Link { text, .. } => text,
        // Raw content on purpose: the escape chars are part of the source offsets.
        SyntaxSpan::Grammar { text, .. } => text,
    }
}

pub(crate) fn span_char_len(span: &SyntaxSpan) -> usize {
    span_text(span).chars().count()
}

/// Character offset (within `line`) where the inline content starts, together with the
/// spans parsed from that content. The prefix markers (heading hashes, quote caret, list
/// bullets) are block syntax and stay outside the inline reveal model.
pub fn line_inline_spans(line: &str, classified: &LineKind<'_>) -> (Vec<SyntaxSpan>, usize) {
    let (rest, prefix_bytes) = match classified {
        LineKind::Heading1(_) => (line.strip_prefix("# ").unwrap_or(line), 2),
        LineKind::Heading2(_) => (line.strip_prefix("## ").unwrap_or(line), 3),
        LineKind::Heading3(_) => (line.strip_prefix("### ").unwrap_or(line), 4),
        LineKind::Heading4(_) => (line.strip_prefix("#### ").unwrap_or(line), 5),
        LineKind::Heading5(_) => (line.strip_prefix("##### ").unwrap_or(line), 6),
        LineKind::Heading6(_) => (line.strip_prefix("###### ").unwrap_or(line), 7),
        LineKind::Quote(_) => {
            let prefix_bytes = callout_prefix(line)
                .map(|c| c.bytes)
                .or_else(|| quote_prefix(line).map(|q| q.bytes))
                .unwrap_or(0);
            (
                &line[prefix_bytes.min(line.len())..],
                prefix_bytes,
            )
        }
        LineKind::Task { .. } | LineKind::BulletList { .. } | LineKind::NumberedList { .. } => {
            let p_len = marker_prefix_len(line, classified);
            let rest = if line.len() >= p_len {
                &line[p_len..]
            } else {
                ""
            };
            (rest, p_len)
        }
        _ => (line, 0),
    };
    (
        parse_inline_syntax_spans(rest),
        head_char_count(line, prefix_bytes),
    )
}

/// Ranges of the inline elements (opening marker + content + closing marker) as char
/// offsets inside the span sequence. Revealing an element makes exactly its own markers
/// visible, which is the Obsidian live-preview behaviour.
pub fn inline_element_ranges(spans: &[SyntaxSpan]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut starts = Vec::with_capacity(spans.len());
    let mut acc = 0usize;
    for span in spans {
        starts.push(acc);
        acc += span_char_len(span);
    }

    let mut i = 0usize;
    while i < spans.len() {
        if i + 2 < spans.len() {
            let paired = matches!(spans[i], SyntaxSpan::Marker(_))
                && matches!(spans[i + 2], SyntaxSpan::Marker(_))
                && is_revealable_content(&spans[i + 1]);
            if paired {
                let end = starts[i] + span_char_len(&spans[i])
                    + span_char_len(&spans[i + 1])
                    + span_char_len(&spans[i + 2]);
                ranges.push((starts[i], end));
                i += 3;
                continue;
            }
        }
        if i + 1 < spans.len() {
            if let (SyntaxSpan::Marker(m), SyntaxSpan::Text(_)) = (&spans[i], &spans[i + 1]) {
                if m == "\\" {
                    ranges.push((starts[i], starts[i] + span_char_len(&spans[i]) + span_char_len(&spans[i + 1])));
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    ranges
}

pub(crate) fn is_revealable_content(span: &SyntaxSpan) -> bool {
    matches!(
        span,
        SyntaxSpan::Link { .. }
            | SyntaxSpan::Bold(_)
            | SyntaxSpan::Italic(_)
            | SyntaxSpan::BoldItalic(_)
            | SyntaxSpan::InlineCode(_)
            | SyntaxSpan::Strikethrough(_)
            | SyntaxSpan::Underline(_)
            | SyntaxSpan::Highlight(_)
            | SyntaxSpan::Grammar { .. }
    )
}

/// Reveal decision for one source line.
///
/// `cursor_char` and `selection` are char offsets *within the line*. A selection reveals
/// every element it touches, and the caret reveals the element it sits in or ends right
/// after. `reveal_all` is the Alt+click escape hatch that shows the whole markup of the
/// line.
pub fn inline_reveal(
    line: &str,
    classified: &LineKind<'_>,
    cursor_char: Option<usize>,
    selection: Option<(usize, usize)>,
    reveal_all: bool,
) -> InlineReveal {
    if reveal_all {
        return InlineReveal::All;
    }
    // Nothing to reveal on this line: skip parsing it at all. This is the common case for
    // every visible row the caret and the selection do not touch.
    let has_selection = selection.map_or(false, |(s, e)| s < e);
    if cursor_char.is_none() && !has_selection {
        return InlineReveal::None;
    }
    // Code and rules have no inline markup worth revealing piecewise.
    if matches!(
        classified,
        LineKind::CodeLine(_) | LineKind::CodeFence { .. } | LineKind::HorizontalRule
    ) {
        return InlineReveal::None;
    }

    let (spans, base) = line_inline_spans(line, classified);
    let elements = inline_element_ranges(&spans);
    // A task checkbox is a marker as well: it gives way to the raw `[x]` while
    // the caret or the selection touches it, exactly like an inline marker.
    let task_range = match classified {
        LineKind::Task { .. } => task_prefix(line).map(|task| (0, task.chars)),
        _ => None,
    };
    if elements.is_empty() && task_range.is_none() {
        return InlineReveal::None;
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if let Some((s, e)) = selection {
        if s < e {
            if let Some(range) = task_range {
                if s < range.1 && range.0 < e {
                    ranges.push(range);
                }
            }
            for &(a, b) in &elements {
                let range = (base + a, base + b);
                if s < range.1 && range.0 < e {
                    ranges.push(range);
                }
            }
        }
    }
    if let Some(c) = cursor_char {
        if let Some(range) = task_range {
            // Unlike an inline marker, the end of a task prefix is where the
            // content starts: a caret sitting just after the checkbox must not
            // hide it, or the box could never be clicked from the line it edits.
            if range.0 <= c && c < range.1 && !ranges.contains(&range) {
                ranges.push(range);
            }
        }
        for &(a, b) in &elements {
            let range = (base + a, base + b);
            // The end boundary counts as inside. A hidden closing marker has no
            // cell, so a click right after the visible text (`word|` in
            // `[word](url)`) lands on the raw char that follows the marker and
            // would otherwise collapse the element the caret is sitting at the
            // end of.
            if range.0 <= c && c <= range.1 && !ranges.contains(&range) {
                ranges.push(range);
            }
        }
    }

    if ranges.is_empty() {
        InlineReveal::None
    } else {
        InlineReveal::Ranges(ranges)
    }
}

/// The part of a byte-range selection that falls inside one line, in line-char
/// offsets. `None` when the selection does not touch the line or is empty.
///
/// Shared by the render loop and the wrap geometry so a selected line is
/// wrapped for exactly the markers the renderer reveals on it.
pub fn selection_chars_in_line(
    line: &str,
    line_start: usize,
    selection: Option<&std::ops::Range<usize>>,
) -> Option<(usize, usize)> {
    let selection = selection?;
    let (start, end) = (selection.start, selection.end);
    if start >= end {
        return None;
    }
    let line_end = line_start + line.len();
    if end <= line_start || start >= line_end {
        return None;
    }
    let to_line_char = |byte: usize| {
        let byte = byte.clamp(line_start, line_end);
        let mut rel = byte - line_start;
        while rel > 0 && !line.is_char_boundary(rel) {
            rel -= 1;
        }
        line[..rel].chars().count()
    };
    Some((to_line_char(start), to_line_char(end)))
}
