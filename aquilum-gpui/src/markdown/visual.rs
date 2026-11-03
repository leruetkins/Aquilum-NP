use super::*;

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
        } else if limit < char_count && chars[limit - 1] == ']' && (chars[limit] == '(' || chars[limit] == '[') {
            limit
        } else {
            // Look backwards for whitespace between start + 1 and limit
            let mut found = None;
            for i in (start + 1..limit).rev() {
                if chars[i].is_whitespace() {
                    found = Some(i + 1);
                    break;
                }
                if chars[i - 1] == ']' && (chars[i] == '(' || chars[i] == '[') {
                    found = Some(i);
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

/// How one line is painted right now: whether its markup is in the active
/// (visible) state and which inline markers the caret/selection reveals.
///
/// The wrap is derived from this, so the two callers that need the same answer
/// — the render loop and the geometry side (`workspace::view_state`) — must
/// build it the same way. [`line_paint_spec`] is the shared constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PaintedLineSpec {
    pub active: bool,
    pub reveal: InlineReveal,
}

/// Painted width of every raw char of a line, plus the indent its continuation
/// rows spend on synthetic padding.
///
/// `char_widths[i]` is the number of painted cells raw char `i` occupies: 1 for
/// a visible char (including a hidden inline-code delimiter, which keeps its
/// cell), 0 for a collapsed marker. A continuation row adds
/// `continuation_cols` cells of padding before its content, and those cells do
/// not belong to any raw char, so they are a per-row constant rather than part
/// of the array.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaintedLineMetrics {
    pub char_widths: Vec<u8>,
    pub continuation_cols: usize,
    /// Raw char where the inline content starts (after the block prefix). A
    /// wrap may break inside the prefix only as a last resort, so this keeps a
    /// hidden or painted `"# "` from filling a row on its own.
    pub content_start: usize,
}

/// Painted state of a line under the caret, the selection or an Alt+click.
pub fn line_paint_spec(
    line: &str,
    classified: &LineKind<'_>,
    cursor_char: Option<usize>,
    selection: Option<(usize, usize)>,
    reveal_all: bool,
) -> PaintedLineSpec {
    let has_selection = selection.map_or(false, |(s, e)| s < e);
    PaintedLineSpec {
        // A selected line is painted with its block markers, exactly like the
        // caret line is.
        active: cursor_char.is_some() || has_selection,
        reveal: inline_reveal(line, classified, cursor_char, selection, reveal_all),
    }
}

/// Columns a wrapped continuation row spends on padding before its content.
///
/// Mirrors the continuation branch of [`row_painted_text`] branch for branch.
pub fn continuation_prefix_cols(line: &str, classified: &LineKind<'_>) -> usize {
    let indent_chars = line.chars().count() - line.trim_start().chars().count();
    match classified {
        LineKind::Quote(_) => {
            let base = quote_prefix(line).map_or(0, |q| q.chars);
            if callout_prefix(line).is_some() {
                base + CALLOUT_ICON_CELLS
            } else {
                base
            }
        }
        LineKind::Task { .. } => task_prefix(line).map_or(0, |task| task.chars),
        LineKind::BulletList { .. } => indent_chars + 2,
        LineKind::NumberedList { num, .. } => indent_chars + num.chars().count() + 2,
        _ => 0,
    }
}

/// Measure the painted width of every raw char of `line`.
///
/// The measurement comes from [`row_painted_text`] itself — the same function
/// the renderer's geometry mirrors — so a wrap derived from it can never
/// disagree with what is painted.
pub fn painted_line_metrics(
    line: &str,
    classified: &LineKind<'_>,
    is_active_line: bool,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
) -> PaintedLineMetrics {
    let char_count = line.chars().count();
    let mut char_widths = vec![0u8; char_count];
    if char_count > 0 {
        let row = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: char_count,
        };
        let painted = row_painted_text(line, classified, &row, is_active_line, reveal, headings);
        for &raw in painted.line_chars.iter().take(painted.columns()) {
            if let Some(w) = char_widths.get_mut(raw) {
                *w = w.saturating_add(1);
            }
        }
    }
    PaintedLineMetrics {
        char_widths,
        continuation_cols: continuation_prefix_cols(line, classified),
        content_start: head_char_count(line, row_prefix_offset(line, classified)),
    }
}

