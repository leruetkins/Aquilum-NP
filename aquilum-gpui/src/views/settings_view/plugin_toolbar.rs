use super::*;

impl UiSink for SettingsView {
    fn send_plugin_ui_event(
        &mut self,
        plugin_id: &str,
        view_id: u64,
        node_id: String,
        event: &str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(app) = self.weak_app.upgrade() {
            let pid = plugin_id.to_string();
            app.update(cx, |app, cx| {
                app.send_plugin_ui_event(&pid, view_id, node_id, event, value, cx);
            });
        }
    }

    fn plugin_input_focus_handle(&self) -> &FocusHandle {
        &self.plugin_input_focus_handle
    }

    fn plugin_input_blink_enable(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.enable_view(|s| &mut s.plugin_input_blink, cx);
    }

    fn plugin_input_blink_pause_and_show(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.pause_and_show_view(|s| &mut s.plugin_input_blink, cx);
    }

    fn active_plugin_input(&self) -> Option<&PluginInputFocusState> {
        self.active_plugin_input.as_ref()
    }

    fn set_active_plugin_input(&mut self, state: Option<PluginInputFocusState>) {
        self.active_plugin_input = state;
    }

    fn active_plugin_input_mut(&mut self) -> Option<&mut PluginInputFocusState> {
        self.active_plugin_input.as_mut()
    }

    fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut input_state) = self.active_plugin_input.clone() else {
            return;
        };

        self.plugin_input_blink.pause_and_show_view(|s| &mut s.plugin_input_blink, cx);

        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;

        if key == "escape" {
            self.active_plugin_input = None;
            self.plugin_input_blink.disable_view(cx);
            cx.notify();
            return;
        }

        let mut chars: Vec<char> = input_state.value.chars().collect();
        let total_chars = chars.len();
        let mut pos = input_state.cursor_pos.min(total_chars);
        let sel_range = if let Some(anchor) = input_state.selection_anchor {
            if anchor != pos {
                Some((anchor.min(pos), anchor.max(pos)))
            } else {
                None
            }
        } else {
            None
        };

        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input_state.selection_anchor = Some(0);
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            let text_to_copy = if let Some((start, end)) = sel_range {
                chars.iter().skip(start).take(end - start).collect::<String>()
            } else {
                input_state.value.clone()
            };
            if !text_to_copy.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text_to_copy));
            }
            return;
        }

        let mut text_changed = false;

        let delete_selection = |chars: &mut Vec<char>, start: usize, end: usize| {
            chars.drain(start..end);
        };

        if key == "backspace" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut chars, start, end);
                pos = start;
                input_state.selection_anchor = None;
                text_changed = true;
            } else if pos > 0 {
                chars.remove(pos - 1);
                pos -= 1;
                text_changed = true;
            }
        } else if key == "delete" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut chars, start, end);
                pos = start;
                input_state.selection_anchor = None;
                text_changed = true;
            } else if pos < total_chars {
                chars.remove(pos);
                text_changed = true;
            }
        } else if key == "arrowleft" || key == "left" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
                pos = pos.saturating_sub(1);
            } else if let Some((start, _)) = sel_range {
                input_state.selection_anchor = None;
                pos = start;
            } else {
                input_state.selection_anchor = None;
                pos = pos.saturating_sub(1);
            }
        } else if key == "arrowright" || key == "right" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
                pos = (pos + 1).min(total_chars);
            } else if let Some((_, end)) = sel_range {
                input_state.selection_anchor = None;
                pos = end;
            } else {
                input_state.selection_anchor = None;
                pos = (pos + 1).min(total_chars);
            }
        } else if key == "home" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            pos = 0;
        } else if key == "end" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            pos = total_chars;
        } else if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let paste = if input_state.is_multiline {
                        text
                    } else {
                        text.replace('\n', " ").replace('\r', "")
                    };
                    if let Some((start, end)) = sel_range {
                        delete_selection(&mut chars, start, end);
                        pos = start;
                        input_state.selection_anchor = None;
                    }
                    for c in paste.chars() {
                        chars.insert(pos, c);
                        pos += 1;
                    }
                    text_changed = true;
                }
            }
        } else if key == "enter" {
            if input_state.is_multiline {
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut chars, start, end);
                    pos = start;
                    input_state.selection_anchor = None;
                }
                chars.insert(pos, '\n');
                pos += 1;
                text_changed = true;
            }
        } else {
            let typed = if let Some(c) = &event.keystroke.key_char {
                c.as_str()
            } else if key.chars().count() == 1 {
                key.as_str()
            } else {
                ""
            };
            if !typed.is_empty() && !ctrl {
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut chars, start, end);
                    pos = start;
                    input_state.selection_anchor = None;
                }
                for c in typed.chars() {
                    chars.insert(pos, c);
                    pos += 1;
                }
                text_changed = true;
            }
        }

        input_state.value = chars.into_iter().collect();
        input_state.cursor_pos = pos;

        if text_changed {
            if let Some(app) = self.weak_app.upgrade() {
                let pid = input_state.plugin_id.clone();
                let vid = input_state.view_id;
                let nid = input_state.node_id.clone();
                let val = input_state.value.clone();
                app.update(cx, |app, cx| {
                    if let Some(tree) = app.plugin_views.get_mut(&(pid.clone(), vid)) {
                        tree.update_input_value(&nid, &val);
                    }
                    let _ = app.state.plugin_host.send_event(
                        &pid,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id: vid,
                            node_id: nid,
                            event: "change".to_string(),
                            value: Some(val),
                        },
                    );
                    cx.notify();
                });
            }
        }

        self.active_plugin_input = Some(input_state);
        cx.notify();
    }

    fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<SelectOption>,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_plugin_select(plugin_id, view_id, node_id, options, current_value, window, cx);
    }

    fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        self.close_plugin_select(cx);
    }

    fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.is_plugin_select_open(plugin_id, view_id, node_id)
    }
}

