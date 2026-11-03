use super::*;

/// Shaped geometry of one painted visual row.
///
/// The editor paints rows from `SyntaxSpan` slices, so nothing downstream knew where
/// the glyphs actually landed: hit-testing and the caret/selection rectangles used a
/// naive `column * char_width` cell model instead. That model is exact for monospace
/// runs of narrow glyphs, but drifts for wide ones (emoji, CJK, VS16/ZWJ sequences),
/// which made the selection edge twitch while dragging across them.
///
/// This follows Zed's editor: the row's painted text is shaped once and every
/// pixel <-> column conversion goes through the real glyph positions
/// (`LineLayout::closest_index_for_x` and `ShapedGlyph::position`).
#[derive(Clone, Debug)]
pub(crate) struct TableColResizeDrag {
    pub table_first_line: usize,
    pub col_idx: usize,
    pub start_x: f32,
    pub start_width: f32,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct TableLayout {
    pub first_line: usize,
    pub col_widths: Vec<f32>,
    pub col_x: Vec<f32>,
    pub total_width: f32,
    pub aligns: Vec<crate::table::Align>,
    /// Painted columns each cell of a column can hold before its text wraps.
    /// The row geometry wraps cells with these exact budgets, so the height a
    /// row claims always matches the grid painted over it.
    pub col_chars: Vec<usize>,
    /// Font size cells are shaped and painted at.
    pub font_size: f32,
    /// Height of one painted line inside a cell, at this font size.
    pub line_h: f32,
}

/// One wrapped sub-line of a table cell, shaped on its own.
///
/// A cell that does not fit its column is painted as a stack of these; every
/// pixel <-> column conversion inside the cell resolves to one of them, so the
/// caret and the selection land on the line they belong to.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TableCellLine {
    pub text: SharedString,
    pub layout: Arc<gpui::LineLayout>,
    /// Painted columns of this sub-line.
    pub columns: usize,
    /// Raw document char painted at each column, plus one trailing entry.
    pub line_chars: Vec<usize>,
    /// Raw document chars this sub-line covers.
    pub raw_start: usize,
    pub raw_end: usize,
    /// X of the first painted column inside the cell, alignment included.
    pub content_x: f32,
}

impl TableCellLine {
    fn byte_for_column(&self, col: usize) -> usize {
        self.text
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }

    /// X of painted column `col` of this sub-line, relative to the row.
    fn glyph_x(&self, col: usize) -> f32 {
        if col >= self.columns {
            return self.content_x + f32::from(self.layout.width);
        }
        let byte = self.byte_for_column(col);
        let mut cluster_x = 0.0f32;
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index == byte {
                    return self.content_x + f32::from(glyph.position.x);
                }
                if glyph.index > byte {
                    return self.content_x + cluster_x;
                }
                cluster_x = f32::from(glyph.position.x);
            }
        }
        self.content_x + cluster_x
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TableCellLayout {
    pub col_idx: usize,
    pub cell_x: f32,
    pub cell_w: f32,
    pub cell_start_col: usize,
    pub cell_columns: usize,
    pub content_range: std::ops::Range<usize>,
    pub outer_range: std::ops::Range<usize>,
    pub lines: Vec<TableCellLine>,
    pub code_ranges: Vec<(usize, usize)>,
    pub tag_ranges: Vec<(usize, usize, String)>,
}

impl TableCellLayout {
    /// Sub-line and column-in-line a flat column of this cell resolves to.
    fn line_col(&self, flat: usize) -> (usize, usize) {
        let mut acc = 0usize;
        for (i, line) in self.lines.iter().enumerate() {
            if flat < acc + line.columns {
                return (i, flat - acc);
            }
            acc += line.columns;
        }
        let last = self.lines.len().saturating_sub(1);
        (last, self.lines.get(last).map_or(0, |l| l.columns))
    }

    fn line_offset(&self, line: usize) -> usize {
        self.lines.iter().take(line).map(|l| l.columns).sum()
    }

