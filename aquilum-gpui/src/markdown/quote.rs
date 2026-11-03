use super::*;

/// The leading markers of a block-quote line.
///
/// A level is a `>` optionally followed by the space markdown allows after it,
/// so `> > text` nests two levels and a bare `>` closes a paragraph inside the
/// quote without leaving the quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotePrefix {
    /// Number of `>` levels, at least one.
    pub depth: usize,
    /// Length of the marker prefix in bytes.
    pub bytes: usize,
    /// Length of the marker prefix in chars. This is how many cells the row
    /// spends on the prefix: the markers are hidden but not collapsed, so the
    /// canonical and the edited layout stay on the same grid.
    pub chars: usize,
}

/// The block-quote markers at the head of `line`, or `None` for anything else.
///
/// The scan only matches at the start of the line: an indented `>` is left to
/// the paragraph, exactly like an indented heading marker is.
pub fn quote_prefix(line: &str) -> Option<QuotePrefix> {
    let mut rest = line;
    let mut depth = 0usize;
    while let Some(after) = rest.strip_prefix('>') {
        depth += 1;
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    if depth == 0 {
        return None;
    }
    let bytes = line.len() - rest.len();
    Some(QuotePrefix {
        depth,
        bytes,
        chars: line[..bytes].chars().count(),
    })
}

/// Columns, measured in cells from the start of the row, of the `>` that opens
/// each quote level — the cell each level's bar hangs in.
///
/// `prefix` is the marker prefix of the line (see [`quote_prefix`]). The bars
/// follow the raw markers rather than a fixed step, so `> > >` and `>>>` both
/// get a bar per level and none of them lands on the text.
pub fn quote_bar_columns(prefix: &str) -> Vec<f32> {
    prefix
        .chars()
        .enumerate()
        .filter(|&(_, c)| c == '>')
        .map(|(i, _)| i as f32)
        .collect()
}

/// The family a callout belongs to, which is what its label is coloured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Info,
    Tip,
    Warning,
    Danger,
    Quote,
}

/// The name a callout type is filed under, or `None` for a quote that only looks
/// like one.
///
/// The aliases are the ones Obsidian accepts, so a note written for it keeps its
/// colour here. An unknown type is not a callout: the marker then stays ordinary
/// quoted text, visible, instead of turning a typo into a styled badge.
fn callout_kind(tag: &str) -> Option<CalloutKind> {
    match tag.to_ascii_lowercase().as_str() {
        "note" | "abstract" | "summary" | "tldr" | "example" => Some(CalloutKind::Note),
        "info" | "todo" | "question" | "help" | "faq" => Some(CalloutKind::Info),
        "tip" | "hint" | "important" | "success" | "check" | "done" => Some(CalloutKind::Tip),
        "warning" | "caution" | "attention" => Some(CalloutKind::Warning),
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => Some(CalloutKind::Danger),
        "quote" | "cite" => Some(CalloutKind::Quote),
        _ => None,
    }
}

/// A callout header: `> [!type] title`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutPrefix<'a> {
    pub kind: CalloutKind,
    /// The type exactly as the source spells it, e.g. `note`.
    pub label: &'a str,
    /// Length of the marker region in bytes: the quote prefix, `[!type]` and the
    /// single space markdown allows after it, when present.
    pub bytes: usize,
    /// Length of the marker region in chars — the cells the source spends on the
    /// type and its padding. An untitled header uses them for its type label.
    pub chars: usize,
    /// Everything after the marker region, i.e. the title (empty when there is
    /// none, so an untitled header shows its type instead).
    pub title: &'a str,
}

/// The callout marker at the head of a block quote, or `None` for a plain quote.
///
/// Only a quote at the top level carries a callout. Several `>` are a nested
/// quote first of all: what follows them is quoted text, so a `[!type]` there
/// stays in the source's own form instead of becoming a styled block.
pub fn callout_prefix(line: &str) -> Option<CalloutPrefix<'_>> {
    let quote = quote_prefix(line)?;
    if quote.depth > 1 {
        return None;
    }
    let inner = line[quote.bytes..].strip_prefix("[!")?;
    let close = inner.find(']')?;
    let tag = &inner[..close];
    if tag.is_empty() || !tag.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let kind = callout_kind(tag)?;
    // Past `]`, and past the one space markdown allows between marker and title.
    let marker = quote.bytes + 2 + close + 1;
    let bytes = if line[marker..].starts_with(' ') { marker + 1 } else { marker };
    Some(CalloutPrefix {
        kind,
        label: tag,
        bytes,
        chars: line[..bytes].chars().count(),
        title: &line[bytes..],
    })
}

/// What an idle untitled callout header paints in place of `[!type] `: the type
/// in title case, padded to the cells the marker has left after `leading_cells` —
/// the quote prefix and the icon.
///
/// A titled header hides the type name, so this padding only has to keep the
/// placeholder row as wide as the source marker it replaces.
pub fn callout_label_padded(callout: &CalloutPrefix<'_>, leading_cells: usize) -> String {
    let region = callout.chars.saturating_sub(leading_cells);
    let mut rest = callout.label.chars();
    let label: String = match rest.next() {
        Some(first) => first.to_uppercase().chain(rest).collect(),
        None => String::new(),
    };
    let used = label.chars().count().min(region);
    label
        .chars()
        .take(used)
        .chain(std::iter::repeat(' ').take(region - used))
        .collect()
}

