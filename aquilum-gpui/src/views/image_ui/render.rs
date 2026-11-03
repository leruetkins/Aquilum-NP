use super::*;

impl AppView {
    /// Draws one image block: the picture, its clipping radius, and the hover
    /// toolbar. Called from the editor row loop instead of the styled text row.
    pub fn render_image_block(
        &self,
        image: &ResolvedImage,
        line_idx: usize,
        show_toolbar: bool,
        interactive: bool,
        selected: bool,
        caption: Option<(String, String)>,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let radius = if image.show_original { 0.0 } else { image.radius_px() };
        let width = image.width;
        let height = image.height;
        let hover_line = line_idx;
        let editing = self
            .image_editor
            .as_ref()
            .filter(|editor| editor.line == line_idx && editor.target == image.reference.target);
        let editing_annotate = editing
            .map(|editor| editor.mode == crate::views::image_editor::ImageEditorMode::Annotate)
            .unwrap_or(false);
        let editing_crop = editing
            .map(|editor| editor.mode == crate::views::image_editor::ImageEditorMode::Crop)
            .unwrap_or(false);
        let normal_interaction = interactive && editing.is_none();

        let caption_editing = self
            .caption_editor
            .as_ref()
            .is_some_and(|state| state.line == line_idx && state.path == image.path);
        let caption_above_side = caption
            .as_ref()
            .map(|(_, position)| position == "above")
            .unwrap_or(self.config.images.caption_position == "above");
        let caption_text = caption.as_ref().map(|(text, _)| text.clone());
        // Same formula as images::block_height: the slot never shrinks below
        // the inline editor, so the layout does not jump when it opens.
        let caption_content_h = (crate::images::caption_height(&image.edit, width) - 6.0).max(0.0);
        let caption_slot_h = caption_content_h.max(crate::images::CAPTION_SLOT_H);

        let picture = if let Some(crop) = image.crop {
            match crate::image_render::cropped_display(&image.path, crop) {
                Some(source) => gpui::img(source),
                None => gpui::img(image.path.clone()),
            }
        } else {
            gpui::img(image.path.clone())
        }
        .w_full()
        .h_full()
        // `overflow_hidden` clips a rectangle, not a rounded shape; the image
        // quad itself carries the radii.
        .rounded(px(radius))
        .object_fit(gpui::ObjectFit::Fill);

        let mut wrapper = div()
            .id(("image-block", line_idx))
            .relative()
            .w(px(width))
            .h(px(height))
            .rounded(px(radius))
            .overflow_hidden()
            .flex_shrink_0()
            .when(normal_interaction, |d| d.cursor_pointer())
            // Annotation tools draw on the picture: the inherited I-beam read
            // as a text caret over the image.
            .when(editing_annotate, |d| d.cursor(gpui::CursorStyle::Arrow))
            .when(normal_interaction, |d| d.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut AppView, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if this.caption_editor.is_some() {
                        this.save_caption_editor(cx);
                    }
                    if event.click_count >= 2 {
                        // Second click is the way back into the markup: place
                        // the caret on the reference line, which reveals it.
                        if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                            let offset = doc.line_end_byte(line_idx);
                            view.cursor_offset = offset;
                            view.selection_anchor = Some(offset);
                        }
                        this.clear_image_selection();
                        this.hovered_image_line = None;
                        this.hovered_image_doc = None;
                    } else {
                        this.select_image_line(line_idx);
                        this.hovered_image_line = Some(line_idx);
                        this.hovered_image_doc = this.state.active_doc_id();
                    }
                    window.focus(&this.focus_handle);
                    cx.notify();
                }),
            ))
            .when(normal_interaction, |d| d.on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this
                        .size_drag
                        .as_ref()
                        .is_some_and(|drag| drag.line == line_idx)
                    {
                        this.image_size_drag_end(cx);
                    }
                }),
            ))
            .child(picture)
            .when(editing_annotate, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.image_editor_pointer_down(event.position, window, cx);
                    }),
                )
            });

        // Vector annotations live above the picture and stay editable. The
        // geometry comes from the same model the exporter rasterises. While a
        // tool is dragging, its draft is painted in the same pass so lines,
        // arrows and shapes are visible before the mouse is released.
        let mut annotations = image.edit.annotations.clone();
        if editing_annotate {
            if let Some(editor) = self.image_editor.as_ref() {
                if let Some(draft) = editor
                    .annotation_drag
                    .as_ref()
                    .and_then(|drag| drag.draft.as_ref())
                {
                    annotations.push(draft.clone());
                }
            }
        }
        if !annotations.is_empty() {
            let scale = if image.visible.0 > 0 {
                width / image.visible.0 as f32
            } else {
                1.0
            };
            let offset = image.crop.map_or((0.0, 0.0), |crop| (crop.x, crop.y));
            let painted = annotations.clone();
            wrapper = wrapper.child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint_annotation_paths(window, &painted, scale, offset, bounds.origin);
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .w_full()
                .h_full(),
            );

            let mut labels = 0usize;
            for annotation in annotations.iter().filter(|annotation| annotation.is_textual()) {
                let point = annotation.points.first().copied().unwrap_or_default();
                let mut text = annotation.text.clone().unwrap_or_default();
                let is_label = matches!(annotation.kind, crate::images::AnnotationKind::Label);
                if is_label {
                    labels += 1;
                    if text.trim().is_empty() {
                        text = labels.to_string();
                    }
                }
                if text.is_empty() {
                    continue;
                }
                let color = if is_label {
                    [255, 255, 255, 255]
                } else {
                    crate::images::parse_hex_color(&annotation.color)
                };
                let font_size = (annotation.font_size * scale).max(6.0);
                let x = (point.x - offset.0) * scale;
                let y = (point.y - offset.1) * scale;
                let max_width = (width - x).max(font_size);
                if let Some((source, text_w, text_h)) =
                    crate::image_render::gpui_text_image(&text, font_size, color, max_width)
                {
                    let left = if is_label { x - text_w as f32 / 2.0 } else { x };
                    let top = if is_label { y - text_h as f32 / 2.0 } else { y };
                    wrapper = wrapper.child(
                        div()
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(text_w as f32))
                            .h(px(text_h as f32))
                            .child(gpui::img(source).w_full().h_full().object_fit(gpui::ObjectFit::Fill)),
                    );
                }
            }
        }

        if editing_annotate {
            if let Some(editor) = self.image_editor.as_ref() {
                if editor.tool == crate::views::image_editor::AnnotationTool::Select {
                    if let Some(index) = editor.selected {
                        if let Some(annotation) = editor.edit.annotations.get(index) {
                            let scale = if image.visible.0 > 0 {
                                width / image.visible.0 as f32
                            } else {
                                1.0
                            };
                            let offset = image.crop.map_or((0.0, 0.0), |crop| (crop.x, crop.y));
                            if let Some(bounds) = annotation.bounds() {
                                wrapper = wrapper.child(
                                    div()
                                        .id(gpui::ElementId::Name(SharedString::from(format!(
                                            "annotation-frame-{}",
                                            index
                                        ))))
                                        .absolute()
                                        .left(px((bounds.x - offset.0) * scale))
                                        .top(px((bounds.y - offset.1) * scale))
                                        .w(px(bounds.w * scale))
                                        .h(px(bounds.h * scale))
                                        .border_1()
                                        .border_dashed()
                                        .border_color(theme.accent)
                                        .cursor(gpui::CursorStyle::OpenHand)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                cx.stop_propagation();
                                                this.image_editor_begin_move(
                                                    index,
                                                    event.position,
                                                    cx,
                                                );
                                            }),
                                        ),
                                );
                            }
                            if matches!(
                                annotation.kind,
                                crate::images::AnnotationKind::Arrow
                                    | crate::images::AnnotationKind::Line
                                    | crate::images::AnnotationKind::Rect
                                    | crate::images::AnnotationKind::Ellipse
                            ) {
                                let points = annotation.points.clone();
                                for (point_index, point) in points.iter().take(2).enumerate() {
                                    let x = (point.x - offset.0) * scale;
                                    let y = (point.y - offset.1) * scale;
                                    let origin = points.clone();
                                    let start_image = *point;
                                    wrapper = wrapper.child(
                                        div()
                                            .id(gpui::ElementId::Name(SharedString::from(format!(
                                                "annotation-handle-{}-{}",
                                                index, point_index
                                            ))))
                                            .absolute()
                                            .left(px(x - 4.0))
                                            .top(px(y - 4.0))
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .bg(gpui::rgb(0xffffff))
                                            .border_1()
                                            .border_color(theme.accent)
                                            .rounded_xs()
                                            .cursor(gpui::CursorStyle::Crosshair)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    if let Some(editor) = this.image_editor.as_mut() {
                                                        editor.annotation_drag =
                                                            Some(crate::views::image_editor::AnnotationDrag {
                                                                origin: origin.clone(),
                                                                start_image,
                                                                index: Some(index),
                                                                handle: Some(point_index),
                                                                draft: None,
                                                            });
                                                    }
                                                    let _ = event;
                                                    cx.notify();
                                                }),
                                            ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let size_popover = self.image_popover.as_ref().filter(|popover| {
            popover.line == line_idx && popover.kind == ImagePopoverKind::Size
        });
        let show_resize = normal_interaction && (selected || size_popover.is_some());
        if show_resize {
            wrapper = wrapper.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w_full()
                    .h_full()
                    .border_1()
                    .border_color(theme.accent),
            );
            let lock_default = size_popover.map(|popover| popover.lock).unwrap_or(true);
            let max_width = image.max_width.max(32.0);
            let handles: [(&str, f32, f32, f32, f32, gpui::CursorStyle); 8] = [
                ("nw", 1.0, 1.0, -1.0, -1.0, gpui::CursorStyle::ResizeUpLeftDownRight),
                ("n", width / 2.0 - 4.0, 1.0, 0.0, -1.0, gpui::CursorStyle::ResizeUpDown),
                ("ne", width - 9.0, 1.0, 1.0, -1.0, gpui::CursorStyle::ResizeUpRightDownLeft),
                ("e", width - 9.0, height / 2.0 - 4.0, 1.0, 0.0, gpui::CursorStyle::ResizeLeftRight),
                ("se", width - 9.0, height - 9.0, 1.0, 1.0, gpui::CursorStyle::ResizeUpLeftDownRight),
                ("s", width / 2.0 - 4.0, height - 9.0, 0.0, 1.0, gpui::CursorStyle::ResizeUpDown),
                ("sw", 1.0, height - 9.0, -1.0, 1.0, gpui::CursorStyle::ResizeUpRightDownLeft),
                ("w", 1.0, height / 2.0 - 4.0, -1.0, 0.0, gpui::CursorStyle::ResizeLeftRight),
            ];
            for (name, left, top, sx, sy, cursor) in handles {
                wrapper = wrapper.child(
                    div()
                        .id(gpui::ElementId::Name(SharedString::from(format!(
                            "image-size-handle-{}",
                            name
                        ))))
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(8.0))
                        .h(px(8.0))
                        .bg(gpui::rgb(0xffffff))
                        .border_1()
                        .border_color(theme.accent)
                        .rounded_xs()
                        .cursor(cursor)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.size_drag = Some(SizeDrag {
                                    line: line_idx,
                                    max_width,
                                    start: (
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                    ),
                                    start_size: (width, height),
                                    current: (width, height),
                                    sign: (sx, sy),
                                    lock: lock_default && !event.modifiers.shift,
                                });
                                cx.notify();
                            }),
                        ),
                );
            }
        }

        // A non-destructive crop keeps hidden pixels in the file; say so on the
        // picture itself.
        if image.crop.is_some() && !image.show_original {
            wrapper = wrapper.child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .bottom(px(6.0))
                    .h(px(18.0))
                    .px(px(5.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .rounded_sm()
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(icons::icon(icons::ICON_CANVAS, 10.0, theme.text_muted))
                    .child(crate::i18n::t(
                        "editor.image.cropped",
                        &self.config.ui.language,
                    )),
            );
        }

        if editing_crop {
            wrapper = wrapper.child(self.render_crop_frame_overlay(image, line_idx, theme, cx));
        }

        // Pictures are centered in the note column; the toolbar band and the
        // caption slot share the same width, so they move together. While the
        // inline caption editor is open the block grows to a usable minimum,
        // so a narrow picture cannot squeeze the text field.
        let caption_width = if caption_editing {
            width
                .max(CAPTION_EDITOR_MIN_W)
                .min(image.max_width.max(width))
        } else {
            width
        };
        let block_width = width.max(caption_width);
        let center_margin = ((image.max_width - block_width) / 2.0).max(0.0);
        let mut block = div()
            .id(gpui::ElementId::Name(SharedString::from(format!(
                "image-block-body-{}",
                line_idx
            ))))
            .ml(px(center_margin))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .w(px(block_width))
            .flex_shrink_0()
            // Hover belongs to the whole block, not just the picture: moving
            // from the picture to its floating bar crosses the gap and must
            // not make the bar disappear under the pointer.
            .when(normal_interaction, |d| d.on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    if this.hovered_image_line != Some(hover_line) {
                        this.hovered_image_line = Some(hover_line);
                        this.hovered_image_doc = this.state.active_doc_id();
                        cx.notify();
                    }
                } else if this.hovered_image_line == Some(hover_line)
                    && this.selected_image_line != Some(hover_line)
                    && this.image_popover.as_ref().map_or(true, |p| p.line != hover_line)
                {
                    this.hovered_image_line = None;
                    this.hovered_image_doc = None;
                    cx.notify();
                }
            })));
        // The action bar lives in this band, not floating over the previous
        // paragraph. The band is always reserved so hover/selection never
        // shifts the note.
        let mut band = div()
            .relative()
            .w(px(width))
            .h(px(crate::images::TOOLBAR_BAND))
            .flex_shrink_0();
        if show_toolbar && editing.is_none() {
            // Center the bar on the same axis as the picture/page instead of
            // pinning it to the picture's right edge; narrow pictures then
            // still show a centered toolbar.
            band = band.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(self.render_image_toolbar(image, line_idx, theme, cx)),
            );
        }
        block = block.child(band);
        // A fixed-height slot holds either the static caption or the editor.
        // Its whole area is clickable, so the hitbox is taller than one text
        // line and the reserved room never reflows when the editor opens.
        let mut caption_box = if image.show_original {
            None
        } else {
            let mut caption_box = div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "image-caption-slot-{}",
                    line_idx
                ))))
                .w(px(block_width))
                .h(px(caption_slot_h))
                .flex()
                .flex_col()
                .items_center()
                .flex_shrink_0()
                .when(caption_above_side, |d| {
                    d.justify_end().mb(px(crate::images::CAPTION_SEPARATION))
                })
                .when(!caption_above_side, |d| {
                    d.justify_start().mt(px(crate::images::CAPTION_SEPARATION))
                });
            if caption_editing {
                caption_box = caption_box
                    .child(self.render_caption_inline_editor(image, line_idx, theme, window, cx));
            } else if let Some(text) = caption_text.clone() {
                let holder = image.clone();
                caption_box = caption_box
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_caption_editor(line_idx, &holder, window, cx);
                        }),
                    )
                    .child(self.render_image_caption(text, width, theme, line_idx, image, cx));
            }
            Some(caption_box)
        };
        if caption_above_side {
            if let Some(caption_box) = caption_box.take() {
                block = block.child(caption_box);
            }
        }
        let mut area = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .child(wrapper);
        if let Some(drag) = self.size_drag.as_ref().filter(|drag| drag.line == line_idx) {
            area = area.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(drag.current.0))
                    .h(px(drag.current.1))
                    .border_1()
                    .border_dashed()
                    .border_color(theme.accent),
            );
        }
        block = block.child(area);
        if !caption_above_side {
            if let Some(caption_box) = caption_box.take() {
                block = block.child(caption_box);
            }
        }
        block.into_any_element()
    }

    pub(super) fn render_image_caption(
        &self,
        text: String,
        width: f32,
        theme: &Theme,
        line_idx: usize,
        image: &ResolvedImage,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let holder = image.clone();
        div()
            .id(("image-caption", line_idx))
            .w(px(width))
            .flex()
            .justify_center()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_center()
            .text_color(theme.text_secondary)
            .cursor_pointer()
            .hover(|h| h.text_color(theme.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_caption_editor(line_idx, &holder, window, cx);
                }),
            )
            .child(text)
    }

    pub(super) fn render_image_toolbar(
        &self,
        image: &ResolvedImage,
        line_idx: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let lang = self.config.ui.language.as_str();
        // The menu strings carry a trailing ellipsis because the context menu
        // opens a dialog. A toolbar caption is a direct action, so drop it.
        let label = |key: &str| {
            crate::i18n::t(key, lang)
                .trim_end_matches('…')
                .trim()
                .to_string()
        };
        let holder_size = image.clone();
        let holder_radius = image.clone();
        let holder_crop = image.clone();
        let holder_annotate = image.clone();
        let holder_caption = image.clone();
        let holder_compress = image.clone();
        let holder_line = line_idx;

        let mut bar = div()
            .id(("image-toolbar", line_idx))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered && this.hovered_image_line != Some(line_idx) {
                    this.hovered_image_line = Some(line_idx);
                    cx.notify();
                }
            }))
            .h(px(crate::images::TOOLBAR_H))
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                if this.caption_editor.is_some() {
                    this.save_caption_editor(cx);
                }
            }))
            .child(self.image_tool_button(
                "image-tool-view",
                icons::ICON_SEARCH,
                label("editor.image.view_full"),
                true,
                theme,
                {
                    let viewer_path = image.path.clone();
                    let viewer_natural = image.natural;
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_image_viewer(viewer_path.clone(), viewer_natural, window, cx);
                    })
                },
            ))
            .child(self.image_tool_button(
                "image-tool-crop",
                icons::ICON_IMAGE_CROP,
                label("editor.image.menu_crop"),
                true,
                theme,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_image_editor(
                        crate::views::image_editor::ImageEditorMode::Crop,
                        holder_line,
                        &holder_crop,
                        cx,
                    );
                }),
            ))
            .child(self.image_tool_button(
                "image-tool-size",
                icons::ICON_IMAGE_RESIZE,
                label("editor.image.menu_size"),
                true,
                theme,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_image_popover(
                        ImagePopoverKind::Size,
                        &holder_size,
                        holder_line,
                        x,
                        y,
                        window,
                        cx,
                    );
                }),
            ))
            .child(self.image_tool_button(
                "image-tool-radius",
                icons::ICON_IMAGE_ROUND,
                label("editor.image.menu_radius"),
                true,
                theme,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_image_popover(
                        ImagePopoverKind::Radius,
                        &holder_radius,
                        holder_line,
                        x,
                        y,
                        window,
                        cx,
                    );
                }),
            ))
            .child(self.image_tool_button(
                "image-tool-compress",
                icons::ICON_IMAGE_COMPRESS,
                label("editor.image.menu_compress"),
                true,
                theme,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_compress_dialog(holder_line, &holder_compress, window, cx);
                }),
            ))
            .child(self.image_tool_button(
                "image-tool-annotate",
                icons::ICON_IMAGE_ANNOTATE,
                label("editor.image.menu_annotate"),
                true,
                theme,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_image_editor(
                        crate::views::image_editor::ImageEditorMode::Annotate,
                        holder_line,
                        &holder_annotate,
                        cx,
                    );
                }),
            ))
            .child(self.image_tool_button(
                "image-tool-caption",
                icons::ICON_IMAGE_CAPTION,
                label("editor.image.menu_caption"),
                true,
                theme,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_caption_editor(holder_line, &holder_caption, window, cx);
                }),
            ));

        if image.crop.is_some() {
            bar = bar.child(self.image_tool_button(
                "image-tool-show-original",
                icons::ICON_EYE,
                crate::i18n::t(
                    if image.show_original {
                        "editor.image.hide_original"
                    } else {
                        "editor.image.show_original"
                    },
                    lang,
                ),
                true,
                theme,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_image_editor_at_line(
                        crate::views::image_editor::ImageEditorMode::Crop,
                        holder_line,
                        cx,
                    );
                }),
            ));
        }

        bar.child(self.image_tool_button(
            "image-tool-delete",
            icons::ICON_IMAGE_DELETE,
            label("editor.image.menu_delete"),
            true,
            theme,
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.delete_image_at_line(holder_line, cx);
            }),
        ))
        .into_any_element()
    }

    /// Opens the full-window preview of the original file (not the cropped
    /// view). The magnifier on the toolbar is the entry point.
    pub(crate) fn open_image_viewer(
        &mut self,
        path: PathBuf,
        natural: (u32, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".to_string());
        // Leave room for the title bar and a small breathing margin. The
        // larger natural side decides which direction is limiting.
        let size = window.bounds().size;
        let available = (
            (f32::from(size.width) - 48.0).max(1.0),
            (f32::from(size.height) - 88.0).max(1.0),
        );
        let fit = fit_scale(natural, available);
        self.image_popover = None;
        self.image_viewer = Some(ImageViewerState {
            path,
            title,
            natural,
            scale: fit,
            fit,
            pan: (0.0, 0.0),
            drag: None,
        });
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn close_image_viewer(&mut self, cx: &mut Context<Self>) {
        if self.image_viewer.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_image_viewer_drag(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(viewer) = self.image_viewer.as_mut() else {
            return;
        };
        if event.click_count >= 2 {
            viewer.scale = viewer.fit;
            viewer.pan = (0.0, 0.0);
            viewer.drag = None;
        } else {
            let x: f32 = event.position.x.into();
            let y: f32 = event.position.y.into();
            viewer.drag = Some((x, y, viewer.pan.0, viewer.pan.1));
        }
        cx.notify();
    }

    pub(super) fn drag_image_viewer(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(viewer) = self.image_viewer.as_mut() else {
            return;
        };
        let Some((start_x, start_y, pan_x, pan_y)) = viewer.drag else {
            return;
        };
        let x: f32 = event.position.x.into();
        let y: f32 = event.position.y.into();
        viewer.pan = (pan_x + x - start_x, pan_y + y - start_y);
        cx.notify();
    }

    pub(super) fn end_image_viewer_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = self.image_viewer.as_mut() {
            if viewer.drag.take().is_some() {
                cx.notify();
            }
        }
    }

    pub(super) fn zoom_image_viewer(&mut self, factor: f32, cx: &mut Context<Self>) {
        if let Some(viewer) = self.image_viewer.as_mut() {
            viewer.scale = (viewer.scale * factor).clamp(0.05, 8.0);
            cx.notify();
        }
    }

    pub(crate) fn handle_image_viewer_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_image_viewer(cx);
                window.focus(&self.focus_handle);
            }
            "+" | "=" => self.zoom_image_viewer(1.1, cx),
            "-" => self.zoom_image_viewer(1.0 / 1.1, cx),
            "0" => {
                if let Some(viewer) = self.image_viewer.as_mut() {
                    viewer.scale = viewer.fit;
                    viewer.pan = (0.0, 0.0);
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn render_image_viewer(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(viewer) = self.image_viewer.as_ref() else {
            return div().into_any_element();
        };
        let window_w = f32::from(window.bounds().size.width);
        let window_h = f32::from(window.bounds().size.height);
        let natural_w = viewer.natural.0.max(1) as f32;
        let natural_h = viewer.natural.1.max(1) as f32;
        let width = natural_w * viewer.scale;
        let height = natural_h * viewer.scale;
        let left = (window_w - width) / 2.0 + viewer.pan.0;
        let top = (window_h - height) / 2.0 + viewer.pan.1;
        let dragging = viewer.drag.is_some();
        let title = format!(
            "{}  ·  {:.0}%  ·  {} × {}",
            viewer.title,
            viewer.scale * 100.0,
            viewer.natural.0,
            viewer.natural.1
        );

        div()
            .id("image-viewer-backdrop")
            .absolute()
            .inset_0()
            .bg(gpui::rgba(0x000000f2))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_image_viewer(cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.drag_image_viewer(event, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.end_image_viewer_drag(cx);
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
                let dy = match event.delta {
                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                    gpui::ScrollDelta::Lines(l) => l.y * 30.0,
                };
                if dy != 0.0 {
                    this.zoom_image_viewer((dy / 320.0).exp(), cx);
                }
            }))
            .child(
                div()
                    .id("image-viewer-picture")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .cursor(if dragging {
                        gpui::CursorStyle::ClosedHand
                    } else {
                        gpui::CursorStyle::OpenHand
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.begin_image_viewer_drag(event, cx);
                        }),
                    )
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.end_image_viewer_drag(cx);
                    }))
                    .child(
                        gpui::img(viewer.path.clone())
                            .w_full()
                            .h_full()
                            .object_fit(gpui::ObjectFit::Fill),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(12.0))
                    .left(px(16.0))
                    .right(px(16.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .bg(gpui::rgba(0x00000080))
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded_md()
                            .child(title),
                    )
                    .child(self.image_close_button(
                        "image-viewer-close",
                        theme,
                        cx.listener(|this, _, _, cx| this.close_image_viewer(cx)),
                    )),
            )
            .into_any_element()
    }

    /// Close cross for every image editing window. Sits in the top-right
    /// corner of the header row; the caller picks the close semantics.
    pub(crate) fn image_close_button(
        &self,
        id: &'static str,
        theme: &Theme,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.bg(theme.bg_hover))
            .on_mouse_down(MouseButton::Left, listener)
            .child(icons::icon(icons::ICON_CLOSE, 13.0, theme.text_muted))
    }

    pub(super) fn image_tool_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: String,
        enabled: bool,
        theme: &Theme,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let color = if enabled {
            theme.text_secondary
        } else {
            theme.text_muted
        };
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .h(px(26.0))
            .px(px(7.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded_md()
            .when(enabled, |d| {
                d.cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |d| d.opacity(0.35).cursor_not_allowed())
            .child(icons::icon(icon, 14.0, color))
            .child(
                div()
                    .h(px(15.0))
                    .line_height(px(14.0))
                    .text_size(px(10.5))
                    .font_family("Segoe UI")
                    .whitespace_nowrap()
                    .text_color(color)
                    .child(label),
            )
    }

    /// A reference whose file cannot be resolved: a dashed warning strip in
    /// place of the picture, without letting the row collapse into text.
    pub fn render_broken_image(&self, target: &str, theme: &Theme) -> AnyElement {
        div()
            .h(px(24.0))
            .max_w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(theme.marked_line_red())
            .text_xs()
            .text_color(theme.text_muted)
            .whitespace_nowrap()
            .child(icons::icon(icons::ICON_CIRCLE_ALERT, 13.0, theme.marked_line_red()))
            .child(crate::i18n::t("editor.image.broken", &self.config.ui.language))
            .child(div().text_color(theme.text_secondary).child(target.to_string()))
            .into_any_element()
    }

    // ─────────────────────── Context menu ───────────────────────
}