/// Lines the renderer paints as a whole, whatever sub-row it is given: a rule
/// draws one horizontal line and a fence prints its raw text on every row.
/// Wrapping them would paint the text twice, so they always stay one row.
pub fn line_never_wraps(classified: &LineKind<'_>) -> bool {
    matches!(
        classified,
        LineKind::HorizontalRule | LineKind::CodeFence { .. }
    )
}

/// Wrap a line by its **painted** width instead of its raw char count.
///
/// A hidden marker costs no column, so a long URL that is not revealed takes no
/// room and the line wraps where the reader sees the text end — not where the
/// source ends. Ranges are contiguous raw char ranges, which is what
/// [`VisualRow`] and the renderer's slicing expect.
pub fn wrap_line_ranges_painted(
    line: &str,
    metrics: &PaintedLineMetrics,
    max_cols: usize,
) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let char_count = chars.len();
    if char_count == 0 {
        return vec![(0, 0)];
    }
    if max_cols == 0 || metrics.char_widths.len() != char_count {
        return vec![(0, char_count)];
    }

    // Cumulative painted width, so a range's width is one subtraction.
    let mut cum = Vec::with_capacity(char_count + 1);
    cum.push(0usize);
    for &w in &metrics.char_widths {
        cum.push(cum[cum.len() - 1] + w as usize);
    }
    if cum[char_count] <= max_cols {
        return vec![(0, char_count)];
    }

    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut first = true;
    while start < char_count {
        // Continuation rows pay for their synthetic indent out of the same
        // budget; the old raw wrap gave them the full width and let them
        // overflow the right edge.
        let prefix = if first { 0 } else { metrics.continuation_cols };
        let budget = max_cols.saturating_sub(prefix).max(1);
        let base = cum[start];

        // Largest raw boundary whose painted width still fits. Zero-width
        // (hidden) chars extend for free, so a boundary never lands on a
        // marker that paints nothing.
        let mut end = start + 1;
        while end < char_count && cum[end + 1] - base <= budget {
            end += 1;
        }

        if end >= char_count {
            ranges.push((start, char_count));
            break;
        }

        // Prefer a break after whitespace, but only if the row keeps at least
        // one painted content cell: the block prefix ("# ", "> ") must not
        // fill a row by itself while the content is pushed to the next one.
        // Also allow breaking between a markdown link label and its destination
        // (`](` or `][`) when revealed so the label stays on the line.
        let content_base = base.max(cum[metrics.content_start.min(char_count)]);
        let mut cut = None;
        for b in (start + 1..=end).rev() {
            if chars[b - 1].is_whitespace() && cum[b] > content_base {
                cut = Some(b);
                break;
            }
            if chars[b - 1] == ']'
                && (chars[b] == '(' || chars[b] == '[')
                && metrics.char_widths.get(b).copied().unwrap_or(0) > 0
                && cum[b] > content_base
            {
                cut = Some(b);
                break;
            }
        }
        let cut = cut.unwrap_or(end).max(start + 1);
        ranges.push((start, cut));
        start = cut;
        first = false;
    }
    ranges
}

