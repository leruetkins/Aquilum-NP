use super::*;

impl AppView {
    pub fn open_caption_editor_at_line(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line);
        let wrap_cols = self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80);
        let Some(image) = images::resolve_for_note(&doc.path, &text, wrap_cols, false) else {
            return;
        };
        self.open_caption_editor(line, &image, window, cx);
    }

    pub fn open_caption_editor(
        &mut self,
        line: usize,
        image: &images::ResolvedImage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_editor.is_some() {
            return;
        }
        let caption = image.edit.caption.clone().unwrap_or_else(|| images::Caption {
            text: String::new(),
            position: self.config.images.caption_position.clone(),
            numbered: self.config.images.caption_numbered,
        });
        // The editor container listens for keys on AppView's stored handle,
        // so the input must use that exact handle, not a fresh one.
        let focus = self.focus_handle.clone();
        let mut input = TextInput::new(focus.clone());
        input.set_text(caption.text.clone());
        input.select_all();
        window.focus(&focus);
        self.caption_editor = Some(CaptionEditorState {
            line,
            path: image.path.clone(),
            edit: image.edit.clone(),
            input,
            focus,
            position: if caption.position.is_empty() {
                self.config.images.caption_position.clone()
            } else {
                caption.position
            },
            numbered: caption.numbered,
            position_menu_open: false,
        });
        if let Some(state) = self.caption_editor.as_mut() {
            state
                .input
                .blink
                .enable(crate::blink_manager::BlinkTarget::Caption, cx);
        }
        self.pending_caption_reveal = Some(line);
        self.select_image_line(line);
        cx.notify();
    }

    pub fn save_caption_editor(&mut self, cx: &mut Context<Self>) {
        let Some(mut state) = self.caption_editor.take() else {
            return;
        };
        state.input.blink.disable(cx);
        let mut edit = state.edit;
        let text = state.input.text().trim().to_string();
        edit.caption = if text.is_empty() {
            None
        } else {
            Some(images::Caption {
                text,
                position: state.position,
                numbered: state.numbered,
            })
        };
        let _ = images::save_edit(&state.path, &edit);
        images::invalidate_resolved_images();
        if let Some(view) = self.state.active_view_mut() {
            view.visual_cache_dirty = true;
        }
        self.clear_image_selection();
        cx.notify();
    }

    pub fn cancel_caption_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(mut state) = self.caption_editor.take() {
            state.input.blink.disable(cx);
            self.clear_image_selection();
            cx.notify();
        }
    }

    pub(crate) fn handle_caption_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            if self
                .caption_editor
                .as_ref()
                .is_some_and(|state| state.position_menu_open)
            {
                if let Some(state) = self.caption_editor.as_mut() {
                    state.position_menu_open = false;
                }
                cx.notify();
                return;
            }
            self.cancel_caption_editor(cx);
            window.focus(&self.focus_handle);
            return;
        }
        if key == "enter" {
            self.save_caption_editor(cx);
            window.focus(&self.focus_handle);
            return;
        }
        if let Some(state) = self.caption_editor.as_mut() {
            state.input.handle_key_down(event, cx);
            state
                .input
                .blink
                .pause_and_show(crate::blink_manager::BlinkTarget::Caption, cx);
            cx.notify();
        }
    }

    /// Inline caption editor: the field sits on the caption line itself.
    /// Position and numbering controls share that line; the text is committed
    /// on Enter or a click outside, Esc restores the saved caption.
    pub fn render_caption_inline_editor(
        &self,
        image: &images::ResolvedImage,
        line: usize,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(state) = self
            .caption_editor
            .as_ref()
            .filter(|state| state.line == line && state.path == image.path)
        else {
            return div().into_any_element();
        };
        let lang = self.config.ui.language.as_str();
        let above = state.position == "above";
        let menu_open = state.position_menu_open;
        let position_label = crate::i18n::t(
            if above {
                "editor.image.caption_above_image"
            } else {
                "editor.image.caption_below_image"
            },
            lang,
        );

        let option = |id: &'static str, label: String, selected: bool, is_above: bool| {
            div()
                .id(gpui::ElementId::Name(SharedString::from(id)))
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded_sm()
                .when(selected, |d| d.bg(theme.bg_selected))
                .text_size(px(12.0))
                .text_color(if selected { theme.accent } else { theme.text_secondary })
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(state) = this.caption_editor.as_mut() {
                            state.position = if is_above { "above" } else { "below" }.to_string();
                            state.position_menu_open = false;
                        }
                        cx.notify();
                    }),
                )
                .child(label)
        };

        let position_select = div()
            .relative()
            .w(px(132.0))
            .h(px(34.0))
            .flex_shrink_0()
            .child(
                div()
                    .id("caption-position-select")
                    .size_full()
                    .px(px(8.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .rounded_md()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(state) = this.caption_editor.as_mut() {
                                state.position_menu_open = !state.position_menu_open;
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .h(px(16.0))
                            .line_height(px(15.0))
                            .font_family("Segoe UI")
                            .child(position_label.clone()),
                    )
                    .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)),
            )
            .when(menu_open, |d| {
                d.child(
                    div()
                        .id("caption-position-menu")
                        .absolute()
                        .top(px(37.0))
                        .left_0()
                        .w(px(132.0))
                        .p(px(3.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border)
                        .child(option(
                            "caption-pos-above",
                            crate::i18n::t("editor.image.caption_above_image", lang),
                            above,
                            true,
                        ))
                        .child(option(
                            "caption-pos-below",
                            crate::i18n::t("editor.image.caption_below_image", lang),
                            !above,
                            false,
                        )),
                )
            });

        let numbered = state.numbered;
        let numbered_toggle = div()
            .id("caption-numbered-toggle")
            .h(px(34.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .flex_shrink_0()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(state) = this.caption_editor.as_mut() {
                        state.numbered = !state.numbered;
                    }
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(28.0))
                    .h(px(16.0))
                    .rounded_full()
                    .bg(if numbered { theme.accent } else { theme.bg_selected })
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .left(px(if numbered { 14.0 } else { 2.0 }))
                            .w(px(12.0))
                            .h(px(12.0))
                            .rounded_full()
                            .bg(if numbered { theme.text_on_accent } else { theme.text_muted }),
                    ),
            )
            .child(
                div()
                    .h(px(16.0))
                    .line_height(px(15.0))
                    .text_size(px(12.0))
                    .font_family("Segoe UI")
                    .text_color(theme.text_primary)
                    .child(crate::i18n::t("editor.image.numbered", lang)),
            );

        div()
            .id("caption-inline-editor")
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            // A little air under the line, plus room for the dropdown list.
            .pb(px(if menu_open { 62.0 } else { 6.0 }))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.save_caption_editor(cx);
                window.focus(&this.focus_handle);
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(
                div()
                    .id("caption-inline-field")
                    .h(px(34.0))
                    .flex_1()
                    .min_w_0()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.accent)
                    .track_focus(&state.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event, window, cx| {
                            cx.stop_propagation();
                            if let Some(state) = this.caption_editor.as_mut() {
                                // Mirror the sidebar quick filter: a click
                                // places the caret, drag selects, double-click
                                // picks a word. Focusing alone left the old
                                // caret/selection in place.
                                state.input.handle_mouse_down(event, window, 12.0, "Segoe UI");
                                state
                                    .input
                                    .blink
                                    .pause_and_show(crate::blink_manager::BlinkTarget::Caption, cx);
                                window.focus(&state.focus);
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event, window, cx| {
                        if let Some(state) = this.caption_editor.as_mut() {
                            if state.input.is_mouse_selecting {
                                state.input.handle_mouse_move(event, window, 12.0, "Segoe UI");
                                state
                                    .input
                                    .blink
                                    .pause_and_show(crate::blink_manager::BlinkTarget::Caption, cx);
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        if let Some(state) = this.caption_editor.as_mut() {
                            if state.input.is_mouse_selecting {
                                state.input.handle_mouse_up();
                                cx.notify();
                            }
                        }
                    }))
                    .child(state.input.render(
                        &crate::i18n::t("editor.image.caption_title", lang),
                        theme,
                        window,
                        window.scale_factor(),
                        12.0,
                        "Segoe UI",
                    )),
            )
            .child(
                div()
                    .h(px(16.0))
                    .line_height(px(15.0))
                    .text_size(px(12.0))
                    .font_family("Segoe UI")
                    .text_color(theme.text_secondary)
                    .flex_shrink_0()
                    .child(crate::i18n::t("editor.image.caption_position", lang)),
            )
            .child(position_select)
            .child(numbered_toggle)
            .into_any_element()
    }
}