    /// Raw document char a flat column of this cell maps to.
    fn line_char_for_flat(&self, flat: usize) -> usize {
        if flat >= self.cell_columns {
            return self.content_range.end;
        }
        let (line_idx, col) = self.line_col(flat);
        self.lines
            .get(line_idx)
            .and_then(|l| l.line_chars.get(col))
            .copied()
            .unwrap_or(self.content_range.end)
    }

    /// Flat column of a raw document char, clamped to the cell.
    fn flat_for_line_char(&self, lc: usize) -> usize {
        if lc <= self.content_range.start {
            return 0;
        }
        if lc >= self.content_range.end {
            return self.cell_columns;
        }
        let mut offset = 0usize;
        for (i, line) in self.lines.iter().enumerate() {
            let is_last = i + 1 == self.lines.len();
            if line.raw_end >= lc || is_last {
                let col = line
                    .line_chars
                    .partition_point(|&ch| ch < lc)
                    .min(line.columns);
                return offset + col;
            }
            offset += line.columns;
        }
        self.cell_columns
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TableRowLayout {
    pub is_delimiter: bool,
    pub cells: Vec<TableCellLayout>,
    pub total_width: f32,
    pub total_columns: usize,
    pub line_chars: Vec<usize>,
    pub code_ranges: Vec<(usize, usize)>,
    pub tag_ranges: Vec<(usize, usize, String)>,
    /// Height of one painted line inside a cell.
    pub line_h: f32,
}

impl TableRowLayout {
    pub fn columns(&self) -> usize {
        self.total_columns
    }

    fn cell_for_col(&self, col: usize) -> Option<&TableCellLayout> {
        self.cells
            .iter()
            .find(|c| col <= c.cell_start_col + c.cell_columns)
    }

    fn cell_for_x(&self, x: f32) -> Option<&TableCellLayout> {
        self.cells
            .iter()
            .find(|c| x >= c.cell_x && x < c.cell_x + c.cell_w)
    }

    /// Vertical position of a sub-line inside the row content box.
    pub fn line_top(&self, line: usize) -> f32 {
        crate::markdown::TABLE_PAD_Y + line as f32 * self.line_h
    }

    pub fn x_for_column(&self, col: usize, cell_w: f32) -> f32 {
        if self.is_delimiter || self.cells.is_empty() {
            return 0.0;
        }
        if col >= self.total_columns {
            return self.total_width + (col - self.total_columns) as f32 * cell_w;
        }
        if let Some(cell) = self.cell_for_col(col) {
            let flat = col - cell.cell_start_col;
            let (line_idx, col_in_line) = cell.line_col(flat);
            return cell
                .lines
                .get(line_idx)
                .map_or(cell.cell_x, |l| l.glyph_x(col_in_line));
        }
        if let Some(last) = self.cells.last() {
            last.lines
                .last()
                .map_or(last.cell_x + last.cell_w, |l| l.glyph_x(l.columns))
        } else {
            0.0
        }
    }

    /// Vertical position of the sub-line a flat column belongs to, relative to
    /// the row content box.
    pub fn top_for_column(&self, col: usize) -> f32 {
        if self.is_delimiter || self.cells.is_empty() {
            return 0.0;
        }
        let cell = self.cell_for_col(col).or_else(|| self.cells.last());
        match cell {
            Some(cell) => {
                let (line_idx, _) = cell.line_col(col.saturating_sub(cell.cell_start_col));
                self.line_top(line_idx)
            }
            None => 0.0,
        }
    }

    /// Column a click at `(x, y)` inside the row content box resolves to.
    ///
    /// The y picks the wrapped sub-line, so clicking the second line of a cell
    /// puts the caret on the second line instead of the first.
    pub fn column_for_xy(&self, x: f32, y: f32) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        if x < 0.0 {
            return 0;
        }
        if let Some(cell) = self.cell_for_x(x) {
            if cell.lines.is_empty() {
                return cell.cell_start_col;
            }
            let line_idx = if y <= crate::markdown::TABLE_PAD_Y {
                0
            } else {
                (((y - crate::markdown::TABLE_PAD_Y) / self.line_h).floor()
                    as usize)
                    .min(cell.lines.len() - 1)
            };
            let offset = cell.line_offset(line_idx);
            let line = &cell.lines[line_idx];
            let local_x = (x - line.content_x).max(0.0);
            let byte = line.layout.closest_index_for_x(px(local_x));
            let col_in_line = line
                .text
                .get(..byte)
                .map(|s| s.chars().count())
                .unwrap_or(0)
                .min(line.columns);
            return cell.cell_start_col + offset + col_in_line;
        }
        if let Some(last) = self.cells.last() {
            return last.cell_start_col + last.cell_columns;
        }
        self.total_columns
    }

    pub fn line_char_for_column(&self, col: usize) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        if let Some(cell) = self.cell_for_col(col) {
            return cell.line_char_for_flat(col - cell.cell_start_col);
        }
        if let Some(last) = self.cells.last() {
            last.content_range.end
        } else {
            0
        }
    }