/// Canonical visual rows: every line wrapped as it is painted when no caret,
/// selection or Alt+click reveals anything.
///
/// This is the stable layout preview, Ctrl-selection and idle editing share.
/// Lines under the caret or inside the selection are re-wrapped on top of it
/// (see `EditorWindow::rewrap_line`), so only the lines the reader is actually
/// touching reflow.
pub fn build_visual_rows_canonical(
    lines: &[&str],
    code_states: &[Option<CodeBlockState>],
    word_wrap: bool,
    wrap_cols: usize,
    headings: Option<&crate::settings::HeadingsSettings>,
    source_mode: bool,
    is_table_row: impl Fn(usize) -> bool,
) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    let default_headings = crate::settings::HeadingsSettings::default();
    let h_settings = headings.unwrap_or(&default_headings);
    for (line_idx, line) in lines.iter().enumerate() {
        let state = code_states.get(line_idx).and_then(|s| s.as_ref());
        let classified = if source_mode {
            match classify_line(line, state.map(|s| (s.kind, s.lang.as_deref()))) {
                LineKind::CodeFence { is_opening, lang } => LineKind::CodeFence { is_opening, lang },
                LineKind::CodeLine(raw) => LineKind::CodeLine(raw),
                _ => LineKind::Paragraph(line),
            }
        } else {
            classify_line(line, state.map(|s| (s.kind, s.lang.as_deref())))
        };
        let ranges = if is_table_row(line_idx)
            || !(word_wrap && wrap_cols > 0)
            || line_never_wraps(&classified)
        {
            vec![(0, line.chars().count())]
        } else {
            let metrics = painted_line_metrics(
                line,
                &classified,
                source_mode,
                if source_mode { &InlineReveal::All } else { &InlineReveal::None },
                h_settings,
            );
            wrap_line_ranges_painted(line, &metrics, wrap_cols)
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

pub fn build_visual_rows(lines: &[&str], word_wrap: bool, wrap_cols: usize) -> Vec<VisualRow> {
    build_visual_rows_with_table_filter(lines, word_wrap, wrap_cols, |_| false)
}

/// Painted boundary column of a raw char offset inside one row.
///
/// Mirrors `RowShapedLayout::column_for_line_char`: a hidden marker shares the
/// column of the cell that follows it, so vertical motion can keep the caret's
/// visual x instead of its raw char offset.
pub fn painted_column_of_line_char(painted: &PaintedRow, raw_char: usize) -> usize {
    painted
        .line_chars
        .partition_point(|&line_char| line_char < raw_char)
        .min(painted.columns())
}

/// Raw line char painted at a boundary column of the row, i.e. the inverse of
/// [`painted_column_of_line_char`].
pub fn line_char_at_painted_column(painted: &PaintedRow, column: usize) -> usize {
    painted.line_chars[column.min(painted.columns())]
}

/// Raw line char a click on `column` of a row must put the caret on.
///
/// `line_chars` maps painted cells to the raw chars they came from (with the
/// row's end as the last entry). A hidden marker run leaves a gap between two
/// painted cells, and taking the next cell's raw char would put the caret past
/// the markup: clicking the end of `[word](url)` would land after the URL and
/// collapse the element. The boundary therefore resolves to the char right
/// after the previous visible one; the row's end stays the row's end so typing
/// there still appends after the whole line.
pub fn line_char_for_painted_boundary(line_chars: &[usize], column: usize) -> usize {
    let Some(&first) = line_chars.first() else {
        return 0;
    };
    let columns = line_chars.len() - 1;
    let column = column.min(columns);
    if column == 0 {
        return first;
    }
    if column == columns {
        return line_chars[columns];
    }
    line_chars[column].min(line_chars[column - 1].saturating_add(1))
}

pub fn find_visual_row_for_cursor(
    visual_rows: &[VisualRow],
    cursor_line: usize,
    cursor_col: usize,
) -> Option<(usize, usize)> {
    if visual_rows.is_empty() {
        return None;
    }
    let start_idx = visual_rows.partition_point(|v| v.line_idx < cursor_line);
    if start_idx < visual_rows.len() && visual_rows[start_idx].line_idx == cursor_line {
        let mut last_sub = (start_idx, 0);
        for v_idx in start_idx..visual_rows.len() {
            let v = &visual_rows[v_idx];
            if v.line_idx != cursor_line {
                break;
            }
            if cursor_col >= v.char_start && (cursor_col < v.char_end || v.is_last_sub) {
                let col_in_row = cursor_col.saturating_sub(v.char_start);
                return Some((v_idx, col_in_row));
            }
            last_sub = (v_idx, cursor_col.saturating_sub(v.char_start));
        }
        return Some(last_sub);
    }
    if start_idx >= visual_rows.len() {
        return visual_rows.last().map(|v| (visual_rows.len() - 1, cursor_col.saturating_sub(v.char_start)));
    }
    visual_rows.first().map(|_| (0, 0))
}

/// Rows per entry in [`VisualRowHeightIndex::bases`].
///
/// The base of a block is the cumulative height of every row before it, so a
/// lookup only has to add up the rows of a single block.
const HEIGHT_BLOCK: usize = 256;

/// Row geometry of a virtualized list, addressed in `f64`.
///
/// A plain `f32` prefix sum cannot describe a huge document: rows are 24 px
/// tall, and past 2^27 logical px (≈5.6M rows, i.e. a ~450 MB note) the f32 grid
/// itself is 16 px wide, so `y + 24` rounds to `y + 32` and two rows end up
/// either 16 or 32 px apart — never the 24 px the rows are actually painted at.
/// That mismatch is what leaves gaps and overlaps between selection rectangles
/// near the end of a very large note. `f64` has the range to hold the same
/// numbers exactly (its grid is 16 *nanometres* at 2^27), and storing a base per
/// block keeps the memory *below* the old per-row prefix: one `f64` per 256 rows
/// against one `f32` per row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualRowHeightIndex {
    heights: Vec<f32>,
    /// bases[b] = exact cumulative height of rows `0..b * HEIGHT_BLOCK`.
    bases: Vec<f64>,
    /// Exact cumulative height of every row.
    total: f64,
}

/// Calculates row height for a heading with font size `h_size` and given `base_line_h`.
/// Headings are aligned to multiples of 4 logical pixels, ensuring that at any standard
/// display scale (100%, 125%, 150%, 175%, 200%, 225%, 250%) their height is an exact
/// integer number of physical device pixels, eliminating subpixel gaps and overlaps.
pub fn heading_row_height(h_size: f32, base_line_h: f32) -> f32 {
    let unrounded = (h_size * 1.35).max(base_line_h);
    ((unrounded / 4.0).ceil() * 4.0).max(base_line_h)
}

/// Height of the visual row(s) of one line: fixed heights for table rows,
/// otherwise the heading height or the plain 24 px line. Used both when
/// building a window from scratch and when updating a single line in place.
pub fn visual_row_height(
    line_str: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
    source_mode: bool,
    table_kind: Option<TableRowVisualKind>,
) -> f32 {
    if source_mode {
        return 24.0;
    }
    if let Some(kind) = table_kind {
        return kind.height();
    }
    let default_headings = crate::settings::HeadingsSettings::default();
    let h_settings = headings.unwrap_or(&default_headings);
    let classified = classify_line(line_str, None);
    let h_size = match classified {
        LineKind::Heading1(_) => h_settings.h1.font_size as f32,
        LineKind::Heading2(_) => h_settings.h2.font_size as f32,
        LineKind::Heading3(_) => h_settings.h3.font_size as f32,
        LineKind::Heading4(_) => h_settings.h4.font_size as f32,
        LineKind::Heading5(_) => h_settings.h5.font_size as f32,
        LineKind::Heading6(_) => h_settings.h6.font_size as f32,
        _ => 14.0,
    };
    if matches!(
        classified,
        LineKind::Heading1(_)
            | LineKind::Heading2(_)
            | LineKind::Heading3(_)
            | LineKind::Heading4(_)
            | LineKind::Heading5(_)
            | LineKind::Heading6(_)
    ) {
        heading_row_height(h_size, 24.0)
    } else {
        24.0
    }
}

impl VisualRowHeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut bases = Vec::with_capacity(heights.len() / HEIGHT_BLOCK + 1);
        let mut running = 0.0f64;
        for (i, &h) in heights.iter().enumerate() {
            if i % HEIGHT_BLOCK == 0 {
                bases.push(running);
            }
            // `f32` heights are exact in `f64`, and the running sum stays far
            // below 2^53, so the bases carry no rounding error at all.
            running += h as f64;
        }
        Self {
            heights,
            bases,
            total: running,
        }
    }

    /// Splices the heights of `old_count` rows starting at `first` and rebuilds
    /// the prefix bases. Pure numeric work — callers pass heights they have
    /// already computed for the replaced rows only.
    pub fn replace_rows(&mut self, first: usize, old_count: usize, new_heights: &[f32]) {
        let end = (first + old_count).min(self.heights.len());
        if first > end {
            return;
        }
        self.heights.splice(first..end, new_heights.iter().copied());
        self.bases.clear();
        let mut running = 0.0f64;
        for (i, &h) in self.heights.iter().enumerate() {
            if i % HEIGHT_BLOCK == 0 {
                self.bases.push(running);
            }
            running += h as f64;
        }
        self.total = running;
    }

    pub fn from_rows_with_tables(
        lines: &[impl AsRef<str>],
        visual_rows: &[VisualRow],
        headings: Option<&crate::settings::HeadingsSettings>,
        source_mode: bool,
        table_row_info: impl Fn(usize, &str) -> Option<TableRowVisualKind>,
        image_row_height: impl Fn(usize, &str) -> Option<f32>,
    ) -> Self {
        let mut heights = Vec::with_capacity(visual_rows.len());

        for v in visual_rows {
            let line_str = lines.get(v.line_idx).map(|s| s.as_ref()).unwrap_or("");
            // An image block owns its whole row: its height is the scaled
            // picture height, not the text line height.
            let height = image_row_height(v.line_idx, line_str)
                .filter(|_| !source_mode)
                .unwrap_or_else(|| visual_row_height(line_str, headings, source_mode, table_row_info(v.line_idx, line_str)));
            heights.push(height);
        }

        Self::new(heights)
    }

    pub fn from_rows(
        lines: &[impl AsRef<str>],
        visual_rows: &[VisualRow],
        headings: Option<&crate::settings::HeadingsSettings>,
        source_mode: bool,
    ) -> Self {
        Self::from_rows_with_tables(lines, visual_rows, headings, source_mode, |_, _| None, |_, _| None)
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    /// Top edge of row `v_idx`, or the total height when past the last row.
    ///
    /// Exact for any document size: the block base is exact and the local sum is
    /// at most `HEIGHT_BLOCK` heights, i.e. a few thousand pixels.
    pub fn top_of(&self, v_idx: usize) -> f64 {
        if v_idx >= self.heights.len() {
            return self.total;
        }
        let start = (v_idx / HEIGHT_BLOCK) * HEIGHT_BLOCK;
        let mut y = self.bases[v_idx / HEIGHT_BLOCK];
        for &h in &self.heights[start..v_idx] {
            y += h as f64;
        }
        y
    }

    pub fn height_of(&self, v_idx: usize) -> f32 {
        self.heights.get(v_idx).copied().unwrap_or(0.0)
    }

    pub fn v_idx_at_y(&self, y: f64) -> usize {
        if self.heights.is_empty() || y <= 0.0 {
            return 0;
        }
        // The last block whose base is at or above `y`; the row containing `y`
        // cannot be past it, because the next base is the end of that block.
        let block = self
            .bases
            .partition_point(|&base| base <= y)
            .saturating_sub(1)
            .min(self.bases.len() - 1);
        let start = block * HEIGHT_BLOCK;
        let end = (start + HEIGHT_BLOCK).min(self.heights.len());
        let mut bottom = self.bases[block];
        for k in start..end {
            bottom += self.heights[k] as f64;
            if bottom > y {
                return k;
            }
        }
        end - 1
    }

    pub fn visible_range(&self, scroll_top: f64, viewport_h: f64, overscan_px: f64) -> std::ops::Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let top = (scroll_top - overscan_px).max(0.0);
        let bottom = scroll_top + viewport_h + overscan_px;
        let first = self.v_idx_at_y(top);
        let last = (self.v_idx_at_y(bottom) + 1).min(self.heights.len());
        first..last.max(first)
    }

    /// Bytes held by the index. Used by the open-path budget benchmark in `perf.rs`
    /// to show that row geometry costs 4 bytes per row plus a base per block.
    pub fn bytes(&self) -> usize {
        self.heights.capacity() * std::mem::size_of::<f32>()
            + self.bases.capacity() * std::mem::size_of::<f64>()
    }
}

