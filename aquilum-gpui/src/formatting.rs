//! Markdown formatting operations and transaction calculations.
//!
//! Provides pure, testable operations for toggling inline markers (bold, italic, etc.),
//! headings, lists, indentations, tables, callouts, and code blocks using char offsets
//! and single-step undoable document replacements.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignKind {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormattingOp {
    ToggleInline {
        open: &'static str,
        close: &'static str,
    },
    ToggleHeading(usize),
    ClearFormatting,
    ToggleTask,
    ListBullet,
    ListNumbered,
    ListTask,
    Indent,
    Outdent,
    Align(AlignKind),
    InsertTable,
    CodeBlock,
    InsertCallout(String),
    AttachFile(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattingResult {
    /// Byte range in the document to replace.
    pub byte_range: Range<usize>,
    /// New replacement text for the byte range.
    pub new_text: String,
    /// New cursor byte offset after applying the edit.
    pub new_cursor: usize,
    /// New selection anchor byte offset (if selection should be preserved/updated).
    pub new_anchor: Option<usize>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ActiveFormatting {
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_strikethrough: bool,
    pub is_underline: bool,
    pub is_highlight: bool,
    pub is_inline_code: bool,
    pub is_code_block: bool,
    pub heading_level: Option<u8>,
    pub is_bullet_list: bool,
    pub is_numbered_list: bool,
    pub is_task_list: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Applies an inline toggle (e.g. `**` bold or `*` italic).
///
/// Behavior:
/// 1. If selection is empty:
///    - If cursor is immediately between `open` and `close` (e.g. `**|**`), strip them.
///    - Otherwise, insert `open` and `close` and put cursor between them.
/// 2. If selection is non-empty:
///    - If selected text begins with `open` and ends with `close`, strip them.
///    - If text immediately outside the selection has `open` before and `close` after, strip outer markers.
///    - Otherwise, wrap selected text in `open` and `close`.
pub fn apply_toggle_inline(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
    open: &str,
    close: &str,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let total_chars = chars.len();
    let sel_start = char_sel_start.min(total_chars);
    let sel_end = char_sel_end.min(total_chars);
    let (sel_min, sel_max) = if sel_start <= sel_end {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    };

    let open_chars: Vec<char> = open.chars().collect();
    let close_chars: Vec<char> = close.chars().collect();
    let open_len = open_chars.len();
    let close_len = close_chars.len();

    if sel_min == sel_max {
        let cur = sel_min;
        // Check if cursor is right between open and close: e.g. "**|**"
        let has_open_before = cur >= open_len && chars[cur - open_len..cur] == open_chars[..];
        let has_close_after = cur + close_len <= total_chars && chars[cur..cur + close_len] == close_chars[..];

        if has_open_before && has_close_after {
            // Remove the markers around cursor
            let replace_char_start = cur - open_len;
            let replace_char_end = cur + close_len;
            let byte_start = char_to_byte_offset(full_text, replace_char_start);
            let byte_end = char_to_byte_offset(full_text, replace_char_end);
            return FormattingResult {
                byte_range: byte_start..byte_end,
                new_text: String::new(),
                new_cursor: byte_start,
                new_anchor: None,
            };
        }

        // Insert open + close, cursor between them
        let byte_cur = char_to_byte_offset(full_text, cur);
        let new_text = format!("{}{}", open, close);
        let new_cursor = byte_cur + open.len();
        return FormattingResult {
            byte_range: byte_cur..byte_cur,
            new_text,
            new_cursor,
            new_anchor: None,
        };
    }

    // Selection is non-empty
    let selected_chars = &chars[sel_min..sel_max];

    // Case A: Inner wrap - the selection itself starts with open and ends with close
    let inner_has_open = selected_chars.len() >= open_len + close_len
        && selected_chars[..open_len] == open_chars[..];
    let inner_has_close = selected_chars.len() >= open_len + close_len
        && selected_chars[selected_chars.len() - close_len..] == close_chars[..];

    if inner_has_open && inner_has_close {
        let unwrapped_chars = &selected_chars[open_len..selected_chars.len() - close_len];
        let unwrapped_text: String = unwrapped_chars.iter().collect();
        let byte_start = char_to_byte_offset(full_text, sel_min);
        let byte_end = char_to_byte_offset(full_text, sel_max);
        let new_anchor = byte_start;
        let new_cursor = byte_start + unwrapped_text.len();
        return FormattingResult {
            byte_range: byte_start..byte_end,
            new_text: unwrapped_text,
            new_cursor,
            new_anchor: Some(new_anchor),
        };
    }

    // Case B: Outer wrap - markers sit right outside the selection: e.g. "**[selection]**"
    let outer_has_open = sel_min >= open_len && chars[sel_min - open_len..sel_min] == open_chars[..];
    let outer_has_close = sel_max + close_len <= total_chars
        && chars[sel_max..sel_max + close_len] == close_chars[..];

    if outer_has_open && outer_has_close {
        let inner_text: String = selected_chars.iter().collect();
        let byte_start = char_to_byte_offset(full_text, sel_min - open_len);
        let byte_end = char_to_byte_offset(full_text, sel_max + close_len);
        let new_anchor = byte_start;
        let new_cursor = byte_start + inner_text.len();
        return FormattingResult {
            byte_range: byte_start..byte_end,
            new_text: inner_text,
            new_cursor,
            new_anchor: Some(new_anchor),
        };
    }

    // Case C: Neither - wrap the selection with open and close
    let selected_text: String = selected_chars.iter().collect();
    let new_text = format!("{}{}{}", open, selected_text, close);
    let byte_start = char_to_byte_offset(full_text, sel_min);
    let byte_end = char_to_byte_offset(full_text, sel_max);
    let new_anchor = byte_start + open.len();
    let new_cursor = new_anchor + selected_text.len();
    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text,
        new_cursor,
        new_anchor: Some(new_anchor),
    }
}

/// Toggles heading level (1..=6) on the line(s) containing the selection.
pub fn apply_toggle_heading(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
    level: usize,
) -> FormattingResult {
    let target_level = level.clamp(1, 6);
    let target_prefix = format!("{} ", "#".repeat(target_level));

    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let mut new_lines = Vec::new();

    for line in lines {
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];
        let current_level = detect_heading_level(trimmed_start);

        if current_level == Some(target_level) {
            // Remove heading
            let after_prefix = trimmed_start.strip_prefix(&format!("{} ", "#".repeat(target_level))).unwrap_or(trimmed_start);
            new_lines.push(format!("{}{}", indent, after_prefix));
        } else if let Some(cur_lvl) = current_level {
            // Change heading level
            let after_prefix = trimmed_start.strip_prefix(&format!("{} ", "#".repeat(cur_lvl))).unwrap_or(trimmed_start);
            new_lines.push(format!("{}{}{}", indent, target_prefix, after_prefix));
        } else {
            // Add heading
            new_lines.push(format!("{}{}{}", indent, target_prefix, trimmed_start));
        }
    }

    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Strips Markdown formatting (bold, italic, strikethrough, underline, highlight, inline code,
/// headers, task marks, links) from the selected text or current line.
pub fn apply_clear_formatting(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let (sel_min, sel_max) = if char_sel_start <= char_sel_end {
        (char_sel_start.min(chars.len()), char_sel_end.min(chars.len()))
    } else {
        (char_sel_end.min(chars.len()), char_sel_start.min(chars.len()))
    };

    let (replace_char_start, replace_char_end, target_text) = if sel_min == sel_max {
        // Clear on current line
        let (ls, le, lines) = get_enclosing_lines_chars(full_text, sel_min, sel_max);
        (ls, le, lines.join("\n"))
    } else {
        (sel_min, sel_max, chars[sel_min..sel_max].iter().collect::<String>())
    };

    let cleaned = strip_markdown_formatting(&target_text);
    let byte_start = char_to_byte_offset(full_text, replace_char_start);
    let byte_end = char_to_byte_offset(full_text, replace_char_end);
    let new_cursor = byte_start + cleaned.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: cleaned,
        new_cursor,
        new_anchor: None,
    }
}

/// Toggles task checkbox mark (`- [ ] ` <-> `- [x] `) on current line(s).
pub fn apply_toggle_task(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let mut new_lines = Vec::new();

    for line in lines {
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];

        if trimmed_start.starts_with("- [ ] ") {
            let rest = &trimmed_start["- [ ] ".len()..];
            new_lines.push(format!("{}- [x] {}", indent, rest));
        } else if trimmed_start.starts_with("- [x] ") || trimmed_start.starts_with("- [X] ") {
            let rest = &trimmed_start["- [x] ".len()..];
            new_lines.push(format!("{}- [ ] {}", indent, rest));
        } else if trimmed_start.starts_with("- ") {
            let rest = &trimmed_start["- ".len()..];
            new_lines.push(format!("{}- [ ] {}", indent, rest));
        } else if trimmed_start.starts_with("* ") {
            let rest = &trimmed_start["* ".len()..];
            new_lines.push(format!("{}- [ ] {}", indent, rest));
        } else {
            new_lines.push(format!("{}- [ ] {}", indent, trimmed_start));
        }
    }

    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Toggles bullet list (`- `) on lines.
pub fn apply_list_bullet(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let mut new_lines = Vec::new();

    for line in lines {
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];

        if trimmed_start.starts_with("- ") {
            // Remove bullet
            new_lines.push(format!("{}{}", indent, &trimmed_start[2..]));
        } else if trimmed_start.starts_with("* ") {
            new_lines.push(format!("{}{}", indent, &trimmed_start[2..]));
        } else if let Some(rest) = strip_numbered_prefix(trimmed_start) {
            new_lines.push(format!("{}- {}", indent, rest));
        } else {
            new_lines.push(format!("{}- {}", indent, trimmed_start));
        }
    }

    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Toggles numbered list (`1. `, `2. `) on lines.
pub fn apply_list_numbered(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let mut new_lines = Vec::new();

    let all_numbered = lines.iter().all(|l| strip_numbered_prefix(l.trim_start()).is_some());

    for (idx, line) in lines.iter().enumerate() {
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];

        if all_numbered {
            // Remove numbering
            if let Some(rest) = strip_numbered_prefix(trimmed_start) {
                new_lines.push(format!("{}{}", indent, rest));
            } else {
                new_lines.push(line.clone());
            }
        } else if let Some(rest) = strip_numbered_prefix(trimmed_start) {
            new_lines.push(format!("{}{}. {}", indent, idx + 1, rest));
        } else if trimmed_start.starts_with("- [ ] ") || trimmed_start.starts_with("- [x] ") {
            let rest = &trimmed_start[6..];
            new_lines.push(format!("{}{}. {}", indent, idx + 1, rest));
        } else if trimmed_start.starts_with("- ") || trimmed_start.starts_with("* ") {
            let rest = &trimmed_start[2..];
            new_lines.push(format!("{}{}. {}", indent, idx + 1, rest));
        } else {
            new_lines.push(format!("{}{}. {}", indent, idx + 1, trimmed_start));
        }
    }

    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Indents lines by adding 4 spaces at line start.
pub fn apply_indent(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let new_lines: Vec<String> = lines.into_iter().map(|l| format!("    {}", l)).collect();
    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Outdents lines by removing up to 4 leading spaces (or 1 leading tab).
pub fn apply_outdent(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let new_lines: Vec<String> = lines
        .into_iter()
        .map(|l| {
            if l.starts_with('\t') {
                l[1..].to_string()
            } else if l.starts_with("    ") {
                l[4..].to_string()
            } else if l.starts_with("   ") {
                l[3..].to_string()
            } else if l.starts_with("  ") {
                l[2..].to_string()
            } else if l.starts_with(' ') {
                l[1..].to_string()
            } else {
                l
            }
        })
        .collect();

    let replacement = new_lines.join("\n");
    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Inserts a fenced code block with optional inner text.
pub fn apply_code_block(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let (sel_min, sel_max) = if char_sel_start <= char_sel_end {
        (char_sel_start.min(chars.len()), char_sel_end.min(chars.len()))
    } else {
        (char_sel_end.min(chars.len()), char_sel_start.min(chars.len()))
    };

    let selected_text: String = chars[sel_min..sel_max].iter().collect();
    let byte_start = char_to_byte_offset(full_text, sel_min);
    let byte_end = char_to_byte_offset(full_text, sel_max);

    let (replacement, new_cursor) = if selected_text.is_empty() {
        ("```\n\n```\n".to_string(), byte_start + 4)
    } else {
        (format!("```\n{}\n```\n", selected_text), byte_start + 4 + selected_text.len())
    };

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Inserts a standard 3x3 markdown table.
pub fn apply_insert_table(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let sel_min = char_sel_start.min(char_sel_end).min(chars.len());
    let sel_max = char_sel_start.max(char_sel_end).min(chars.len());
    let byte_start = char_to_byte_offset(full_text, sel_min);
    let byte_end = char_to_byte_offset(full_text, sel_max);

    let template = "\n| Column 1 | Column 2 | Column 3 |\n| -------- | -------- | -------- |\n| Item 1   | Item 2   | Item 3   |\n| Item 4   | Item 5   | Item 6   |\n";
    let new_cursor = byte_start + template.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: template.to_string(),
        new_cursor,
        new_anchor: None,
    }
}

/// Inserts a callout box (e.g. `> [!note]\n> Content`).
pub fn apply_insert_callout(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
    callout_type: &str,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let sel_min = char_sel_start.min(char_sel_end).min(chars.len());
    let sel_max = char_sel_start.max(char_sel_end).min(chars.len());
    let byte_start = char_to_byte_offset(full_text, sel_min);
    let byte_end = char_to_byte_offset(full_text, sel_max);

    let selected_text: String = chars[sel_min..sel_max].iter().collect();
    let c_type = if callout_type.trim().is_empty() { "note" } else { callout_type.trim() };

    let (replacement, new_cursor) = if selected_text.is_empty() {
        (format!("> [!{}]\n> \n", c_type), byte_start + format!("> [!{}]\n> ", c_type).len())
    } else {
        let lines: Vec<String> = selected_text.lines().map(|l| format!("> {}", l)).collect();
        let content = if lines.is_empty() { "> ".to_string() } else { lines.join("\n") };
        let rep = format!("> [!{}]\n{}\n", c_type, content);
        let cur = byte_start + rep.len();
        (rep, cur)
    };

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Inserts file attachment / link (`![[filename]]` or `![alt](url)`).
pub fn apply_attach_file(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
    target: Option<&str>,
) -> FormattingResult {
    let chars: Vec<char> = full_text.chars().collect();
    let sel_min = char_sel_start.min(char_sel_end).min(chars.len());
    let sel_max = char_sel_start.max(char_sel_end).min(chars.len());
    let byte_start = char_to_byte_offset(full_text, sel_min);
    let byte_end = char_to_byte_offset(full_text, sel_max);

    let selected_text: String = chars[sel_min..sel_max].iter().collect();
    let (replacement, new_cursor) = match target {
        Some(path) => {
            let alt = if selected_text.is_empty() { "" } else { &selected_text };
            (format!("![{}]({})", alt, path), byte_start + format!("![{}]({})", alt, path).len())
        }
        None => {
            if selected_text.is_empty() {
                ("![[]]".to_string(), byte_start + 3) // Cursor inside ![[|]]
            } else {
                (format!("![[{}]]", selected_text), byte_start + format!("![[{}]]", selected_text).len())
            }
        }
    };

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Wraps lines in HTML align tag: `<p align="center">...</p>`.
pub fn apply_align(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
    align: AlignKind,
) -> FormattingResult {
    let (line_char_start, line_char_end, lines) = get_enclosing_lines_chars(full_text, char_sel_start, char_sel_end);
    let content = lines.join("\n");
    let align_str = match align {
        AlignKind::Left => "left",
        AlignKind::Center => "center",
        AlignKind::Right => "right",
    };

    let byte_start = char_to_byte_offset(full_text, line_char_start);
    let byte_end = char_to_byte_offset(full_text, line_char_end);

    let replacement = format!("<p align=\"{}\">\n{}\n</p>", align_str, content);
    let new_cursor = byte_start + replacement.len();

    FormattingResult {
        byte_range: byte_start..byte_end,
        new_text: replacement,
        new_cursor,
        new_anchor: None,
    }
}

/// Evaluates active formatting state based on cursor position and selection.
pub fn inspect_active_formatting(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> ActiveFormatting {
    let chars: Vec<char> = full_text.chars().collect();
    let total_chars = chars.len();
    let sel_min = char_sel_start.min(char_sel_end).min(total_chars);
    let sel_max = char_sel_start.max(char_sel_end).min(total_chars);

    let mut state = ActiveFormatting::default();

    // Line-level inspection
    let (line_start_char, _, lines) = get_enclosing_lines_chars(full_text, sel_min, sel_max);
    if let Some(first_line) = lines.first() {
        let trimmed = first_line.trim_start();
        state.heading_level = detect_heading_level(trimmed).map(|l| l as u8);
        state.is_bullet_list = trimmed.starts_with("- ") || trimmed.starts_with("* ");
        state.is_numbered_list = strip_numbered_prefix(trimmed).is_some();
        state.is_task_list = trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ");
    }

    // Inline inspection around current cursor / selection
    state.is_bold = is_enclosed_by(&chars, sel_min, sel_max, "**", "**");
    state.is_italic = is_enclosed_by(&chars, sel_min, sel_max, "*", "*");
    state.is_strikethrough = is_enclosed_by(&chars, sel_min, sel_max, "~~", "~~");
    state.is_underline = is_enclosed_by(&chars, sel_min, sel_max, "<u>", "</u>");
    state.is_highlight = is_enclosed_by(&chars, sel_min, sel_max, "==", "==");
    state.is_inline_code = is_enclosed_by(&chars, sel_min, sel_max, "`", "`");

    // Code block check
    let prefix_text: String = chars[..line_start_char].iter().collect();
    let code_fence_count = prefix_text.lines().filter(|l| l.trim_start().starts_with("```")).count();
    state.is_code_block = code_fence_count % 2 == 1;

    state
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.chars().take(char_offset).map(|c| c.len_utf8()).sum()
}

pub fn byte_to_char_offset(text: &str, byte_offset: usize) -> usize {
    let mut bound = byte_offset.min(text.len());
    while bound > 0 && !text.is_char_boundary(bound) {
        bound -= 1;
    }
    text[..bound].chars().count()
}

fn get_enclosing_lines_chars(
    full_text: &str,
    char_sel_start: usize,
    char_sel_end: usize,
) -> (usize, usize, Vec<String>) {
    let chars: Vec<char> = full_text.chars().collect();
    let total = chars.len();
    let sel_min = char_sel_start.min(char_sel_end).min(total);
    let sel_max = char_sel_start.max(char_sel_end).min(total);

    // Find start of first line
    let mut line_start = sel_min;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }

    // Find end of last line
    let mut line_end = sel_max;
    while line_end < total && chars[line_end] != '\n' {
        line_end += 1;
    }

    let slice_chars = &chars[line_start..line_end];
    let slice_str: String = slice_chars.iter().collect();
    let lines: Vec<String> = slice_str.split('\n').map(|s| s.to_string()).collect();

    (line_start, line_end, lines)
}

pub(crate) fn detect_heading_level(trimmed_line: &str) -> Option<usize> {
    for lvl in (1..=6).rev() {
        let prefix = format!("{} ", "#".repeat(lvl));
        if trimmed_line.starts_with(&prefix) {
            return Some(lvl);
        }
    }
    None
}

pub(crate) fn strip_numbered_prefix(trimmed_line: &str) -> Option<&str> {
    let mut digits_end = 0;
    for (i, c) in trimmed_line.char_indices() {
        if c.is_ascii_digit() {
            digits_end = i + 1;
        } else {
            break;
        }
    }
    if digits_end > 0 && trimmed_line[digits_end..].starts_with(". ") {
        Some(&trimmed_line[digits_end + 2..])
    } else {
        None
    }
}

pub(crate) fn is_enclosed_by(chars: &[char], sel_min: usize, sel_max: usize, open: &str, close: &str) -> bool {
    let open_chars: Vec<char> = open.chars().collect();
    let close_chars: Vec<char> = close.chars().collect();
    let open_len = open_chars.len();
    let close_len = close_chars.len();

    let check_standalone = |idx: usize, len: usize| -> bool {
        if open == "*" {
            if idx > 0 && chars[idx - 1] == '*' {
                return false;
            }
            if idx + len < chars.len() && chars[idx + len] == '*' {
                return false;
            }
        }
        true
    };

    // 1. Check inner: selection itself starts with open and ends with close
    if sel_max >= sel_min + open_len + close_len {
        let has_open = chars[sel_min..sel_min + open_len] == open_chars[..] && check_standalone(sel_min, open_len);
        let has_close = chars[sel_max - close_len..sel_max] == close_chars[..] && check_standalone(sel_max - close_len, close_len);
        if has_open && has_close {
            return true;
        }
    }

    // 2. Check outer: markers immediately surround selection
    if sel_min >= open_len && sel_max + close_len <= chars.len() {
        let has_open = chars[sel_min - open_len..sel_min] == open_chars[..] && check_standalone(sel_min - open_len, open_len);
        let has_close = chars[sel_max..sel_max + close_len] == close_chars[..] && check_standalone(sel_max, close_len);
        if has_open && has_close {
            return true;
        }
    }

    // 3. Scan enclosing line around cursor
    let mut line_start = sel_min;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }
    let mut line_end = sel_max;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }

    let line_chars = &chars[line_start..line_end];
    let rel_cursor = sel_min.saturating_sub(line_start);

    let mut opens = Vec::new();
    let mut closes = Vec::new();
    let n = line_chars.len();
    let mut i = 0;
    while i + open_len <= n {
        if line_chars[i..i + open_len] == open_chars[..] && check_standalone(line_start + i, open_len) {
            opens.push(i);
        }
        i += 1;
    }
    let mut j = 0;
    while j + close_len <= n {
        if line_chars[j..j + close_len] == close_chars[..] && check_standalone(line_start + j, close_len) {
            closes.push(j);
        }
        j += 1;
    }

    for &op_pos in &opens {
        for &cl_pos in &closes {
            if op_pos + open_len <= rel_cursor && rel_cursor <= cl_pos {
                return true;
            }
        }
    }

    false
}

fn strip_markdown_formatting(text: &str) -> String {
    let mut s = text.to_string();

    // Remove inline markers
    for marker in &["***", "**", "*", "~~", "==", "`", "<u>", "</u>"] {
        s = s.replace(marker, "");
    }

    // Clean links: [title](url) -> title, [[link|title]] -> title, [[link]] -> link
    let mut res = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
            // Wikilink [[...]]
            if let Some(end) = chars[i + 2..].windows(2).position(|w| w == &[']', ']']) {
                let inner: String = chars[i + 2..i + 2 + end].iter().collect();
                if let Some(pipe) = inner.find('|') {
                    res.push_str(&inner[pipe + 1..]);
                } else {
                    res.push_str(&inner);
                }
                i += 2 + end + 2;
                continue;
            }
        } else if chars[i] == '[' {
            // Markdown link [text](url)
            if let Some(close_bracket) = chars[i + 1..].iter().position(|&c| c == ']') {
                let title: String = chars[i + 1..i + 1 + close_bracket].iter().collect();
                let after = i + 1 + close_bracket + 1;
                if after < chars.len() && chars[after] == '(' {
                    if let Some(close_paren) = chars[after + 1..].iter().position(|&c| c == ')') {
                        res.push_str(&title);
                        i = after + 1 + close_paren + 1;
                        continue;
                    }
                }
            }
        }
        res.push(chars[i]);
        i += 1;
    }

    // Strip leading header or list prefixes from lines
    let cleaned_lines: Vec<String> = res
        .lines()
        .map(|l| {
            let trimmed = l.trim_start();
            let indent = &l[..l.len() - trimmed.len()];
            let no_heading = detect_heading_level(trimmed)
                .map(|lvl| trimmed.strip_prefix(&format!("{} ", "#".repeat(lvl))).unwrap_or(trimmed))
                .unwrap_or(trimmed);
            let no_task = if no_heading.starts_with("- [ ] ") || no_heading.starts_with("- [x] ") {
                &no_heading[6..]
            } else if no_heading.starts_with("- ") || no_heading.starts_with("* ") {
                &no_heading[2..]
            } else if let Some(rest) = strip_numbered_prefix(no_heading) {
                rest
            } else {
                no_heading
            };
            format!("{}{}", indent, no_task)
        })
        .collect();

    cleaned_lines.join("\n")
}

#[cfg(test)]
mod tests;
