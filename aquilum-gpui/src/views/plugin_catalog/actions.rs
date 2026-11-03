use super::*;

impl AppView {
    pub(super) fn render_markdown_content(&self, text: &str, cx: &Context<Self>) -> Vec<Div> {
        let theme = &self.theme;
        let mut rendered_markdown_lines: Vec<Div> = Vec::new();
        let mut lines_iter = text.lines().peekable();
        let mut line_counter = 0usize;

        while let Some(line) = lines_iter.next() {
            let trimmed = line.trim();

            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                let lang = trimmed.trim_start_matches(fence).trim().to_string();
                let lang_opt = if !lang.is_empty() { Some(lang) } else { None };

                let mut code_lines = Vec::new();
                while let Some(next_line) = lines_iter.peek() {
                    let trimmed_peek = next_line.trim();
                    if trimmed_peek.starts_with(fence) {
                        lines_iter.next();
                        break;
                    }
                    code_lines.push(lines_iter.next().unwrap());
                }

                let code_str = code_lines.join("\n");
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_col()
                        .my_1p5()
                        .rounded_md()
                        .bg(theme.bg_editor)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .overflow_x_hidden()
                        .when(lang_opt.is_some(), |parent| {
                            parent.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.bg_sidebar)
                                    .border_b_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child(lang_opt.unwrap()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .p_3()
                                .font_family("Consolas")
                                .text_xs()
                                .line_height(px(18.0))
                                .text_color(theme.text_primary)
                                .child(code_str),
                        ),
                );
                continue;
            }

            if trimmed.is_empty() {
                rendered_markdown_lines.push(div().h(px(8.0)));
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("### ") {
                rendered_markdown_lines.push(
                    div()
                        .text_base()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_3()
                        .mb_1()
                        .child(rest.to_string()),
                );
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("## ") {
                rendered_markdown_lines.push(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_4()
                        .mb_1p5()
                        .child(rest.to_string()),
                );
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("# ") {
                rendered_markdown_lines.push(
                    div()
                        .text_xl()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .mt_4()
                        .mb_2()
                        .child(rest.to_string()),
                );
                continue;
            }

            if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
                let checked = !trimmed.starts_with("- [ ] ");
                let item_text = &trimmed[6..];
                let spans = crate::markdown::parse_inline_spans(item_text);
                let span_elements: Vec<AnyElement> = spans
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| self.render_markdown_inline_span(s, i, cx))
                    .collect();

                let checkbox = div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .mt(px(2.0))
                    .rounded_xs()
                    .border_1()
                    .border_color(if checked { theme.accent } else { theme.border })
                    .bg(if checked { theme.accent } else { theme.bg_editor })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |c| c.child(icons::icon(icons::ICON_CHECK, 10.0, theme.text_on_accent)));

                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .my_0p5()
                        .child(checkbox)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .text_sm()
                                .children(span_elements),
                        ),
                );
                continue;
            }

            if let Some(bullet) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
                let spans = crate::markdown::parse_inline_spans(bullet);
                let span_elements: Vec<AnyElement> = spans
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| self.render_markdown_inline_span(s, i, cx))
                    .collect();

                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .my_0p5()
                        .child(div().text_color(theme.accent).child("•"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .text_sm()
                                .children(span_elements),
                        ),
                );
                continue;
            }

            let spans = crate::markdown::parse_inline_spans(line);
            line_counter += 1;
            let span_elements: Vec<AnyElement> = spans
                .into_iter()
                .enumerate()
                .map(|(i, s)| self.render_markdown_inline_span(s, line_counter * 100 + i, cx))
                .collect();

            rendered_markdown_lines.push(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .text_sm()
                    .line_height(gpui::relative(1.4))
                    .children(span_elements),
            );
        }

        rendered_markdown_lines
    }

    pub(super) fn render_markdown_inline_span(
        &self,
        span: InlineSpan,
        _idx: usize,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        match span {
            InlineSpan::Text(t) => div().text_color(theme.text_secondary).child(t).into_any_element(),
            InlineSpan::Bold(t) => div().font_weight(gpui::FontWeight::BOLD).text_color(theme.text_primary).child(t).into_any_element(),
            InlineSpan::Italic(t) => div().italic().text_color(theme.text_secondary).child(t).into_any_element(),
            InlineSpan::BoldItalic(t) => div().font_weight(gpui::FontWeight::BOLD).italic().text_color(theme.text_primary).child(t).into_any_element(),
            InlineSpan::InlineCode(t) => div()
                .px_1p5()
                .py_0p5()
                .mx_0p5()
                .rounded_xs()
                .bg(theme.bg_editor)
                .border_1()
                .border_color(theme.border_subtle)
                .font_family("Consolas")
                .text_xs()
                .text_color(theme.accent)
                .child(t)
                .into_any_element(),
            InlineSpan::Link { label, url } => {
                let click_url = url.clone();
                div()
                    .id(ElementId::Name(format!("app-cat-md-link-{}", _idx).into()))
                    .text_color(theme.accent)
                    .cursor_pointer()
                    .hover(|h| h.underline())
                    .child(if label.is_empty() { url } else { label })
                    .on_click(move |_, _, cx| {
                        cx.open_url(&click_url);
                    })
                    .into_any_element()
            }
            InlineSpan::Strikethrough(t) => div().line_through().text_color(theme.text_muted).child(t).into_any_element(),
        }
    }

    /// Opens the Community Plugins catalog in a separate OS window (Multi-Window GPUI).
    pub fn open_plugin_catalog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let vault_name = self.state.vault.as_ref().map(|v| v.name.clone()).unwrap_or_else(|| "Vault".to_string());
        let vault_root = self.state.vault.as_ref().map(|v| v.root.clone());
        let theme = self.theme.clone();
        let language = self.config.ui.language.clone();
        let app_data_dir = crate::state::app_data_dir();
        let weak_self = cx.weak_entity();

        cx.defer(move |cx| {
            let handle = weak_self.clone();
            let _ = weak_self.update(cx, |this, cx| {
                this.catalog_window_manager.open_or_focus(
                    handle,
                    vault_name,
                    vault_root,
                    theme,
                    language,
                    &app_data_dir,
                    cx,
                );
            });
        });
    }

    /// Closes the Community Plugins catalog modal.
    pub fn close_plugin_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.plugin_catalog.is_open {
            return;
        }
        self.plugin_catalog.is_open = false;
        self.plugin_catalog.selection_anchor = None;
        self.plugin_catalog.is_mouse_selecting = false;
        self.close_catalog_compat_popup(cx);
        self.catalog_blink.disable(cx);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Performs an asynchronous background fetch of the live Obsidian registry (~2000+ plugins)
    /// without blocking the UI.
    pub fn refresh_remote_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        if self.plugin_catalog.is_loading {
            return;
        }

        self.plugin_catalog.is_loading = true;
        self.plugin_catalog.status_message = Some("Загрузка реестра с GitHub...".to_string());
        cx.notify();

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                let res = bg_executor
                    .spawn(async { crate::plugins::registry::fetch_remote_community_plugins() })
                    .await;

                let _ = entity.update(&mut async_cx, |view, cx| {
                    view.plugin_catalog.is_loading = false;
                    match res {
                        Ok(entries) => {
                            let count = entries.len();
                            view.plugin_catalog.entries = entries;
                            view.plugin_catalog.status_message =
                                Some(format!("Загружено плагинов: {}", count));
                            if view.plugin_catalog.selected_plugin_id.is_none() {
                                if let Some(first) = view.plugin_catalog.entries.first() {
                                    view.plugin_catalog.selected_plugin_id = Some(first.id.clone());
                                }
                            }
                        }
                        Err(err) => {
                            view.plugin_catalog.status_message =
                                Some(format!("Ошибка сети: {}. Используется кэш.", err));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Follows the pointer while the plugin catalog scrollbar thumb is dragged.
    pub fn handle_catalog_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.catalog_scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.catalog_scroll_handle, &self.catalog_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Follows the pointer while the plugin catalog details scrollbar thumb is dragged.
    pub fn handle_catalog_details_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.catalog_details_scrollbar_drag.active {
            return;
        }
        let style = catalog_scrollbar_style();
        let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_details_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.catalog_details_scroll_handle, &self.catalog_details_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Sets the vertical scroll offset of the catalog list.
    pub fn set_catalog_scroll_top(&mut self, top: f32) {
        self.catalog_scroll_handle.set_offset(point(px(0.0), px(-top.max(0.0))));
    }

    /// Sets the vertical scroll offset of the catalog details view.
    pub fn set_catalog_details_scroll_top(&mut self, top: f32) {
        self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(-top.max(0.0))));
    }

    /// Automatically scrolls the catalog list to ensure the selected plugin is in view.
    pub fn scroll_catalog_to_selected(&mut self, selected_idx: usize) {
        let item_top = selected_idx as f32 * CATALOG_ROW_H;
        let item_bottom = item_top + CATALOG_ROW_H;
        let viewport_h = f32::from(self.catalog_scroll_handle.bounds().size.height);
        if viewport_h <= 0.0 {
            return;
        }
        let current_scroll = -f32::from(self.catalog_scroll_handle.offset().y);

        if item_top < current_scroll {
            self.set_catalog_scroll_top(item_top);
        } else if item_bottom > current_scroll + viewport_h {
            self.set_catalog_scroll_top(item_bottom - viewport_h);
        }
    }

    /// Selects a plugin in the catalog and immediately starts background loading of its README.md if not cached.
    pub fn select_catalog_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        self.plugin_catalog.selected_plugin_id = Some(plugin_id.to_string());
        self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
        self.ensure_catalog_plugin_readme_loaded(plugin_id, cx);
        cx.notify();
    }

    /// Opens the compatibility dropdown of the in-settings catalog panel.
    /// Same shared `TabListPopup` as the standalone window version above.
    pub fn open_catalog_compat_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tabs, active_ix) = compat_popup_entries(self.plugin_catalog.compat_filter);
        let theme = self.theme.clone();
        let popup = cx.new(|cx| {
            crate::views::tab_list::TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                None,
                None,
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &crate::views::tab_list::TabListEvent, _window, cx| {
                match event {
                    crate::views::tab_list::TabListEvent::Activate(id) => {
                        if let Some(option) = CatalogCompatFilter::ALL.get(*id) {
                            this.plugin_catalog.compat_filter = *option;
                            this.on_catalog_search_query_changed();
                        }
                        this.close_catalog_compat_popup(cx);
                    }
                    crate::views::tab_list::TabListEvent::Dismiss => {
                        this.close_catalog_compat_popup(cx)
                    }
                    _ => {}
                }
            },
        );
        window.focus(&popup.read(cx).focus_handle());
        self.catalog_compat_popup = Some(popup);
        self.catalog_compat_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_catalog_compat_popup(&mut self, cx: &mut Context<Self>) {
        if self.catalog_compat_popup.take().is_none()
            && self.catalog_compat_popup_subscription.take().is_none()
        {
            return;
        }
        self.catalog_compat_popup_subscription = None;
        cx.notify();
    }

    pub fn catalog_compat_popup_is_open(&self) -> bool {
        self.catalog_compat_popup.is_some()
    }

    /// Checks if README.md for the given plugin is available locally or in cache; if not, fetches it asynchronously from GitHub.
    pub fn ensure_catalog_plugin_readme_loaded(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if self.plugin_catalog.readme_cache.contains_key(plugin_id) {
            return;
        }

        // Check local disk first if vault exists
        if let Some(vault) = &self.state.vault {
            let p_dir = vault.root.join(".vault").join("plugins").join(plugin_id);
            for name in &["README.md", "readme.md"] {
                let file = p_dir.join(name);
                if file.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&file) {
                        self.plugin_catalog.readme_cache.insert(plugin_id.to_string(), content);
                        return;
                    }
                }
            }
        }

        // Find repository in catalog entries
        let Some(entry) = self.plugin_catalog.entries.iter().find(|e| e.id == plugin_id) else {
            return;
        };

        let repo = entry.repo.clone();
        let p_id = plugin_id.to_string();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        self.plugin_catalog.is_loading_readme = true;

        cx.foreground_executor()
            .spawn(async move {
                let res = bg_executor
                    .spawn(async move { crate::plugins::registry::fetch_readme_for_plugin(&repo) })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.plugin_catalog.is_loading_readme = false;
                    match res {
                        Ok(content) => {
                            view.plugin_catalog.readme_cache.insert(p_id, content);
                        }
                        Err(_) => {
                            view.plugin_catalog.readme_cache.insert(
                                p_id,
                                "# Описание отсутствует\n\nНе удалось загрузить README.md из репозитория GitHub.".to_string(),
                            );
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Inserts text into the Plugin Catalog search query at the current cursor / replacing selection.
    pub(super) fn insert_catalog_search_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
        let pos = if let Some((s, e)) = self.plugin_catalog.selection_range() {
            chars.drain(s..e);
            self.plugin_catalog.selection_anchor = None;
            s
        } else {
            self.plugin_catalog.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.plugin_catalog.cursor_pos = pos + text.chars().count();
        self.plugin_catalog.search_query = chars.into_iter().collect();
    }

    /// Called whenever the search query changes to keep selection in sync.
    pub(super) fn on_catalog_search_query_changed(&mut self) {
        let filtered = self.plugin_catalog.filtered_entries();
        if !filtered.iter().any(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref()) {
            self.plugin_catalog.selected_plugin_id = filtered.first().map(|e| e.id.clone());
        }
        self.set_catalog_scroll_top(0.0);
    }

    /// Maps a click x (window coords) to a query char index.
    pub fn catalog_search_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.plugin_catalog.search_query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.plugin_catalog.box_origin_x.get();
        let rel_x = (click_x - box_x).max(0.0);
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        query[..byte_idx.min(query.len())].chars().count()
    }

    /// Follows the pointer while catalog search input selection is being dragged.
    pub fn handle_catalog_search_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_catalog.is_open || !self.plugin_catalog.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.catalog_search_click_pos(x, window);
        if self.plugin_catalog.cursor_pos != new_pos {
            self.plugin_catalog.cursor_pos = new_pos;
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
        }
    }

    /// Ends an input drag selection in plugin catalog.
    pub fn handle_catalog_search_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.plugin_catalog.is_mouse_selecting {
            self.plugin_catalog.is_mouse_selecting = false;
            if self.plugin_catalog.selection_anchor == Some(self.plugin_catalog.cursor_pos) {
                self.plugin_catalog.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Handles keyboard events while the plugin catalog is focused.
    pub(super) fn handle_catalog_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key == "escape" {
            self.close_plugin_catalog(window, cx);
            return;
        }

        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;

        let filtered = self.plugin_catalog.filtered_entries();

        // Arrow Up navigation
        if key == "up" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = current_idx.saturating_sub(1);
                let new_id = filtered[new_idx].id.clone();
                self.plugin_catalog.selected_plugin_id = Some(new_id.clone());
                self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_catalog_to_selected(new_idx);
                self.ensure_catalog_plugin_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        // Arrow Down navigation
        if key == "down" {
            if !filtered.is_empty() {
                let current_idx = filtered
                    .iter()
                    .position(|e| Some(&e.id) == self.plugin_catalog.selected_plugin_id.as_ref())
                    .unwrap_or(0);
                let new_idx = (current_idx + 1).min(filtered.len().saturating_sub(1));
                let new_id = filtered[new_idx].id.clone();
                self.plugin_catalog.selected_plugin_id = Some(new_id.clone());
                self.catalog_details_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                self.scroll_catalog_to_selected(new_idx);
                self.ensure_catalog_plugin_readme_loaded(&new_id, cx);
                cx.notify();
            }
            return;
        }

        // Home / End
        if key == "home" || key == "end" {
            let total = self.plugin_catalog.search_query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.plugin_catalog.selection_anchor.is_none() {
                    self.plugin_catalog.selection_anchor = Some(self.plugin_catalog.cursor_pos);
                }
            } else {
                self.plugin_catalog.selection_anchor = None;
            }
            self.plugin_catalog.cursor_pos = pos;
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Left / Right cursor movement
        if key == "left" || key == "right" {
            let total = self.plugin_catalog.search_query.chars().count();
            if shift && self.plugin_catalog.selection_anchor.is_none() {
                self.plugin_catalog.selection_anchor = Some(self.plugin_catalog.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.plugin_catalog.cursor_pos = self.plugin_catalog.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.plugin_catalog.selection_range() {
                    self.plugin_catalog.cursor_pos = start;
                    self.plugin_catalog.selection_anchor = None;
                } else {
                    self.plugin_catalog.cursor_pos = self.plugin_catalog.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.plugin_catalog.cursor_pos = (self.plugin_catalog.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.plugin_catalog.selection_range() {
                self.plugin_catalog.cursor_pos = end;
                self.plugin_catalog.selection_anchor = None;
            } else {
                self.plugin_catalog.cursor_pos = (self.plugin_catalog.cursor_pos + 1).min(total);
            }
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.plugin_catalog.selection_anchor = Some(0);
            self.plugin_catalog.cursor_pos = self.plugin_catalog.search_query.chars().count();
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                let text: String = self.plugin_catalog.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                let text: String = self.plugin_catalog.search_query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
                chars.drain(s..e);
                self.plugin_catalog.search_query = chars.into_iter().collect();
                self.plugin_catalog.cursor_pos = s;
                self.plugin_catalog.selection_anchor = None;
                self.on_catalog_search_query_changed();
                self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                cx.notify();
            }
            return;
        }

        // Ctrl+V paste
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_catalog_search_text(&filtered);
                        self.on_catalog_search_query_changed();
                        self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        if ctrl || alt || event.keystroke.modifiers.platform {
            return;
        }

        // Backspace / Delete
        if key == "backspace" || key == "delete" {
            let mut chars: Vec<char> = self.plugin_catalog.search_query.chars().collect();
            if let Some((s, e)) = self.plugin_catalog.selection_range() {
                chars.drain(s..e);
                self.plugin_catalog.cursor_pos = s;
                self.plugin_catalog.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.plugin_catalog.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.plugin_catalog.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.plugin_catalog.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.plugin_catalog.search_query = chars.into_iter().collect();
            self.on_catalog_search_query_changed();
            self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
            cx.notify();
            return;
        }

        // Plain text input
        let text_to_insert = if key == "space" {
            Some(" ".to_string())
        } else if let Some(ch) = &event.keystroke.key_char {
            Some(ch.to_string())
        } else if key.chars().count() == 1 {
            Some(key.clone())
        } else {
            None
        };
        if let Some(text) = text_to_insert {
            let filtered: String = text
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                self.insert_catalog_search_text(&filtered);
                self.on_catalog_search_query_changed();
                self.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                cx.notify();
            }
        }
    }

    /// Real asynchronous installation of a community plugin from GitHub Releases.
    /// Downloads manifest.json, main.js, styles.css and README.md into `<vault>/.vault/plugins/<id>/`
    /// without freezing the UI, and automatically activates the plugin.
    pub fn install_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .plugin_catalog
            .entries
            .iter()
            .find(|e| e.id == plugin_id)
            .cloned()
        else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            self.state.status_text = "Cannot install plugin: No vault opened".to_string();
            cx.notify();
            return;
        };

        if self.plugin_catalog.is_downloading {
            return;
        }

        self.plugin_catalog.is_downloading = true;
        self.plugin_catalog.downloading_plugin_id = Some(entry.id.clone());
        self.plugin_catalog.download_step = Some("Подключение к GitHub Releases...".to_string());
        cx.notify();

        let plugin_dir = vault.root.join(".vault").join("plugins").join(&entry.id);
        let repo = entry.repo.clone();
        let plugin_name = entry.name.clone();
        let entry_id = entry.id.clone();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let bg_executor = cx.background_executor().clone();

        cx.foreground_executor()
            .spawn(async move {
                let p_dir = plugin_dir.clone();
                let res = bg_executor
                    .spawn(async move {
                        crate::plugins::registry::download_plugin_release(&repo, &p_dir, |_step| {})
                    })
                    .await;

                let _ = entity.update(&mut async_cx, move |view, cx| {
                    view.plugin_catalog.is_downloading = false;
                    view.plugin_catalog.downloading_plugin_id = None;
                    view.plugin_catalog.download_step = None;

                    match res {
                        Ok(()) => {
                            if let Some(vault) = &view.state.vault {
                                let _ = vault.enable_community_plugin(&entry_id);
                                if !view.config.community_plugins.safe_mode {
                                    view.load_installed_plugins_for_vault();
                                }
                                view.state.status_text =
                                    format!("Плагин '{}' успешно установлен и активирован", plugin_name);
                            }
                        }
                        Err(e) => {
                            view.state.status_text =
                                format!("Ошибка установки плагина '{}': {}", plugin_name, e);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
    }

    /// Uninstalls an installed plugin by removing its directory and disabling it.
    pub fn uninstall_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(vault_root) = self.state.vault.as_ref().map(|v| v.root.clone()) else {
            return;
        };

        if let Some(vault) = &self.state.vault {
            let _ = vault.disable_community_plugin(plugin_id);
        }

        // Unload runtime and UI
        self.unload_plugin(plugin_id, cx);

        let plugin_dir = vault_root.join(".vault").join("plugins").join(plugin_id);
        let _ = std::fs::remove_dir_all(&plugin_dir);

        self.state.status_text = format!("Uninstalled plugin '{}'", plugin_id);
        cx.notify();
    }

    /// Renders the Community Plugins Catalog modal window.
    pub fn render_plugin_catalog_modal(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let entries = self.plugin_catalog.filtered_entries();
        let selected_id = self.plugin_catalog.selected_plugin_id.clone();
        let selected_entry = entries
            .iter()
            .find(|e| Some(&e.id) == selected_id.as_ref())
            .or_else(|| entries.first());

        div()
            .id("plugin-catalog-root")
            .key_context("plugin_catalog")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .cursor(gpui::CursorStyle::Arrow)
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.catalog_details_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_details_scrollbar_drag(y, cx);
                    cx.stop_propagation();
                } else if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_move(event, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                let mut changed = false;
                if this.catalog_scrollbar_drag.active {
                    this.catalog_scrollbar_drag.end();
                    changed = true;
                }
                if this.catalog_details_scrollbar_drag.active {
                    this.catalog_details_scrollbar_drag.end();
                    changed = true;
                }
                if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_up(cx);
                    changed = true;
                }
                if changed {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.plugin_catalog.is_mouse_selecting {
                    this.handle_catalog_search_mouse_up(cx);
                }
                if this.catalog_scrollbar_drag.active {
                    this.catalog_scrollbar_drag.end();
                }
                if this.catalog_details_scrollbar_drag.active {
                    this.catalog_details_scrollbar_drag.end();
                }
                cx.notify();
            }))
            // Backdrop
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000aa))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_plugin_catalog(window, cx);
                    })),
            )
            // Centered Catalog Dialog Window
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("plugin-catalog-dialog")
                            .flex()
                            .flex_col()
                            .w(px(920.0))
                            .h(px(640.0))
                            .max_w(px(960.0))
                            .max_h(px(720.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                                if this.catalog_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                } else if this.catalog_details_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_catalog_details_scrollbar_drag(y, cx);
                                    cx.stop_propagation();
                                } else if this.plugin_catalog.is_mouse_selecting {
                                    this.handle_catalog_search_mouse_move(event, window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                let mut changed = false;
                                if this.catalog_scrollbar_drag.active {
                                    this.catalog_scrollbar_drag.end();
                                    changed = true;
                                }
                                if this.catalog_details_scrollbar_drag.active {
                                    this.catalog_details_scrollbar_drag.end();
                                    changed = true;
                                }
                                if this.plugin_catalog.is_mouse_selecting {
                                    this.handle_catalog_search_mouse_up(cx);
                                    changed = true;
                                }
                                if changed {
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Catalog Header with Search
                            .child(self.render_catalog_header(window, cx))
                            // Catalog Body: List (left) + Details (right)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .child(self.render_catalog_list(&entries, cx))
                                    .child(self.render_catalog_details(selected_entry, cx)),
                            ),
                    )
            )
    }

    pub(super) fn render_catalog_header(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.plugin_catalog.search_query.clone();
        let total_count = self.plugin_catalog.entries.len();
        let filtered_count = self.plugin_catalog.filtered_entries().len();
        let is_loading = self.plugin_catalog.is_loading;
        let pixel_scale = window.scale_factor();

        // Input caret geometry
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.plugin_catalog.cursor_pos.min(query_chars.len());
        let selection_range = self.plugin_catalog.selection_range();
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let cursor_x = if query.is_empty() {
            0.0
        } else {
            let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };
        let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..e.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snapped_span(
                    f32::from(shaped.x_for_index(b1)),
                    f32::from(shaped.x_for_index(b2)),
                    pixel_scale,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        let is_input_focused = self.catalog_focus_handle.is_focused(window);
        let caret_visible = is_input_focused && self.catalog_blink.visible();

        div()
            .flex()
            .flex_col()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .child(icons::icon(icons::ICON_GLOBE, 18.0, theme.accent))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child(if is_ru {
                                        "Каталог сторонних плагинов"
                                    } else {
                                        "Community Plugins Catalog"
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .bg(theme.bg_hover)
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{} {}",
                                        total_count,
                                        if is_ru { "плагинов" } else { "plugins" }
                                    )),
                            )
                            .when(is_loading, |parent| {
                                parent.child(
                                    div()
                                        .text_xs()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_full()
                                        .bg(theme.accent)
                                        .text_color(theme.text_on_accent)
                                        .child(if is_ru {
                                            "Загрузка с GitHub..."
                                        } else {
                                            "Fetching from GitHub..."
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Refresh registry button
                            .child(
                                div()
                                    .id("btn-refresh-plugin-catalog")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.bg_hover)
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.border))
                                    .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_secondary))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_primary)
                                            .child(if is_ru { "Обновить" } else { "Refresh" }),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_remote_plugin_catalog(cx);
                                    })),
                            )
                            // Close dialog button
                            .child(
                                div()
                                    .id("btn-close-plugin-catalog")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .p_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_CLOSE, 16.0, theme.text_muted))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_plugin_catalog(window, cx);
                                    })),
                            ),
                    ),
            )
            // Search Bar & Sort options
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    // Search input box
                    .child(
                        div()
                            .id("catalog-search-input-box")
                            .flex()
                            .flex_1()
                            .flex_row()
                            .items_center()
                            .bg(theme.bg_editor)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_md()
                            .px_3()
                            .py_1p5()
                            .gap_2()
                            .cursor(gpui::CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                window.focus(&this.catalog_focus_handle);
                                this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                            }))
                            .child(icons::icon(icons::ICON_SEARCH, 14.0, theme.text_muted))
                            .child(
                                div()
                                    .id("catalog-search-input")
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(22.0))
                                    .track_focus(&self.catalog_focus_handle)
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        window.focus(&this.catalog_focus_handle);
                                        this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                                        let click_x: f32 = event.position.x.into();
                                        let pos = this.catalog_search_click_pos(click_x, window);
                                        let total_chars = this.plugin_catalog.search_query.chars().count();
                                        if event.click_count >= 3 {
                                            this.plugin_catalog.selection_anchor = Some(0);
                                            this.plugin_catalog.cursor_pos = total_chars;
                                            this.plugin_catalog.is_mouse_selecting = false;
                                        } else if event.click_count == 2 {
                                            let (s, e) = word_range_at(&this.plugin_catalog.search_query, pos);
                                            this.plugin_catalog.selection_anchor = Some(s);
                                            this.plugin_catalog.cursor_pos = e;
                                            this.plugin_catalog.is_mouse_selecting = false;
                                        } else if event.modifiers.shift {
                                            if this.plugin_catalog.selection_anchor.is_none() {
                                                this.plugin_catalog.selection_anchor = Some(this.plugin_catalog.cursor_pos);
                                            }
                                            this.plugin_catalog.cursor_pos = pos;
                                            this.plugin_catalog.is_mouse_selecting = true;
                                        } else {
                                            this.plugin_catalog.cursor_pos = pos;
                                            this.plugin_catalog.selection_anchor = Some(pos);
                                            this.plugin_catalog.is_mouse_selecting = true;
                                        }
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.handle_catalog_key(event, window, cx);
                                    }))
                                    .child(
                                        gpui::canvas(
                                            {
                                                let cell = self.plugin_catalog.box_origin_x.clone();
                                                move |bounds, _, _| {
                                                    cell.set(f32::from(bounds.origin.x));
                                                }
                                            },
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .size_full(),
                                    )
                                    .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                        s.child(
                                            div()
                                                .absolute()
                                                .left(px(sel_x1))
                                                .top(px(0.0))
                                                .w(px(sel_w))
                                                .h(px(22.0))
                                                .bg(theme.selection_bg)
                                                .rounded_xs(),
                                        )
                                    })
                                    .child(if query.is_empty() {
                                        div()
                                            .font_family("Segoe UI")
                                            .text_size(px(14.0))
                                            .line_height(px(22.0))
                                            .text_color(theme.text_muted)
                                            .child(if is_ru {
                                                "Поиск по названию, автору или описанию...".to_string()
                                            } else {
                                                "Search community plugins...".to_string()
                                            })
                                    } else {
                                        div()
                                            .font_family("Segoe UI")
                                            .text_size(px(14.0))
                                            .line_height(px(22.0))
                                            .text_color(theme.text_primary)
                                            .child(query.clone())
                                    })
                                    .when(caret_visible, |s| {
                                        s.child(
                                            div()
                                                .absolute()
                                                .left(px(cursor_x))
                                                .top(px(2.0))
                                                .w(px(1.5))
                                                .h(px(18.0))
                                                .bg(theme.accent)
                                                .rounded_xs(),
                                        )
                                    }),
                            )
                            .when(!query.is_empty(), |parent| {
                                parent.child(
                                    div()
                                        .id("btn-clear-catalog-search")
                                        .cursor_pointer()
                                        .hover(|h| h.text_color(theme.text_primary))
                                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.plugin_catalog.search_query.clear();
                                            this.plugin_catalog.cursor_pos = 0;
                                            this.plugin_catalog.selection_anchor = None;
                                            this.on_catalog_search_query_changed();
                                            window.focus(&this.catalog_focus_handle);
                                            this.catalog_blink.pause_and_show(BlinkTarget::PluginCatalog, cx);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    // Showing count
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(if is_ru {
                                format!("Найдено: {}", filtered_count)
                            } else {
                                format!("Found: {}", filtered_count)
                            }),
                    )
                    // Sort order pills
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("sort-catalog-popular")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::MostPopular {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::MostPopular {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "Популярные" } else { "Popular" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::MostPopular;
                                        window.focus(&this.catalog_focus_handle);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("sort-catalog-alpha")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::Alphabetical {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::Alphabetical {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "А-Я" } else { "A-Z" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::Alphabetical;
                                        window.focus(&this.catalog_focus_handle);
                                        cx.notify();
                                     })),
                            )
                            .child(
                                div()
                                    .id("sort-catalog-updated")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.plugin_catalog.sort_order == CatalogSortOrder::RecentlyUpdated {
                                        theme.accent
                                    } else {
                                        theme.bg_hover
                                    })
                                    .text_color(if self.plugin_catalog.sort_order == CatalogSortOrder::RecentlyUpdated {
                                        theme.text_on_accent
                                    } else {
                                        theme.text_secondary
                                    })
                                    .text_xs()
                                    .cursor_pointer()
                                    .child(if is_ru { "Обновленные" } else { "Updated" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.plugin_catalog.sort_order = CatalogSortOrder::RecentlyUpdated;
                                        window.focus(&this.catalog_focus_handle);
                                        cx.notify();
                                     })),
                            ),
                    )
                    // Compatibility dropdown over the embedded corpus registry.
                    // The shared `TabListPopup` hangs from this button (deferred,
                    // so it paints above the list/details panes); a click outside
                    // it emits `Dismiss` and closes the list.
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .id("compat-dropdown-panel-toggle")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(if self.catalog_compat_popup_is_open() { theme.accent } else { theme.border_subtle })
                                    .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(self.plugin_catalog.compat_filter.dot_color(theme)))
                                    .child(self.plugin_catalog.compat_filter.label(is_ru))
                                    .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.catalog_compat_popup_is_open() {
                                            this.close_catalog_compat_popup(cx);
                                        } else {
                                            this.open_catalog_compat_popup(window, cx);
                                        }
                                    })),
                            )
                            .when_some(self.catalog_compat_popup.clone(), |parent, popup| {
                                parent.child(
                                    gpui::deferred(
                                        div()
                                            .id("compat-dropdown-panel-popup")
                                            .absolute()
                                            .top_full()
                                            .right_0()
                                            .mt_1()
                                            .child(popup),
                                    )
                                    .with_priority(999),
                                )
                            }),
                    ),
            )
    }

    /// Renders the left virtualized scrollable list of plugin entries in the catalog.
    pub(super) fn render_catalog_list(&self, entries: &[CatalogPluginEntry], cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let selected_id = self.plugin_catalog.selected_plugin_id.as_deref().unwrap_or("");
        let is_ru = self.config.ui.language.starts_with("ru");

        let total_items = entries.len();
        let total_height = total_items as f32 * CATALOG_ROW_H;

        // Viewport bounds & current scroll offset
        let viewport_h = f32::from(self.catalog_scroll_handle.bounds().size.height).max(100.0);
        let max_offset = f32::from(self.catalog_scroll_handle.max_offset().height);
        let current_scroll_y = (-f32::from(self.catalog_scroll_handle.offset().y)).clamp(0.0, max_offset.max(0.0));

        // Calculate virtual window slice
        let start_row = (current_scroll_y / CATALOG_ROW_H).floor() as usize;
        let visible_count = (viewport_h / CATALOG_ROW_H).ceil() as usize;

        let visible_start = start_row.saturating_sub(CATALOG_OVERSCAN_ROWS);
        let visible_end = (start_row + visible_count + CATALOG_OVERSCAN_ROWS).min(total_items);

        let top_spacer_h = visible_start as f32 * CATALOG_ROW_H;
        let bottom_spacer_h = (total_height - (visible_end as f32 * CATALOG_ROW_H)).max(0.0);

        let mut item_elements: Vec<AnyElement> = Vec::with_capacity((visible_end - visible_start) + 2);

        if top_spacer_h > 0.0 {
            item_elements.push(
                div()
                    .id("catalog-top-spacer")
                    .w_full()
                    .h(px(top_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        let query = self.plugin_catalog.search_query.clone();
        let terms = highlight_query_terms(&query);

        for (rel_idx, entry) in entries[visible_start..visible_end].iter().enumerate() {
            let actual_idx = visible_start + rel_idx;
            let is_selected = entry.id == selected_id;
            let is_installed = self
                .state
                .vault
                .as_ref()
                .map(|v| v.root.join(".vault").join("plugins").join(&entry.id).exists())
                .unwrap_or(false);

            let entry_id = entry.id.clone();

            let dl_formatted = format_download_count(entry.downloads);
            let author_display = format!("{}: {}", if is_ru { "Автор" } else { "Author" }, entry.author);

            let item_el = div()
                .id(ElementId::from(SharedString::from(format!("cat-item-{}", entry.id))))
                .flex()
                .flex_col()
                .gap_2()
                .h(px(160.0))
                .mb(px(8.0))
                .px_3p5()
                .py_3()
                .rounded_lg()
                .border_1()
                .border_color(theme.border_subtle)
                .bg(if is_selected {
                    theme.bg_hover
                } else if theme.is_dark {
                    theme.bg_card
                } else {
                    gpui::rgba(0xffffffff)
                })
                .hover(|h| h.bg(theme.bg_hover))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_catalog_plugin(&entry_id, cx);
                    this.scroll_catalog_to_selected(actual_idx);
                    window.focus(&this.catalog_focus_handle);
                }))
                // Top row: Plug icon (larger, on the left) + Title + Downloads (with download icon) & Installed badge (on the right)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        // Left: Plug icon + Plugin Name
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(icons::icon(icons::ICON_PLUG, 18.0, theme.accent))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(highlight_terms(&entry.name, &terms, theme.text_primary, is_selected, theme)),
                                ),
                        )
                        // Right: Installed badge + Download count with download icon
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .when(is_installed, |parent| {
                                    parent.child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(rgba(0x7c4dffcc))
                                            .text_color(gpui::white())
                                            .child(if is_ru { "УСТАНОВЛЕН" } else { "INSTALLED" }),
                                    )
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(icons::icon(icons::ICON_DOWNLOAD, 12.0, theme.text_muted))
                                        .child(dl_formatted),
                                ),
                        ),
                )
                // Snippet / description (full description, no 2-line clamp)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_xs()
                        .line_height(px(17.0))
                        .whitespace_normal()
                        .child(highlight_terms(&entry.description, &terms, theme.text_secondary, is_selected, theme)),
                )
                // Author row
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_xs()
                        .truncate()
                        .child(highlight_terms(&author_display, &terms, theme.text_muted, is_selected, theme)),
                );

            item_elements.push(item_el.into_any_element());
        }

        if bottom_spacer_h > 0.0 {
            item_elements.push(
                div()
                    .id("catalog-bottom-spacer")
                    .w_full()
                    .h(px(bottom_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        let scroll_container = div()
            .id("catalog-items-scroll")
            .track_scroll(&self.catalog_scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .overflow_scroll()
            .p_2()
            .pr_3() // leave breathing room for overlay scrollbar
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .children(item_elements);

        let scroll_style = catalog_scrollbar_style();
        let mut viewport = div()
            .id("catalog-list-viewport")
            .relative()
            .flex()
            .flex_col()
            .w(px(340.0))
            .min_w(px(340.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_scroll_handle, &scroll_style) {
            let dragging = self.catalog_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_catalog_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.catalog_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport
    }

    /// Renders the right detail preview pane of the currently selected plugin.
    pub(super) fn render_catalog_details(
        &self,
        entry_opt: Option<&CatalogPluginEntry>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.plugin_catalog.search_query.clone();
        let terms = highlight_query_terms(&query);

        let Some(entry) = entry_opt else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.text_muted)
                .child(if is_ru { "Выберите плагин из списка" } else { "Select a plugin from the list" })
                .into_any_element();
        };

        let plugin_dir = self
            .state
            .vault
            .as_ref()
            .map(|v| v.root.join(".vault").join("plugins").join(&entry.id));

        let is_installed = plugin_dir.as_ref().map(|d| d.exists()).unwrap_or(false);

        let installed_version = if is_installed {
            plugin_dir.as_ref().and_then(|d| {
                let manifest_path = d.join("manifest.json");
                std::fs::read_to_string(manifest_path).ok().and_then(|c| {
                    serde_json::from_str::<serde_json::Value>(&c).ok().and_then(|j| {
                        j.get("version").and_then(|v| v.as_str()).map(|s| s.to_string())
                    })
                })
            })
        } else {
            None
        };

        let is_enabled = self
            .state
            .vault
            .as_ref()
            .map(|v| v.is_community_plugin_enabled(&entry.id))
            .unwrap_or(false);

        let is_this_downloading = self.plugin_catalog.is_downloading
            && self.plugin_catalog.downloading_plugin_id.as_deref() == Some(&entry.id);

        let plugin_id = entry.id.clone();
        let plugin_id_uninstall = entry.id.clone();
        let plugin_id_toggle = entry.id.clone();
        let author_name = entry.author.clone();
        let author_url = format!("https://github.com/{}", entry.author);
        let repo_url = format!("https://github.com/{}", entry.repo);
        let author_click = author_url.clone();
        let repo_click = repo_url.clone();

        let version_line = if let Some(inst_ver) = &installed_version {
            format!(
                "{}: {} ({}: {})",
                if is_ru { "Версия" } else { "Version" },
                entry.version,
                if is_ru { "установлена" } else { "installed" },
                inst_ver
            )
        } else {
            format!("{}: {}", if is_ru { "Версия" } else { "Version" }, entry.version)
        };

        let author_line_prefix = if is_ru { "Автор: " } else { "Author: " };
        let repo_line_prefix = if is_ru { "Репозиторий: " } else { "Repository: " };
        let updated_line_prefix = if is_ru { "Последнее обновление: " } else { "Last updated: " };
        let updated_text = format_updated_date(entry.updated, is_ru);

        // Compatibility from the embedded corpus registry (static doctor scan of the
        // real top-1500 Obsidian corpus); absent for plugins outside the corpus.
        let compat = crate::plugins::compat::catalog_compat(&entry.id);
        let (compat_text, compat_bg, compat_fg, compat_detail) = match &compat {
            Some(record) => {
                let (text, bg, fg) = match record.status {
                    crate::plugins::compat::CompatibilityStatus::Full => (
                        if is_ru { "Совместим" } else { "Compatible" }.to_string(),
                        rgba(0x2ea44f22),
                        rgba(0x3fb950ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Partial(score) => (
                        if is_ru {
                            format!("Совместим {:.0}%", score * 100.0)
                        } else {
                            format!("Compatible {:.0}%", score * 100.0)
                        },
                        rgba(0xdbab0922),
                        rgba(0xd29922ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Incompatible => (
                        if is_ru { "Не совместим" } else { "Incompatible" }.to_string(),
                        rgba(0xf8514922),
                        rgba(0xf85149ff),
                    ),
                    crate::plugins::compat::CompatibilityStatus::Unknown => (
                        if is_ru { "Совместимость неизвестна" } else { "Compatibility unknown" }.to_string(),
                        rgba(0x8b949e22),
                        rgba(0x8b949eff),
                    ),
                };
                let mut detail = String::new();
                if !record.blockers.is_empty() {
                    detail = if is_ru {
                        format!("Требует: {}", record.blockers.join(", "))
                    } else {
                        format!("Requires: {}", record.blockers.join(", "))
                    };
                } else if !record.missing_exports.is_empty() {
                    detail = if is_ru {
                        format!("Нет API: {}", record.missing_exports.join(", "))
                    } else {
                        format!("Missing API: {}", record.missing_exports.join(", "))
                    };
                } else if !record.missing_app.is_empty() {
                    detail = if is_ru {
                        format!("Нет: app.{}", record.missing_app.join(", app."))
                    } else {
                        format!("Missing: app.{}", record.missing_app.join(", app."))
                    };
                }
                (text, bg, fg, detail)
            }
            None => (
                if is_ru { "Не проверен" } else { "Unverified" }.to_string(),
                rgba(0x8b949e18),
                theme.text_muted,
                String::new(),
            ),
        };

        // Top actions bar
        let top_actions_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_2()
            .when(!is_installed && !is_this_downloading, |parent| {
                parent.child(
                    div()
                        .id("btn-install-catalog-plugin")
                        .px_3p5()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.accent)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.accent_hover))
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_on_accent)
                        .child(if is_ru { "Установить" } else { "Install" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install_plugin_from_catalog(&plugin_id, cx);
                        })),
                )
            })
            .when(!is_installed && is_this_downloading, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.bg_hover)
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_muted)
                        .child(icons::icon(icons::ICON_REFRESH, 12.0, theme.accent))
                        .child(if is_ru { "Скачивание..." } else { "Downloading..." }),
                )
            })
            .when(is_installed, |parent| {
                parent
                    .child(
                        div()
                            .id("btn-toggle-catalog-plugin")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(if is_enabled { theme.bg_hover } else { theme.accent })
                            .cursor_pointer()
                            .hover(|h| h.bg(if is_enabled { theme.border } else { theme.accent_hover }))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if is_enabled { theme.text_primary } else { theme.text_on_accent })
                            .child(if is_enabled {
                                if is_ru { "Отключить" } else { "Disable" }
                            } else {
                                if is_ru { "Включить" } else { "Enable" }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_community_plugin(&plugin_id_toggle, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-uninstall-catalog-plugin")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(theme.bg_hover)
                            .cursor_pointer()
                            .hover(|h| h.bg(rgba(0xea433522)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgba(0xea4335ff))
                            .child(if is_ru { "Удалить" } else { "Uninstall" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.uninstall_plugin_from_catalog(&plugin_id_uninstall, cx);
                            })),
                    )
                    .when_some(plugin_dir.clone(), |p, d| {
                        let d_str = d.to_string_lossy().to_string();
                        p.child(
                            div()
                                .id("btn-catalog-open-folder")
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.bg_hover)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.border))
                                .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_muted))
                                .on_click(cx.listener(move |_this, _, _, _cx| {
                                    let _ = std::process::Command::new("explorer").arg(&d_str).spawn();
                                })),
                        )
                    })
            });

        // Top info card
        let info_card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_2()
            .p_5()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            // Title row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2p5()
                    .child(icons::icon(icons::ICON_PLUG, 22.0, theme.accent))
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(highlight_terms(&entry.name, &terms, theme.text_primary, false, theme)),
                    )
                    .when(is_installed, |p| {
                        p.child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(rgba(0x7c4dffcc))
                                .text_color(gpui::white())
                                .child(if is_ru { "УСТАНОВЛЕН" } else { "INSTALLED" }),
                        )
                    }),
            )
            // Download row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(icons::icon(icons::ICON_DOWNLOAD, 13.0, theme.text_muted))
                    .child(format_download_count(entry.downloads)),
            )
            // Version line
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(version_line),
            )
            // Author line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(author_line_prefix))
                    .child(
                        div()
                            .id("catalog-author-link")
                            .text_color(rgba(0x7c4dffff))
                            .cursor_pointer()
                            .hover(|h| h.underline())
                            .child(author_name)
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.open_url(&author_click);
                            })),
                    ),
            )
            // Repo line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(repo_line_prefix))
                    .child(
                        div()
                            .id("catalog-repo-link")
                            .text_color(rgba(0x7c4dffff))
                            .cursor_pointer()
                            .hover(|h| h.underline())
                            .child(repo_url.clone())
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.open_url(&repo_click);
                            })),
                    ),
            )
            // Last updated line
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(updated_line_prefix))
                    .child(
                        div()
                            .text_color(rgba(0x7c4dffff))
                            .child(updated_text),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(div().text_color(theme.text_secondary).child(if is_ru { "Совместимость: " } else { "Compatibility: " }))
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(compat_bg)
                            .text_color(compat_fg)
                            .child(compat_text.clone()),
                    ),
            )
            .when(!compat_detail.is_empty(), |parent| {
                parent.child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(compat_detail.clone()),
                )
            })
            // Description
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .mt_1()
                    .text_sm()
                    .line_height(gpui::relative(1.4))
                    .text_color(theme.text_primary)
                    .whitespace_normal()
                    .child(highlight_terms(&entry.description, &terms, theme.text_primary, false, theme)),
            );

        // README Section
        let readme_text = self.plugin_catalog.readme_cache.get(&entry.id).cloned().or_else(|| {
            plugin_dir.as_ref().and_then(|d| {
                for name in &["README.md", "readme.md"] {
                    let f = d.join(name);
                    if f.is_file() {
                        if let Ok(c) = std::fs::read_to_string(f) {
                            return Some(c);
                        }
                    }
                }
                None
            })
        });

        let readme_section = if self.plugin_catalog.is_loading_readme {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .p_8()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(icons::icon(icons::ICON_REFRESH, 16.0, theme.accent))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(if is_ru {
                                    "Загрузка README.md из GitHub..."
                                } else {
                                    "Loading README.md from GitHub..."
                                }),
                        ),
                )
        } else if let Some(text) = readme_text {
            let rendered_markdown_lines = self.render_markdown_content(&text, cx);
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .overflow_x_hidden()
                        .p_5()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .children(rendered_markdown_lines),
                )
        } else {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .w_full()
                        .min_w_0()
                        .p_8()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(if is_ru {
                                    "README.md не найден"
                                } else {
                                    "README.md not available"
                                }),
                        ),
                )
        };

        let scroll_container = div()
            .id("catalog-details-scroll")
            .track_scroll(&self.catalog_details_scroll_handle)
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .bg(theme.bg_editor)
            .p_6()
            .pr_8() // leave breathing room for overlay scrollbar
            .gap_4()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_details_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_details_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_details_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(top_actions_bar)
            .when(is_this_downloading, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .p_2p5()
                        .rounded_md()
                        .bg(rgba(0x4285f422))
                        .border_1()
                        .border_color(rgba(0x4285f455))
                        .child(icons::icon(icons::ICON_REFRESH, 14.0, theme.accent))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.accent)
                                .child(self.plugin_catalog.download_step.clone().unwrap_or_else(|| {
                                    if is_ru {
                                        "Загрузка файлов из GitHub Releases...".to_string()
                                    } else {
                                        "Downloading files from GitHub Releases...".to_string()
                                    }
                                })),
                        ),
                )
            })
            .child(info_card)
            .child(readme_section);

        let scroll_style = catalog_scrollbar_style();
        let mut viewport = div()
            .id("catalog-details-viewport")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let line_h = 36.0;
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                };
                if dy != 0.0 {
                    let cur = f32::from(this.catalog_details_scroll_handle.offset().y);
                    let max = f32::from(this.catalog_details_scroll_handle.max_offset().height);
                    let next = (cur + dy).clamp(-max, 0.0);
                    if next != cur {
                        this.catalog_details_scroll_handle.set_offset(point(px(0.0), px(next)));
                        cx.notify();
                    }
                }
                cx.stop_propagation();
            }))
            .child(scroll_container);

        if let Some(geometry) = ScrollbarGeometry::from_handle(&self.catalog_details_scroll_handle, &scroll_style) {
            let dragging = self.catalog_details_scrollbar_drag.active;
            viewport = viewport.child(overlay_scrollbar(
                "catalog-details-scrollbar-track",
                &geometry,
                &scroll_style,
                theme,
                dragging,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let pointer_y: f32 = event.position.y.into();
                    this.set_catalog_details_scroll_top(geometry.offset_for_track_click(pointer_y));
                    cx.notify();
                }),
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.catalog_details_scrollbar_drag.begin(event.position.y.into(), geometry.scroll_y);
                    cx.notify();
                }),
            ));
        }

        viewport.into_any_element()
    }
}
