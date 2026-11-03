use super::*;

impl AppView {
    /// Keys are modal while the image editor is open.
    pub(crate) fn handle_image_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;

        if self.image_editor.as_ref().is_some_and(|editor| editor.text_entry.is_some()) {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.text_entry = None;
                }
                window.focus(&self.focus_handle);
                cx.notify();
                return;
            }
            if key == "enter" {
                self.image_editor_commit_text(cx);
                window.focus(&self.focus_handle);
                return;
            }
            if let Some(editor) = self.image_editor.as_mut() {
                if let Some(entry) = editor.text_entry.as_mut() {
                    entry.input.handle_key_down(event, cx);
                }
            }
            return;
        }

        if self.image_editor.as_ref().is_some_and(|editor| editor.confirm.is_some()) {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.confirm = None;
                }
                cx.notify();
            } else if key == "enter" {
                self.image_editor_confirm(cx);
            }
            return;
        }

        let active_field = self.image_editor.as_ref().and_then(|editor| editor.active_field);
        let active_ratio = self.image_editor.as_ref().and_then(|editor| editor.active_ratio);
        if let Some(index) = active_field {
            match key {
                "escape" => {
                    self.close_image_editor(false, window, cx);
                    return;
                }
                "enter" => {
                    self.close_image_editor(true, window, cx);
                    return;
                }
                "tab" => {
                    let next = (index + 1) % 4;
                    if let Some(editor) = self.image_editor.as_mut() {
                        editor.active_field = Some(next);
                        editor.active_ratio = None;
                        editor.crop_fields[next].select_all();
                        let handle = editor.crop_fields[next].focus_handle.clone();
                        if let Some(handle) = handle {
                            window.focus(&handle);
                        }
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            }
            if let Some(editor) = self.image_editor.as_mut() {
                editor.crop_fields[index].handle_key_down(event, cx);
                let value = editor.crop_fields[index].text().trim().parse::<f32>().ok();
                let field = match index {
                    0 => CropField::X,
                    1 => CropField::Y,
                    2 => CropField::W,
                    _ => CropField::H,
                };
                if let Some(value) = value {
                    let _ = field;
                    let _ = value;
                }
            }
            if let Some(editor) = self.image_editor.as_ref() {
                let field = match index {
                    0 => CropField::X,
                    1 => CropField::Y,
                    2 => CropField::W,
                    _ => CropField::H,
                };
                if let Some(value) = editor.crop_fields[index].text().trim().parse::<f32>().ok() {
                    self.image_editor_set_field(field, value, cx);
                }
            }
            cx.notify();
            return;
        }
        if let Some(index) = active_ratio {
            if key == "escape" {
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.active_ratio = None;
                    editor.ratio_fields[index].clear();
                }
                cx.notify();
                return;
            }
            if key == "tab" {
                let next = (index + 1) % 2;
                if let Some(editor) = self.image_editor.as_mut() {
                    editor.active_ratio = Some(next);
                    editor.active_field = None;
                    editor.ratio_fields[next].select_all();
                    let handle = editor.ratio_fields[next].focus_handle.clone();
                    if let Some(handle) = handle {
                        window.focus(&handle);
                    }
                }
                cx.notify();
                return;
            }
            if let Some(editor) = self.image_editor.as_mut() {
                editor.ratio_fields[index].handle_key_down(event, cx);
            }
            self.apply_custom_ratio(cx);
            return;
        }

        let mode = self.image_editor.as_ref().map(|editor| editor.mode);
        match key {
            "escape" => {
                self.close_image_editor(false, window, cx);
                return;
            }
            "enter" if mode == Some(ImageEditorMode::Crop) => {
                self.close_image_editor(true, window, cx);
                return;
            }
            _ => {}
        }
        if mode == Some(ImageEditorMode::Annotate) {
            if ctrl && (key.eq_ignore_ascii_case("z") || key == "я") {
                if event.keystroke.modifiers.shift {
                    self.image_editor_redo(cx);
                } else {
                    self.image_editor_undo(cx);
                }
                return;
            }
            if ctrl && (key.eq_ignore_ascii_case("y") || key == "н") {
                self.image_editor_redo(cx);
                return;
            }
            if key == "delete" || key == "backspace" {
                self.image_editor_delete_selected(cx);
                return;
            }
            if key == "enter" {
                self.close_image_editor(true, window, cx);
            }
        }
    }

    pub(super) fn apply_custom_ratio(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.image_editor.as_mut() else {
            return;
        };
        let w = editor.ratio_fields[0].text().trim().parse::<u32>().ok();
        let h = editor.ratio_fields[1].text().trim().parse::<u32>().ok();
        if let (Some(w), Some(h)) = (w, h) {
            if w > 0 && h > 0 {
                let aspect = CropAspect::Ratio(w, h);
                editor.aspect = aspect;
                let ratio = aspect.ratio(editor.natural).unwrap_or(1.0);
                let natural = editor.natural;
                let center_x = editor.frame.x + editor.frame.w / 2.0;
                let center_y = editor.frame.y + editor.frame.h / 2.0;
                let mut fw = editor.frame.w;
                let mut fh = fw / ratio;
                if fh > natural.1 as f32 {
                    fh = natural.1 as f32;
                    fw = fh * ratio;
                }
                if fw > natural.0 as f32 {
                    fw = natural.0 as f32;
                    fh = fw / ratio;
                }
                editor.frame = CropRect {
                    x: (center_x - fw / 2.0).clamp(0.0, (natural.0 as f32 - fw).max(0.0)),
                    y: (center_y - fh / 2.0).clamp(0.0, (natural.1 as f32 - fh).max(0.0)),
                    w: fw,
                    h: fh,
                };
                editor.edit.crop = Some(editor.frame);
                cx.notify();
            }
        }
    }

    pub fn image_editor_confirm(&mut self, cx: &mut Context<Self>) {
        let action = self.image_editor.as_mut().and_then(|editor| editor.confirm.take());
        match action {
            Some(ConfirmAction::ApplyCrop) => self.image_editor_apply_crop(cx),
            Some(ConfirmAction::Flatten) => self.image_editor_flatten(cx),
            Some(ConfirmAction::Blur(region)) => self.image_editor_blur(region, false, cx),
            Some(ConfirmAction::Pixelate(region)) => self.image_editor_blur(region, true, cx),
            None => {}
        }
    }

    pub fn open_image_editor_at_line(
        &mut self,
        mode: ImageEditorMode,
        line: usize,
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
        self.open_image_editor(mode, line, &image, cx);
    }

    pub fn image_editor_matches(&self, line: usize, target: &str) -> bool {
        self.image_editor
            .as_ref()
            .is_some_and(|editor| editor.line == line && editor.target == target)
    }

    // ─────────────────────── Crop frame (in the row) ───────────────────────

    pub fn render_crop_frame_overlay(
        &self,
        image: &images::ResolvedImage,
        line: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(editor) = self.image_editor.as_ref() else {
            return div().into_any_element();
        };
        if editor.line != line || editor.target != image.reference.target {
            return div().into_any_element();
        }
        let scale = if image.visible.0 > 0 {
            image.width / image.visible.0 as f32
        } else {
            1.0
        };
        let width = image.width;
        let height = image.height;
        let frame = editor.frame;
        let fx = (frame.x * scale).clamp(0.0, width);
        let fy = (frame.y * scale).clamp(0.0, height);
        let fw = (frame.w * scale).clamp(0.0, width - fx);
        let fh = (frame.h * scale).clamp(0.0, height - fy);
        let shade = gpui::rgba(0x00000073);
        // Fractional layout coordinates can leave a 1px unshaded column at the
        // frame edge, which reads as a bright vertical line. Overlap the
        // shading a little under the frame border.
        // Just enough to hide the antialiasing seam, but still under the
        // one-pixel frame border (a larger overlap showed as a dark stripe
        // inside the crop).
        const OVERLAP: f32 = 1.0;
        let full_frame =
            fx <= 0.5 && fy <= 0.5 && fw >= width - 0.5 && fh >= height - 0.5;

        let mut layer = div().id("crop-frame-overlay").absolute().inset_0();
        if !full_frame {
            let right_start = (fx + fw - OVERLAP).max(0.0);
            let bottom_start = (fy + fh - OVERLAP).max(0.0);
            layer = layer
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px((fx + OVERLAP).min(width)))
                        .h_full()
                        .bg(shade),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(right_start))
                        .top_0()
                        .w(px((width - right_start).max(0.0)))
                        .h_full()
                        .bg(shade),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(fx))
                        .top_0()
                        .w(px(fw))
                        .h(px((fy + OVERLAP).min(height)))
                        .bg(shade),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(fx))
                        .top(px(bottom_start))
                        .w(px(fw))
                        .h(px((height - bottom_start).max(0.0)))
                        .bg(shade),
                );
        }
        layer = layer
            .child(
                div()
                    .id("crop-frame")
                    .absolute()
                    .left(px(fx))
                    .top(px(fy))
                    .w(px(fw))
                    .h(px(fh))
                    // No solid outline: the mask contrast and the white corner
                    // handles already mark the crop, and the accent border
                    // read as stray vertical lines along both sides.
                    .cursor(gpui::CursorStyle::OpenHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_drag_start(
                                CropDragKind::Move,
                                event.position,
                                event.modifiers.shift,
                                cx,
                            );
                        }),
                    ),
            );

        let handles = [
            (CropHandle::Nw, fx, fy),
            (CropHandle::N, fx + fw / 2.0, fy),
            (CropHandle::Ne, fx + fw, fy),
            (CropHandle::E, fx + fw, fy + fh / 2.0),
            (CropHandle::Se, fx + fw, fy + fh),
            (CropHandle::S, fx + fw / 2.0, fy + fh),
            (CropHandle::Sw, fx, fy + fh),
            (CropHandle::W, fx, fy + fh / 2.0),
        ];
        for (handle, x, y) in handles {
            layer = layer.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("crop-handle-{:?}", handle)),
                    ))
                    .absolute()
                    .left(px(x - 5.0))
                    .top(px(y - 5.0))
                    .w(px(10.0))
                    .h(px(10.0))
                    .bg(gpui::rgb(0xffffff))
                    .border_1()
                    .border_color(theme.accent)
                    .rounded_sm()
                    .cursor(gpui::CursorStyle::Crosshair)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_drag_start(
                                CropDragKind::Handle(handle),
                                event.position,
                                event.modifiers.shift,
                                cx,
                            );
                        }),
                    ),
            );
        }
        layer.into_any_element()
    }

    // ─────────────────────── Overlay panels ───────────────────────

    pub fn render_image_editor_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let Some(editor) = self.image_editor.as_ref() else {
            return div().into_any_element();
        };
        let (rx, ry, rw, rh) = editor.rect.get();
        let window_w = f32::from(window.bounds().size.width);
        let window_h = f32::from(window.bounds().size.height);
        let panel_x = if rx + rw + PANEL_W + 16.0 <= window_w {
            rx + rw + 12.0
        } else if rx - PANEL_W - 12.0 >= 8.0 {
            rx - PANEL_W - 12.0
        } else {
            (window_w - PANEL_W - 8.0).max(8.0)
        };
        let panel_y = ry.clamp(8.0, (window_h - 420.0).max(8.0));

        let mut root = div()
            .id("image-editor-overlay")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }));

        if editor.drag.is_some() {
            root = root.child(
                div()
                    .id("crop-drag-capture")
                    .absolute()
                    .inset_0()
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.image_editor_drag_move(event.position, event.modifiers.shift, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_drag_end(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_drag_end(cx)),
                    ),
            );
        }
        if editor.annotation_drag.is_some() {
            root = root.child(
                div()
                    .id("annotation-drag-capture")
                    .absolute()
                    .inset_0()
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.image_editor_pointer_move(event.position, event.modifiers.shift, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_pointer_up(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_editor_pointer_up(cx)),
                    ),
            );
        }

        if let Some(entry) = editor.text_entry.as_ref() {
            let entry_focus = entry.focus.clone();
            let scale = editor.scale.get().max(0.0001);
            let left = (rx + entry.point.x * scale).clamp(8.0, (window_w - 240.0).max(8.0));
            let top = (ry + entry.point.y * scale).clamp(8.0, (window_h - 46.0).max(8.0));
            root = root.child(
                div()
                    .id("annotation-text-entry")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(232.0))
                    .h(px(31.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.accent)
                    .track_focus(&entry.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, window, cx| {
                            cx.stop_propagation();
                            window.focus(&entry_focus);
                        }),
                    )
                    .child(entry.input.render(
                        &crate::i18n::t("editor.image.text_placeholder", &self.config.ui.language),
                        theme,
                        window,
                        window.scale_factor(),
                        13.0,
                        "Segoe UI",
                    )),
            );
        }

        match editor.mode {
            ImageEditorMode::Crop => {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(panel_x))
                        .top(px(panel_y))
                        .child(self.render_crop_panel(theme, window, cx)),
                );
            }
            ImageEditorMode::Annotate => {
                let toolbar_y = if ry >= 58.0 {
                    ry - 50.0
                } else {
                    (ry + rh + 8.0).min((window_h - 50.0).max(8.0))
                };
                // Center on the note column ("page"), not the window: the
                // picture and its panels share that axis.
                let wrap_cols = self
                    .state
                    .active_view()
                    .map(|view| view.window.wrap_cols)
                    .unwrap_or(80);
                let page_left = self.pane_text_start_x(None, window);
                let page_w = crate::images::text_width(wrap_cols);
                root = root.child(
                    div()
                        .absolute()
                        .top(px(toolbar_y.max(8.0)))
                        .left(px(page_left))
                        .w(px(page_w))
                        .flex()
                        .justify_center()
                        .child(self.render_annotation_toolbar(theme, cx)),
                );
                root = root.child(
                    div()
                        .absolute()
                        .left(px(panel_x))
                        .top(px(panel_y))
                        .child(self.render_annotation_panel(theme, cx)),
                );
            }
        }

        if let Some(confirm) = editor.confirm {
            root = root.child(self.render_image_confirm(confirm, theme, cx));
        }
        root.into_any_element()
    }

    pub(super) fn render_crop_panel(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);
        let pixel_scale = window.scale_factor();

        let mut aspects = div().flex().flex_row().flex_wrap().gap(px(4.0));
        let presets = [
            (CropAspect::Free, t("editor.image.crop_free")),
            (CropAspect::Original, t("editor.image.crop_original")),
            (CropAspect::Ratio(1, 1), "1:1".to_string()),
            (CropAspect::Ratio(4, 3), "4:3".to_string()),
            (CropAspect::Ratio(3, 2), "3:2".to_string()),
            (CropAspect::Ratio(16, 9), "16:9".to_string()),
            (CropAspect::Ratio(10, 15), "10:15".to_string()),
            (CropAspect::Ratio(2, 3), "2:3".to_string()),
            (CropAspect::Ratio(3, 4), "3:4".to_string()),
        ];
        for (aspect, label) in presets {
            let selected = editor.aspect == aspect;
            aspects = aspects.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("crop-aspect-{}", label)),
                    ))
                    .h(px(24.0))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .bg(if selected { theme.bg_selected } else { theme.bg_sidebar })
                    .text_xs()
                    .text_color(if selected { theme.accent } else { theme.text_secondary })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_set_aspect(aspect, cx);
                        }),
                    )
                    .child(label),
            );
        }

        let ratio_field = |index: usize| {
            let input = &editor.ratio_fields[index];
            div()
                .h(px(26.0))
                .w(px(52.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.bg_sidebar)
                .border_1()
                .border_color(if editor.active_ratio == Some(index) {
                    theme.accent
                } else {
                    theme.border
                })
                .cursor(gpui::CursorStyle::IBeam)
                .when_some(input.focus_handle.as_ref(), |d, handle| d.track_focus(handle))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.active_ratio = Some(index);
                            editor.active_field = None;
                            editor.ratio_fields[index].select_all();
                            let handle = editor.ratio_fields[index].focus_handle.clone();
                            if let Some(handle) = handle {
                                window.focus(&handle);
                            }
                        }
                        cx.notify();
                    }),
                )
                .child(input.render("0", theme, window, pixel_scale, 12.0, "Segoe UI"))
        };

        let field = |index: usize, label: String| {
            let input = &editor.crop_fields[index];
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                // Two fields per row with slack; the exact half width wrapped to one column.
                .w(px((PANEL_W - 24.0 - 8.0) / 2.0 - 6.0))
                .child(div().text_xs().text_color(theme.text_muted).child(label))
                .child(
                    div()
                        .h(px(26.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded_md()
                        .bg(theme.bg_sidebar)
                        .border_1()
                        .border_color(if editor.active_field == Some(index) {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .cursor(gpui::CursorStyle::IBeam)
                        .when_some(input.focus_handle.as_ref(), |d, handle| d.track_focus(handle))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(editor) = this.image_editor.as_mut() {
                                    editor.active_field = Some(index);
                                    editor.active_ratio = None;
                                    editor.crop_fields[index].select_all();
                                    let handle = editor.crop_fields[index].focus_handle.clone();
                                    if let Some(handle) = handle {
                                        window.focus(&handle);
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child(input.render("0", theme, window, pixel_scale, 13.0, "Segoe UI")),
                )
        };

        let caption = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(t("editor.image.crop_title")),
            )
            .child(self.image_close_button(
                "crop-close-x",
                theme,
                cx.listener(|this, _, window, cx| this.close_image_editor(false, window, cx)),
            ));

        div()
            .w(px(PANEL_W))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .p(px(12.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(caption)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().text_color(theme.text_muted).child(t("editor.image.crop_aspect")))
                    .child(
                        div()
                            .id("crop-aspect-swap")
                            .h(px(22.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_swap_aspect(cx);
                                }),
                            )
                            .child(t("editor.image.crop_swap")),
                    ),
            )
            .child(aspects)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().text_xs().text_color(theme.text_muted).child(t("editor.image.crop_custom")))
                    .child(ratio_field(0))
                    .child(div().text_xs().text_color(theme.text_muted).child(":"))
                    .child(ratio_field(1)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(field(0, t("editor.image.crop_x")))
                    .child(field(1, t("editor.image.crop_y")))
                    .child(field(2, t("editor.image.crop_w")))
                    .child(field(3, t("editor.image.crop_h"))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(t("editor.image.crop_warning")),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(self.editor_text_button(
                        "crop-reset",
                        t("editor.image.crop_reset"),
                        theme,
                        false,
                        cx.listener(|this, _, _, cx| this.image_editor_reset_crop(cx)),
                    ))
                    .child(self.editor_text_button(
                        "crop-apply",
                        t("editor.image.crop_apply"),
                        theme,
                        true,
                        cx.listener(|this, _, _, cx| {
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.confirm = Some(ConfirmAction::ApplyCrop);
                            }
                            cx.notify();
                        }),
                    ))
                    .child(self.editor_text_button(
                        "crop-cancel",
                        t("editor.image.crop_cancel"),
                        theme,
                        false,
                        cx.listener(|this, _, window, cx| this.close_image_editor(false, window, cx)),
                    ))
                    .child(self.editor_text_button(
                        "crop-done",
                        t("editor.image.crop_done"),
                        theme,
                        true,
                        cx.listener(|this, _, window, cx| this.close_image_editor(true, window, cx)),
                    )),
            )
    }

    pub(super) fn render_annotation_toolbar(&self, theme: &Theme, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let tools = [
            (AnnotationTool::Select, icons::ICON_MOVE_ARROWS, "editor.image.tool_select"),
            (AnnotationTool::Arrow, icons::ICON_ARROW_RIGHT, "editor.image.tool_arrow"),
            (AnnotationTool::Line, icons::ICON_MINIMIZE, "editor.image.tool_line"),
            (AnnotationTool::Rect, icons::ICON_MAXIMIZE, "editor.image.tool_rect"),
            (AnnotationTool::Ellipse, icons::ICON_CIRCLE_PLUS, "editor.image.tool_ellipse"),
            (AnnotationTool::Highlight, icons::ICON_HIGHLIGHTER, "editor.image.tool_highlight"),
            (AnnotationTool::Pencil, icons::ICON_PENCIL, "editor.image.tool_pencil"),
            (AnnotationTool::Text, icons::ICON_TYPE, "editor.image.tool_text"),
            (AnnotationTool::Label, icons::ICON_HASH, "editor.image.tool_label"),
            (AnnotationTool::Blur, icons::ICON_ERASER, "editor.image.tool_blur"),
            (AnnotationTool::Pixelate, icons::ICON_CANVAS, "editor.image.tool_pixelate"),
        ];
        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0))
            .px(px(6.0))
            .h(px(38.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }));
        for (tool, icon, _key) in tools {
            let selected = editor.tool == tool;
            bar = bar.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-tool-{:?}", tool)),
                    ))
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .when(selected, |d| d.bg(theme.bg_selected))
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.tool = tool;
                                editor.selected = None;
                            }
                            cx.notify();
                        }),
                    )
                    .child(icons::icon(
                        icon,
                        14.0,
                        if selected { theme.accent } else { theme.text_secondary },
                    )),
            );
        }
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        for color in PALETTE {
            let selected = editor.color.eq_ignore_ascii_case(color);
            let rgba = images::parse_hex_color(color);
            bar = bar.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-color-{}", color)),
                    ))
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded_full()
                    .bg(gpui::rgba(
                        (rgba[0] as u32) << 24
                            | (rgba[1] as u32) << 16
                            | (rgba[2] as u32) << 8
                            | rgba[3] as u32,
                    ))
                    .when(selected, |d| d.border_2().border_color(theme.accent))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(editor) = this.image_editor.as_mut() {
                                editor.color = color.to_string();
                            }
                            cx.notify();
                        }),
                    ),
            );
        }
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        bar = bar.child(
            div()
                .id("ann-width-down")
                .w(px(20.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .child("-")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.width = (editor.width - 1.0).clamp(1.0, 40.0);
                        }
                        cx.notify();
                    }),
                ),
        );
        bar = bar.child(
            div()
                .w(px(30.0))
                .text_xs()
                .text_color(theme.text_secondary)
                .text_center()
                .child(format!("{:.0}", editor.width)),
        );
        bar = bar.child(
            div()
                .id("ann-width-up")
                .w(px(20.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .child("+")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.width = (editor.width + 1.0).clamp(1.0, 40.0);
                        }
                        cx.notify();
                    }),
                ),
        );
        bar = bar.child(
            div()
                .id("ann-fill")
                .h(px(24.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .rounded_sm()
                .bg(if editor.filled { theme.bg_selected } else { theme.bg_sidebar })
                .border_1()
                .border_color(if editor.filled { theme.accent } else { theme.border })
                .text_xs()
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(editor) = this.image_editor.as_mut() {
                            editor.filled = !editor.filled;
                        }
                        cx.notify();
                    }),
                )
                .child(crate::i18n::t("editor.image.fill", lang)),
        );
        bar = bar.child(div().w(px(1.0)).h(px(20.0)).mx(px(3.0)).bg(theme.border));
        bar = bar.child(self.editor_icon_button(
            "ann-undo",
            icons::ICON_UNDO,
            !editor.undo.is_empty(),
            theme,
            cx.listener(|this, _, _, cx| this.image_editor_undo(cx)),
        ));
        bar = bar.child(self.editor_icon_button(
            "ann-redo",
            icons::ICON_REDO,
            !editor.redo.is_empty(),
            theme,
            cx.listener(|this, _, _, cx| this.image_editor_redo(cx)),
        ));
        bar = bar.child(
            div()
                .id("ann-done")
                .h(px(26.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.accent)
                .text_xs()
                .text_color(theme.text_on_accent)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_image_editor(true, window, cx);
                    }),
                )
                .child(crate::i18n::t("editor.image.crop_done", lang)),
        );
        bar = bar.child(
            div()
                .id("ann-cancel")
                .h(px(26.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(theme.bg_sidebar)
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_image_editor(false, window, cx);
                    }),
                )
                .child(crate::i18n::t("editor.image.crop_cancel", lang)),
        );
        bar
    }

    pub(super) fn render_annotation_panel(&self, theme: &Theme, cx: &Context<Self>) -> Div {
        let Some(editor) = self.image_editor.as_ref() else {
            return div();
        };
        let lang = self.config.ui.language.as_str();
        let mut layers = div().flex().flex_col().gap(px(2.0)).max_h(px(250.0)).overflow_hidden();
        if editor.edit.annotations.is_empty() {
            layers = layers.child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(crate::i18n::t("editor.image.layers", lang)),
            );
        }
        for (index, annotation) in editor.edit.annotations.iter().enumerate().rev() {
            let selected = editor.selected == Some(index);
            let label = annotation_label(annotation.kind, lang);
            layers = layers.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("ann-layer-{}", index)),
                    ))
                    .h(px(24.0))
                    .px(px(6.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .rounded_sm()
                    .when(selected, |d| d.bg(theme.bg_selected))
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.image_editor_edit_layer(index, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(if selected { theme.accent } else { theme.text_secondary })
                            .child(label),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-edit-{}", index)),
                            ))
                            .w(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_edit_layer(index, cx);
                                }),
                            )
                            .child(icons::icon(icons::ICON_PENCIL, 11.0, theme.text_muted)),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-up-{}", index)),
                            ))
                            .w(px(18.0))
                            .text_xs()
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_move_layer(index, 1, cx);
                                }),
                            )
                            .child("↑"),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-down-{}", index)),
                            ))
                            .w(px(18.0))
                            .text_xs()
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.image_editor_move_layer(index, -1, cx);
                                }),
                            )
                            .child("↓"),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                SharedString::from(format!("ann-layer-del-{}", index)),
                            ))
                            .w(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(editor) = this.image_editor.as_mut() {
                                        editor.selected = Some(index);
                                    }
                                    this.image_editor_delete_selected(cx);
                                }),
                            )
                            .child(icons::icon(icons::ICON_TRASH_2, 11.0, theme.text_muted)),
                    ),
            );
        }

        div()
            .w(px(PANEL_W))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_key(event, window, cx);
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(crate::i18n::t("editor.image.annotate_title", lang)),
                    )
                    .child(self.image_close_button(
                        "annotate-close-x",
                        theme,
                        cx.listener(|this, _, window, cx| {
                            this.close_image_editor(false, window, cx)
                        }),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(crate::i18n::t("editor.image.layers", lang)),
            )
            .child(layers)
            .child(self.editor_text_button(
                "ann-flatten",
                crate::i18n::t("editor.image.flatten", lang),
                theme,
                false,
                cx.listener(|this, _, _, cx| {
                    if let Some(editor) = this.image_editor.as_mut() {
                        editor.confirm = Some(ConfirmAction::Flatten);
                    }
                    cx.notify();
                }),
            ))
    }

    pub(super) fn render_image_confirm(
        &self,
        action: ConfirmAction,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let lang = self.config.ui.language.as_str();
        let message = match action {
            ConfirmAction::ApplyCrop => crate::i18n::t("editor.image.confirm_crop", lang),
            ConfirmAction::Flatten => crate::i18n::t("editor.image.confirm_flatten", lang),
            ConfirmAction::Blur(_) => crate::i18n::t("editor.image.confirm_blur", lang),
            ConfirmAction::Pixelate(_) => crate::i18n::t("editor.image.confirm_pixelate", lang),
        };
        div()
            .id("image-confirm-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .child(
                div()
                    .w(px(360.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .rounded_lg()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .child(message),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(6.0))
                            .child(self.editor_text_button(
                                "confirm-cancel",
                                crate::i18n::t("editor.image.crop_cancel", lang),
                                theme,
                                false,
                                cx.listener(|this, _, _, cx| {
                                    if let Some(editor) = this.image_editor.as_mut() {
                                        editor.confirm = None;
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(self.editor_text_button(
                                "confirm-apply",
                                crate::i18n::t("editor.image.apply", lang),
                                theme,
                                true,
                                cx.listener(|this, _, _, cx| this.image_editor_confirm(cx)),
                            )),
                    ),
            )
    }

    pub(super) fn editor_text_button(
        &self,
        id: &'static str,
        label: String,
        theme: &Theme,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .h(px(26.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded_md()
            .bg(if primary { theme.accent } else { theme.bg_sidebar })
            .border_1()
            .border_color(if primary { theme.accent } else { theme.border })
            .text_xs()
            .text_color(if primary { theme.text_on_accent } else { theme.text_secondary })
            .cursor_pointer()
            .hover(|h| h.bg(if primary { theme.accent } else { theme.bg_hover }))
            .on_mouse_down(MouseButton::Left, listener)
            .child(label)
    }

    pub(super) fn editor_icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        enabled: bool,
        theme: &Theme,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .w(px(26.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .when(enabled, |d| {
                d.cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |d| d.opacity(0.35).cursor_not_allowed())
            .child(icons::icon(
                icon,
                14.0,
                if enabled { theme.text_secondary } else { theme.text_muted },
            ))
    }
}
