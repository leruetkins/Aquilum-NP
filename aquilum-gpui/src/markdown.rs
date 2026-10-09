#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeBlockKind {
    OpeningFence,
    InsideCode,
    ClosingFence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlockState {
    pub kind: CodeBlockKind,
    pub lang: Option<String>,
    pub block_id: usize,
}

pub fn analyze_code_blocks(lines: &[&str]) -> Vec<Option<CodeBlockState>> {
    let mut in_code_block = false;
    let mut current_block_id = 0;
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                result.push(Some(CodeBlockState {
                    kind: CodeBlockKind::ClosingFence,
                    lang: None,
                    block_id: current_block_id,
                }));
                in_code_block = false;
                current_block_id += 1;
            } else {
                let lang = trimmed.trim_start_matches('`').trim();
                let lang_opt = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                result.push(Some(CodeBlockState {
                    kind: CodeBlockKind::OpeningFence,
                    lang: lang_opt,
                    block_id: current_block_id,
                }));
                in_code_block = true;
            }
        } else if in_code_block {
            result.push(Some(CodeBlockState {
                kind: CodeBlockKind::InsideCode,
                lang: None,
                block_id: current_block_id,
            }));
        } else {
            result.push(None);
        }
    }

    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind<'a> {
    Heading1(&'a str),
    Heading2(&'a str),
    Heading3(&'a str),
    Heading4(&'a str),
    Quote(&'a str),
    Task { checked: bool, text: &'a str },
    BulletList { indent: usize, text: &'a str },
    NumberedList { num: &'a str, text: &'a str },
    HorizontalRule,
    CodeFence { is_opening: bool, lang: Option<&'a str> },
    CodeLine(&'a str),
    Paragraph(&'a str),
}

pub fn classify_line<'a>(
    line: &'a str,
    code_state: Option<(CodeBlockKind, Option<&'a str>)>,
) -> LineKind<'a> {
    if let Some((kind, lang)) = code_state {
        return match kind {
            CodeBlockKind::OpeningFence => LineKind::CodeFence {
                is_opening: true,
                lang,
            },
            CodeBlockKind::InsideCode => LineKind::CodeLine(line),
            CodeBlockKind::ClosingFence => LineKind::CodeFence {
                is_opening: false,
                lang: None,
            },
        };
    }

    let trimmed_start = line.trim_start();
    let indent = line.len() - trimmed_start.len();

    if let Some(rest) = trimmed_start.strip_prefix("# ") {
        LineKind::Heading1(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("## ") {
        LineKind::Heading2(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("### ") {
        LineKind::Heading3(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("#### ") {
        LineKind::Heading4(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("> ") {
        LineKind::Quote(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("- [ ] ") {
        LineKind::Task { checked: false, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [ ] ") {
        LineKind::Task { checked: false, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- [x] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- [X] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [x] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [X] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- ") {
        LineKind::BulletList { indent, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* ") {
        LineKind::BulletList { indent, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("+ ") {
        LineKind::BulletList { indent, text: rest }
    } else if trimmed_start == "---" || trimmed_start == "***" || trimmed_start == "___" {
        LineKind::HorizontalRule
    } else {
        // Проверка нумерованного списка "1. ", "2. "
        let digits_len = trimmed_start.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len > 0 && trimmed_start[digits_len..].starts_with(". ") {
            let num = &trimmed_start[..digits_len];
            let text = &trimmed_start[digits_len + 2..];
            LineKind::NumberedList { num, text }
        } else {
            LineKind::Paragraph(line)
        }
    }
}

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

pub fn parse_inline_spans(mut s: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Inline code: `...`
        if s.starts_with('`') {
            if let Some(end_idx) = s[1..].find('`') {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let code_content = &s[1..1 + end_idx];
                spans.push(InlineSpan::InlineCode(code_content.to_string()));
                s = &s[2 + end_idx..];
                continue;
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

        // Italic: *...* (исключая списки)
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

        // Обычный символ
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxSpan {
    Marker(String),
    Text(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    InlineCode(String),
    Strikethrough(String),
}

pub fn parse_inline_syntax_spans(mut s: &str) -> Vec<SyntaxSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Inline code: `...`
        if s.starts_with('`') {
            if let Some(end_idx) = s[1..].find('`') {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("`".to_string()));
                let code_content = &s[1..1 + end_idx];
                spans.push(SyntaxSpan::InlineCode(code_content.to_string()));
                spans.push(SyntaxSpan::Marker("`".to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Bold italic: ***...***
        if s.starts_with("***") {
            if let Some(end_idx) = s[3..].find("***") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("***".to_string()));
                let content = &s[3..3 + end_idx];
                spans.push(SyntaxSpan::BoldItalic(content.to_string()));
                spans.push(SyntaxSpan::Marker("***".to_string()));
                s = &s[6 + end_idx..];
                continue;
            }
        }

        // Bold: **...**
        if s.starts_with("**") {
            if let Some(end_idx) = s[2..].find("**") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("**".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Bold(content.to_string()));
                spans.push(SyntaxSpan::Marker("**".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Italic: *...*
        if s.starts_with('*') {
            if let Some(end_idx) = s[1..].find('*') {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("*".to_string()));
                let content = &s[1..1 + end_idx];
                spans.push(SyntaxSpan::Italic(content.to_string()));
                spans.push(SyntaxSpan::Marker("*".to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if s.starts_with("~~") {
            if let Some(end_idx) = s[2..].find("~~") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Strikethrough(content.to_string()));
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Regular character
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            plain_accum.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    if !plain_accum.is_empty() {
        spans.push(SyntaxSpan::Text(plain_accum));
    }

    spans
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    pub line_idx: usize,
    pub sub_idx: usize,
    pub is_last_sub: bool,
    pub char_start: usize,
    pub char_end: usize,
}

pub fn wrap_line_ranges(line: &str, max_cols: usize) -> Vec<(usize, usize)> {
    let char_count = line.chars().count();
    if char_count == 0 {
        return vec![(0, 0)];
    }
    if max_cols == 0 || char_count <= max_cols {
        return vec![(0, char_count)];
    }

    let chars: Vec<char> = line.chars().collect();
    let mut ranges = Vec::new();
    let mut start = 0;

    while start < char_count {
        let remaining = char_count - start;
        if remaining <= max_cols {
            ranges.push((start, char_count));
            break;
        }

        let limit = start + max_cols;
        // Check if there is whitespace right at limit
        let next_start = if limit < char_count && chars[limit].is_whitespace() {
            limit + 1
        } else {
            // Look backwards for whitespace between start + 1 and limit
            let mut found = None;
            for i in (start + 1..limit).rev() {
                if chars[i].is_whitespace() {
                    found = Some(i + 1);
                    break;
                }
            }
            found.unwrap_or(limit)
        };

        let next_start = next_start.max(start + 1).min(char_count);
        ranges.push((start, next_start));
        start = next_start;
    }

    ranges
}

pub fn build_visual_rows(lines: &[&str], word_wrap: bool, wrap_cols: usize) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let ranges = if word_wrap && wrap_cols > 0 {
            wrap_line_ranges(line, wrap_cols)
        } else {
            vec![(0, line.chars().count())]
        };
        let count = ranges.len();
        for (sub_idx, (char_start, char_end)) in ranges.into_iter().enumerate() {
            visual_rows.push(VisualRow {
                line_idx,
                sub_idx,
                is_last_sub: sub_idx + 1 == count,
                char_start,
                char_end,
            });
        }
    }
    visual_rows
}

pub fn find_visual_row_for_cursor(
    visual_rows: &[VisualRow],
    cursor_line: usize,
    cursor_col: usize,
) -> Option<(usize, usize)> {
    for (v_idx, v) in visual_rows.iter().enumerate() {
        if v.line_idx == cursor_line {
            if cursor_col >= v.char_start && (cursor_col < v.char_end || v.is_last_sub) {
                let col_in_row = cursor_col.saturating_sub(v.char_start);
                return Some((v_idx, col_in_row));
            }
        }
    }
    visual_rows.first().map(|_| (0, 0))
}

pub fn slice_syntax_spans(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
) -> Vec<SyntaxSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut curr_char = 0;

    for span in spans {
        let (kind, text) = match span {
            SyntaxSpan::Marker(t) => (0, t),
            SyntaxSpan::Text(t) => (1, t),
            SyntaxSpan::Bold(t) => (2, t),
            SyntaxSpan::Italic(t) => (3, t),
            SyntaxSpan::BoldItalic(t) => (4, t),
            SyntaxSpan::InlineCode(t) => (5, t),
            SyntaxSpan::Strikethrough(t) => (6, t),
        };

        let span_len = text.chars().count();
        let span_end = curr_char + span_len;

        if span_end > start_char && curr_char < end_char {
            let s = start_char.saturating_sub(curr_char);
            let e = (end_char - curr_char).min(span_len);
            if s < e {
                let sliced_str: String = text.chars().skip(s).take(e - s).collect();
                let sliced_span = match kind {
                    0 => SyntaxSpan::Marker(sliced_str),
                    1 => SyntaxSpan::Text(sliced_str),
                    2 => SyntaxSpan::Bold(sliced_str),
                    3 => SyntaxSpan::Italic(sliced_str),
                    4 => SyntaxSpan::BoldItalic(sliced_str),
                    5 => SyntaxSpan::InlineCode(sliced_str),
                    6 => SyntaxSpan::Strikethrough(sliced_str),
                    _ => SyntaxSpan::Text(sliced_str),
                };
                result.push(sliced_span);
            }
        }

        curr_char = span_end;
        if curr_char >= end_char {
            break;
        }
    }

    result
}

pub fn visual_col_to_row_char(
    row_spans: &[SyntaxSpan],
    vis_col: usize,
    is_active_line: bool,
    hidden_heading_prefix: usize,
) -> usize {
    if is_active_line {
        return vis_col;
    }
    let mut remaining_vis = vis_col;
    let mut raw_offset = hidden_heading_prefix;

    for span in row_spans {
        let (is_marker, text) = match span {
            SyntaxSpan::Marker(t) => (true, t),
            SyntaxSpan::Text(t) => (false, t),
            SyntaxSpan::Bold(t) => (false, t),
            SyntaxSpan::Italic(t) => (false, t),
            SyntaxSpan::BoldItalic(t) => (false, t),
            SyntaxSpan::InlineCode(t) => (false, t),
            SyntaxSpan::Strikethrough(t) => (false, t),
        };
        let span_len = text.chars().count();
        if is_marker {
            raw_offset += span_len;
        } else {
            if remaining_vis <= span_len {
                return raw_offset + remaining_vis;
            }
            remaining_vis -= span_len;
            raw_offset += span_len;
        }
    }
    raw_offset + remaining_vis
}

pub fn row_char_to_visual_col(
    row_spans: &[SyntaxSpan],
    raw_char: usize,
    is_active_line: bool,
    hidden_heading_prefix: usize,
) -> usize {
    if is_active_line {
        return raw_char;
    }
    let mut target_raw = raw_char.saturating_sub(hidden_heading_prefix);
    let mut vis_col = 0;

    for span in row_spans {
        let (is_marker, text) = match span {
            SyntaxSpan::Marker(t) => (true, t),
            SyntaxSpan::Text(t) => (false, t),
            SyntaxSpan::Bold(t) => (false, t),
            SyntaxSpan::Italic(t) => (false, t),
            SyntaxSpan::BoldItalic(t) => (false, t),
            SyntaxSpan::InlineCode(t) => (false, t),
            SyntaxSpan::Strikethrough(t) => (false, t),
        };
        let span_len = text.chars().count();
        if is_marker {
            target_raw = target_raw.saturating_sub(span_len);
        } else {
            if target_raw <= span_len {
                return vis_col + target_raw;
            }
            target_raw -= span_len;
            vis_col += span_len;
        }
    }
    vis_col + target_raw
}

pub fn row_spans_and_hidden_prefix(
    line: &str,
    classified: &LineKind<'_>,
    v: &VisualRow,
    is_active_line: bool,
) -> (Vec<SyntaxSpan>, usize) {
    let (text_to_parse, prefix_offset, hidden_heading) = match classified {
        LineKind::Heading1(_) => (line.strip_prefix("# ").unwrap_or(line), 2, if !is_active_line && v.sub_idx == 0 { 2 } else { 0 }),
        LineKind::Heading2(_) => (line.strip_prefix("## ").unwrap_or(line), 3, if !is_active_line && v.sub_idx == 0 { 3 } else { 0 }),
        LineKind::Heading3(_) => (line.strip_prefix("### ").unwrap_or(line), 4, if !is_active_line && v.sub_idx == 0 { 4 } else { 0 }),
        LineKind::Heading4(_) => (line.strip_prefix("#### ").unwrap_or(line), 5, if !is_active_line && v.sub_idx == 0 { 5 } else { 0 }),
        LineKind::Quote(_) => (line.strip_prefix("> ").unwrap_or(line), 2, 0),
        LineKind::Task { .. } => {
            let p_len = if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            };
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let bullet_len = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { 2 } else { 0 };
            let p_len = indent_len + bullet_len;
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let num_p_len = num.len() + 2;
            let p_len = indent_len + if trimmed.len() >= num_p_len { num_p_len } else { 0 };
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        _ => (line, 0, 0),
    };
    let spans = parse_inline_syntax_spans(text_to_parse);
    let s = v.char_start.saturating_sub(prefix_offset);
    let e = v.char_end.saturating_sub(prefix_offset);
    let sliced = slice_syntax_spans(&spans, s, e);
    (sliced, hidden_heading)
}