pub fn slice_syntax_spans(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
) -> Vec<SyntaxSpan> {
    slice_syntax_spans_revealed(spans, start_char, end_char, 0, &InlineReveal::All)
        .into_iter()
        .map(|v| v.span)
        .collect()
}

/// Slice `[start_char, end_char)` out of the span sequence, recording for every marker
/// whether `reveal` shows it. `abs_start` is the char offset of the span sequence inside
/// the raw line, so reveal ranges (which are line-relative) line up with the spans.
pub fn slice_syntax_spans_revealed(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
    abs_start: usize,
    reveal: &InlineReveal,
) -> Vec<VisibleSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut curr_char = 0;

    for span in spans {
        // Grammar spans carry their colours, so they are sliced on their own instead
        // of going through the kind code below. Their inline content is sliced too,
        // with the member's absolute offset, so a nested marker reveals with the line
        // it belongs to.
        if let SyntaxSpan::Grammar { kind, line_color, text_color, text, content } = span {
            let span_len = text.chars().count();
            let span_end = curr_char + span_len;
            if span_end > start_char && curr_char < end_char {
                let s = start_char.saturating_sub(curr_char);
                let e = (end_char - curr_char).min(span_len);
                if s < e {
                    let sliced: String = text.chars().skip(s).take(e - s).collect();
                    result.push(VisibleSpan {
                        span: SyntaxSpan::Grammar {
                            kind: *kind,
                            line_color: *line_color,
                            text_color: *text_color,
                            text: sliced,
                            content: slice_visible_spans_revealed(
                                content,
                                s,
                                e,
                                abs_start + curr_char,
                                reveal,
                            ),
                        },
                        marker_visible: true,
                    });
                }
            }
            curr_char = span_end;
            if curr_char >= end_char {
                break;
            }
            continue;
        }

        let (kind, text, url) = match span {
            SyntaxSpan::Marker(t) => (0, t, None),
            SyntaxSpan::Text(t) => (1, t, None),
            SyntaxSpan::Bold(t) => (2, t, None),
            SyntaxSpan::Italic(t) => (3, t, None),
            SyntaxSpan::BoldItalic(t) => (4, t, None),
            SyntaxSpan::InlineCode(t) => (5, t, None),
            SyntaxSpan::Strikethrough(t) => (6, t, None),
            SyntaxSpan::Link { text, url } => (7, text, Some(url)),
            SyntaxSpan::Underline(t) => (8, t, None),
            SyntaxSpan::Highlight(t) => (9, t, None),
            SyntaxSpan::Tag(t) => (10, t, None),
            SyntaxSpan::Grammar { .. } => unreachable!("grammar spans are sliced above"),
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
                    7 => SyntaxSpan::Link {
                        text: sliced_str,
                        url: url.cloned().unwrap_or_default(),
                    },
                    8 => SyntaxSpan::Underline(sliced_str),
                    9 => SyntaxSpan::Highlight(sliced_str),
                    10 => SyntaxSpan::Tag(sliced_str),
                    _ => SyntaxSpan::Text(sliced_str),
                };
                // Only markers ever reach a hidden state; everything else is content.
                // `abs_start + curr_char + s` is the marker's absolute char position.
                let marker_visible = kind != 0
                    || reveal.shows(abs_start + curr_char + s, abs_start + curr_char + e);
                result.push(VisibleSpan {
                    span: sliced_span,
                    marker_visible,
                });
            }
        }

        curr_char = span_end;
        if curr_char >= end_char {
            break;
        }
    }

    result
}