/// The name a callout type is translated under, with Obsidian's aliases folded
/// into the type they stand for.
fn callout_name(tag: &str) -> &'static str {
    match tag.to_ascii_lowercase().as_str() {
        "abstract" | "summary" | "tldr" => "abstract",
        "info" => "info",
        "todo" => "todo",
        "tip" | "hint" | "important" => "tip",
        "success" | "check" | "done" => "success",
        "question" | "help" | "faq" => "question",
        "warning" | "caution" | "attention" => "warning",
        "failure" | "fail" | "missing" => "failure",
        "danger" | "error" => "danger",
        "bug" => "bug",
        "example" => "example",
        "quote" | "cite" => "quote",
        _ => "note",
    }
}

/// The localized name an idle untitled callout header shows, e.g. the type label for
/// `[!tip]` in Russian.
///
/// An unknown translation falls back to the source type in title case, so a
/// missing locale key never hides the header's text.
pub fn callout_label(callout: &CalloutPrefix<'_>, lang: &str) -> String {
    let key = format!("editor.callout.{}", callout_name(callout.label));
    let translated = crate::i18n::t(&key, lang);
    if translated != key {
        return translated;
    }
    let mut rest = callout.label.chars();
    match rest.next() {
        Some(first) => first.to_uppercase().chain(rest).collect(),
        None => String::new(),
    }
}


/// Cells an idle callout header spends before its label: the type's icon and the
/// gap that separates it from the text. Measured from the start of the marker
/// region, so the label and the title keep their columns.
pub const CALLOUT_ICON_CELLS: usize = 3;

/// Width of a callout's left bar in logical px.
///
/// It matches the block's corner radius (`rounded_*_md`, 6px): at the block's end
/// the bar's rounded corner then follows the block's own arc across its whole
/// width, where a narrow bar would show a step between its rounded and square
/// halves. A plain quote keeps its thin hairline bar.
pub const CALLOUT_BAR_WIDTH: f32 = 6.0;

/// Colour of a callout's label and title.
///
/// The palette is the Catppuccin set `theme.rs` is built from; `Theme` carries no
/// warning or danger role, and a callout is markdown content rather than chrome,
/// so the colour is resolved here the way tag colours already are.
pub fn callout_color(kind: CalloutKind, is_dark: bool) -> gpui::Rgba {
    gpui::rgb(callout_hex(kind, is_dark))
}

/// Wash behind a callout block: the family colour at a fraction of its opacity,
/// so the block reads as one rectangle while its prose keeps the text colours it
/// already has.
pub fn callout_bg(kind: CalloutKind, is_dark: bool) -> gpui::Rgba {
    let alpha = if is_dark { 0x24 } else { 0x1a };
    gpui::rgba((callout_hex(kind, is_dark) << 8) | alpha)
}

fn callout_hex(kind: CalloutKind, is_dark: bool) -> u32 {
    match (kind, is_dark) {
        (CalloutKind::Note | CalloutKind::Info, true) => 0x89b4fa,
        (CalloutKind::Note | CalloutKind::Info, false) => 0x1e66f5,
        (CalloutKind::Tip, true) => 0xa6e3a1,
        (CalloutKind::Tip, false) => 0x40a02b,
        (CalloutKind::Warning, true) => 0xf9e2af,
        (CalloutKind::Warning, false) => 0xdf8e1d,
        (CalloutKind::Danger, true) => 0xf38ba8,
        (CalloutKind::Danger, false) => 0xd20f39,
        (CalloutKind::Quote, true) => 0x6c7086,
        (CalloutKind::Quote, false) => 0x8c8fa1,
    }
}

/// The callout block a line belongs to, so every row of it can be tinted and
/// only its two ends rounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutBlock {
    pub kind: CalloutKind,
    /// First row of the block: the header.
    pub first: bool,
    /// Last row of the block.
    pub last: bool,
}

/// One entry per line: the callout block covering it, if any.
///
/// A block starts at a header and runs over the block quote under it — the extent
/// Obsidian draws. A second header ends the first block rather than joining it,
/// and `code_states` keeps a `> [!note]` inside a fence from opening one.
pub fn callout_blocks(
    lines: &[String],
    code_states: &[Option<CodeBlockState>],
) -> Vec<Option<CalloutBlock>> {
    let in_code = |idx: usize| code_states.get(idx).map_or(false, |s| s.is_some());
    let mut blocks = vec![None; lines.len()];
    let mut idx = 0;
    while idx < lines.len() {
        let header = if in_code(idx) { None } else { callout_prefix(&lines[idx]) };
        let Some(header) = header else {
            idx += 1;
            continue;
        };
        let mut end = idx;
        while end + 1 < lines.len()
            && !in_code(end + 1)
            && quote_prefix(&lines[end + 1]).is_some()
            && callout_prefix(&lines[end + 1]).is_none()
        {
            end += 1;
        }
        for row in idx..=end {
            blocks[row] = Some(CalloutBlock {
                kind: header.kind,
                first: row == idx,
                last: row == end,
            });
        }
        idx = end + 1;
    }
    blocks
}
