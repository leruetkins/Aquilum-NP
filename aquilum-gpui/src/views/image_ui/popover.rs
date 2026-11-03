use super::*;

impl AppView {
    pub fn render_image_popover(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let Some(popover) = self.image_popover.as_ref() else {
            return div().into_any_element();
        };

        let lang = self.config.ui.language.as_str();
        let title = match popover.kind {
            ImagePopoverKind::Size => crate::i18n::t("editor.image.size", lang),
            ImagePopoverKind::Radius => crate::i18n::t("editor.image.radius", lang),
        };

        let body = match popover.kind {
            ImagePopoverKind::Size => self.render_size_popover_body(theme, window, cx),
            ImagePopoverKind::Radius => self.render_radius_popover_body(theme, cx),
        };

        let window_w = f32::from(window.bounds().size.width);
        let window_h = f32::from(window.bounds().size.height);
        // Sit to the right of the picture, exactly like the crop editor panel;
        // fall back to the left side when the right edge has no room.
        let (panel_x, panel_y) = match self.image_block_screen_rect(popover.line, window) {
            Some((rx, ry, rw, _rh)) => {
                let x = if rx + rw + POPOVER_W + 16.0 <= window_w {
                    rx + rw + 12.0
                } else if rx - POPOVER_W - 12.0 >= 8.0 {
                    rx - POPOVER_W - 12.0
                } else {
                    (window_w - POPOVER_W - 8.0).max(8.0)
                };
                let y = ry.clamp(8.0, (window_h - 220.0).max(8.0));
                (x, y)
            }
            None => (
                popover.x.clamp(8.0, (window_w - POPOVER_W - 8.0).max(8.0)),
                (popover.y + 8.0).clamp(8.0, (window_h - 200.0).max(8.0)),
            ),
        };
        let panel = div()
            .id("image-popover-panel")
            .track_focus(&popover.focus)
            .absolute()
            .left(px(panel_x))
            .top(px(panel_y))
            .w(px(POPOVER_W))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
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
                            .child(title),
                    )
                    .child(self.image_close_button(
                        "image-popover-close-x",
                        theme,
                        cx.listener(|this, _, window, cx| this.close_image_popover(window, cx)),
                    )),
            )
            .child(body);

        // No full-window backdrop: the size handles stay clickable while the
        // panel is open. Esc / Done close it.
        let mut root = div().id("image-popover-root").absolute().inset_0();
        if popover.kind == ImagePopoverKind::Size && self.size_drag.is_some() {
            root = root.child(
                div()
                    .id("image-size-drag-capture")
                    .absolute()
                    .inset_0()
                    .on_mouse_move(
                        cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                            this.image_size_drag_move(event.position, event.modifiers.shift, cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_size_drag_end(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.image_size_drag_end(cx)),
                    ),
            );
        }
        root.child(panel).into_any_element()
    }

    /// Full-window catcher for a resize that started on a picture handle.
    /// Without it the drag only sees events while the Size popover is open, so
    /// a plain drag on a selected picture would leave `size_drag` stuck.
    pub fn render_image_size_drag_capture(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("image-size-drag-capture-global")
            .absolute()
            .inset_0()
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    this.image_size_drag_move(event.position, event.modifiers.shift, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.image_size_drag_end(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.image_size_drag_end(cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_size_popover_body(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let lang = self.config.ui.language.as_str();
        let Some(popover) = self.image_popover.as_ref() else {
            return div().into_any_element();
        };
        let pixel_scale = window.scale_factor();

        let field = |label: &str,
                     input: &TextInput,
                     active: bool,
                     focus: SizeField,
                     this_cx: &Context<Self>,
                     this_theme: &Theme| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(
                            SharedString::from(format!("image-size-field-{:?}", focus)),
                        ))
                        .h(px(28.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded_md()
                        .bg(this_theme.bg_sidebar)
                        .border_1()
                        .border_color(if active {
                            this_theme.accent
                        } else {
                            this_theme.border
                        })
                        .cursor(gpui::CursorStyle::IBeam)
                        .when_some(input.focus_handle.as_ref(), |d, handle| {
                            d.track_focus(handle)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            this_cx.listener(move |this: &mut AppView, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(popover) = this.image_popover.as_mut() {
                                    popover.active = focus;
                                    let handle = popover.active_input().focus_handle.clone();
                                    popover.active_input_mut().select_all();
                                    if let Some(handle) = handle {
                                        window.focus(&handle);
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child(input.render("0", this_theme, window, pixel_scale, 13.0, "Segoe UI")),
                )
        };

        let locked = popover.lock;
        let lock_button = div()
            .id("image-size-lock")
            .w(px(28.0))
            .h(px(28.0))
            .mt(px(16.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .bg(if locked { theme.bg_selected } else { theme.bg_sidebar })
            .border_1()
            .border_color(if locked { theme.accent } else { theme.border })
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                if let Some(popover) = this.image_popover.as_mut() {
                    popover.lock = !popover.lock;
                }
                cx.notify();
            }))
            .child(icons::icon(
                icons::ICON_LOCK,
                13.0,
                if locked { theme.accent } else { theme.text_muted },
            ));

        let mut presets = div().flex().flex_row().gap(px(6.0));
        for percent in [25u32, 50, 75, 100] {
            presets = presets.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("image-size-pct-{}", percent)),
                    ))
                    .flex_1()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_image_size_percent(percent, cx);
                        }),
                    )
                    .child(format!("{}%", percent)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(field(
                        &crate::i18n::t("editor.image.width", lang),
                        &popover.width,
                        popover.active == SizeField::Width,
                        SizeField::Width,
                        cx,
                        theme,
                    ))
                    .child(lock_button)
                    .child(field(
                        &crate::i18n::t("editor.image.height", lang),
                        &popover.height,
                        popover.active == SizeField::Height,
                        SizeField::Height,
                        cx,
                        theme,
                    )),
            )
            .child(presets)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("image-size-reset")
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.reset_image_size(cx);
                            }))
                            .child(crate::i18n::t("editor.image.reset", lang)),
                    )
                    .child(
                        div()
                            .id("image-size-done")
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(theme.accent)
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.text_on_accent)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.apply_image_size(cx);
                                this.close_image_popover(window, cx);
                            }))
                            .child(crate::i18n::t("editor.image.done", lang)),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_radius_popover_body(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let lang = self.config.ui.language.as_str();
        let Some(popover) = self.image_popover.as_ref() else {
            return div().into_any_element();
        };
        let edit = crate::images::load_edit(&popover.path);
        let value = crate::images::current_corner_radius(&edit);
        let fill_w = RADIUS_TRACK_W * value / 50.0;

        let origin = std::rc::Rc::new(std::cell::Cell::new(0.0f32));
        let origin_for_canvas = origin.clone();
        let track = div()
            .id("image-radius-track")
            .relative()
            .w(px(RADIUS_TRACK_W))
            .h(px(18.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let percent = ((x - origin.get()) / RADIUS_TRACK_W * 50.0).clamp(0.0, 50.0);
                    this.set_image_radius(percent, cx);
                }),
            )
            .child(
                gpui::canvas(
                    move |bounds, _, _| origin_for_canvas.set(f32::from(bounds.origin.x)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(7.0))
                    .w_full()
                    .h(px(4.0))
                    .rounded_full()
                    .bg(theme.border),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(7.0))
                    .w(px(fill_w))
                    .h(px(4.0))
                    .rounded_full()
                    .bg(theme.accent),
            )
            .child(
                div()
                    .absolute()
                    .left(px((fill_w - 6.0).max(0.0)))
                    .top(px(2.0))
                    .w(px(12.0))
                    .h(px(12.0))
                    .rounded_full()
                    .bg(theme.accent)
                    .border_1()
                    .border_color(theme.bg_card),
            );

        let mut presets = div().flex().flex_row().gap(px(6.0));
        for percent in [0u32, 5, 10, 20, 50] {
            presets = presets.child(
                div()
                    .id(gpui::ElementId::Name(
                        SharedString::from(format!("image-radius-pct-{}", percent)),
                    ))
                    .flex_1()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_image_radius(percent as f32, cx);
                        }),
                    )
                    .child(format!("{}%", percent)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(track)
                    .child(
                        div()
                            .w(px(38.0))
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .child(format!("{}%", value.round() as u32)),
                    ),
            )
            .child(presets)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("image-radius-default")
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.reset_image_radius(cx);
                            }))
                            .child(crate::i18n::t("editor.image.default", lang)),
                    )
                    .child(
                        div()
                            .id("image-radius-done")
                            .h(px(26.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(theme.accent)
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.text_on_accent)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_image_popover(window, cx);
                            }))
                            .child(crate::i18n::t("editor.image.done", lang)),
                    ),
            )
            .into_any_element()
    }
}
