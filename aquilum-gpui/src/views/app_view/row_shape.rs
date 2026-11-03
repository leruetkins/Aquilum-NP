use super::*;

#[derive(Clone)]
pub(crate) struct RowShapedLayout {
    pub(super) layout: Arc<gpui::LineLayout>,
    pub(super) text: SharedString,
    pub(super) line_chars: Arc<Vec<usize>>,
    pub(super) code_ranges: Arc<Vec<(usize, usize)>>,
    pub(super) tag_ranges: Arc<Vec<(usize, usize, String)>>,
    pub(super) grammar_ranges: Arc<Vec<(usize, usize, crate::markdown::GrammarSpanStyle)>>,
    pub(super) cell_w: f32,
    pub(super) table_row: Option<Arc<TableRowLayout>>,
}

impl RowShapedLayout {
    pub fn columns(&self) -> usize {
        if let Some(tr) = &self.table_row {
            tr.columns()
        } else {
            self.line_chars.len().saturating_sub(1)
        }
    }

    pub fn width(&self) -> f32 {
        f32::from(self.layout.width)
    }

    fn byte_for_column(&self, col: usize) -> usize {
        self.text
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }

    pub fn x_for_column(&self, col: usize) -> f32 {
        if let Some(tr) = &self.table_row {
            return tr.x_for_column(col, self.cell_w);
        }
        let columns = self.columns();
        if col >= columns {
            return f32::from(self.layout.width) + (col - columns) as f32 * self.cell_w;
        }
        let byte = self.byte_for_column(col);
        let mut cluster_x = 0.0f32;
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index == byte {
                    return f32::from(glyph.position.x);
                }
                if glyph.index > byte {
                    return cluster_x;
                }
                cluster_x = f32::from(glyph.position.x);
            }
        }
        cluster_x
    }

    /// Column a click at `(x, y)` resolves to. `y` is relative to the row's
    /// content box and only matters for table rows, where it selects the
    /// wrapped sub-line of a cell.
    pub fn column_for_xy(&self, x: f32, y: f32) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.column_for_xy(x, y);
        }
        let columns = self.columns();
        let end_x = f32::from(self.layout.width);
        if x >= end_x {
            return columns;
        }
        let byte = self.layout.closest_index_for_x(px(x));
        let col = self.text.get(..byte).map(|s| s.chars().count()).unwrap_or(0).min(columns);
        if col == columns && columns > 0 {
            let mid = (self.x_for_column(columns - 1) + end_x) / 2.0;
            if x < mid {
                return columns - 1;
            }
        }
        col
    }

    pub fn column_for_x(&self, x: f32) -> usize {
        self.column_for_xy(x, 0.0)
    }

    pub fn line_char_for_column(&self, col: usize) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.line_char_for_column(col);
        }
        crate::markdown::line_char_for_painted_boundary(&self.line_chars, col)
    }

    pub fn column_for_line_char(&self, lc: usize) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.column_for_line_char(lc);
        }
        self.line_chars
            .partition_point(|&line_char| line_char < lc)
            .min(self.columns())
    }

    pub fn code_ranges(&self) -> &[(usize, usize)] {
        if let Some(tr) = &self.table_row {
            &tr.code_ranges
        } else {
            &self.code_ranges
        }
    }

    pub fn tag_ranges(&self) -> &[(usize, usize, String)] {
        if let Some(tr) = &self.table_row {
            &tr.tag_ranges
        } else {
            &self.tag_ranges
        }
    }

    /// Grammar members of this row. Table rows are shaped cell by cell, so their
    /// members only get text colour there and no line.
    pub fn grammar_ranges(&self) -> &[(usize, usize, crate::markdown::GrammarSpanStyle)] {
        if self.table_row.is_some() {
            &[]
        } else {
            &self.grammar_ranges
        }
    }

    pub fn selection_rect(&self, sel_s: usize, sel_e: usize, scale_factor: f32) -> (f32, f32) {
        crate::snap::snapped_span(
            self.x_for_column(sel_s),
            self.x_for_column(sel_e),
            scale_factor,
        )
    }

    /// Rectangles a painted column range covers, as `(x, width, top)`.
    ///
    /// A table row with wrapped cells yields one rectangle per painted
    /// sub-line; every other row yields exactly one, at the top of the row.
    pub fn span_rects(&self, start_col: usize, end_col: usize, scale_factor: f32) -> Vec<(f32, f32, f32)> {
        if let Some(tr) = &self.table_row {
            return tr
                .span_rects(start_col, end_col)
                .into_iter()
                .map(|(x, w, top)| {
                    let (sx, sw) = crate::snap::snapped_span(x, x + w, scale_factor);
                    (sx, sw, top)
                })
                .collect();
        }
        if start_col >= end_col {
            return Vec::new();
        }
        let (x, w) = self.selection_rect(start_col, end_col, scale_factor);
        vec![(x, w, 0.0)]
    }

    /// Vertical position of the painted line a column belongs to, relative to
    /// the row content box. Zero for every row that is not a table row.
    pub fn top_for_column(&self, col: usize) -> f32 {
        match &self.table_row {
            Some(tr) => tr.top_for_column(col),
            None => 0.0,
        }
    }
}

