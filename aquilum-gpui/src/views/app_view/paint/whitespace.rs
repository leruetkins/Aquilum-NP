use super::super::RowShapedLayout;

/// Returns char-index ranges `[start, end)` of spaces that show whitespace dots in `line`:
/// 1. Any leading space at the start of the line (even a single space).
/// 2. Any run of 2 or more consecutive spaces anywhere in the line.
/// 3. Any trailing space at the end of the line (even a single space).
#[allow(dead_code)]
pub(crate) fn whitespace_dot_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
    whitespace_dot_ranges_with_mode(line, false)
}

pub(crate) fn whitespace_dot_ranges_with_mode(line: &str, show_all: bool) -> Vec<std::ops::Range<usize>> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut ranges = Vec::new();
    if show_all {
        for (i, ch) in line.chars().enumerate() {
            if ch == ' ' {
                ranges.push(i..i + 1);
            }
        }
        return ranges;
    }
    let mut run_start: Option<usize> = None;
    let mut count = 0;
    for (i, ch) in line.chars().enumerate() {
        if ch == ' ' {
            if run_start.is_none() {
                run_start = Some(i);
            }
            count += 1;
        } else {
            // Leading spaces (start == 0) and runs of >= 2 spaces anywhere show dots.
            if count >= 2 || (run_start == Some(0) && count >= 1) {
                if let Some(start) = run_start {
                    ranges.push(start..i);
                }
            }
            run_start = None;
            count = 0;
        }
    }
    // Trailing spaces at the end of the line are visible even if count == 1.
    if count >= 1 {
        if let Some(start) = run_start {
            ranges.push(start..start + count);
        }
    }
    ranges
}

/// Computes quads `(x, y, width, height, corner_radius)` for faint dots
/// centered in each space of consecutive space runs (>= 2 spaces in a row)
/// or trailing spaces at the end of the line.
///
/// If `preview_selection` is `Some((sel_x, sel_w))`, dots that fall inside
/// that horizontal selection range are omitted.
///
/// When `show_all` is true (non-printable characters mode), every space is marked.
///
/// `top_offset` is the distance from the row's top to the box the text is
/// painted in: a grid table keeps `TABLE_EDGE_MARGIN` above its header row, so
/// its cells start that much lower than a plain text row.
pub(crate) fn whitespace_dot_quads(
    line: &str,
    row_layout: &RowShapedLayout,
    row_h: f32,
    scale: f32,
    guide_columns: &[f32],
    preview_selection: Option<(f32, f32)>,
    show_all: bool,
    top_offset: f32,
) -> Vec<(f32, f32, f32, f32, f32)> {
    let ranges = whitespace_dot_ranges_with_mode(line, show_all);
    if ranges.is_empty() {
        return Vec::new();
    }

    let dot_device_px = (2.0 * scale).round().max(2.0);
    let dot_size = if scale > 0.0 { dot_device_px / scale } else { 2.0 };
    let radius = dot_size / 2.0;

    let snap = |v: f32| {
        if scale > 0.0 {
            (v * scale).round() / scale
        } else {
            v
        }
    };

    let mut quads = Vec::new();
    let push_dot = |quads: &mut Vec<(f32, f32, f32, f32, f32)>, center_x: f32, center_y: f32| {
        // Skip dots that fall inside an active preview selection rectangle.
        if let Some((sel_x, sel_w)) = preview_selection {
            if sel_w > 0.0 {
                let sel_right = sel_x + sel_w;
                if center_x >= sel_x - 0.5 && center_x <= sel_right + 0.5 {
                    return;
                }
            }
        }
        let left = snap(center_x - radius);
        let top = snap(center_y - radius);
        quads.push((left, top, dot_size, dot_size, radius));
    };

    // A table row paints its cells as stacked sub-lines, so the dot of a space
    // sits in the sub-line that holds it, at the cell's own glyph position.
    if let Some(table_row) = &row_layout.table_row {
        for cell in &table_row.cells {
            for (line_idx, sub) in cell.lines.iter().enumerate() {
                let center_y = top_offset
                    + crate::markdown::TABLE_PAD_Y
                    + line_idx as f32 * table_row.line_h
                    + table_row.line_h / 2.0;
                let columns = sub.columns.min(sub.line_chars.len().saturating_sub(1));
                for (col, ch) in sub.text.chars().enumerate().take(columns) {
                    if ch != ' ' {
                        continue;
                    }
                    let lc = sub.line_chars[col];
                    let is_synthetic = (col + 1 < sub.line_chars.len()
                        && sub.line_chars[col + 1] == lc)
                        || (col > 0 && sub.line_chars[col - 1] == lc);
                    if is_synthetic {
                        continue;
                    }
                    if !ranges.iter().any(|r| r.contains(&lc)) {
                        continue;
                    }
                    let x0 = sub.glyph_x(col);
                    let x1 = sub.glyph_x(col + 1);
                    if x1 <= x0 {
                        continue;
                    }
                    push_dot(&mut quads, (x0 + x1) / 2.0, center_y);
                }
            }
        }
        return quads;
    }

    let center_y = row_h / 2.0;
    let columns = row_layout.columns();
    let line_chars = &row_layout.line_chars;

    for (col, ch) in row_layout.text.chars().enumerate().take(columns) {
        if ch != ' ' {
            continue;
        }
        if col >= line_chars.len() {
            break;
        }
        let lc = line_chars[col];

        // Skip synthetic continuation padding where line_chars repeats the same source index.
        let is_synthetic = (col + 1 < line_chars.len() && line_chars[col + 1] == lc)
            || (col > 0 && line_chars[col - 1] == lc);
        if is_synthetic {
            continue;
        }

        // Skip spaces that fall under list indent guide lines.
        if guide_columns.iter().any(|&g| g >= 0.0 && (g.floor() as usize) == col) {
            continue;
        }

        if !ranges.iter().any(|r| r.contains(&lc)) {
            continue;
        }

        let x0 = row_layout.x_for_column(col);
        let x1 = row_layout.x_for_column(col + 1);
        if x1 <= x0 {
            continue;
        }

        push_dot(&mut quads, (x0 + x1) / 2.0, center_y);
    }

    quads
}
