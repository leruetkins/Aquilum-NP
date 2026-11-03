use super::*;
use super::classify::marker_prefix_len;

/// Byte offset of the first char of a row's content, i.e. the length of the leading
/// bytes of `line` that the renderer paints as a marker instead of as span content.
pub fn row_prefix_offset(line: &str, classified: &LineKind<'_>) -> usize {
    match classified {
        LineKind::Heading1(_) => 2,
        LineKind::Heading2(_) => 3,
        LineKind::Heading3(_) => 4,
        LineKind::Heading4(_) => 5,
        LineKind::Heading5(_) => 6,
        LineKind::Heading6(_) => 7,
        LineKind::Quote(_) => callout_prefix(line)
            .map(|c| c.bytes)
            .or_else(|| quote_prefix(line).map(|q| q.bytes))
            .unwrap_or(0),
        _ => marker_prefix_len(line, classified),
    }
}

/// The dot an idle unordered-list row paints in place of its raw `- `/`* `/`+ `.
///
/// It is the same glyph the app's markdown previews draw (`views::settings_view`,
/// `views::plugin_catalog`), and it advances one cell in every editor font the app
/// ships or offers, so the substitution costs no column and the wrap geometry is
/// unchanged. The line under the caret keeps its raw marker instead, exactly like a
/// heading does, so the source stays editable where the user is working.
pub const RENDERED_BULLET: &str = "• ";

/// Marker text a bullet row paints: the raw source marker while the line is being
/// edited, the rendered dot otherwise.
///
/// The two must keep the same char count — the dot maps onto the raw marker's chars
/// cell for cell, which is what makes swapping it free for the caret grid — so an
/// unexpected marker length falls back to the raw text instead.
pub fn bullet_paint_text<'a>(raw_marker: &'a str, is_active_line: bool) -> &'a str {
    if is_active_line || raw_marker.chars().count() != RENDERED_BULLET.chars().count() {
        raw_marker
    } else {
        RENDERED_BULLET
    }
}

/// Columns, measured in cells from the start of the row, where a nested list row
/// draws its vertical indentation guides.
///
/// Bullet and numbered rows guide the same way: every guide lines up with the
/// centre of the ancestor marker cell of the list level it hangs from, so the line
/// runs straight under the parent markers instead of standing in front of the
/// row's own marker. `indent_step` is the document's nesting step (see
/// [`list_indent_step`]); an indent that is not a whole number of steps gets one
/// guide per completed step.
pub fn bullet_guide_columns(indent_chars: usize, indent_step: usize) -> Vec<f32> {
    if indent_step == 0 || indent_chars < indent_step {
        return Vec::new();
    }
    (0..indent_chars / indent_step)
        .map(|level| (level * indent_step) as f32 + 0.5)
        .collect()
}

/// Indent of a bullet or numbered list row, in chars. `None` for everything else.
pub fn list_item_indent(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return Some(line.chars().count() - trimmed.chars().count());
    }
    let digits_len = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits_len > 0 && trimmed[digits_len..].starts_with(". ") {
        Some(line.chars().count() - trimmed.chars().count())
    } else {
        None
    }
}

/// Nesting step of the lists in `lines`: the smallest positive indent a bullet or
/// numbered row starts with.
///
/// It cannot be read off a single line — four spaces are one level of a four-space
/// document and two levels of a two-space one — so the renderer measures it across
/// the list block the row belongs to. Indents below two columns cannot nest a list
/// item (the marker itself is two columns wide) and are ignored. Zero means the
/// block holds no nested list items at all.
pub fn list_indent_step<'a>(lines: impl IntoIterator<Item = &'a str>) -> usize {
    lines
        .into_iter()
        .filter_map(list_item_indent)
        .filter(|&indent| indent >= 2)
        .min()
        .unwrap_or(0)
}

/// Guide columns for every line of a list window, in cells from the row start.
///
/// A line outside a list block (blank lines, prose, code rows passed as `None`)
/// gets no columns. Each contiguous block is measured on its own, so a sloppy
/// indent in one list cannot move the guides of its neighbour. Numbered rows use
/// the same guides as bullet rows: the line hangs under the ancestor markers, not
/// under the row's own number.
pub fn list_block_guide_columns<'a>(
    lines: impl IntoIterator<Item = Option<&'a str>>,
) -> Vec<Vec<f32>> {
    let lines: Vec<Option<&str>> = lines.into_iter().collect();
    let mut columns = vec![Vec::new(); lines.len()];
    let mut at = 0;
    while at < lines.len() {
        let Some(line) = lines[at] else {
            at += 1;
            continue;
        };
        if list_item_indent(line).is_none() {
            at += 1;
            continue;
        }
        let start = at;
        while at < lines.len() {
            let Some(line) = lines[at] else { break };
            if list_item_indent(line).is_none() {
                break;
            }
            at += 1;
        }
        let step = list_indent_step(lines[start..at].iter().filter_map(|line| *line));
        if step == 0 {
            continue;
        }
        for (offset, line) in lines[start..at].iter().enumerate() {
            let Some(line) = *line else { continue };
            let indent = list_item_indent(line).unwrap_or(0);
            columns[start + offset] = bullet_guide_columns(indent, step);
        }
    }
    columns
}