    pub fn column_for_line_char(&self, lc: usize) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        for cell in &self.cells {
            if lc <= cell.outer_range.end {
                return cell.cell_start_col + cell.flat_for_line_char(lc);
            }
        }
        if let Some(last) = self.cells.last() {
            last.cell_start_col + last.cell_columns
        } else {
            self.total_columns
        }
    }

    /// One rectangle per painted sub-line that the column range covers, as
    /// `(x, width, top)`. A range inside a multi-line cell therefore yields one
    /// rectangle per line it spans, which is what makes the selection follow the
    /// wrap instead of jumping to a single line.
    pub fn span_rects(&self, start_col: usize, end_col: usize) -> Vec<(f32, f32, f32)> {
        if self.is_delimiter || self.cells.is_empty() || start_col >= end_col {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for cell in &self.cells {
            let mut offset = 0usize;
            for (line_idx, line) in cell.lines.iter().enumerate() {
                let line_start = cell.cell_start_col + offset;
                let line_end = line_start + line.columns;
                let s = start_col.max(line_start);
                let e = end_col.min(line_end);
                if s < e {
                    let x0 = line.glyph_x(s - line_start);
                    let x1 = line.glyph_x(e - line_start);
                    if x1 > x0 {
                        rects.push((x0, x1 - x0, self.line_top(line_idx)));
                    }
                }
                offset += line.columns;
            }
        }
        rects
    }
}

