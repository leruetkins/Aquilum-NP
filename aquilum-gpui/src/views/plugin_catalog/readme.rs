use super::*;

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;

use crate::markdown::{
    callout_blocks, classify_line, list_block_guide_columns, table_row_wrap_lines,
    visual_row_height, wrap_line_ranges, CodeScanner, InlineReveal, TableRowVisualKind,
    VisualRow, TABLE_EDGE_MARGIN,
};
use crate::table::{split_row, TableIndex};
use crate::views::app_view::{
    code_chip_inset, render_live_styled_row, shape_row, shape_table_row, AppView, RowShapedLayout,
};

struct ReadmeRowPlan {
    sub_idx: usize,
    is_last_sub: bool,
    char_start: usize,
    char_end: usize,
    row_h: f32,
    layout: RowShapedLayout,
}

/// Reads `name="value"` / `name='value'` from an HTML tag.
fn html_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let at = from + pos + name.len();
        let rest = tag[at..].trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let after = &rest[1..];
                let end = after.find(quote)?;
                return Some(after[..end].to_string());
            }
            let end = rest
                .find(|ch: char| ch.is_whitespace() || ch == '>')
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
        from = at;
    }
    None
}

/// An image-only line: `![alt](url)`, `![[target]]`, or a simple HTML image
/// wrapper.
pub(super) fn readme_image(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("![[") {
        let inner = rest.strip_suffix("]]")?;
        let target = inner.split('|').next()?.trim();
        if target.is_empty() {
            return None;
        }
        return Some((target.to_string(), target.to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("![") {
        let (alt, rest) = rest.split_once("](")?;
        let url = rest.strip_suffix(')')?;
        let url = url.split_whitespace().next()?.trim();
        let url = url.trim_start_matches('<').trim_end_matches('>');
        if url.is_empty() {
            return None;
        }
        return Some((alt.to_string(), url.to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<img")
        || ((lower.starts_with("<p") || lower.starts_with("<div") || lower.starts_with("<a"))
            && lower.contains("<img"))
    {
        let src = html_attr(trimmed, "src")?;
        if src.is_empty() {
            return None;
        }
        let alt = html_attr(trimmed, "alt").unwrap_or_default();
        return Some((alt, src));
    }
    None
}

fn image_cache_path(url: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    crate::state::app_data_dir()
        .join("plugin-readme-images")
        .join(format!("{:016x}.img", hasher.finish()))
}

fn download_readme_image(urls: &[String], path: &Path) -> Result<(), String> {
    if path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(20))
        .redirects(5)
        .build();
    let mut last = String::new();
    for url in urls {
        match agent
            .get(url)
            .set("User-Agent", "Aquilum")
            .set("Accept", "image/*,*/*")
            .call()
        {
            Ok(response) => {
                let mut bytes = Vec::new();
                if response
                    .into_reader()
                    .read_to_end(&mut bytes)
                    .is_ok()
                    && !bytes.is_empty()
                {
                    let tmp = path.with_extension("tmp");
                    std::fs::write(&tmp, &bytes).map_err(|err| err.to_string())?;
                    std::fs::rename(&tmp, path).map_err(|err| err.to_string())?;
                    return Ok(());
                }
                last = format!("empty response for {url}");
            }
            Err(err) => last = err.to_string(),
        }
    }
    Err(last)
}

impl PluginCatalogView {
    /// Downloads remote README images in the background. Local images of an
    /// installed plugin are left to the renderer, which resolves them against
    /// the plugin directory.
    pub(super) fn prefetch_readme_images(
        &mut self,
        plugin_id: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let (repo, branch) = match self
            .state
            .entries
            .iter()
            .find(|entry| entry.id == plugin_id)
        {
            Some(entry) => (
                entry.repo.trim().trim_matches('/').to_string(),
                entry.branch.clone(),
            ),
            None => return,
        };
        let plugin_dir = self
            .vault_root
            .as_ref()
            .map(|root| root.join(".vault").join("plugins").join(plugin_id));

        let mut targets: Vec<(String, Vec<String>)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut scanner = CodeScanner::new();
        for line in text.lines() {
            if scanner.step(line).is_some() {
                continue;
            }
            let Some((_alt, raw)) = readme_image(line) else {
                continue;
            };
            let raw = raw.trim().to_string();
            if raw.is_empty() || raw.starts_with("data:") {
                continue;
            }
            if !seen.insert(raw.clone()) {
                continue;
            }
            let key = format!("{plugin_id}::{raw}");
            if self.state.readme_images.contains_key(&key)
                || self.state.readme_images_loading.contains(&key)
                || self.state.readme_images_failed.contains(&key)
            {
                continue;
            }

            let urls = if raw.starts_with("http://") || raw.starts_with("https://") {
                vec![raw.clone()]
            } else {
                let decoded = crate::markdown::percent_decode(&raw);
                let local_rel = decoded.trim_start_matches("./").trim_start_matches('/');
                if let Some(dir) = &plugin_dir {
                    if dir.join(local_rel).is_file() {
                        continue;
                    }
                }
                if repo.is_empty() {
                    continue;
                }
                let rel = raw.trim_start_matches("./").trim_start_matches('/');
                match &branch {
                    Some(branch) => vec![format!(
                        "https://raw.githubusercontent.com/{repo}/{branch}/{rel}"
                    )],
                    None => vec![
                        format!("https://raw.githubusercontent.com/{repo}/master/{rel}"),
                        format!("https://raw.githubusercontent.com/{repo}/main/{rel}"),
                    ],
                }
            };
            targets.push((key, urls));
        }

        for (key, urls) in targets {
            let path = image_cache_path(&key);
            if path.is_file() {
                self.state.readme_images.insert(key, path);
                continue;
            }
            self.state.readme_images_loading.insert(key.clone());
            let entity = cx.weak_entity();
            let mut async_cx = cx.to_async();
            let bg = cx.background_executor().clone();
            let key_for_update = key.clone();
            cx.foreground_executor()
                .spawn(async move {
                    let path_for_download = path.clone();
                    let result = bg
                        .spawn(async move { download_readme_image(&urls, &path_for_download) })
                        .await;
                    let _ = entity.update(&mut async_cx, move |view, cx| {
                        view.state.readme_images_loading.remove(&key_for_update);
                        match result {
                            Ok(()) => {
                                view.state.readme_images.insert(key_for_update, path);
                            }
                            Err(_) => {
                                view.state.readme_images_failed.insert(key_for_update);
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
        }
    }

    fn render_readme_image(
        &self,
        plugin_id: &str,
        alt: &str,
        raw: &str,
        plugin_dir: Option<&Path>,
        theme: &Theme,
        max_w: f32,
    ) -> AnyElement {
        let raw = raw.trim();
        let key = format!("{plugin_id}::{raw}");
        let cached = self.state.readme_images.get(&key).cloned();
        let local = cached.or_else(|| {
            if raw.starts_with("http://")
                || raw.starts_with("https://")
                || raw.starts_with("data:")
            {
                return None;
            }
            let decoded = crate::markdown::percent_decode(raw);
            let rel = decoded.trim_start_matches("./").trim_start_matches('/');
            plugin_dir
                .map(|dir| dir.join(rel))
                .filter(|path| path.is_file())
        });

        if let Some(path) = local {
            if let Ok((width, height)) = image::image_dimensions(&path) {
                if width > 0 && height > 0 {
                    let scale = (max_w / width as f32).min(1.0);
                    let disp_w = (width as f32 * scale).max(1.0);
                    let disp_h = (height as f32 * scale).max(1.0);
                    return div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .py_2()
                        .child(
                            gpui::img(path)
                                .w(px(disp_w))
                                .h(px(disp_h))
                                .object_fit(gpui::ObjectFit::Contain)
                                .rounded_md(),
                        )
                        .into_any_element();
                }
            }
        }

        let label = if alt.trim().is_empty() { raw } else { alt.trim() };
        let suffix = if self.state.readme_images_loading.contains(&key) {
            if self.is_ru { " (загрузка…)" } else { " (loading…)" }
        } else if self.state.readme_images_failed.contains(&key) {
            if self.is_ru {
                " (изображение недоступно)"
            } else {
                " (image unavailable)"
            }
        } else {
            ""
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .py_2()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(format!("{label}{suffix}")),
            )
            .into_any_element()
    }

    /// Advance of one editor cell at the configured font size, matching the
    /// editor's wrap column budget.
    fn readme_char_width(&self, cx: &gpui::App) -> f32 {
        let font_size = self.editor_typography.base_font_size;
        let font_id = cx
            .text_system()
            .resolve_font(&gpui::font(self.editor_font.clone()));
        cx.text_system()
            .advance(font_id, px(font_size), 'x')
            .map(|size| f32::from(size.width))
            .unwrap_or(font_size * 7.7 / crate::settings::REFERENCE_FONT_SIZE)
    }

    /// Renders the plugin README with the editor's live markdown row renderer.
    ///
    /// The catalog is a separate view, so `render_live_styled_row` is called in
    /// its static mode: no editor state, no table editing. Links use the
    /// supplied opener and open externally.
    pub(super) fn render_readme_editor(
        &self,
        text: &str,
        plugin_id: &str,
        plugin_dir: Option<&Path>,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let typography = &self.editor_typography;
        let font = &self.editor_font;
        let char_w = self.readme_char_width(cx).max(1.0);
        let scale = window.scale_factor();

        // The details pane is the window minus the 360 px list column (and its
        // border); its scroll container and the README card each contribute
        // their horizontal padding.
        let window_w: f32 = window.bounds().size.width.into();
        let details_w = (window_w - 361.0).max(240.0);
        let content_w = (details_w - 104.0).max(160.0);
        let wrap_cols = ((content_w / char_w).floor() as usize).max(8);

        let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
        if lines.is_empty() {
            return div().into_any_element();
        }
        let line_refs: Vec<&str> = lines.iter().map(|line| line.as_str()).collect();

        let mut scanner = CodeScanner::new();
        let code_states: Vec<Option<crate::markdown::CodeBlockState>> =
            lines.iter().map(|line| scanner.step(line)).collect();
        let callouts = callout_blocks(&lines, &code_states);
        let guides = list_block_guide_columns(lines.iter().enumerate().map(|(idx, line)| {
            code_states
                .get(idx)
                .map_or(true, |state| state.is_none())
                .then_some(line.as_str())
        }));
        let mut tables = TableIndex::new();
        tables.refresh(1, &line_refs);
        let table_layouts: RefCell<HashMap<usize, (u64, std::sync::Arc<crate::views::app_view::TableLayout>)>> =
            RefCell::new(HashMap::new());

        let heading_widths =
            AppView::resolve_heading_char_widths(cx.text_system(), font, typography, char_w);
        let opener: std::rc::Rc<dyn Fn(&str, &mut gpui::App)> =
            std::rc::Rc::new(|url, app| app.open_url(url));
        let lang = if self.is_ru { "ru" } else { "en" };

        let mut rows: Vec<AnyElement> = Vec::new();
        for (line_idx, line) in lines.iter().enumerate() {
            let code_state = code_states
                .get(line_idx)
                .and_then(|state| state.as_ref())
                .map(|state| (state.kind, state.lang.as_deref()));
            if code_state.is_none() {
                if let Some((alt, raw)) = readme_image(line) {
                    rows.push(self.render_readme_image(
                        plugin_id, &alt, &raw, plugin_dir, theme, content_w,
                    ));
                    continue;
                }
            }
            let classified = classify_line(line, code_state);
            let (_, row_char_w, _, _, _) = AppView::heading_typography_metrics_with_widths(
                &classified,
                typography,
                char_w,
                Some(&heading_widths),
            );
            let callout = callouts.get(line_idx).copied().flatten();
            let guide = guides
                .get(line_idx)
                .map(|columns| columns.as_slice())
                .unwrap_or(&[]);
            let is_opening_fence =
                matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
            let is_closing_fence = matches!(
                classified,
                crate::markdown::LineKind::CodeFence { is_opening: false, .. }
            );
            let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));

            let grid_tbl = if code_state.is_none() {
                tables.at(line_idx)
            } else {
                None
            };
            if grid_tbl.is_some_and(|tbl| line_idx == tbl.sep_line()) {
                rows.push(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("catalog-readme-row-{}-0", line_idx).into(),
                        ))
                        .w_full()
                        .h(px(1.0))
                        .bg(theme.border_subtle)
                        .into_any_element(),
                );
                continue;
            }

            let mut table_layout = None;
            let (plans, is_table) = if let Some(tbl) = grid_tbl {
                let layout = AppView::get_or_compute_table_layout(
                    &table_layouts,
                    window.text_system(),
                    &|idx| lines.get(idx).map(|line| line.as_str()),
                    tbl,
                    font,
                    None,
                    char_w,
                    content_w,
                    typography,
                );
                let cells = split_row(line);
                let ranges: Vec<std::ops::Range<usize>> =
                    cells.iter().map(|cell| cell.content.clone()).collect();
                let count = table_row_wrap_lines(
                    line,
                    &ranges,
                    &layout.col_chars,
                    None,
                    &InlineReveal::None,
                    &typography.headings,
                );
                let kind = if line_idx == tbl.first_line {
                    TableRowVisualKind::Header { lines: count }
                } else if line_idx == tbl.last_line {
                    TableRowVisualKind::LastRow { lines: count }
                } else {
                    TableRowVisualKind::Body { lines: count }
                };
                let row_h = kind.height(layout.line_h);
                let visual = VisualRow {
                    line_idx,
                    sub_idx: 0,
                    is_last_sub: true,
                    char_start: 0,
                    char_end: line.chars().count(),
                };
                let shaped = shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    line_idx,
                    None,
                    layout.as_ref(),
                    &typography.headings,
                    font,
                    row_char_w,
                );
                table_layout = Some(layout);
                (
                    vec![ReadmeRowPlan {
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: visual.char_start,
                        char_end: visual.char_end,
                        row_h,
                        layout: shaped,
                    }],
                    true,
                )
            } else {
                let row_h = visual_row_height(line, Some(typography), false, None);
                let ranges = wrap_line_ranges(line, wrap_cols);
                let sub_count = ranges.len();
                let plans = ranges
                    .iter()
                    .enumerate()
                    .map(|(sub_idx, &(char_start, char_end))| {
                        let is_last_sub = sub_idx + 1 == sub_count;
                        let visual = VisualRow {
                            line_idx,
                            sub_idx,
                            is_last_sub,
                            char_start,
                            char_end,
                        };
                        let shaped = shape_row(
                            window.text_system(),
                            line,
                            &classified,
                            &visual,
                            false,
                            &InlineReveal::None,
                            typography,
                            font,
                            row_char_w,
                        );
                        ReadmeRowPlan {
                            sub_idx,
                            is_last_sub,
                            char_start,
                            char_end,
                            row_h,
                            layout: shaped,
                        }
                    })
                    .collect();
                (plans, false)
            };

            for plan in plans {
                let table_render_arg = table_layout.as_ref().and_then(|layout| {
                    grid_tbl.map(|tbl| {
                        (
                            tbl,
                            layout.as_ref(),
                            None,
                            line_idx,
                            plan.row_h,
                        )
                    })
                });
                let edge_top = if table_layout.is_some()
                    && grid_tbl.is_some_and(|tbl| line_idx == tbl.first_line)
                {
                    TABLE_EDGE_MARGIN
                } else {
                    0.0
                };

                let chip_cell_h = if is_table {
                    typography.table_line_height
                } else {
                    plan.row_h
                };
                let chip_inset = code_chip_inset(chip_cell_h);
                let mut backdrops: Vec<AnyElement> = Vec::new();
                for &(code_start, code_end) in plan.layout.code_ranges().iter() {
                    for (chip_x, chip_w, line_top) in
                        plan.layout.span_rects(code_start, code_end, scale)
                    {
                        backdrops.push(
                            div()
                                .absolute()
                                .left(px(chip_x))
                                .top(px(edge_top + line_top + chip_inset))
                                .h(px((chip_cell_h - 2.0 * chip_inset).max(1.0)))
                                .w(px(chip_w))
                                .bg(theme.inline_code_bg())
                                .rounded_sm()
                                .into_any_element(),
                        );
                    }
                }

                let content = render_live_styled_row(
                    line,
                    &classified,
                    plan.char_start,
                    plan.char_end,
                    line_idx,
                    plan.sub_idx,
                    plan.is_last_sub,
                    false,
                    &InlineReveal::None,
                    false,
                    &typography.headings,
                    theme,
                    font,
                    None,
                    None,
                    false,
                    None,
                    &self.focus_handle,
                    None,
                    None,
                    false,
                    table_render_arg,
                    false,
                    false,
                    None,
                    false,
                    window,
                    None,
                    Some(&opener),
                    char_w,
                    guide,
                    callout,
                    lang,
                    false,
                    "catalog-readme",
                )
                .into_any_element();

                let mut row = div()
                    .id(gpui::ElementId::Name(
                        format!(
                            "catalog-readme-row-{}-{}",
                            line_idx, plan.sub_idx
                        )
                        .into(),
                    ))
                    .relative()
                    .w_full()
                    .h(px(plan.row_h))
                    .line_height(px(plan.row_h))
                    .font_family(font.clone())
                    .text_size(px(typography.base_font_size))
                    .whitespace_nowrap();
                if is_opening_fence {
                    row = row.bg(theme.code_bg).rounded_t_md();
                } else if is_closing_fence {
                    row = row.bg(theme.code_bg).rounded_b_md();
                } else if is_code_line {
                    row = row.bg(theme.code_bg);
                } else if let Some(block) = callout {
                    row = row
                        .bg(crate::markdown::callout_bg(block.kind, theme.is_dark))
                        .when(block.first && plan.sub_idx == 0, |d| d.rounded_t_md())
                        .when(block.last && plan.is_last_sub, |d| d.rounded_b_md());
                }

                rows.push(
                    row.children(backdrops)
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .flex_row()
                                .when(is_table, |d| d.items_start())
                                .when(!is_table, |d| d.items_center())
                                .whitespace_nowrap()
                                .child(content),
                        )
                        .into_any_element(),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .children(rows)
            .into_any_element()
    }
}
