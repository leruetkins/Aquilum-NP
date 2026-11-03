use super::*;

/// Height of one painted text line inside a table cell.
pub const TABLE_LINE_H: f32 = 20.0;
/// Vertical padding above and below the painted lines of a table row.
pub const TABLE_PAD_Y: f32 = 4.0;
/// Gap the table keeps above its header row and below its last row.
pub const TABLE_EDGE_MARGIN: f32 = 14.0;

/// A table row as the row geometry knows it: its kind and how many wrapped
/// text lines its tallest cell needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRowVisualKind {
    Delimiter,
    Header { lines: usize },
    Body { lines: usize },
    LastRow { lines: usize },
}

impl TableRowVisualKind {
    /// Total height of the row in logical pixels, margins included.
    pub fn height(&self) -> f32 {
        match self {
            Self::Delimiter => 1.0,
            Self::Header { lines } => self.cell_height(*lines) + TABLE_EDGE_MARGIN,
            Self::Body { lines } => self.cell_height(*lines),
            Self::LastRow { lines } => self.cell_height(*lines) + TABLE_EDGE_MARGIN,
        }
    }

    fn cell_height(&self, lines: usize) -> f32 {
        (lines.max(1) as f32) * TABLE_LINE_H + 2.0 * TABLE_PAD_Y
    }
}

/// One wrapped sub-line of a table cell.
///
/// `painted_*` are offsets into the cell's painted text (markers hidden), which
/// is what the cell is wrapped by; `raw_*` are the offsets into the document
/// line that the sub-line covers, which is what the caret and hit-testing use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellWrapLine {
    pub painted_start: usize,
    pub painted_end: usize,
    pub raw_start: usize,
    pub raw_end: usize,
}

/// Split one table cell into word-wrapped sub-lines at `max_cols` painted
/// columns.
///
/// The wrap is computed from the painted text — the same text the renderer
/// paints, with hidden markup costing no column — so a long cell wraps where
/// the reader sees it end. `raw_*` offsets come from the painted row's
/// `line_chars`, which keeps every sub-line boundary on a real document char
/// even when hidden markers sit between two visible ones.
pub fn table_cell_wrap_lines(
    line: &str,
    content: std::ops::Range<usize>,
    is_active_cell: bool,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
    max_cols: usize,
) -> Vec<CellWrapLine> {
    let v = VisualRow {
        line_idx: 0,
        sub_idx: 0,
        is_last_sub: true,
        char_start: content.start,
        char_end: content.end,
    };
    let painted = row_painted_text(
        line,
        &LineKind::Paragraph(Default::default()),
        &v,
        is_active_cell,
        reveal,
        headings,
    );
    wrap_painted_cell(&painted, content.end, max_cols)
}

/// Wrapped sub-lines of a cell whose painted row is already known.
pub fn wrap_painted_cell(
    painted: &PaintedRow,
    content_end: usize,
    max_cols: usize,
) -> Vec<CellWrapLine> {
    let mut text = String::new();
    for segment in &painted.segments {
        text.push_str(&segment.text);
    }
    let columns = painted.columns();
    let ranges = wrap_line_ranges(&text, max_cols.max(1));
    let raw_at = |painted_col: usize| -> usize {
        painted
            .line_chars
            .get(painted_col.min(columns))
            .copied()
            .unwrap_or(content_end)
    };
    ranges
        .into_iter()
        .map(|(ps, pe)| CellWrapLine {
            painted_start: ps,
            painted_end: pe,
            raw_start: raw_at(ps),
            raw_end: raw_at(pe),
        })
        .collect()
}

/// Number of wrapped lines the tallest cell of one table row needs.
///
/// `column_cols[c]` is the painted column budget of column `c`; a missing
/// entry means the column is unknown and does not force a wrap.
pub fn table_row_wrap_lines(
    line: &str,
    content_ranges: &[std::ops::Range<usize>],
    column_cols: &[usize],
    active_cell: Option<usize>,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
) -> usize {
    let mut lines = 1usize;
    for (idx, range) in content_ranges.iter().enumerate() {
        let Some(&cols) = column_cols.get(idx) else { continue; };
        if cols == 0 || range.start >= range.end {
            continue;
        }
        let reveal = if active_cell == Some(idx) {
            InlineReveal::All
        } else {
            reveal.clone()
        };
        let count = table_cell_wrap_lines(
            line,
            range.clone(),
            active_cell == Some(idx),
            &reveal,
            headings,
            cols,
        )
        .len();
        lines = lines.max(count);
    }
    lines
}

/// Slice an already revealed span sequence by char range.
///
/// Table cells are painted one wrapped sub-line at a time, but their spans are
/// revealed once for the whole cell, so each sub-line needs the piece of that
/// sequence that falls inside it. The span kinds are preserved, which keeps a
/// link or a code run styled across a wrap.
pub fn slice_visible_spans(
    spans: &[VisibleSpan],
    start_char: usize,
    end_char: usize,
) -> Vec<VisibleSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut curr = 0usize;
    for visible in spans {
        let len = span_text(&visible.span).chars().count();
        let span_end = curr + len;
        if span_end > start_char && curr < end_char {
            let s = start_char.saturating_sub(curr);
            let e = (end_char - curr).min(len);
            if s < e {
                let sliced = slice_syntax_span(&visible.span, s, e);
                result.push(VisibleSpan {
                    span: sliced,
                    marker_visible: visible.marker_visible,
                });
            }
        }
        curr = span_end;
        if curr >= end_char {
            break;
        }
    }
    result
}

fn slice_syntax_span(span: &SyntaxSpan, start_char: usize, end_char: usize) -> SyntaxSpan {
    let take = |text: &str| -> String {
        text.chars().skip(start_char).take(end_char - start_char).collect()
    };
    match span {
        SyntaxSpan::Marker(t) => SyntaxSpan::Marker(take(t)),
        SyntaxSpan::Text(t) => SyntaxSpan::Text(take(t)),
        SyntaxSpan::Bold(t) => SyntaxSpan::Bold(take(t)),
        SyntaxSpan::Italic(t) => SyntaxSpan::Italic(take(t)),
        SyntaxSpan::BoldItalic(t) => SyntaxSpan::BoldItalic(take(t)),
        SyntaxSpan::InlineCode(t) => SyntaxSpan::InlineCode(take(t)),
        SyntaxSpan::Strikethrough(t) => SyntaxSpan::Strikethrough(take(t)),
        SyntaxSpan::Underline(t) => SyntaxSpan::Underline(take(t)),
        SyntaxSpan::Highlight(t) => SyntaxSpan::Highlight(take(t)),
        SyntaxSpan::Tag(t) => SyntaxSpan::Tag(take(t)),
        SyntaxSpan::Link { text, url } => SyntaxSpan::Link {
            text: take(text),
            url: url.clone(),
        },
        SyntaxSpan::Grammar { kind, line_color, text_color, text, content } => SyntaxSpan::Grammar {
            kind: *kind,
            line_color: *line_color,
            text_color: *text_color,
            text: take(text),
            content: slice_visible_spans(content, start_char, end_char),
        },
    }
}

pub fn build_visual_rows_with_table_filter(
    lines: &[&str],
    word_wrap: bool,
    wrap_cols: usize,
    is_table_row: impl Fn(usize) -> bool,
) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let ranges = if is_table_row(line_idx) || !(word_wrap && wrap_cols > 0) {
            vec![(0, line.chars().count())]
        } else {
            wrap_line_ranges(line, wrap_cols)
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