impl SettingsView {
    /// Renders the complete settings and customization view for the Editor Toolbar plugin.
    pub(super) fn render_editor_toolbar_settings(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        let items = crate::plugins::editor_toolbar::load_toolbar_config(vault_root.as_deref());

        // Back button to Core Plugins
        let back_btn = div()
            .id("btn-back-to-core-plugins")
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .rounded_md()
            .bg(theme.bg_hover)
            .cursor_pointer()
            .hover(|h| h.bg(theme.border))
            .child(icons::icon(icons::ICON_ARROW_LEFT, 14.0, theme.text_secondary))
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.back")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_tab = SettingsTab::CorePlugins;
                cx.notify();
            }));

        // Header
        let header = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.title")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(self.t("settings.toolbar.desc")),
            );

        // Status banner
        let status_banner = self.toolbar_status_msg.as_ref().map(|msg| {
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(theme.bg_hover)
                .border_1()
                .border_color(theme.accent)
                .text_xs()
                .text_color(theme.accent)
                .child(msg.clone())
        });

        // Top Actions Bar (Buttons)
        let actions_bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("tb-cfg-add-btn")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.accent)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.accent_hover))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_on_accent)
                    .child(self.t("settings.toolbar.add_button"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_add_button_open = !this.toolbar_add_button_open;
                        this.toolbar_add_dropdown_open = false;
                        this.toolbar_command_dropdown_open = false;
                        this.toolbar_icon_dropdown_open = false;
                        this.toolbar_selected_command = None;
                        this.toolbar_selected_icon = None;
                        this.toolbar_custom_tooltip.clear();
                        this.close_toolbar_command_popup(cx);
                        this.close_toolbar_icon_popup(cx);
                        this.toolbar_target_dropdown_id = None;
                        cx.notify();
                    }))
            )
            .child(
                div()
                    .id("tb-cfg-add-dropdown")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.add_dropdown"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_add_dropdown_open = !this.toolbar_add_dropdown_open;
                        this.toolbar_add_button_open = false;
                        this.close_toolbar_command_popup(cx);
                        this.close_toolbar_icon_popup(cx);
                        this.toolbar_target_dropdown_id = None;
                        cx.notify();
                    }))
            )
            .child(
                div()
                    .id("tb-cfg-add-sep")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.add_separator"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let sep = crate::toolbar::ToolbarItem::separator();
                        this.toolbar_add_item_at_target(sep, cx);
                    }))
            )
            .child(
                div()
                    .id("tb-cfg-reset")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(self.t("settings.toolbar.reset"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_reset_defaults(cx);
                    }))
            )
            .child(
                div()
                    .id("tb-cfg-export")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(self.t("settings.toolbar.export"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_export_json(cx);
                    }))
            )
            .child(
                div()
                    .id("tb-cfg-import")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(self.t("settings.toolbar.import"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_import_json(cx);
                    }))
            );

        // Add Forms
        let add_button_form = if self.toolbar_add_button_open {
            Some(self.render_toolbar_add_button_form(window, cx))
        } else {
            None
        };

        let add_dropdown_form = if self.toolbar_add_dropdown_open {
            Some(self.render_toolbar_add_dropdown_form(cx))
        } else {
            None
        };

        // Render current items list
        let mut items_container = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_4()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle);

        let total_items = items.len();
        for (i, it) in items.iter().enumerate() {
            let row = self.render_toolbar_item_row(it, &[i], i, total_items, cx);
            items_container = items_container.child(row);
        }

        let mut content = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(back_btn)
            .child(header);

        if let Some(banner) = status_banner {
            content = content.child(banner);
        }

        content = content.child(actions_bar);

        if let Some(form) = add_button_form {
            content = content.child(form);
        }
        if let Some(form) = add_dropdown_form {
            content = content.child(form);
        }

        content = content.child(items_container);
        content
    }

    /// Renders an item row with Move Up, Move Down, Delete, and recursive sub-items.
    pub(super) fn render_toolbar_item_row(
        &self,
        item: &crate::toolbar::ToolbarItem,
        path: &[usize],
        idx: usize,
        total: usize,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let theme = &self.theme;

        let path_vec = path.to_vec();
        let path_move_up = path_vec.clone();
        let path_move_down = path_vec.clone();
        let path_delete = path_vec.clone();

        let can_move_up = idx > 0;
        let can_move_down = idx + 1 < total;
        let item_id_str = item.id.clone();

        let move_up_btn = div()
            .id(gpui::SharedString::from(format!("tb-up-{}", item_id_str)))
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .when(can_move_up, |d| {
                d.cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .text_color(theme.text_primary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toolbar_move_item_at_path(&path_move_up, true, cx);
                    }))
            })
            .when(!can_move_up, |d| d.opacity(0.3).cursor_not_allowed())
            .child("↑");

        let move_down_btn = div()
            .id(gpui::SharedString::from(format!("tb-down-{}", item_id_str)))
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .when(can_move_down, |d| {
                d.cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .text_color(theme.text_primary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toolbar_move_item_at_path(&path_move_down, false, cx);
                    }))
            })
            .when(!can_move_down, |d| d.opacity(0.3).cursor_not_allowed())
            .child("↓");

        let delete_btn = div()
            .id(gpui::SharedString::from(format!("tb-del-{}", item_id_str)))
            .px_2()
            .py_0p5()
            .rounded_sm()
            .cursor_pointer()
            .hover(|h| h.bg(theme.bg_hover))
            .text_color(theme.text_muted)
            .child("✕")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toolbar_delete_item_at_path(&path_delete, cx);
            }));

        let mut row_container = div()
            .flex()
            .flex_col()
            .gap_1();

        match &item.kind {
            crate::toolbar::ToolbarItemKind::Separator => {
                let main_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_editor)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(self.t("settings.toolbar.separator")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(move_up_btn)
                            .child(move_down_btn)
                            .child(delete_btn),
                    );
                row_container = row_container.child(main_row);
            }
            crate::toolbar::ToolbarItemKind::Button { command_id, icon, tooltip } => {
                let icon_asset = icons::resolve_toolbar_icon(icon);
                let label = tooltip.clone().unwrap_or_else(|| command_id.clone());
                let is_plugin = self
                    .weak_app
                    .upgrade()
                    .and_then(|a| a.read(cx).state.commands.get(command_id))
                    .map(|c| is_plugin_command(&c.id, c.owner.as_deref()))
                    .unwrap_or_else(|| is_plugin_command(command_id, None));
                let cmd_type_icon = command_type_icon(is_plugin);

                let main_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_editor)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                gpui::svg()
                                    .path(icon_asset)
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .text_color(theme.accent)
                                    .flex_shrink_0()
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .child(icons::icon(cmd_type_icon, 12.0, theme.text_muted)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(format!("({})", command_id)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(move_up_btn)
                            .child(move_down_btn)
                            .child(delete_btn),
                    );
                row_container = row_container.child(main_row);
            }
            crate::toolbar::ToolbarItemKind::Dropdown { icon, tooltip, command, items: sub_items } => {
                let icon_asset = icons::resolve_toolbar_icon(icon);
                let title = tooltip
                    .clone()
                    .unwrap_or_else(|| self.t("settings.toolbar.dropdown_fallback"));
                let target_dd_id = item.id.clone();
                let split_type_icon = command.as_ref().map(|cmd_id| {
                    let is_plugin = self
                        .weak_app
                        .upgrade()
                        .and_then(|a| a.read(cx).state.commands.get(cmd_id))
                        .map(|c| is_plugin_command(&c.id, c.owner.as_deref()))
                        .unwrap_or_else(|| is_plugin_command(cmd_id, None));
                    command_type_icon(is_plugin)
                });

                let target_dd_str = target_dd_id.clone();
                let add_inside_btn = div()
                    .id(gpui::SharedString::from(format!("tb-inside-{}", target_dd_str)))
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(theme.bg_hover)
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .text_color(theme.accent)
                    .child(self.t("settings.toolbar.add_inside"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toolbar_target_dropdown_id = Some(target_dd_id.clone());
                        this.toolbar_add_button_open = true;
                        this.toolbar_command_dropdown_open = false;
                        this.toolbar_icon_dropdown_open = false;
                        this.toolbar_selected_command = None;
                        this.toolbar_selected_icon = None;
                        this.toolbar_custom_tooltip.clear();
                        this.close_toolbar_command_popup(cx);
                        this.close_toolbar_icon_popup(cx);
                        cx.notify();
                    }));

                let main_row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                gpui::svg()
                                    .path(icon_asset)
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .text_color(theme.accent)
                                    .flex_shrink_0()
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child(format!("{} ▾", title)),
                            )
                            .when_some(split_type_icon, |row, ic| {
                                row.child(
                                    div()
                                        .flex_shrink_0()
                                        .child(icons::icon(ic, 12.0, theme.text_muted)),
                                )
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                        .child(format!(
                            "({}){}",
                            self.t_fmt("settings.toolbar.items", &[&sub_items.len().to_string()]),
                            if let Some(cmd) = command { format!(" [split: {}]", cmd) } else { "".to_string() }
                        )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(add_inside_btn)
                            .child(move_up_btn)
                            .child(move_down_btn)
                            .child(delete_btn),
                    );
                row_container = row_container.child(main_row);

                // Nested child items
                if !sub_items.is_empty() {
                    let mut sub_list = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pl_5()
                        .border_l_2()
                        .border_color(theme.border_subtle);

                    let sub_total = sub_items.len();
                    for (c_idx, child_item) in sub_items.iter().enumerate() {
                        let mut child_path = path_vec.clone();
                        child_path.push(c_idx);
                        let child_row = self.render_toolbar_item_row(
                            child_item,
                            &child_path,
                            c_idx,
                            sub_total,
                            cx,
                        );
                        sub_list = sub_list.child(child_row);
                    }
                    row_container = row_container.child(sub_list);
                }
            }
        }

        row_container
    }

    pub fn open_toolbar_command_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_toolbar_icon_popup(cx);
        let all_commands = self
            .weak_app
            .upgrade()
            .map(|a| a.read(cx).state.commands.all())
            .unwrap_or_default();

        let active_ix = self
            .toolbar_selected_command
            .as_ref()
            .and_then(|sel_id| all_commands.iter().position(|c| &c.id == sel_id));

        let tabs: Vec<TabEntry> = all_commands
            .iter()
            .enumerate()
            .map(|(id, cmd)| {
                let is_plugin = is_plugin_command(&cmd.id, cmd.owner.as_deref());
                let type_icon = command_type_icon(is_plugin);
                TabEntry {
                    id,
                    title: SharedString::from(cmd.name.clone()),
                    subtitle: Some(SharedString::from(cmd.id.clone())),
                    icon: type_icon,
                    font_family: None,
                    dot_color: None,
                    trailing_icon: None,
                }
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = all_commands.len().clamp(1, 8) as f32;
        let search_placeholder = self.t("settings.toolbar.search");
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                active_ix,
                theme,
                None,
                Some(px(42.0 * max_rows)),
                Some(50.0),
                cx,
            )
            .with_full_width()
            .with_subtitle_below()
            .with_search(search_placeholder)
        });

        let cmds_for_sub = all_commands;
        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(cmd) = cmds_for_sub.get(*id) {
                            this.toolbar_selected_command = Some(cmd.id.clone());
                            this.toolbar_custom_tooltip = cmd.name.clone();
                            if let Some(ref ic) = cmd.icon {
                                this.toolbar_selected_icon = Some(ic.clone());
                            }
                            cx.notify();
                        }
                        this.close_toolbar_command_popup(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_toolbar_command_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.toolbar_command_popup = Some(popup);
        self.toolbar_command_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_toolbar_command_popup(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_command_popup.take().is_none() {
            return;
        }
        self.toolbar_command_popup_subscription = None;
        cx.notify();
    }

    pub fn toolbar_command_popup_is_open(&self) -> bool {
        self.toolbar_command_popup.is_some()
    }

    pub fn open_toolbar_icon_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_toolbar_command_popup(cx);
        let available_icons = [
            "bold", "italic", "strikethrough", "underline", "highlighter",
            "h1", "h2", "h3", "h4", "h5", "h6", "heading-n",
            "undo", "redo", "eraser", "paperclip", "table", "check",
            "inline-code", "code-block", "pilcrow", "message-square", "list-bullet",
            "list-numbered", "list-task", "indent", "outdent",
            "align-left", "align-center", "align-right", "fullscreen",
            "search", "save", "settings", "file-text", "folder",
            "bookmark", "link", "external-link", "eye", "copy",
        ];

        let active_ix = self
            .toolbar_selected_icon
            .as_ref()
            .and_then(|sel_ic| available_icons.iter().position(|ic| ic == sel_ic));

        let tabs: Vec<TabEntry> = available_icons
            .iter()
            .enumerate()
            .map(|(id, ic)| TabEntry {
                id,
                title: SharedString::from(*ic),
                subtitle: None,
                icon: icons::map_obsidian_icon(ic),
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = available_icons.len().clamp(1, 10) as f32;
        let search_placeholder = self.t("settings.toolbar.search");
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                active_ix,
                theme,
                None,
                Some(px(ROW_HEIGHT * max_rows)),
                Some(50.0),
                cx,
            )
            .with_full_width()
            .with_search(search_placeholder)
        });

        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(ic) = available_icons.get(*id) {
                            this.toolbar_selected_icon = Some(ic.to_string());
                            cx.notify();
                        }
                        this.close_toolbar_icon_popup(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_toolbar_icon_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.toolbar_icon_popup = Some(popup);
        self.toolbar_icon_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_toolbar_icon_popup(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_icon_popup.take().is_none() {
            return;
        }
        self.toolbar_icon_popup_subscription = None;
        cx.notify();
    }

    pub fn toolbar_icon_popup_is_open(&self) -> bool {
        self.toolbar_icon_popup.is_some()
    }

    /// Renders inline form to add a new button.
    pub(super) fn render_toolbar_add_button_form(&self, _window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let all_commands = self
            .weak_app
            .upgrade()
            .map(|a| a.read(cx).state.commands.all())
            .unwrap_or_default();

        let selected_cmd = self
            .toolbar_selected_command
            .as_ref()
            .and_then(|sel_id| all_commands.iter().find(|c| &c.id == sel_id));

        let is_cmd_open = self.toolbar_command_popup_is_open();
        let cmd_select_trigger = div()
            .id("tb-cfg-cmd-select-trigger")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(32.0))
            .px_3()
            .bg(theme.bg_editor)
            .border_1()
            .border_color(if is_cmd_open { theme.accent } else { theme.border })
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.border_color(theme.accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(if let Some(cmd) = selected_cmd {
                        let is_plugin = is_plugin_command(&cmd.id, cmd.owner.as_deref());
                        let type_icon = command_type_icon(is_plugin);
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .child(icons::icon(type_icon, 14.0, theme.text_secondary)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(theme.text_primary)
                                    .child(cmd.name.clone()),
                            )
                    } else if let Some(ref sel_id) = self.toolbar_selected_command {
                        let is_plugin = is_plugin_command(sel_id, None);
                        let type_icon = command_type_icon(is_plugin);
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .child(icons::icon(type_icon, 14.0, theme.text_secondary)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(theme.text_primary)
                                    .child(sel_id.clone()),
                            )
                    } else {
                        div().text_color(theme.text_muted).child(
                            self.t("settings.toolbar.select_command"),
                        )
                    }),
            )
            .child(
                if self.toolbar_selected_command.is_some() {
                    div()
                        .id("tb-cfg-cmd-clear-btn")
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|h| h.text_color(theme.text_primary))
                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toolbar_selected_command = None;
                            this.toolbar_custom_tooltip.clear();
                            this.close_toolbar_command_popup(cx);
                            cx.notify();
                        }))
                } else {
                    div()
                        .id("tb-cfg-cmd-chevron")
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted))
                }
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.toolbar_command_popup_is_open() {
                    this.close_toolbar_command_popup(cx);
                } else {
                    this.open_toolbar_command_popup(window, cx);
                }
            }));

        let is_icon_open = self.toolbar_icon_popup_is_open();
        let icon_select_trigger = div()
            .id("tb-cfg-icon-select-trigger")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(32.0))
            .px_3()
            .bg(theme.bg_editor)
            .border_1()
            .border_color(if is_icon_open { theme.accent } else { theme.border })
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.border_color(theme.accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(if let Some(ref icon_name) = self.toolbar_selected_icon {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                gpui::svg()
                                    .path(icons::resolve_toolbar_icon(icon_name))
                                    .w(px(16.0))
                                    .h(px(16.0))
                                    .text_color(theme.text_primary)
                                    .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .text_color(theme.text_primary)
                                    .child(icon_name.clone()),
                            )
                    } else {
                        div()
                            .text_color(theme.text_muted)
                            .child(self.t("settings.toolbar.select_icon"))
                    }),
            )
            .child(
                if self.toolbar_selected_icon.is_some() {
                    div()
                        .id("tb-cfg-icon-clear-btn")
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|h| h.text_color(theme.text_primary))
                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toolbar_selected_icon = None;
                            this.close_toolbar_icon_popup(cx);
                            cx.notify();
                        }))
                } else {
                    div()
                        .id("tb-cfg-icon-chevron")
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted))
                }
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.toolbar_icon_popup_is_open() {
                    this.close_toolbar_icon_popup(cx);
                } else {
                    this.open_toolbar_icon_popup(window, cx);
                }
            }));

        let cmd_column = div()
            .flex_1()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(self.t("settings.toolbar.step_command"))
                    )
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(format!("{}/{}", all_commands.len(), all_commands.len())),
                    ),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .child(cmd_select_trigger)
                    .when_some(self.toolbar_command_popup.clone(), |parent, popup| {
                        parent.child(
                            gpui::deferred(
                                div()
                                    .id("tb-cmd-popup-anchor")
                                    .absolute()
                                    .top_full()
                                    .left_0()
                                    .right_0()
                                    .w_full()
                                    .mt_1()
                                    .child(popup),
                            )
                            .with_priority(999),
                        )
                    }),
            );

        let icon_column = div()
            .flex_1()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(self.t("settings.toolbar.step_icon"))
                    ),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .child(icon_select_trigger)
                    .when_some(self.toolbar_icon_popup.clone(), |parent, popup| {
                        parent.child(
                            gpui::deferred(
                                div()
                                    .id("tb-icon-popup-anchor")
                                    .absolute()
                                    .top_full()
                                    .left_0()
                                    .right_0()
                                    .w_full()
                                    .mt_1()
                                    .child(popup),
                            )
                            .with_priority(999),
                        )
                    }),
            );

        let columns_row = div()
            .flex()
            .flex_row()
            .gap_3()
            .items_start()
            .w_full()
            .child(cmd_column)
            .child(icon_column);

        // ================= BOTTOM ACTIONS ROW =================
        let can_save = self.toolbar_selected_command.is_some();

        let actions_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("tb-save-custom-btn")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .when(can_save, |d| {
                        d.bg(theme.accent)
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.accent_hover))
                            .text_color(theme.text_on_accent)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(cmd_id) = this.toolbar_selected_command.clone() {
                                    let icon = this.toolbar_selected_icon.clone().unwrap_or_else(|| "inline-code".to_string());
                                    let tooltip = if this.toolbar_custom_tooltip.is_empty() {
                                        None
                                    } else {
                                        Some(this.toolbar_custom_tooltip.clone())
                                    };
                                    let new_btn = crate::toolbar::ToolbarItem::button(
                                        format!("tb-custom-{}", uuid::Uuid::new_v4().simple()),
                                        cmd_id,
                                        icon,
                                        tooltip.as_deref(),
                                    );
                                    this.toolbar_add_item_at_target(new_btn, cx);
                                    this.toolbar_add_button_open = false;
                                    this.toolbar_selected_command = None;
                                    this.toolbar_selected_icon = None;
                                    this.toolbar_custom_tooltip.clear();
                                    this.close_toolbar_command_popup(cx);
                                    this.close_toolbar_icon_popup(cx);
                                    cx.notify();
                                }
                            }))
                    })
                    .when(!can_save, |d| {
                        d.bg(theme.bg_hover)
                            .text_color(theme.text_muted)
                            .cursor_not_allowed()
                    })
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.t("settings.toolbar.save_button")),
            )
            .child(
                div()
                    .id("tb-cancel-custom-btn")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme.bg_hover)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.border))
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(self.t("common.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toolbar_add_button_open = false;
                        this.toolbar_selected_command = None;
                        this.toolbar_selected_icon = None;
                        this.toolbar_custom_tooltip.clear();
                        this.close_toolbar_command_popup(cx);
                        this.close_toolbar_icon_popup(cx);
                        cx.notify();
                    })),
            );

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.accent)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.add_formatting_button")),
            )
            .child(columns_row)
            .child(actions_row)
    }

    /// Renders inline form to add a new dropdown group.
    pub(super) fn render_toolbar_add_dropdown_form(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let available_icons = [
            "heading-n", "type", "list-bullet", "align-left", "message-square", "more-horizontal",
        ];

        let mut icon_grid = div().flex().flex_wrap().gap_1p5();
        for ic in available_icons {
            let is_sel = self.toolbar_selected_icon.as_deref() == Some(ic);
            let ic_str = ic.to_string();
            let icon_asset = icons::resolve_toolbar_icon(ic);

            let icon_box = div()
                .id(gpui::SharedString::from(format!("tb-sel-dd-icon-{}", ic)))
                .p_1()
                .rounded_md()
                .cursor_pointer()
                .when(is_sel, |d| d.bg(theme.accent).border_1().border_color(theme.accent))
                .when(!is_sel, |d| d.bg(theme.bg_editor).hover(|h| h.bg(theme.bg_hover)))
                .child(
                    gpui::svg()
                        .path(icon_asset)
                        .w(px(16.0))
                        .h(px(16.0))
                        .text_color(if is_sel { theme.text_on_accent } else { theme.text_primary })
                        .flex_shrink_0()
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toolbar_selected_icon = Some(ic_str.clone());
                    cx.notify();
                }));
            icon_grid = icon_grid.child(icon_box);
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.accent)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(self.t("settings.toolbar.create_group_title")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(self.t("settings.toolbar.select_group_icon")),
            )
            .child(icon_grid)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("tb-save-dd-btn")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(theme.accent)
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.accent_hover))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_on_accent)
                            .child(self.t("settings.toolbar.create_group"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let group_name = this.t("settings.toolbar.default_group");
                                let new_dropdown = crate::toolbar::ToolbarItem::dropdown(
                                    format!("tb-grp-{}", uuid::Uuid::new_v4().simple()),
                                    this.toolbar_selected_icon.clone().unwrap_or_else(|| "heading-n".to_string()),
                                    Some(&group_name),
                                    None,
                                    Vec::new(),
                                );
                                this.toolbar_add_item_at_target(new_dropdown, cx);
                                this.toolbar_add_dropdown_open = false;
                                this.toolbar_selected_icon = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("tb-cancel-dd-btn")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(theme.bg_hover)
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.border))
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .child(self.t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toolbar_add_dropdown_open = false;
                                this.toolbar_selected_icon = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Moves a toolbar item up or down at a hierarchical path.
    pub(super) fn toolbar_move_item_at_path(&mut self, path: &[usize], up: bool, cx: &mut Context<Self>) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        let mut items = crate::plugins::editor_toolbar::load_toolbar_config(vault_root.as_deref());

        if path.is_empty() {
            return;
        }

        if path.len() == 1 {
            let idx = path[0];
            if up && idx > 0 && idx < items.len() {
                items.swap(idx, idx - 1);
            } else if !up && idx + 1 < items.len() {
                items.swap(idx, idx + 1);
            }
        } else if path.len() == 2 {
            let p0 = path[0];
            let p1 = path[1];
            if let Some(parent) = items.get_mut(p0) {
                if let crate::toolbar::ToolbarItemKind::Dropdown { ref mut items, .. } = parent.kind {
                    if up && p1 > 0 && p1 < items.len() {
                        items.swap(p1, p1 - 1);
                    } else if !up && p1 + 1 < items.len() {
                        items.swap(p1, p1 + 1);
                    }
                }
            }
        }

        self.toolbar_save_and_sync(items, None, cx);
    }

    /// Deletes a toolbar item at a hierarchical path.
    pub(super) fn toolbar_delete_item_at_path(&mut self, path: &[usize], cx: &mut Context<Self>) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        let mut items = crate::plugins::editor_toolbar::load_toolbar_config(vault_root.as_deref());

        if path.is_empty() {
            return;
        }

        if path.len() == 1 {
            let idx = path[0];
            if idx < items.len() {
                items.remove(idx);
            }
        } else if path.len() == 2 {
            let p0 = path[0];
            let p1 = path[1];
            if let Some(parent) = items.get_mut(p0) {
                if let crate::toolbar::ToolbarItemKind::Dropdown { ref mut items, .. } = parent.kind {
                    if p1 < items.len() {
                        items.remove(p1);
                    }
                }
            }
        }

        self.toolbar_save_and_sync(items, None, cx);
    }

    /// Appends a new item into target dropdown group or to root items.
    pub(super) fn toolbar_add_item_at_target(&mut self, new_item: crate::toolbar::ToolbarItem, cx: &mut Context<Self>) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        let mut items = crate::plugins::editor_toolbar::load_toolbar_config(vault_root.as_deref());

        if let Some(ref target_id) = self.toolbar_target_dropdown_id {
            let mut added = false;
            for it in items.iter_mut() {
                if it.id == *target_id {
                    if let crate::toolbar::ToolbarItemKind::Dropdown { ref mut items, .. } = it.kind {
                        items.push(new_item.clone());
                        added = true;
                        break;
                    }
                }
            }
            if !added {
                items.push(new_item);
            }
        } else {
            items.push(new_item);
        }

        self.toolbar_save_and_sync(items, None, cx);
    }

    /// Resets the toolbar configuration to defaults.
    pub(super) fn toolbar_reset_defaults(&mut self, cx: &mut Context<Self>) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        if let Some(ref root) = vault_root {
            let _ = crate::plugins::editor_toolbar::reset_toolbar_config(root);
        }
        let defaults = crate::plugins::editor_toolbar::default_toolbar_items();
        let msg = self.t("settings.toolbar.status.reset_done");
        self.toolbar_save_and_sync(defaults, Some(&msg), cx);
    }

    /// Exports current toolbar configuration as JSON to clipboard.
    pub(super) fn toolbar_export_json(&mut self, cx: &mut Context<Self>) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));
        let items = crate::plugins::editor_toolbar::load_toolbar_config(vault_root.as_deref());

        let cfg = crate::plugins::editor_toolbar::ToolbarConfigFile {
            version: 1,
            items,
        };

        if let Ok(json_str) = serde_json::to_string_pretty(&cfg) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(json_str));
            self.toolbar_status_msg = Some(self.t("settings.toolbar.status.copied"));
            cx.notify();
        }
    }

    /// Imports toolbar configuration from JSON in clipboard.
    pub(super) fn toolbar_import_json(&mut self, cx: &mut Context<Self>) {
        let clip_text = cx.read_from_clipboard().and_then(|item| item.text());
        let Some(text) = clip_text else {
            self.toolbar_status_msg = Some(self.t("settings.toolbar.status.clipboard_empty"));
            cx.notify();
            return;
        };

        let parsed_items: Option<Vec<crate::toolbar::ToolbarItem>> =
            serde_json::from_str::<crate::plugins::editor_toolbar::ToolbarConfigFile>(&text)
                .map(|c| c.items)
                .or_else(|_| serde_json::from_str::<Vec<crate::toolbar::ToolbarItem>>(&text))
                .ok();

        if let Some(items) = parsed_items {
            let validated = crate::toolbar::validate_and_prune_items(items, 1);
            let msg = self.t("settings.toolbar.status.import_done");
            self.toolbar_save_and_sync(validated, Some(&msg), cx);
        } else {
            self.toolbar_status_msg = Some(self.t("settings.toolbar.status.import_error"));
            cx.notify();
        }
    }

    /// Saves items to vault disk and live-updates toolbar registry in state.
    pub(super) fn toolbar_save_and_sync(
        &mut self,
        items: Vec<crate::toolbar::ToolbarItem>,
        msg: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let vault_root = self
            .weak_app
            .upgrade()
            .and_then(|a| a.read(cx).state.vault.as_ref().map(|v| v.root.clone()));

        if let Some(ref root) = vault_root {
            let _ = crate::plugins::editor_toolbar::save_toolbar_config(root, &items);
        }

        if let Some(app) = self.weak_app.upgrade() {
            let items_clone = items.clone();
            app.update(cx, |app, cx| {
                crate::plugins::editor_toolbar::register_or_update_bundled_toolbar(&app.state, items_clone);
                cx.notify();
            });
        }

        if let Some(m) = msg {
            self.toolbar_status_msg = Some(m.to_string());
        }
        cx.notify();
    }
}
