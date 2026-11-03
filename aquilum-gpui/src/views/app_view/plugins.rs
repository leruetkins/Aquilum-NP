use super::*;

impl AppView {
    pub fn handle_custom_hex_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        self.reset_hex_cursor_blink(cx);
        let key = &event.keystroke.key;
        let total_chars = self.custom_hex_input.chars().count();
        let sel_range = self.hex_selection_range();

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.hex_selection_anchor = Some(0);
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            let text_to_copy = if let Some((start, end)) = sel_range {
                self.custom_hex_input.chars().skip(start).take(end - start).collect::<String>()
            } else {
                self.custom_hex_input.clone()
            };
            if !text_to_copy.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text_to_copy));
            }
            return;
        }

        if key == "escape" {
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "enter" {
            self.apply_custom_hex_if_valid(cx);
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "arrowleft" || key == "left" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            } else if let Some((start, _)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = start;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            }
            cx.notify();
            return;
        }

        if key == "arrowright" || key == "right" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            } else if let Some((_, end)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = end;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            }
            cx.notify();
            return;
        }

        if key == "home" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = 0;
            cx.notify();
            return;
        }

        if key == "end" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        let delete_selection = |input: &mut String, start: usize, end: usize| {
            let before: String = input.chars().take(start).collect();
            let after: String = input.chars().skip(end).collect();
            *input = format!("{before}{after}");
        };

        if key == "backspace" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos > 0 {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos - 1);
                self.custom_hex_input = chars.into_iter().collect();
                self.hex_cursor_pos -= 1;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if key == "delete" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos < total_chars {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos);
                self.custom_hex_input = chars.into_iter().collect();
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text.trim().chars().filter(|c| c.is_ascii_hexdigit() || *c == '#').take(7).collect();
                    if !filtered.is_empty() {
                        let formatted = if filtered.starts_with('#') { filtered } else { format!("#{filtered}") };
                        self.custom_hex_input = formatted;
                        self.hex_cursor_pos = self.custom_hex_input.chars().count();
                        self.hex_selection_anchor = None;
                        self.apply_custom_hex_if_valid(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        let ch = if let Some(c) = &event.keystroke.key_char {
            c.as_str()
        } else if key.chars().count() == 1 {
            key.as_str()
        } else {
            ""
        };

        for c in ch.chars() {
            if c.is_ascii_hexdigit() || c == '#' {
                let upper = c.to_ascii_uppercase();
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut self.custom_hex_input, start, end);
                    self.hex_cursor_pos = start;
                    self.hex_selection_anchor = None;
                }
                if self.custom_hex_input.chars().count() < 7 {
                    let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                    chars.insert(self.hex_cursor_pos, upper);
                    self.custom_hex_input = chars.into_iter().collect();
                    self.hex_cursor_pos += 1;
                    self.apply_custom_hex_if_valid(cx);
                    cx.notify();
                }
            }
        }
    }

    pub fn apply_custom_hex_if_valid(&mut self, cx: &mut Context<Self>) {
        let Some(picker_id) = self.active_color_picker else {
            return;
        };
        if crate::markdown::parse_hex_color(&self.custom_hex_input).is_some() {
            let hex_val = if self.custom_hex_input.starts_with('#') {
                self.custom_hex_input.clone()
            } else {
                format!("#{}", self.custom_hex_input)
            };
            match picker_id {
                "h1-color" => self.config.editor.headings.h1.color_name = hex_val,
                "h2-color" => self.config.editor.headings.h2.color_name = hex_val,
                "h3-color" => self.config.editor.headings.h3.color_name = hex_val,
                "h4-color" => self.config.editor.headings.h4.color_name = hex_val,
                "h5-color" => self.config.editor.headings.h5.color_name = hex_val,
                "h6-color" => self.config.editor.headings.h6.color_name = hex_val,
                _ => {}
            }
            self.save_settings(cx);
        }
    }

    /// Checks whether any plugin modal dialog is currently active.
    pub fn has_active_plugin_modal(&self) -> bool {
        self.plugin_views.iter().any(|((_, vid), tree)| {
            if *vid != 1 {
                return false;
            }
            if let crate::plugins::ui::UiNode::Text { ref value, .. } = tree {
                !value.is_empty()
            } else {
                true
            }
        })
    }

    /// Closes an active plugin modal dialog and dispatches the close event to the plugin worker.
    pub fn close_active_plugin_modal(&mut self, cx: &mut Context<Self>) {
        println!("[app_view] close_active_plugin_modal called");
        let modal_keys: Vec<(String, u64)> = self.plugin_views.keys()
            .filter(|(_, vid)| *vid == 1)
            .cloned()
            .collect();
        for (p_id, vid) in modal_keys {
            println!("[app_view] Closing active modal for plugin '{}' (view_id = {})", p_id, vid);
            let _ = self.state.plugin_host.send_event(
                &p_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: vid,
                    node_id: "modal".to_string(),
                    event: "close".to_string(),
                    value: None,
                },
            );
            self.plugin_views.remove(&(p_id, vid));
        }
        self.active_plugin_input = None;
        self.active_plugin_select = None;
        self.plugin_modal_scrollbar_drag.end();
        self.plugin_modal_middle_active = false;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Renders an active plugin modal dialog overlay matching Obsidian modal style.
    pub fn render_plugin_modal_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let active_modal = self.plugin_views.iter().find(|((_, vid), tree)| {
            if *vid != 1 {
                return false;
            }
            if let crate::plugins::ui::UiNode::Text { ref value, .. } = tree {
                !value.is_empty()
            } else {
                true
            }
        });

        let ((p_id, _), modal_tree) = active_modal?;
        let p_id = p_id.clone();
        let caret_visible = self.plugin_input_blink.visible();

        let is_ru = self.config.ui.language.starts_with("ru");
        fn find_heading(node: &crate::plugins::ui::UiNode) -> Option<String> {
            match node {
                crate::plugins::ui::UiNode::Text { value, style: crate::plugins::ui::TextStyle::Heading, .. } => {
                    if !value.trim().is_empty() {
                        Some(value.clone())
                    } else {
                        None
                    }
                }
                crate::plugins::ui::UiNode::Column { children, .. } | crate::plugins::ui::UiNode::Row { children, .. } => {
                    children.iter().find_map(find_heading)
                }
                // The styles wrapper is not a node but a box: the heading behind it remains a heading.
                // Without this branch the modal showed two headings at once — its own and the fallback.
                crate::plugins::ui::UiNode::Styled { child, .. } => find_heading(child),
                _ => None,
            }
        }

        let has_heading = find_heading(modal_tree).is_some();
        let modal_title = find_heading(modal_tree).unwrap_or_else(|| {
            if is_ru { "Окно плагина".to_string() } else { "Plugin Dialog".to_string() }
        });

        let scroll_ctx = crate::plugins::ui::PluginScrollContext {
            scroll_handle: Some(&self.plugin_modal_scroll_handle),
            scrollbar_drag_active: self.plugin_modal_scrollbar_drag.active,
            middle_scroll_active: self.plugin_modal_middle_active,
            smart_bounds: Some(self.plugin_modal_smart_bounds.clone()),
        };

        let rendered_modal = crate::plugins::ui::render_ui_node(
            modal_tree,
            &p_id,
            1,
            theme,
            self.active_plugin_input.as_ref(),
            &self.plugin_input_focus_handle,
            self.active_plugin_select.as_ref(),
            self.plugin_select_popup.as_ref(),
            caret_visible,
            window,
            cx,
            Some(&scroll_ctx),
        );

        Some(
            gpui::deferred(
                div()
                    .id("global-plugin-modal-backdrop-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000cc))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_active_plugin_modal(cx);
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.plugin_modal_scrollbar_drag.end();
                    }))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if this.plugin_modal_middle_active {
                            let y: f32 = event.position.y.into();
                            this.on_plugin_modal_middle_mouse_move(y, cx);
                        } else if this.plugin_modal_scrollbar_drag.active {
                            let y: f32 = event.position.y.into();
                            this.handle_plugin_modal_scrollbar_drag(y, cx);
                        }
                    }))
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .id("global-plugin-modal-dialog-box")
                            .relative()
                            .flex()
                            .flex_col()
                            .w(px(580.0))
                            .max_w(px(660.0))
                            .max_h(px(640.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_xl()
                            .cursor(gpui::CursorStyle::Arrow)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.plugin_modal_scrollbar_drag.end();
                            }))
                            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.stop_plugin_modal_middle_scroll(cx);
                            }))
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                if this.plugin_modal_middle_active {
                                    let y: f32 = event.position.y.into();
                                    this.on_plugin_modal_middle_mouse_move(y, cx);
                                } else if this.plugin_modal_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_plugin_modal_scrollbar_drag(y, cx);
                                }
                            }))
                            .on_click(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Optional fallback title if modal tree didn't provide its own heading
                            .when(!has_heading, |this| {
                                this.child(
                                    div()
                                        .px(px(24.0))
                                        .pt(px(20.0))
                                        .pb(px(8.0))
                                        .text_base()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .child(modal_title),
                                )
                            })
                            .child(
                                div()
                                    .id("global-plugin-modal-body")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(rendered_modal),
                            )
                            // Obsidian top-right [X] close button (rendered last so it sits above body)
                            .child(
                                div()
                                    .id("global-plugin-modal-close-x")
                                    .absolute()
                                    .top(px(14.0))
                                    .right(px(16.0))
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor(gpui::CursorStyle::PointingHand)
                                    .rounded_md()
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .text_color(theme.text_muted)
                                    .child(icons::icon(icons::ICON_CLOSE, 14.0, theme.text_muted))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        println!("[app_view] Close button [X] mouse down!");
                                        this.close_active_plugin_modal(cx);
                                    }))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        println!("[app_view] Close button [X] clicked!");
                                        this.close_active_plugin_modal(cx);
                                    })),
                            ),
                    ),
            )
            .with_priority(9999),
        )
    }

    /// Handles keyboard navigation and text editing inside an active plugin input field.
    pub fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut input_state) = self.active_plugin_input.clone() else {
            return;
        };

        self.plugin_input_blink.pause_and_show(BlinkTarget::PluginInput, cx);

        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;

        if key == "escape" {
            self.active_plugin_input = None;
            self.plugin_input_blink.disable(cx);
            cx.notify();
            return;
        }

        let mut chars: Vec<char> = input_state.value.chars().collect();
        let total_chars = chars.len();
        let mut changed = false;

        if key == "enter" {
            if input_state.is_multiline {
                if let Some((s, e)) = input_state.selection_range() {
                    chars.drain(s..e.min(chars.len()));
                    input_state.cursor_pos = s;
                    input_state.selection_anchor = None;
                }
                let pos = input_state.cursor_pos.min(chars.len());
                chars.insert(pos, '\n');
                input_state.cursor_pos = pos + 1;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                self.active_plugin_input = None;
                self.plugin_input_blink.disable(cx);
                cx.notify();
                return;
            }
        }

        if key == "arrowleft" || key == "left" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
                input_state.cursor_pos = input_state.cursor_pos.saturating_sub(1);
            } else {
                if let Some((s, _)) = input_state.selection_range() {
                    input_state.cursor_pos = s;
                } else {
                    input_state.cursor_pos = input_state.cursor_pos.saturating_sub(1);
                }
                input_state.selection_anchor = None;
            }
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "arrowright" || key == "right" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
                input_state.cursor_pos = (input_state.cursor_pos + 1).min(total_chars);
            } else {
                if let Some((_, e)) = input_state.selection_range() {
                    input_state.cursor_pos = e;
                } else {
                    input_state.cursor_pos = (input_state.cursor_pos + 1).min(total_chars);
                }
                input_state.selection_anchor = None;
            }
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "home" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            input_state.cursor_pos = 0;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "end" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input_state.selection_anchor = Some(0);
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = input_state.selection_range() {
                let sub: String = chars[s..e.min(chars.len())].iter().collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(sub));
            } else if !input_state.value.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(input_state.value.clone()));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = input_state.selection_range() {
                let sub: String = chars[s..e.min(chars.len())].iter().collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(sub));
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            }
        }

        // Ctrl+V paste
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let is_multi = input_state.is_multiline;
                    let filtered: String = text
                        .chars()
                        .filter(|c| (!c.is_control() || (is_multi && (*c == '\n' || *c == '\t'))) && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        if let Some((s, e)) = input_state.selection_range() {
                            chars.drain(s..e.min(chars.len()));
                            input_state.cursor_pos = s;
                            input_state.selection_anchor = None;
                        }
                        let pos = input_state.cursor_pos.min(chars.len());
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        input_state.cursor_pos = pos + filtered.chars().count();
                        input_state.value = chars.iter().collect();
                        changed = true;
                    }
                }
            }
        } else if key == "backspace" {
            if let Some((s, e)) = input_state.selection_range() {
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                let pos = input_state.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    input_state.cursor_pos = pos - 1;
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        } else if key == "delete" {
            if let Some((s, e)) = input_state.selection_range() {
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                let pos = input_state.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        } else if !ctrl && !alt && !event.keystroke.modifiers.platform && key != "enter" {
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
                let is_multi = input_state.is_multiline;
                let filtered: String = text
                    .chars()
                    .filter(|c| (!c.is_control() || (is_multi && (*c == '\n' || *c == '\t'))) && *c != '\r')
                    .collect();
                if !filtered.is_empty() {
                    if let Some((s, e)) = input_state.selection_range() {
                        chars.drain(s..e.min(chars.len()));
                        input_state.cursor_pos = s;
                        input_state.selection_anchor = None;
                    }
                    let pos = input_state.cursor_pos.min(chars.len());
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    input_state.cursor_pos = pos + filtered.chars().count();
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        }

        if changed {
            let new_val = input_state.value.clone();
            let p_id = input_state.plugin_id.clone();
            let v_id = input_state.view_id;
            let n_id = input_state.node_id.clone();

            self.active_plugin_input = Some(input_state);

            if let Some(tree) = self.plugin_views.get_mut(&(p_id.clone(), v_id)) {
                tree.update_input_value(&n_id, &new_val);
            }

            let _ = self.state.plugin_host.send_event(
                &p_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: v_id,
                    node_id: n_id,
                    event: "change".to_string(),
                    value: Some(new_val),
                },
            );
        } else {
            self.active_plugin_input = Some(input_state);
        }

        cx.notify();
    }

    /// Triggers a view action registered via view.addAction(icon, title, callback)
    pub fn trigger_plugin_action(&mut self, plugin_id: &str, action_id: &str, cx: &mut Context<Self>) {
        let _ = self.state.plugin_host.send_event(
            plugin_id,
            crate::plugins::api::HostEvent::UiEvent {
                view_id: 0,
                node_id: action_id.to_string(),
                event: "action_click".to_string(),
                value: None,
            },
        );
        cx.notify();
    }

    /// Closes a plugin workspace leaf and notifies the plugin runtime (ItemView onClose)
    pub fn close_plugin_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        if let Some(leaf) = self.plugin_leaf_views.remove(leaf_id) {
            let _ = self.state.plugin_host.send_event(
                &leaf.plugin_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: 0,
                    node_id: leaf_id.to_string(),
                    event: "leaf_close".to_string(),
                    value: None,
                },
            );
        }
        if self.active_right_leaf.as_deref() == Some(leaf_id) {
            self.active_right_leaf = self
                .plugin_leaf_views
                .values()
                .find(|l| l.location == "right")
                .map(|l| l.leaf_id.clone());
        }
        if self.active_left_leaf.as_deref() == Some(leaf_id) {
            self.active_left_leaf = self
                .plugin_leaf_views
                .values()
                .find(|l| l.location == "left")
                .map(|l| l.leaf_id.clone());
            if let Some(ref next_id) = self.active_left_leaf {
                self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(next_id.clone());
            } else if matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)) {
                self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
            }
        }
        if self.active_center_leaf.as_deref() == Some(leaf_id) {
            self.active_center_leaf = None;
        }
        cx.notify();
    }

    /// Activates a plugin workspace leaf tab in the left sidebar
    pub fn select_left_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        if leaf_id == "recent-files" {
            self.open_sidebar_recent_files(cx);
            return;
        }
        self.active_left_leaf = Some(leaf_id.to_string());
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(leaf_id.to_string());
        self.sidebar_collapsed = false;
        cx.notify();
    }

    /// Activates a plugin workspace leaf tab in the right sidebar
    pub fn select_right_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        self.active_right_leaf = Some(leaf_id.to_string());
        self.right_sidebar_collapsed = false;
        cx.notify();
    }

    /// Renders the plugin leaf view placed in the left sidebar
    pub fn render_left_plugin_leaf(
        &self,
        leaf: &crate::plugins::api::PluginLeafView,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let mut container = div()
            .id(gpui::ElementId::Name(format!("left-sidebar-leaf-{}", leaf.leaf_id).into()))
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .gap(px(4.0));

        // 1. Header with title and action buttons
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(theme.border_subtle);

        header = header.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(crate::icons::icon(
                    crate::icons::map_obsidian_icon(leaf.icon.as_deref().unwrap_or("clock")),
                    13.0,
                    theme.accent,
                ))
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .truncate()
                        .child(leaf.title.clone()),
                ),
        );

        let mut actions_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1();

        for act in &leaf.actions {
            let act_id = act.id.clone();
            let act_plugin_id = leaf.plugin_id.clone();
            let act_icon = crate::icons::map_obsidian_icon(&act.icon);
            actions_bar = actions_bar.child(
                div()
                    .p(px(3.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.trigger_plugin_action(&act_plugin_id, &act_id, cx);
                        }),
                    )
                    .child(crate::icons::icon(act_icon, 13.0, theme.text_muted)),
            );
        }

        let leaf_id_close = leaf.leaf_id.clone();
        actions_bar = actions_bar.child(
            div()
                .px(px(4.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .text_xs()
                .text_color(theme.text_muted)
                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_plugin_leaf(&leaf_id_close, cx);
                    }),
                )
                .child("×"),
        );

        header = header.child(actions_bar);
        container = container.child(header);

        // 2. Leaf content
        container = container.child(
            div()
                .id(gpui::ElementId::Name(format!("left-sidebar-leaf-content-{}", leaf.leaf_id).into()))
                .flex()
                .flex_col()
                .flex_1()
                .overflow_scroll()
                .px_2()
                .py_1()
                .child(crate::plugins::ui::render_ui_node(
                    &leaf.tree,
                    &leaf.plugin_id,
                    0,
                    theme,
                    self.active_plugin_input.as_ref(),
                    &self.plugin_input_focus_handle,
                    self.active_plugin_select.as_ref(),
                    self.plugin_select_popup.as_ref(),
                    self.plugin_input_blink.visible(),
                    window,
                    cx,
                    None,
                )),
        );

        container
    }
}