pub(crate) fn fit_table_columns(natural: &[f32], room: f32) -> Vec<f32> {
    const WIDEST: f32 = 480.0;
    const NARROWEST: f32 = 96.0;

    if natural.is_empty() {
        return Vec::new();
    }

    let capped: Vec<f32> = natural.iter().map(|&w| w.min(WIDEST)).collect();
    let floor: Vec<f32> = capped.iter().map(|&w| w.min(NARROWEST)).collect();
    let between = |lo: &[f32], hi: &[f32], r: f32| -> Vec<f32> {
        let (low, high): (f32, f32) = (lo.iter().sum(), hi.iter().sum());
        let share = if high <= r { 1.0 } else if high > low { ((r - low) / (high - low)).max(0.0) } else { 0.0 };
        lo.iter().zip(hi).map(|(&l, &h)| l + (h - l) * share).collect()
    };

    let widths = if capped.iter().sum::<f32>() <= room {
        between(&capped, natural, room)
    } else {
        between(&floor, &capped, room)
    };
    let total: f32 = widths.iter().sum();
    if total > 0.0 && total < room {
        widths.iter().map(|&w| w * room / total).collect()
    } else {
        widths
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_table_layout<'a>(
    text_system: &gpui::WindowTextSystem,
    get_line: &dyn Fn(usize) -> Option<&'a str>,
    block: &crate::table::TableBlock,
    font_family: &SharedString,
    manual_widths: Option<&[f32]>,
    char_width: f32,
    room_px: f32,
    font_size: f32,
    line_h: f32,
) -> TableLayout {
    const PADDING_X: f32 = 8.0;
    const MIN_COL_WIDTH: f32 = 80.0;
    let n_cols = block.col_count();
    let mut max_widths = vec![0.0f32; n_cols];

    let font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::NORMAL,
        style: gpui::FontStyle::Normal,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_font = gpui::Font {
        weight: gpui::FontWeight::BOLD,
        ..font.clone()
    };
    let dummy_color = gpui::rgba(0x000000ff).into();

    for line_idx in block.first_line..=block.last_line {
        if line_idx == block.sep_line() {
            continue;
        }
        let Some(line_str) = get_line(line_idx) else { continue; };
        let cells = crate::table::split_row(line_str);
        let is_header = line_idx == block.first_line;

        for (c_idx, cell) in cells.into_iter().take(n_cols).enumerate() {
            let content = if cell.content.start >= cell.content.end {
                ""
            } else {
                let mut b_start = 0;
                let mut b_end = line_str.len();
                for (ci, (bi, _)) in line_str.char_indices().enumerate() {
                    if ci == cell.content.start {
                        b_start = bi;
                    }
                    if ci == cell.content.end {
                        b_end = bi;
                        break;
                    }
                }
                &line_str[b_start..b_end]
            };
            if content.is_empty() {
                continue;
            }
            let f = if is_header { bold_font.clone() } else { font.clone() };
            let run = gpui::TextRun {
                len: content.len(),
                font: f,
                color: dummy_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let l = text_system.layout_line(content, px(font_size), &[run], None);
            let w = f32::from(l.width);
            if w > max_widths[c_idx] {
                max_widths[c_idx] = w;
            }
        }
    }

    let natural_widths: Vec<f32> = max_widths.iter().map(|&w| (w + PADDING_X * 2.0).max(MIN_COL_WIDTH)).collect();
    let col_widths = if let Some(mw) = manual_widths {
        let mut w = mw.to_vec();
        if w.len() < n_cols {
            for i in w.len()..n_cols {
                let def = natural_widths.get(i).copied().unwrap_or(120.0).max(MIN_COL_WIDTH);
                w.push(def);
            }
        } else if w.len() > n_cols {
            w.truncate(n_cols);
        }
        w
    } else {
        // A table wider than the room the editor has for it would push the
        // note sideways and never wrap; it is fitted to the room instead, which
        // is what turns a long cell into several painted lines.
        let sum_natural: f32 = natural_widths.iter().sum();
        let room = room_px.max(MIN_COL_WIDTH);
        if sum_natural > room {
            fit_table_columns(&natural_widths, room)
        } else if sum_natural < 720.0 {
            fit_table_columns(&natural_widths, 720.0f32.min(room))
        } else {
            natural_widths
        }
    };

    let col_chars: Vec<usize> = col_widths
        .iter()
        .map(|&w| {
            let avail = (w - 2.0 * PADDING_X).max(0.0);
            ((avail / char_width.max(1.0)).floor() as usize).max(1)
        })
        .collect();

    let mut col_x = Vec::with_capacity(n_cols);
    let mut cur_x = 0.0f32;
    for &col_w in &col_widths {
        col_x.push(cur_x);
        cur_x += col_w;
    }

    TableLayout {
        first_line: block.first_line,
        col_widths,
        col_x,
        total_width: cur_x,
        aligns: block.aligns.clone(),
        col_chars,
        font_size,
        line_h,
    }
}

pub(crate) fn shape_table_row(
    text_system: &gpui::WindowTextSystem,
    line: &str,
    block: &crate::table::TableBlock,
    line_idx: usize,
    cursor_col: Option<usize>,
    table_layout: &TableLayout,
    headings: &crate::settings::HeadingsSettings,
    font_family: &SharedString,
    cell_w: f32,
) -> RowShapedLayout {
    const PADDING_X: f32 = 8.0;
    crate::perf::PERF
        .shape_calls
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if line_idx == block.sep_line() {
        let dummy_text: SharedString = "".into();
        let dummy_layout = text_system.layout_line("", px(table_layout.font_size), &[], None);
        let table_row = Arc::new(TableRowLayout {
            is_delimiter: true,
            cells: Vec::new(),
            total_width: table_layout.total_width,
            total_columns: 0,
            line_chars: vec![0, line.chars().count()],
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
            line_h: table_layout.line_h,
        });
        return RowShapedLayout {
            layout: dummy_layout,
            text: dummy_text,
            line_chars: Arc::new(vec![0, line.chars().count()]),
            code_ranges: Arc::new(Vec::new()),
            tag_ranges: Arc::new(Vec::new()),
            grammar_ranges: Arc::new(Vec::new()),
            cell_w,
            table_row: Some(table_row),
        };
    }

    let raw_cells = crate::table::split_row(line);
    let is_header = line_idx == block.first_line;
    let n_cols = table_layout.col_widths.len();
    let active_cell_idx = cursor_col.map(|cc| crate::table::cell_index_at(&raw_cells, cc));

    let mut cells = Vec::with_capacity(n_cols);
    let mut flat_line_chars = Vec::new();
    let mut flat_code_ranges = Vec::new();
    let mut flat_tag_ranges = Vec::new();
    let mut cur_col_offset = 0;

    for c in 0..n_cols {
        let (content_range, outer_range) = if let Some(rc) = raw_cells.get(c) {
            (rc.content.clone(), rc.outer.clone())
        } else {
            let len = line.chars().count();
            (len..len, len..len)
        };

        let is_active_cell = active_cell_idx == Some(c);
        let reveal = if is_active_cell {
            crate::markdown::InlineReveal::All
        } else {
            crate::markdown::InlineReveal::None
        };
        let v = crate::markdown::VisualRow {
            line_idx,
            sub_idx: 0,
            char_start: content_range.start,
            char_end: content_range.end,
            is_last_sub: true,
        };
        let painted = crate::markdown::row_painted_text(
            line,
            &crate::markdown::LineKind::Paragraph(Default::default()),
            &v,
            is_active_cell,
            &reveal,
            headings,
        );

        // Wrap the cell by the painted column budget of its column. The
        // budget comes from the same `TableLayout` the grid is painted with,
        // so the wrap the row height was computed from is the wrap shown here.
        let wrap: Vec<crate::markdown::CellWrapLine> = match table_layout.col_chars.get(c) {
            Some(&max_cols) => {
                crate::markdown::wrap_painted_cell(&painted, content_range.end, max_cols)
            }
            None => vec![crate::markdown::CellWrapLine {
                painted_start: 0,
                painted_end: painted.columns(),
                raw_start: content_range.start,
                raw_end: content_range.end,
            }],
        };

        let painted_text: String = painted.segments.iter().map(|s| s.text.as_str()).collect();
        let color = rgba(0x000000ff).into();
        let cell_x = table_layout.col_x[c];
        let cell_w_val = table_layout.col_widths[c];
        let avail_w = (cell_w_val - 2.0 * PADDING_X).max(0.0);

        let mut lines: Vec<TableCellLine> = Vec::with_capacity(wrap.len());
        let mut cell_columns = 0usize;
        for wl in &wrap {
            let text: String = painted_text
                .chars()
                .skip(wl.painted_start)
                .take(wl.painted_end.saturating_sub(wl.painted_start))
                .collect();
            let mut runs = Vec::new();
            let mut seg_offset = 0usize;
            for seg in &painted.segments {
                let seg_len = seg.text.chars().count();
                let seg_end = seg_offset + seg_len;
                let s = wl.painted_start.max(seg_offset);
                let e = wl.painted_end.min(seg_end);
                if s < e {
                    let part: String = seg
                        .text
                        .chars()
                        .skip(s - seg_offset)
                        .take(e - s)
                        .collect();
                    if !part.is_empty() {
                        let weight = if seg.code {
                            inline_code_weight(if seg.bold || is_header {
                                gpui::FontWeight::BOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                        } else if seg.bold || is_header {
                            gpui::FontWeight::BOLD
                        } else {
                            gpui::FontWeight::NORMAL
                        };
                        runs.push(gpui::TextRun {
                            len: part.len(),
                            font: gpui::Font {
                                family: font_family.clone(),
                                weight,
                                style: if seg.italic {
                                    gpui::FontStyle::Italic
                                } else {
                                    gpui::FontStyle::Normal
                                },
                                features: Default::default(),
                                fallbacks: None,
                            },
                            color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    }
                }
                seg_offset = seg_end;
                if seg_offset >= wl.painted_end {
                    break;
                }
            }

            let text_shared: SharedString = text.into();
            let line_layout =
                text_system.layout_line(text_shared.as_ref(), px(table_layout.font_size), &runs, None);
            let text_w = f32::from(line_layout.width);
            let extra_space = (avail_w - text_w).max(0.0);
            let align_offset = match table_layout.aligns.get(c).copied().unwrap_or_default() {
                crate::table::Align::None | crate::table::Align::Left => 0.0,
                crate::table::Align::Center => extra_space / 2.0,
                crate::table::Align::Right => extra_space,
            };
            let columns = text_shared.chars().count();
            let line_chars = painted
                .line_chars
                .iter()
                .skip(wl.painted_start)
                .take(wl.painted_end.saturating_sub(wl.painted_start) + 1)
                .copied()
                .collect::<Vec<_>>();
            cell_columns += columns;
            lines.push(TableCellLine {
                text: text_shared,
                layout: line_layout,
                columns,
                line_chars,
                raw_start: wl.raw_start,
                raw_end: wl.raw_end,
                content_x: cell_x + PADDING_X + align_offset,
            });
        }

        // Inline code and tags are painted as chips per sub-line, so their
        // painted offsets are rebased onto the cell's flattened column space.
        let flat_of_painted = |painted_col: usize| -> usize {
            let mut acc = 0usize;
            for line in &lines {
                if painted_col <= acc + line.columns {
                    return acc + (painted_col - acc).min(line.columns);
                }
                acc += line.columns;
            }
            acc
        };
        let cell_code_ranges: Vec<(usize, usize)> = painted
            .code_ranges()
            .into_iter()
            .map(|(s, e)| (flat_of_painted(s), flat_of_painted(e)))
            .collect();
        let cell_tag_ranges: Vec<(usize, usize, String)> = painted
            .tag_ranges()
            .into_iter()
            .map(|(s, e, tag)| (flat_of_painted(s), flat_of_painted(e), tag))
            .collect();

        for &(cs, ce) in &cell_code_ranges {
            flat_code_ranges.push((cur_col_offset + cs, cur_col_offset + ce));
        }
        for (cs, ce, tag) in &cell_tag_ranges {
            flat_tag_ranges.push((cur_col_offset + cs, cur_col_offset + ce, tag.clone()));
        }

        for line in &lines {
            for &lc in line.line_chars.iter().take(line.columns) {
                flat_line_chars.push(lc);
            }
        }
        flat_line_chars.push(content_range.end);

        cells.push(TableCellLayout {
            col_idx: c,
            cell_x,
            cell_w: cell_w_val,
            cell_start_col: cur_col_offset,
            cell_columns,
            content_range,
            outer_range,
            lines,
            code_ranges: cell_code_ranges,
            tag_ranges: cell_tag_ranges,
        });

        cur_col_offset += cell_columns + 1;
    }

    let dummy_layout = text_system.layout_line("", px(table_layout.font_size), &[], None);
    let table_row = Arc::new(TableRowLayout {
        is_delimiter: false,
        cells,
        total_width: table_layout.total_width,
        total_columns: cur_col_offset,
        line_chars: flat_line_chars.clone(),
        code_ranges: flat_code_ranges.clone(),
        tag_ranges: flat_tag_ranges.clone(),
        line_h: table_layout.line_h,
    });

    RowShapedLayout {
        layout: dummy_layout,
        text: "".into(),
        line_chars: Arc::new(flat_line_chars),
        code_ranges: Arc::new(flat_code_ranges),
        tag_ranges: Arc::new(flat_tag_ranges),
        grammar_ranges: Arc::new(Vec::new()),
        cell_w,
        table_row: Some(table_row),
    }
}
