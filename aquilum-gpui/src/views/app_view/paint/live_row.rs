use super::super::*;
use super::syntax::*;

/// The icon a callout header shows for its family, matching Obsidian's set where
/// the app already ships a glyph for it.
pub(crate) fn callout_icon(kind: crate::markdown::CalloutKind) -> &'static str {
    match kind {
        crate::markdown::CalloutKind::Note => icons::ICON_NOTE_CREATE,
        crate::markdown::CalloutKind::Info => icons::ICON_INFO,
        crate::markdown::CalloutKind::Tip => icons::ICON_BOLT,
        crate::markdown::CalloutKind::Warning => icons::ICON_TRIANGLE_ALERT,
        crate::markdown::CalloutKind::Danger => icons::ICON_CIRCLE_ALERT,
        crate::markdown::CalloutKind::Quote => icons::ICON_QUOTE,
    }
}

pub(crate) fn render_live_styled_row(
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    char_start: usize,
    char_end: usize,
    line_idx: usize,
    sub_idx: usize,
    is_last_sub: bool,
    is_active_line: bool,
    reveal: &crate::markdown::InlineReveal,
    show_fences: bool,
    headings: &crate::settings::HeadingsSettings,
    theme: &Theme,
    font_family: &gpui::SharedString,
    custom_ui_node: Option<&crate::plugins::ui::UiNode>,
    custom_plugin_id: Option<&str>,
    is_first_code_line: bool,
    active_plugin_input: Option<&crate::plugins::ui::PluginInputFocusState>,
    plugin_input_focus_handle: &FocusHandle,
    active_plugin_select: Option<&(String, u64, String)>,
    plugin_select_popup: Option<&Entity<crate::views::tab_list::TabListPopup>>,
    plugin_input_blink_visible: bool,
    table_render: Option<(&crate::table::TableBlock, &TableLayout, Option<usize>, usize, f32)>,
    ctrl_held: bool,
    hovered_link_id: Option<&str>,
    window: &Window,
    cx: &Context<AppView>,
    // `cell_w` is the width of one character cell of this row in logical px;
    // `guide_columns` is the list guide columns for this row, in cells
    // (see `markdown::list_block_guide_columns`). Only the block decorations
    // drawn outside the text flow — the indent guides of a nested list, the icon
    // of a callout header — need them.
    cell_w: f32,
    guide_columns: &[f32],
    // The callout block this row belongs to, body rows included; the header row
    // itself keeps its own marker.
    callout_block: Option<crate::markdown::CalloutBlock>,
    // Interface language for the callout type names a header shows.
    lang: &str,
) -> gpui::Div {
    if let Some((tbl, table_layout, cursor_col, line_idx, row_h)) = table_render {
        if line_idx == tbl.sep_line() {
            return div()
                .w(px(table_layout.total_width))
                .h(px(1.0))
                .bg(theme.border_subtle);
        }

        let raw_cells = crate::table::split_row(line);
        let is_header = line_idx == tbl.first_line;
        let is_last_row = line_idx == tbl.last_line && line_idx > tbl.sep_line();
        let active_c = cursor_col.map(|cc| crate::table::cell_index_at(&raw_cells, cc));
        // Cell ranges are in chars; map them to byte offsets once per row instead
        // of re-walking the line from the start for every cell.
        let mut char_byte_offsets: Vec<usize> = line.char_indices().map(|(b, _)| b).collect();
        char_byte_offsets.push(line.len());
        // The row height is the wrapped height of its tallest cell plus the
        // edge margin the table keeps above the header and below the last row.
        let edge_margin = if is_header || is_last_row {
            crate::markdown::TABLE_EDGE_MARGIN
        } else {
            0.0
        };
        let content_h = (row_h - edge_margin).max(table_layout.line_h);
        let mut row_div = div().flex().flex_row().items_center()
            .when(is_header, |d| d.h(px(content_h)).mt(px(crate::markdown::TABLE_EDGE_MARGIN)))
            .when(is_last_row, |d| d.h(px(content_h)).mb(px(crate::markdown::TABLE_EDGE_MARGIN)))
            .when(!is_header && !is_last_row, |d| d.h(px(content_h)));
        for (c_idx, &col_w) in table_layout.col_widths.iter().enumerate() {
            let (content_range, outer_range) = match raw_cells.get(c_idx) {
                Some(c) => (c.content.clone(), c.outer.clone()),
                None => {
                    let len = line.chars().count();
                    (len..len, len..len)
                }
            };
            let is_active_cell = active_c == Some(c_idx);
            let content_range = if is_active_cell {
                cell_content_range_with_caret(content_range, outer_range, cursor_col)
            } else {
                content_range
            };
            let cell_text = char_byte_offsets
                .get(content_range.start)
                .zip(char_byte_offsets.get(content_range.end))
                .and_then(|(&start, &end)| line.get(start..end))
                .unwrap_or("");
            // The active table cell reveals its markup as a whole: `shape_table_row`
            // shapes the same cell from the raw line, and both paths must paint the
            // same characters or the caret and selection rectangles drift.
            let cell_reveal = if is_active_cell {
                crate::markdown::InlineReveal::All
            } else {
                crate::markdown::InlineReveal::None
            };
            let raw_spans = crate::markdown::parse_inline_syntax_spans(cell_text);
            let spans = crate::markdown::slice_syntax_spans_revealed(
                &raw_spans,
                0,
                cell_text.chars().count(),
                0,
                &cell_reveal,
            );
            // The cell is wrapped by the same column budget `shape_table_row`
            // used to shape it, so its painted lines match the row height.
            let wrap = match table_layout.col_chars.get(c_idx) {
                Some(&max_cols) => crate::markdown::table_cell_wrap_lines(
                    line,
                    content_range.clone(),
                    is_active_cell,
                    &cell_reveal,
                    headings,
                    max_cols,
                ),
                None => vec![crate::markdown::CellWrapLine {
                    painted_start: 0,
                    painted_end: cell_text.chars().count(),
                    raw_start: content_range.start,
                    raw_end: content_range.end,
                }],
            };
            let align = table_layout.aligns.get(c_idx).copied().unwrap_or_default();
            let font_weight = if is_header { Some(gpui::FontWeight::BOLD) } else { None };
            let is_first_col = c_idx == 0;
            let is_last_col = c_idx + 1 == table_layout.col_widths.len();

            let tbl_block = tbl.clone();
            let row_idx = if is_header { 0 } else { tbl.row_of_line(line_idx).unwrap_or(0) };
            let cell_start_col = raw_cells.get(c_idx).map(|c| c.content.start).unwrap_or(0);

            let tbl_first_line = tbl.first_line;
            let mut cell_div = div()
                .id(gpui::ElementId::Name(format!("tbl-cell-{}-{}-{}", tbl.first_line, line_idx, c_idx).into()))
                .relative()
                .w(px(col_w))
                .h_full()
                .flex()
                .flex_col()
                .justify_start()
                .pt(px(crate::markdown::TABLE_PAD_Y))
                .px_2()
                .border_l_1()
                .border_color(theme.border_subtle)
                .when(is_last_col, |d| d.border_r_1().border_color(theme.border_subtle))
                .when(is_header, |d| d.border_t_1().border_color(theme.border_subtle))
                .when(!is_header, |d| d.border_b_1().border_color(theme.border_subtle))
                .when(is_header, |d| {
                    let mut bg = theme.text_muted;
                    bg.a = 0.08;
                    d.bg(bg)
                })
                .when(is_header && is_first_col, |d| d.rounded_tl_sm())
                .when(is_header && is_last_col, |d| d.rounded_tr_sm())
                .when(is_last_row && is_first_col, |d| d.rounded_bl_sm())
                .when(is_last_row && is_last_col, |d| d.rounded_br_sm())
                .cursor(gpui::CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.focus_handle);
                    this.state.active_set_cursor_line_col(line_idx, cell_start_col);
                    let click_x: f32 = event.position.x.into();
                    let click_y: f32 = event.position.y.into();
                    this.open_table_context_menu(tbl_block.clone(), row_idx, c_idx, click_x, click_y, cx);
                }));
            let cell_key = format!("tbl-cell-{}-{}-{}", line_idx, c_idx, sub_idx);
            let mut lines_col = div().w_full().flex().flex_col();
            for (line_no, wl) in wrap.iter().enumerate() {
                let s = wl.raw_start.saturating_sub(content_range.start);
                let e = wl.raw_end.saturating_sub(content_range.start);
                let line_spans = crate::markdown::slice_visible_spans(&spans, s, e);
                let mut line_div = div()
                    .w_full()
                    .h(px(table_layout.line_h))
                    .line_height(px(table_layout.line_h))
                    .flex()
                    .flex_row()
                    .items_center()
                    .whitespace_nowrap();
                line_div = match align {
                    crate::table::Align::None | crate::table::Align::Left => line_div.justify_start(),
                    crate::table::Align::Center => line_div.justify_center(),
                    crate::table::Align::Right => line_div.justify_end(),
                };
                line_div = line_div.child(render_syntax_spans(
                    line_spans,
                    theme,
                    None,
                    font_weight,
                    None,
                    font_family,
                    &format!("{}-l{}", cell_key, line_no),
                    ctrl_held,
                    hovered_link_id,
                    window.scale_factor(),
                    true,
                    cx,
                ));
                lines_col = lines_col.child(line_div);
            }
            cell_div = cell_div.child(lines_col);

            let col_widths_clone = table_layout.col_widths.clone();
            cell_div = cell_div.child(
                div()
                    .id(gpui::ElementId::Name(format!("tbl-rsz-{}-{}-{}", tbl_first_line, line_idx, c_idx).into()))
                    .absolute()
                    .right(px(-4.0))
                    .top_0()
                    .bottom_0()
                    .w(px(8.0))
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        cx.stop_propagation();
                        let click_x: f32 = event.position.x.into();
                        this.start_table_col_resize(tbl_first_line, c_idx, click_x, &col_widths_clone);
                        cx.notify();
                    }))
            );

            row_div = row_div.child(cell_div);
        }
        return row_div;
    }

    if line.is_empty() || char_start >= char_end {
        return div().h_full().flex().items_center().child(" ");
    }

    let row_key = format!("row-{}-{}", line_idx, sub_idx);
    let scale = window.scale_factor();
    let render_spans = |spans: Vec<crate::markdown::VisibleSpan>, text_color: Option<gpui::Rgba>, weight: Option<gpui::FontWeight>, style: Option<gpui::FontStyle>| {
        render_syntax_spans(
            spans,
            theme,
            text_color,
            weight,
            style,
            font_family,
            &row_key,
            ctrl_held,
            hovered_link_id,
            scale,
            false,
            cx,
        )
    };

    // One vertical guide per nesting level of a list row, hanging straight under
    // the marker of the level it belongs to. Rows are stacked edge to edge, so
    // the segments join into a continuous line down the group. Both the position
    // and the width come from the device grid: the guide has to keep one
    // thickness at every level, and a single device pixel reads as too faint
    // against the editor background, so it is drawn two device pixels wide.
    let indent_guides = || -> Vec<gpui::Div> {
        let scale = window.scale_factor();
        guide_columns
            .iter()
            .map(|&col| {
                // `col` is the centre of the marker cell; the guide is shifted
                // half its own width to the left so it stays centred under the
                // marker instead of growing to the right.
                let (left, width) = crate::snap::snapped_rule(col * cell_w - 1.0 / scale, 2.0, scale);
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .w(px(width))
                    .bg(theme.border_subtle)
            })
            .collect()
    };

    match classified {
        crate::markdown::LineKind::Heading1(_) => {
            let rest = line.strip_prefix("# ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h1.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h1.color_name);
            let h_weight = if headings.h1.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h1.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(2), 2, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("# "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                    2,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading2(_) => {
            let rest = line.strip_prefix("## ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h2.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h2.color_name);
            let h_weight = if headings.h2.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h2.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(3), 3, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("## "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(3),
                    char_end.saturating_sub(3),
                    3,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading3(_) => {
            let rest = line.strip_prefix("### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h3.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h3.color_name);
            let h_weight = if headings.h3.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h3.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(4), 4, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(4),
                    char_end.saturating_sub(4),
                    4,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading4(_) => {
            let rest = line.strip_prefix("#### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h4.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h4.color_name);
            let h_weight = if headings.h4.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h4.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(5), 5, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("#### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(5),
                    char_end.saturating_sub(5),
                    5,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading5(_) => {
            let rest = line.strip_prefix("##### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h5.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h5.color_name);
            let h_weight = if headings.h5.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h5.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(6), 6, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("##### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(6),
                    char_end.saturating_sub(6),
                    6,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading6(_) => {
            let rest = line.strip_prefix("###### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h6.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h6.color_name);
            let h_weight = if headings.h6.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h6.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(7), 7, reveal);
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("###### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(7),
                    char_end.saturating_sub(7),
                    7,
                    reveal,
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Quote(_) => {
            // Markers hidden by default, revealed on the line being edited — the
            // same model headings and list bullets follow. Both spellings are
            // exactly as wide as the markers, so nothing moves when they appear.
            let callout = crate::markdown::callout_prefix(line);
            // A callout claims the whole marker region — the quote prefix plus
            // `[!type] ` — and paints its type there instead; what follows it is the
            // title rather than quoted prose. The region keeps its source width, so
            // the title stays in its column and the header does not shift when the
            // caret reveals the raw marker.
            let prefix = crate::markdown::quote_prefix(line)
                .map(|q| &line[..q.bytes])
                .unwrap_or("");
            let prefix_chars = prefix.chars().count();
            let marker = callout.map_or(prefix, |c| &line[..c.bytes]);
            let marker_chars = callout.map_or(prefix_chars, |c| c.chars);
            let rest = callout.map_or(&line[marker.len()..], |c| c.title);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let prefix_text = if is_active_line && sub_idx == 0 {
                marker.to_string()
            } else if callout.is_some() && sub_idx > 0 {
                // A wrapped header continues under its title, past the icon column.
                // The header's first row draws that column itself below, so only a
                // wrapped row adds it to the indent.
                " ".repeat(prefix_chars + crate::markdown::CALLOUT_ICON_CELLS)
            } else {
                " ".repeat(prefix_chars)
            };
            // One bar per quote level, drawn where that level's `>` sits in the
            // source. Rows stack edge to edge, so the segments join into a
            // continuous bar down the group. The edges are snapped to the device
            // grid like the caret, and the width is fixed in device pixels: a
            // 2-logical-pixel bar rounded on both edges comes out thinner in some
            // columns than in others on a fractional scale, which made deeper
            // levels look heavier than shallow ones. Three device pixels at every
            // level is what reads as a quote bar rather than a hairline.
            let scale = window.scale_factor();
            // A top-level callout paints its bar in the family colour and tints the
            // rows around it, so the bar reads as the block's edge. Its bar is as
            // wide as the block's corner radius, so at the block's ends the corner
            // it takes follows the block's own arc exactly instead of showing a
            // step where a narrower stripe would.
            let bar_color = callout_block
                .map_or(theme.border, |block| crate::markdown::callout_color(block.kind, theme.is_dark));
            let bar_device_w = if callout_block.is_some() {
                (crate::markdown::CALLOUT_BAR_WIDTH * scale).round().max(3.0)
            } else {
                3.0
            };
            let bar_cols = crate::markdown::quote_bar_columns(prefix);
            // A callout's bar is the outermost quote level it opens; any deeper
            // level on the same row is a nested quote inside it and keeps an
            // ordinary stroke.
            let callout_col = callout_block.and_then(|_| bar_cols.first().copied());
            let bar_top_rounded = callout_block.map_or(false, |block| block.first) && sub_idx == 0;
            let bar_bottom_rounded = callout_block.map_or(false, |block| block.last) && is_last_sub;
            // The line being edited reveals its raw `>` markers; the wide callout
            // bar would sit on top of them and hide them, so it steps aside while
            // that line is active and comes back when the caret leaves.
            let bars = if callout_block.is_some() && is_active_line {
                Vec::new()
            } else {
                bar_cols
                    .into_iter()
                    .map(|col| {
                        let edge = callout_col == Some(col);
                        let width = if edge { bar_device_w } else { 3.0 };
                        let (left, width) = crate::snap::snapped_rule(col * cell_w, width, scale);
                        let mut bar = div()
                            .absolute()
                            .left(px(left))
                            .top(px(0.0))
                            .bottom(px(0.0))
                            .w(px(width))
                            .bg(if edge { bar_color } else { theme.border });
                        if edge {
                            if bar_top_rounded {
                                bar = bar.rounded_tl_md();
                            }
                            if bar_bottom_rounded {
                                bar = bar.rounded_bl_md();
                            }
                        }
                        bar
                    })
                    .collect()
            };
            // The markers are block syntax, not quoted prose: they stay upright and
            // muted, the way headings reveal their `#`. Inheriting the row's italic
            // would slant them, which reads as a clipped, oddly narrow glyph — the
            // `>` ink leans away from the cell and from its own bar.
            let marker_color = theme.text_muted;
            let content = |row_spans: Vec<crate::markdown::VisibleSpan>| match callout {
                Some(c) => {
                    let color = crate::markdown::callout_color(c.kind, theme.is_dark);
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(color)
                        .child(render_spans(row_spans, Some(color), Some(gpui::FontWeight::BOLD), Some(gpui::FontStyle::Normal)))
                }
                None => div()
                    .text_color(theme.text_secondary)
                    // Quoted prose is upright; only the spans that are italic in the
                    // source lean. `markdown::row_painted_text` measures the row the
                    // same way, so the chip, the caret and the selection stay on the
                    // glyphs.
                    .child(render_spans(row_spans, Some(theme.text_secondary), None, None)),
            };
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(marker_chars), marker_chars, reveal);
                let mut row = div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(bars)
                    .child(div().text_color(marker_color).child(prefix_text));
                if let (Some(c), false) = (callout, is_active_line) {
                    let color = crate::markdown::callout_color(c.kind, theme.is_dark);
                    // The column is wider than the glyph, and the spare cell is the
                    // gap before the text; the glyph keeps the size it had.
                    let column_w = cell_w * crate::markdown::CALLOUT_ICON_CELLS as f32;
                    let glyph_w = cell_w * 1.6;
                    row = row.child(
                        div()
                            .w(px(column_w))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(icons::icon(callout_icon(c.kind), glyph_w, color)),
                    );
                    // A titled callout is its own heading: the type name is not
                    // repeated, the title follows the icon. Only an untitled header
                    // shows the localized type in place of the marker.
                    if rest.is_empty() {
                        row = row.child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(color)
                                .child(crate::markdown::callout_label(&c, lang)),
                        );
                    }
                }
                // An untitled header has nothing to paint after the label; letting
                // `render_syntax_spans` see an empty span list would add its blank
                // cell and put the row a cell past its painted width.
                if rest.is_empty() {
                    row
                } else {
                    row.child(content(row_spans))
                }
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(marker_chars),
                    char_end.saturating_sub(marker_chars),
                    marker_chars,
                    reveal,
                );
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(bars)
                    .child(div().child(prefix_text))
                    .child(content(row_spans))
            }
        }
        crate::markdown::LineKind::Task { checked, .. } => {
            let task = crate::markdown::task_prefix(line);
            let prefix_bytes = task.as_ref().map_or(0, |task| task.bytes);
            let prefix_chars = task.as_ref().map_or(0, |task| task.chars);
            let indent_bytes = task.as_ref().map_or(0, |task| task.indent_bytes);
            let indent_chars = task.as_ref().map_or(0, |task| task.indent_chars);
            let list_marker = task.as_ref().map_or("", |task| task.list_marker);
            let list_chars = task.as_ref().map_or(0, |task| task.list_chars());
            let box_chars = task.as_ref().map_or(0, |task| task.checkbox.chars().count());
            let ordered = task.as_ref().map_or(false, |task| task.ordered);
            let space_after = task.as_ref().map_or(false, |task| task.space_after);
            let indent = &line[..indent_bytes];
            let rest = if line.len() >= prefix_bytes { &line[prefix_bytes..] } else { "" };
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let task_color = if *checked { Some(theme.text_muted) } else { None };
            let guides = indent_guides();

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(prefix_chars), prefix_chars, reveal);
                // The caret or the selection sitting on the marker reveals its
                // raw source, the way a heading does. Every other row spends the
                // same cells on a real checkbox: the raw `[x]` would only be
                // text.
                let marker = if reveal.shows(0, prefix_chars) {
                    div()
                        .flex_shrink_0()
                        .w(px((prefix_chars - indent_chars) as f32 * cell_w))
                        .whitespace_nowrap()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(if *checked { theme.success } else { theme.text_muted })
                        .child(line[indent_bytes.min(prefix_bytes)..prefix_bytes].to_string())
                } else {
                    let box_size = (cell_w * 2.0).clamp(12.0, 18.0);
                    div()
                        .flex_shrink_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .w(px((prefix_chars - indent_chars) as f32 * cell_w))
                        .whitespace_nowrap()
                        // An unordered item keeps the dot the list paints in
                        // place of its raw marker; a numbered one is still the
                        // row's marker.
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(list_chars as f32 * cell_w))
                                .when(ordered, |d| {
                                    d.font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.accent)
                                        .child(list_marker.to_string())
                                })
                                .when(!ordered, |d| {
                                    d.child(
                                        div()
                                            .w(px(cell_w))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .w(px(cell_w * 0.6))
                                                    .h(px(cell_w * 0.6))
                                                    .rounded_full()
                                                    .bg(theme.accent),
                                            ),
                                    )
                                    .when(list_chars > 1, |d| {
                                        d.child(div().w(px((list_chars as f32 - 1.0) * cell_w)))
                                    })
                                }),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(box_chars as f32 * cell_w))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .child(
                                    div()
                                        .w(px(box_size))
                                        .h(px(box_size))
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(if *checked { theme.accent } else { theme.border })
                                        .bg(if *checked { theme.accent } else { theme.bg_hover })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(*checked, |d| {
                                            d.child(icons::icon(icons::ICON_CHECK, box_size * 0.62, theme.text_on_accent))
                                        }),
                                ),
                        )
                        .when(space_after, |d| d.child(div().flex_shrink_0().w(px(cell_w))))
                };
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(div().flex_shrink_0().w(px(indent_chars as f32 * cell_w)).child(indent.to_string()))
                    .child(marker)
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_spans(row_spans, task_color, None, None)),
                    )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(prefix_chars),
                    char_end.saturating_sub(prefix_chars),
                    prefix_chars,
                    reveal,
                );
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(prefix_chars as f32 * cell_w))
                            .whitespace_nowrap()
                            .child(" ".repeat(prefix_chars)),
                    )
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_spans(row_spans, task_color, None, None)),
                    )
            }
        }
        crate::markdown::LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let (bullet, rest) = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                (&trimmed[..2], &trimmed[2..])
            } else {
                ("", trimmed)
            };
            let prefix_len = indent_len + bullet.len();
            // Same marker text the wrap geometry measured: the rendered dot for an
            // idle line, the raw source marker on the line being edited.
            let bullet_text = crate::markdown::bullet_paint_text(bullet, is_active_line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let guides = indent_guides();
            // An idle row draws the dot itself, sized in cells so it stays centred
            // in its marker cell; the line being edited shows the raw source marker.
            let shows_dot = bullet_text != bullet;

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(prefix_len), prefix_len, reveal);
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(indent.chars().count() as f32 * cell_w))
                            .child(indent.to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .w(px(2.0 * cell_w))
                            .when(shows_dot, |d| {
                                d.child(
                                    div()
                                        .w(px(cell_w))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            div()
                                                .w(px(cell_w * 0.6))
                                                .h(px(cell_w * 0.6))
                                                .rounded_full()
                                                .bg(theme.accent),
                                        ),
                                )
                                .child(div().w(px(cell_w)))
                            })
                            .when(!shows_dot, |d| {
                                d.whitespace_nowrap()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.accent)
                                    .child(bullet_text.to_string())
                            }),
                    )
                    .child(render_spans(row_spans, None, None, None))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                    prefix_len,
                    reveal,
                );
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(div().child(format!("{}  ", indent)))
                    .child(render_spans(row_spans, None, None, None))
            }
        }
        crate::markdown::LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let indent_chars = indent.chars().count();
            let num_prefix_len = num.len() + 2;
            let (num_part, rest) = if trimmed.len() >= num_prefix_len {
                (&trimmed[..num_prefix_len], &trimmed[num_prefix_len..])
            } else {
                ("", trimmed)
            };
            let num_chars = num_part.chars().count();
            let prefix_chars = indent_chars + num_chars;
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let guides = indent_guides();

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, 0, char_end.saturating_sub(prefix_chars), prefix_chars, reveal);
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(div().flex_shrink_0().w(px(indent_chars as f32 * cell_w)).child(indent.to_string()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(num_chars as f32 * cell_w))
                            .whitespace_nowrap()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.accent)
                            .child(num_part.to_string()),
                    )
                    .child(render_spans(row_spans, None, None, None))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans_revealed(
                    &spans,
                    char_start.saturating_sub(prefix_chars),
                    char_end.saturating_sub(prefix_chars),
                    prefix_chars,
                    reveal,
                );
                let spaces = " ".repeat(prefix_chars);
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .children(guides)
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(prefix_chars as f32 * cell_w))
                            .whitespace_nowrap()
                            .child(spaces),
                    )
                    .child(render_spans(row_spans, None, None, None))
            }
        }
        crate::markdown::LineKind::HorizontalRule => {
            if is_active_line {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                // A rule is the thinnest thing the document draws, so it is the one
                // most easily lost. Two whole device pixels of the quiet grey overlay
                // give it the weight a rule has in the rest of the markdown world, in
                // a neutral grey instead of the muted foreground. The width comes
                // from the row content wrapper, which the caller gives a definite
                // width for exactly this reason: a rule has no text of its own to
                // size a shrink-to-fit box with.
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .h(px(crate::snap::device_thickness(2.0, window.scale_factor())))
                            .bg(theme.horizontal_rule()),
                    )
            }
        }
        crate::markdown::LineKind::CodeFence { is_opening, lang } => {
            if *is_opening {
                if show_fences || is_active_line {
                    // Editing the block: paint the raw fence so the caret and the
                    // backticks line up with the source.
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h_full()
                        .text_color(theme.text_muted)
                        .child(line.to_string())
                } else {
                    // Collapsed: the language label stands in for the fence, and the row
                    // loop overlays the copy button at the right end of this same row.
                    let lang_label = lang.unwrap_or("CODE").to_uppercase();
                    let mut header = div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h_full()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child(lang_label),
                        );
                    if let Some(pid) = custom_plugin_id {
                        header = header.child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text_muted)
                                .child(format!("[{}]", pid)),
                        );
                    }
                    header.child(icons::icon(icons::ICON_CODE, 12.0, theme.text_muted))
                }
            } else if show_fences || is_active_line {
                // Editing the block: paint the raw closing fence.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                // Collapsed: the fence is hidden. The row stays empty and only carries
                // the block's background and its rounded bottom corners, so the panel
                // still ends in a visible edge.
                div().h_full()
            }
        }
        crate::markdown::LineKind::CodeLine(_) => {
            if !show_fences && !is_active_line && custom_ui_node.is_some() {
                if is_first_code_line {
                    let tree = custom_ui_node.unwrap();
                    let pid = custom_plugin_id.unwrap_or("plugin");
                    div()
                        .flex()
                        .flex_col()
                        .child(crate::plugins::ui::render_ui_node(
                            tree,
                            pid,
                            0,
                            theme,
                            active_plugin_input,
                            plugin_input_focus_handle,
                            active_plugin_select,
                            plugin_select_popup,
                            plugin_input_blink_visible,
                            window,
                            cx,
                            None,
                        ))
                } else {
                    div().h_0().overflow_hidden()
                }
            } else {
                let row_slice: String = line.chars().skip(char_start).take(char_end - char_start).collect();
                // Plain foreground: the block background alone marks this as code. The
                // green that used to sit here read as an accent colour, not as text.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_primary)
                    .child(row_slice)
            }
        }
        crate::markdown::LineKind::Paragraph(_) => {
            let spans = crate::markdown::parse_inline_syntax_spans(line);
            let row_spans = crate::markdown::slice_syntax_spans_revealed(&spans, char_start, char_end, 0, reveal);
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(render_spans(row_spans, None, None, None))
        }
    }
}