/// Same as [`slice_syntax_spans_revealed`], for a grammar member's already parsed
/// inline content. The content is re-sliced through the span slicer, so its markers
/// get their visibility from the same reveal ranges as the line's own markers.
fn slice_visible_spans_revealed(
    spans: &[VisibleSpan],
    start_char: usize,
    end_char: usize,
    abs_start: usize,
    reveal: &InlineReveal,
) -> Vec<VisibleSpan> {
    let plain: Vec<SyntaxSpan> = spans.iter().map(|v| v.span.clone()).collect();
    slice_syntax_spans_revealed(&plain, start_char, end_char, abs_start, reveal)
}

/// Legacy visual-column -> row-char mapping.
///
/// Superseded for geometry: it assumes every column is one monospace cell, so it is
/// wrong for wide glyphs (emoji, CJK) and for the synthetic continuation indent of a
/// wrapped list/quote row. Row geometry now goes through
/// `views::app_view::RowShapedLayout`, which is built from shaped glyph positions.
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
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => (false, t),
            SyntaxSpan::Link { text, .. } => (false, text),
            SyntaxSpan::Grammar { text, .. } => (false, text),
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

/// Legacy row-char -> visual-column mapping.
///
/// Superseded for geometry: see `visual_col_to_row_char`.
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
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => (false, t),
            SyntaxSpan::Link { text, .. } => (false, text),
            SyntaxSpan::Grammar { text, .. } => (false, text),
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