/// Discriminant-only tag of a `LineKind` for cache keys. The payloads
/// (checked/indent/is_opening/lang) are functions of the line text and its
/// position, which the key hashes separately — the variant identity alone
/// is enough to separate the shapes.
pub(super) enum LineKindTag {
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Quote,
    Task,
    BulletList,
    NumberedList,
    HorizontalRule,
    CodeFence,
    CodeLine,
    Paragraph,
}

impl LineKindTag {
    pub(super) fn of(kind: &crate::markdown::LineKind<'_>) -> Self {
        use crate::markdown::LineKind::*;
        match kind {
            Heading1(_) => Self::Heading1,
            Heading2(_) => Self::Heading2,
            Heading3(_) => Self::Heading3,
            Heading4(_) => Self::Heading4,
            Heading5(_) => Self::Heading5,
            Heading6(_) => Self::Heading6,
            Quote(_) => Self::Quote,
            Task { .. } => Self::Task,
            BulletList { .. } => Self::BulletList,
            NumberedList { .. } => Self::NumberedList,
            HorizontalRule => Self::HorizontalRule,
            CodeFence { .. } => Self::CodeFence,
            CodeLine(_) => Self::CodeLine,
            Paragraph(_) => Self::Paragraph,
        }
    }
}

/// Hash of the heading settings a row's font metrics depend on; a settings
/// change produces a different cache key, so cached layouts stop hitting.
pub(super) fn headings_cache_key(headings: &crate::settings::HeadingsSettings) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for style in [
        &headings.h1, &headings.h2, &headings.h3, &headings.h4, &headings.h5, &headings.h6,
    ] {
        style.font_size.hash(&mut hasher);
        style.bold.hash(&mut hasher);
        style.italic.hash(&mut hasher);
    }
    hasher.finish()
}

/// The part of a byte-range selection that falls inside one line, in line-char offsets.
/// The implementation lives in `markdown` because the wrap geometry needs the same
/// answer as the renderer.
pub(super) fn selection_chars_in_line(
    line: &str,
    line_start: usize,
    selection: Option<&std::ops::Range<usize>>,
) -> Option<(usize, usize)> {
    crate::markdown::selection_chars_in_line(line, line_start, selection)
}

pub(crate) fn shape_row(
    text_system: &gpui::WindowTextSystem,
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    v: &crate::markdown::VisualRow,
    is_active_line: bool,
    reveal: &crate::markdown::InlineReveal,
    headings: &crate::settings::HeadingsSettings,
    font_family: &SharedString,
    cell_w: f32,
) -> RowShapedLayout {
    // Counts every real shaping (cache miss or direct call) — not rows per
    // frame, which used to be counted in the render loop and masked whether
    // the row cache worked at all.
    crate::perf::PERF
        .shape_calls
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let painted =
        crate::markdown::row_painted_text(line, classified, v, is_active_line, reveal, headings);
    let code_ranges = painted.code_ranges();
    let tag_ranges = painted.tag_ranges();
    let grammar_ranges = painted.grammar_ranges();
    let (_, _, font_size, _, _) = AppView::heading_typography_metrics_with_widths(classified, headings, 24.0, cell_w, None);

    let mut text = String::new();
    let mut runs = Vec::new();
    let color = rgba(0x000000ff).into();
    for segment in &painted.segments {
        let weight = if segment.code {
            inline_code_weight(if segment.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
        } else if segment.bold {
            gpui::FontWeight::BOLD
        } else {
            gpui::FontWeight::NORMAL
        };
        text.push_str(&segment.text);
        runs.push(gpui::TextRun {
            len: segment.text.len(),
            font: gpui::Font {
                family: font_family.clone(),
                weight,
                style: if segment.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal },
                features: Default::default(),
                fallbacks: None,
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let text: SharedString = text.into();
    let layout = text_system.layout_line(text.as_ref(), px(font_size), &runs, None);
    RowShapedLayout {
        layout,
        text,
        line_chars: Arc::new(painted.line_chars),
        code_ranges: Arc::new(code_ranges),
        tag_ranges: Arc::new(tag_ranges),
        grammar_ranges: Arc::new(grammar_ranges),
        cell_w,
        table_row: None,
    }
}
