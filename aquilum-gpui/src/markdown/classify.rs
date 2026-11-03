use super::code_scan::CodeBlockKind;
use super::{quote_prefix, RENDERED_BULLET};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind<'a> {
    Heading1(&'a str),
    Heading2(&'a str),
    Heading3(&'a str),
    Heading4(&'a str),
    Heading5(&'a str),
    Heading6(&'a str),
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
    } else if let Some(rest) = trimmed_start.strip_prefix("##### ") {
        LineKind::Heading5(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("###### ") {
        LineKind::Heading6(rest)
    } else if let Some(q) = quote_prefix(trimmed_start) {
        LineKind::Quote(&trimmed_start[q.bytes..])
    } else if let Some(task) = task_prefix(line) {
        LineKind::Task { checked: task.checked, text: &line[task.bytes..] }
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

/// The leading marker of a GFM task item: indent, list marker and checkbox,
/// e.g. `  - [x] ` or `12. [ ] `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPrefix<'a> {
    /// Leading whitespace before the list marker, in chars.
    pub indent_chars: usize,
    /// Byte length of the leading whitespace.
    pub indent_bytes: usize,
    /// The list marker with its trailing space, starting right after the
    /// indent: `- `, `* ` or `12. `.
    pub list_marker: &'a str,
    /// Whether the list marker is a number. An unordered marker is not painted
    /// on an idle task row: the checkbox takes its place.
    pub ordered: bool,
    /// The checkbox with its brackets: `[ ]`, `[x]`, `[✓]`.
    pub checkbox: &'a str,
    pub checked: bool,
    /// Byte range of the mark between the brackets, relative to the line.
    pub mark: std::ops::Range<usize>,
    /// Whether a space separates the checkbox from the content.
    pub space_after: bool,
    /// Byte length of the whole prefix, indent included.
    pub bytes: usize,
    /// Char length of the whole prefix, indent included.
    pub chars: usize,
}

impl TaskPrefix<'_> {
    /// Cell width of the list marker.
    pub fn list_chars(&self) -> usize {
        self.list_marker.chars().count()
    }
}

/// The task marker at the head of `line`, or `None` when the line is not a task.
///
/// GFM accepts any list marker before the checkbox (`-`, `*`, `+` or `1.`).
/// Any single mark counts — `[ ]` is open, anything else (`[x]`, `[X]`, `[✓]`)
/// is done — and the mark may be followed by a space, a tab or the end of the
/// line. The indent is part of the prefix, so a nested task keeps its columns
/// like a nested bullet does.
pub fn task_prefix(line: &str) -> Option<TaskPrefix<'_>> {
    let trimmed = line.trim_start();
    let indent_bytes = line.len() - trimmed.len();
    let indent_chars = line[..indent_bytes].chars().count();
    let (list_len, ordered) = if trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
    {
        (2, false)
    } else {
        let digits_len = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len == 0 || !trimmed[digits_len..].starts_with(". ") {
            return None;
        }
        (digits_len + 2, true)
    };
    let rest = &trimmed[list_len..];
    let inner = rest.strip_prefix('[')?;
    let mark_char = inner.chars().next()?;
    let closed = inner[mark_char.len_utf8()..].strip_prefix(']')?;
    if !closed.is_empty() && !closed.starts_with([' ', '\t']) {
        return None;
    }
    let mark_len = mark_char.len_utf8();
    let checkbox_len = 2 + mark_len;
    let space_after = !closed.is_empty();
    let list_marker = &trimmed[..list_len];
    let checkbox = &rest[..checkbox_len];
    let mark_start = indent_bytes + list_len + 1;
    Some(TaskPrefix {
        indent_chars,
        indent_bytes,
        list_marker,
        ordered,
        checkbox,
        checked: !mark_char.is_whitespace(),
        mark: mark_start..mark_start + mark_len,
        space_after,
        bytes: indent_bytes + list_len + checkbox_len + usize::from(space_after),
        chars: indent_chars
            + list_marker.chars().count()
            + checkbox.chars().count()
            + usize::from(space_after),
    })
}

/// Byte range of the mark and the text that flips it: `x` when the task is
/// open, a space when it is done.
pub fn task_toggle(line: &str) -> Option<(std::ops::Range<usize>, &'static str)> {
    let task = task_prefix(line)?;
    Some((task.mark, if task.checked { " " } else { "x" }))
}

/// Marker text a task row measures before its content.
///
/// An idle row hides the raw bracket marker — the renderer draws a checkbox in
/// those three cells — and keeps the bullet dot an unordered list paints in
/// place of its raw marker. The line being edited shows its raw source. Both
/// spellings are exactly [`TaskPrefix::chars`] cells wide, so the wrap does not
/// move.
pub fn task_paint_prefix(line: &str, task: &TaskPrefix<'_>, is_active_line: bool) -> String {
    if is_active_line {
        return line[..task.bytes].to_string();
    }
    let mut text = String::with_capacity(task.bytes);
    text.push_str(&line[..task.indent_bytes]);
    if task.ordered {
        text.push_str(task.list_marker);
    } else if task.list_chars() == RENDERED_BULLET.chars().count() {
        text.push_str(RENDERED_BULLET);
    } else {
        text.push_str(&" ".repeat(task.list_chars()));
    }
    text.push_str(&" ".repeat(task.checkbox.chars().count()));
    if task.space_after {
        text.push(' ');
    }
    text
}

/// Byte length of the marker that precedes the content of `line` for the list-like
/// kinds (task box, bullet, number). Zero for everything else.
pub(crate) fn marker_prefix_len(line: &str, classified: &LineKind<'_>) -> usize {
    match classified {
        LineKind::Task { .. } => task_prefix(line).map_or(0, |task| task.bytes),
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let bullet_len = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { 2 } else { 0 };
            indent_len + bullet_len
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let num_p_len = num.len() + 2;
            indent_len + if trimmed.len() >= num_p_len { num_p_len } else { 0 }
        }
        _ => 0,
    }
}
