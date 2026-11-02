//! Inline image block, its hover toolbar, and the size/radius popovers.
//!
//! The block itself is painted by `render_central_editor`; everything that
//! needs its own state (hover line, selected line, open popover) lives here so
//! the editor loop stays readable.

use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, IntoElement, MouseButton, MouseDownEvent,
    SharedString, Stateful, StyledImage, Window,
};

use crate::icons;
use crate::images::{self, ImageImportSource, ImageReference, ImportedImage, ResolvedImage};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::notification::NotificationType;
use crate::views::text_input::TextInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePopoverKind {
    Size,
    Radius,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeField {
    Width,
    Height,
}

/// Floating editor anchored at the toolbar button that opened it.
pub struct ImagePopoverState {
    pub kind: ImagePopoverKind,
    pub line: usize,
    pub path: PathBuf,
    pub reference: ImageReference,
    pub natural: (u32, u32),
    pub wrap_cols: usize,
    pub x: f32,
    pub y: f32,
    /// Focus handle of the panel itself, so the radius editor (which has no
    /// text field) still receives keys through the panel's `on_key_down`.
    pub focus: gpui::FocusHandle,
    pub width: TextInput,
    pub height: TextInput,
    pub active: SizeField,
    pub lock: bool,
}

impl ImagePopoverState {
    pub fn active_input(&self) -> &TextInput {
        match self.active {
            SizeField::Width => &self.width,
            SizeField::Height => &self.height,
        }
    }

    pub fn active_input_mut(&mut self) -> &mut TextInput {
        match self.active {
            SizeField::Width => &mut self.width,
            SizeField::Height => &mut self.height,
        }
    }
}

pub struct SizeDrag {
    pub line: usize,
    pub max_width: f32,
    pub start: (f32, f32),
    pub start_size: (f32, f32),
    pub current: (f32, f32),
    pub sign: (f32, f32),
    pub lock: bool,
}

const POPOVER_W: f32 = 268.0;
const RADIUS_TRACK_W: f32 = 196.0;

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
        // caption slot share the same width, so they move together.
        let center_margin = ((image.max_width - width) / 2.0).max(0.0);
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
            .w(px(width))
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
                .w(px(width))
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

    fn render_image_caption(
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

    fn render_image_toolbar(
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

    fn image_tool_button(
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

    pub fn save_image_as(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line);
        let wrap_cols = self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80);
        let Some(image) = images::resolve_for_note(&doc.path, &text, wrap_cols, false) else {
            return;
        };
        let edit = image.edit.clone();
        let caption = edit
            .caption
            .as_ref()
            .filter(|caption| !caption.text.trim().is_empty())
            .map(|caption| (caption.text.clone(), caption.position.clone()));
        let options = crate::image_render::BakeOptions {
            apply_crop: true,
            apply_corner_radius: true,
            caption,
            caption_background: None,
        };
        let baked = match crate::image_render::bake_rgba(&image.path, &edit, &options)
            .and_then(|rgba| crate::image_render::encode_rgba(&rgba, crate::image_render::EncodedFormat::Png, 92))
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.push_notification(NotificationType::Error, None, error, Some(5000), cx);
                return;
            }
        };
        let default_name = format!(
            "{}-edited.png",
            image.path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
        );
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        cx.foreground_executor()
            .spawn(async move {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PNG image (*.png)", &["png"])
                    .set_file_name(&default_name)
                    .save_file()
                    .await;
                if let Some(handle) = handle {
                    let path = handle.path().to_path_buf();
                    let _ = entity.update(&mut async_cx, |this, cx| {
                        match std::fs::write(&path, &baked) {
                            Ok(()) => {
                                this.push_notification(
                                    NotificationType::Success,
                                    None,
                                    crate::i18n::t("editor.image.saved_image", &this.config.ui.language),
                                    Some(4000),
                                    cx,
                                );
                            }
                            Err(_) => {
                                this.push_notification(
                                    NotificationType::Error,
                                    None,
                                    crate::i18n::t("editor.image.save_failed", &this.config.ui.language),
                                    Some(5000),
                                    cx,
                                );
                            }
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
    }

    pub fn open_image_context_menu(&mut self, line: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        use crate::views::editor_context_menu::{item, separator, ContextMenuState};
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);
        let items = vec![
            item("image-copy-reference", t("editor.image.menu_copy_ref"), "copy", false),
            item("image-cut", t("editor.image.menu_cut"), "cut", false),
            item("image-paste", t("editor.image.menu_paste"), "download", false),
            separator("sep-image-clipboard"),
            item("image-copy", t("editor.image.menu_copy"), "copy", false),
            item("image-copy-link", t("editor.image.menu_copy_link"), "link", false),
            item("image-save-as", t("editor.image.menu_save_as"), "download", false),
            separator("sep-image-clipboard-2"),
            item("image-size", t("editor.image.menu_size"), "move", false),
            item("image-radius", t("editor.image.menu_radius"), "maximize", false),
            separator("sep-image-edit"),
            item("image-crop", t("editor.image.menu_crop"), "crop", false),
            item("image-annotate", t("editor.image.menu_annotate"), "pencil", false),
            item("image-caption", t("editor.image.menu_caption"), "type", false),
            item("image-compress", t("editor.image.menu_compress"), "download", false),
            separator("sep-image-destructive"),
            item("image-open-original", t("editor.image.menu_show_original"), "eye", false),
            item("image-delete", t("editor.image.menu_delete"), "trash", false),
        ];
        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.image_popover = None;
        self.editor_context_menu = ContextMenuState {
            is_open: true,
            session_id,
            position_x: x,
            position_y: y,
            items,
            active_path: Vec::new(),
            table_target: None,
            image_target: Some(line),
        };
        self.select_image_line(line);
        cx.notify();
    }

    /// Puts the attachment's own bytes back on the clipboard as an image.
    pub fn copy_image_to_clipboard(&self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let root = images::vault_root();
        let folder = self.config.files.folder.clone();
        let text = doc.line_text(line);
        let Some(reference) = images::parse_image_reference(&text) else {
            return;
        };
        let Some(path) = images::resolve_attachment(root.as_deref(), &doc.path, &folder, &reference.target) else {
            return;
        };
        let Some(format) = std::path::Path::new(&reference.target)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_ascii_lowercase().as_str() {
                "png" => Some(gpui::ImageFormat::Png),
                "jpg" | "jpeg" => Some(gpui::ImageFormat::Jpeg),
                "webp" => Some(gpui::ImageFormat::Webp),
                "gif" => Some(gpui::ImageFormat::Gif),
                "bmp" => Some(gpui::ImageFormat::Bmp),
                "tif" | "tiff" => Some(gpui::ImageFormat::Tiff),
                _ => None,
            })
        else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
            format, bytes,
        )));
    }

    /// Copies the wiki reference line itself, with metadata that lets a paste
    /// into another note copy (or move, for Cut) the image and its sidecar.
    pub fn copy_image_reference(
        &mut self,
        line: usize,
        cut: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.state.active_document() else {
            return false;
        };
        let text = doc.line_text(line);
        let Some(reference) = images::parse_image_reference(&text) else {
            return false;
        };
        if !reference.wiki {
            return false;
        }
        let Some(meta) = self.attachment_clipboard_metadata(&text, cut) else {
            return false;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_json_metadata(
            text.clone(),
            meta,
        ));
        crate::images::set_clipboard_attachment_meta(&doc.path, &reference.target, cut);
        if cut {
            images::protect_cut(&doc.path, &reference.target);
            self.delete_image_at_line(line, cx);
        }
        true
    }

    /// Handles every clipboard payload that carries a picture: a raster, a
    /// file list, or wiki references (with or without attachment metadata).
    /// Returns `true` when the paste was consumed, so callers may fall back to
    /// plain text only for text-only clipboards.
    pub fn paste_clipboard_image_aware(&mut self, cx: &mut Context<Self>) -> bool {
        let raster = cx
            .read_from_clipboard()
            .and_then(|item| {
                item.entries().iter().find_map(|entry| match entry {
                    gpui::ClipboardEntry::Image(image) => {
                        Some((image.bytes().to_vec(), image.format()))
                    }
                    _ => None,
                })
            })
            .or_else(crate::images::clipboard_raster);
        if let Some((bytes, format)) = raster {
            if self.import_clipboard_image(bytes, format, cx) {
                return true;
            }
        }
        if let Some(paths) = crate::images::clipboard_file_paths() {
            if self.import_clipboard_files(paths, cx) {
                return true;
            }
        }
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                if let Some(meta) = crate::images::clipboard_attachment_meta() {
                    if self.copy_pasted_references(&text, Some(&meta), cx) {
                        return true;
                    }
                }
                if let Some(meta) = item
                    .metadata()
                    .and_then(|meta| serde_json::from_str::<serde_json::Value>(meta).ok())
                    .and_then(|value| value.get("vibecodiumAttachment").cloned())
                {
                    if self.copy_pasted_references(&text, Some(&meta), cx) {
                        return true;
                    }
                }
                if text.contains("![[") && self.copy_pasted_references(&text, None, cx) {
                    return true;
                }
            }
        }
        false
    }

    /// Context-menu Paste: inserts the clipboard right after the image line.
    pub fn paste_after_image(&mut self, line: usize, cx: &mut Context<Self>) {
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let offset = doc.line_end_byte(line);
            view.cursor_offset = offset;
            view.selection_anchor = None;
            view.visual_cache_dirty = true;
        }
        if self.paste_clipboard_image_aware(cx) {
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                let text = crate::document_buffer::normalize_newlines(&text).into_owned();
                self.state.active_insert_text(&text);
                self.schedule_auto_save(cx);
                cx.notify();
            }
        }
    }

    /// Image actions from keyboard shortcuts when the picture is selected.
    pub fn handle_selected_image_clipboard_key(
        &mut self,
        key: &str,
        ctrl: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !ctrl {
            return false;
        }
        let Some(line) = self.selected_image_line else {
            return false;
        };
        if self.selected_image_doc != self.state.active_doc_id() {
            return false;
        }
        if key.eq_ignore_ascii_case("c") || key == "с" || key == "С" {
            return self.copy_image_reference(line, false, cx);
        }
        if key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч" {
            return self.copy_image_reference(line, true, cx);
        }
        false
    }

    pub fn copy_image_link(&self, line: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.state.active_document() else {
            return;
        };
        let text = doc.line_text(line).trim().to_string();
        if !text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    /// Context-menu entry point for the size/radius editors: resolves the line
    /// and anchors the popover at the menu position.
    pub fn open_image_popover_at_line(
        &mut self,
        kind: ImagePopoverKind,
        line: usize,
        x: f32,
        y: f32,
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
        self.open_image_popover(kind, &image, line, x, y, window, cx);
    }

    /// "Insert image…" command: native picker, then the same importer as a
    /// drop or a clipboard paste.
    pub fn open_image_file_dialog(&mut self, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let start = self
            .state
            .active_document()
            .and_then(|doc| doc.path.parent().map(|parent| parent.to_path_buf()))
            .or_else(|| self.state.workspace_path.clone());
        cx.foreground_executor()
            .spawn(async move {
                let mut dialog = rfd::AsyncFileDialog::new()
                    .add_filter("Images", images::IMAGE_EXTENSIONS);
                if let Some(dir) = start {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(handles) = dialog.pick_files().await {
                    let paths: Vec<PathBuf> =
                        handles.into_iter().map(|handle| handle.path().to_path_buf()).collect();
                    let _ = entity.update(&mut async_cx, |this, cx| {
                        this.import_image_files(paths, cx);
                    });
                }
            })
            .detach();
    }

    // ─────────────────────── Import ───────────────────────

    /// Imports raster clipboard data and inserts the reference at the caret.
    /// Returns `false` when there is no active note or nothing was written.
    pub fn import_clipboard_image(
        &mut self,
        bytes: Vec<u8>,
        format: gpui::ImageFormat,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        let results = images::import_images(
            &note,
            &folder,
            ImageImportSource::Clipboard { bytes, format },
        );
        self.finish_image_import(results, cx)
    }

    /// Clipboard file list (Explorer's Ctrl+C). Image files are imported;
    /// anything else is skipped with one toast. Returns true when the
    /// clipboard *was* a file list, so the caller does not paste its merged
    /// text representation.
    pub fn import_clipboard_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let images: Vec<PathBuf> = paths
            .iter()
            .filter(|path| images::is_image_path(path))
            .cloned()
            .collect();
        let skipped = paths.len().saturating_sub(images.len());
        if !images.is_empty() {
            self.import_image_files(images, cx);
        }
        if skipped > 0 {
            let language = self.config.ui.language.clone();
            self.push_notification(
                NotificationType::Info,
                None,
                crate::i18n::t("editor.image.drop_not_image", &language),
                Some(5000),
                cx,
            );
        }
        true
    }

    /// Imports files (drop, clipboard file list, picker) next to the note.
    pub fn import_image_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        for path in &paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.len() > 20 * 1024 * 1024 {
                    self.push_notification(
                        NotificationType::Info,
                        None,
                        crate::i18n::t("editor.image.large_file", &self.config.ui.language),
                        Some(6000),
                        cx,
                    );
                    break;
                }
            }
        }
        let results = images::import_images(&note, &folder, ImageImportSource::Files(paths));
        self.finish_image_import(results, cx)
    }

    fn finish_image_import(
        &mut self,
        results: Vec<Result<ImportedImage, images::ImportError>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut imported = Vec::new();
        let mut first_error = None;
        for result in results {
            match result {
                Ok(image) => imported.push(image),
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err.message(&self.config.ui.language));
                    }
                }
            }
        }
        if let Some(message) = first_error {
            let language = self.config.ui.language.clone();
            self.push_notification(
                NotificationType::Error,
                Some(crate::i18n::t("editor.image.import_error", &language)),
                message,
                Some(6000),
                cx,
            );
        }
        if imported.is_empty() {
            return false;
        }

        let references: Vec<String> = imported
            .iter()
            .map(|image| format!("![[{}]]", image.name))
            .collect();
        let text = self.image_insert_text(&references);
        self.state.active_replace_selection(&text);
        self.schedule_auto_save(cx);
        // Put the caret on the empty line created under the last inserted
        // reference and select that reference line, so the picture is painted
        // at once instead of its markup.
        let last_reference = references.last().cloned().unwrap_or_default();
        let reference_count = references.len();
        let inserted_line = self.state.active_view_and_doc_mut().and_then(|(view, doc)| {
            let lines_total = doc.line_index.lines();
            let (caret_line, _) = view.line_col(doc);
            let first = caret_line.saturating_sub(reference_count + 3);
            let mut ref_line = None;
            for line in first..=caret_line.min(lines_total.saturating_sub(1)) {
                if doc.line_text(line).trim() == last_reference {
                    ref_line = Some(line);
                }
            }
            let ref_line = ref_line?;
            let empty_line = (ref_line + 1).min(lines_total.saturating_sub(1));
            let offset = doc.line_start_byte(empty_line);
            view.cursor_offset = offset;
            view.selection_anchor = None;
            view.visual_cache_dirty = true;
            Some(ref_line)
        });
        if let Some(line) = inserted_line {
            self.need_reveal_cursor = true;
            self.select_image_line(line);
            self.hovered_image_line = Some(line);
        }
        if self.config.images.open_editor_after_insert {
            let info = inserted_line.and_then(|line| {
                self.state.active_view_and_doc().map(|(view, doc)| {
                    (
                        line,
                        doc.path.clone(),
                        doc.line_text(line),
                        view.window.wrap_cols,
                    )
                })
            });
            if let Some((line, note, text, wrap_cols)) = info {
                if let Some(image) = images::resolve_for_note(&note, &text, wrap_cols, false) {
                    self.open_image_editor(
                        crate::views::image_editor::ImageEditorMode::Crop,
                        line,
                        &image,
                        cx,
                    );
                }
            }
        }
        self.editor_blink
            .pause_and_show(crate::blink_manager::BlinkTarget::Editor, cx);
        cx.notify();
        true
    }

    /// Wraps the references in line breaks so each one owns a block line and
    /// the rest of the current line is not glued to the last reference.
    fn image_insert_text(&self, references: &[String]) -> String {
        let Some((view, doc)) = self.state.active_view_and_doc() else {
            return references.join("\n");
        };
        let (line, col) = view.line_col(doc);
        let line_start = doc.line_start_byte(line);
        let line_end = doc.line_end_byte(line);
        let text = doc.slice_to_string(line_start..line_end);
        let byte_col = text
            .char_indices()
            .nth(col)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len());
        let before = &text[..byte_col];
        let after = &text[byte_col..];
        let mut out = String::new();
        if !before.trim().is_empty() {
            out.push('\n');
        }
        out.push_str(&references.join("\n"));
        // One break ends the reference line, the second creates the empty
        // line that the caret will occupy under the picture.
        out.push_str("\n\n");
        let _ = after;
        out
    }

    /// Detects a clipboard payload made only of paths to image files.
    pub fn clipboard_image_files(text: &str) -> Option<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for line in text.lines() {
            let candidate = line.trim().trim_matches('"');
            if candidate.is_empty() {
                continue;
            }
            let path = Path::new(candidate);
            if !path.is_file() || !images::is_image_path(path) {
                return None;
            }
            paths.push(path.to_path_buf());
        }
        (!paths.is_empty()).then_some(paths)
    }

    /// Copies attachments referenced by pasted markup into this note under
    /// fresh names, so a copy never aliases the source note's file.
    pub fn copy_pasted_references(
        &mut self,
        text: &str,
        meta: Option<&serde_json::Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(note) = self.state.active_document().map(|doc| doc.path.clone()) else {
            return false;
        };
        let folder = self.config.files.folder.clone();
        let root = images::vault_root();
        let cut = meta
            .and_then(|meta| meta.get("cut"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let source_note = meta
            .and_then(|meta| meta.get("note"))
            .and_then(|value| value.as_str())
            .map(std::path::PathBuf::from);
        let source_name = meta
            .and_then(|meta| meta.get("name"))
            .and_then(|value| value.as_str());
        let mut rewritten = Vec::new();
        let mut any = false;
        let mut moved = false;
        for line in text.lines() {
            let Some(reference) = images::parse_image_reference(line.trim()) else {
                return false;
            };
            if !reference.wiki {
                return false;
            }
            let resolve_note = source_note.as_deref().unwrap_or(&note);
            if images::resolve_attachment(root.as_deref(), resolve_note, &folder, &reference.target).is_none() {
                return false;
            }
            any = true;
            let can_move = cut
                && source_note.is_some()
                && source_name == Some(reference.target.as_str());
            if cut {
                images::release_cut(
                    source_note.as_deref().unwrap_or(&note),
                    &reference.target,
                );
            }
            let replacement = if can_move {
                match images::move_attachment(
                    root.as_deref(),
                    source_note.as_deref().unwrap_or(&note),
                    &note,
                    &folder,
                    &reference.target,
                ) {
                    Some(name) => {
                        moved = true;
                        name
                    }
                    None => images::copy_attachment_to(
                        root.as_deref(),
                        source_note.as_deref().unwrap_or(&note),
                        &note,
                        &folder,
                        &reference.target,
                    )
                    .unwrap_or_else(|| reference.target.clone()),
                }
            } else {
                images::copy_attachment_to(
                    root.as_deref(),
                    resolve_note,
                    &note,
                    &folder,
                    &reference.target,
                )
                .unwrap_or_else(|| reference.target.clone())
            };
            rewritten.push(reference.with_target(replacement).markup());
        }
        if !any {
            return false;
        }
        let text = self.image_insert_text(&rewritten);
        self.state.active_replace_selection(&text);
        if moved {
            let meta = serde_json::json!({
                "vibecodiumAttachment": {
                    "name": "",
                    "note": note.to_string_lossy(),
                    "cut": false
                }
            });
            cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_json_metadata(text.clone(), meta));
            crate::images::set_clipboard_attachment_meta(&note, "", false);
        }
        self.schedule_auto_save(cx);
        cx.notify();
        true
    }



    pub fn open_image_popover(
        &mut self,
        kind: ImagePopoverKind,
        image: &ResolvedImage,
        line: usize,
        x: f32,
        y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut width = TextInput::new(cx.focus_handle());
        let mut height = TextInput::new(cx.focus_handle());
        let displayed_w = image.width.round().max(1.0) as u32;
        let displayed_h = image.height.round().max(1.0) as u32;
        width.set_text(image.reference.width.unwrap_or(displayed_w).to_string());
        height.set_text(image.reference.height.unwrap_or(displayed_h).to_string());
        let focus = cx.focus_handle();
        if kind == ImagePopoverKind::Size {
            if let Some(handle) = width.focus_handle.clone() {
                window.focus(&handle);
            }
        } else {
            window.focus(&focus);
        }
        self.editor_context_menu.is_open = false;
        self.select_image_line(line);
        self.image_popover = Some(ImagePopoverState {
            kind,
            line,
            path: image.path.clone(),
            reference: image.reference.clone(),
            natural: image.natural,
            wrap_cols: self.state.active_view().map(|view| view.window.wrap_cols).unwrap_or(80),
            x,
            y,
            focus,
            width,
            height,
            active: SizeField::Width,
            lock: true,
        });
        cx.notify();
    }

    pub fn image_size_drag_move(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        _shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.size_drag.take() else {
            return;
        };
        let dx = f32::from(position.x) - drag.start.0;
        let dy = f32::from(position.y) - drag.start.1;
        let (start_w, start_h) = drag.start_size;
        let mut width = start_w + drag.sign.0 * dx;
        let mut height = start_h + drag.sign.1 * dy;
        if drag.sign.0 == 0.0 {
            width = start_w;
        }
        if drag.sign.1 == 0.0 {
            height = start_h;
        }
        width = width.clamp(16.0, drag.max_width.max(16.0));
        height = height.clamp(16.0, 4000.0);
        if drag.lock && start_w > 0.0 && start_h > 0.0 {
            if drag.sign.0 != 0.0 {
                height = (width * start_h / start_w).clamp(16.0, 4000.0);
            } else {
                width = (height * start_w / start_h).clamp(16.0, drag.max_width.max(16.0));
            }
        }
        drag.current = (width.round(), height.round());
        self.size_drag = Some(drag);
        cx.notify();
    }

    pub fn image_size_drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.size_drag.take() else {
            return;
        };
        let width = drag.current.0.round().max(16.0) as u32;
        let height = drag.current.1.round().max(16.0) as u32;
        let popover_open = self
            .image_popover
            .as_ref()
            .is_some_and(|popover| popover.line == drag.line && popover.kind == ImagePopoverKind::Size);
        if popover_open {
            if let Some(popover) = self.image_popover.as_mut() {
                popover.width.set_text(width.to_string());
                popover.height.set_text(height.to_string());
                popover.lock = drag.lock;
            }
        }
        let reference = self
            .state
            .active_document()
            .map(|doc| doc.line_text(drag.line))
            .and_then(|text| images::parse_image_reference(&text));
        if let Some(reference) = reference {
            let updated = if drag.lock {
                reference.with_size(Some(width), None)
            } else {
                reference.with_size(Some(width), Some(height))
            };
            self.replace_image_reference(drag.line, &updated.markup(), cx);
            if popover_open {
                if let Some(popover) = self.image_popover.as_mut() {
                    popover.reference = updated;
                }
            }
        }
        cx.notify();
    }

    pub fn close_image_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.size_drag = None;
        if self.image_popover.take().is_some() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    /// Rewrites the reference on the popover's line. The caret is kept where it
    /// was: rewriting a line must not activate it (which would hide the image).
    pub fn apply_image_size(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let line = popover.line;
        let name = popover.reference.target.clone();
        let raw_width = popover.width.text().trim().parse::<u32>().ok();
        let raw_height = popover.height.text().trim().parse::<u32>().ok();
        let (width, height) = if popover.lock {
            (raw_width, None)
        } else {
            (raw_width, raw_height)
        };
        let mut reference = popover.reference.clone();
        reference.target = name;
        reference = reference.with_size(width, height);
        self.replace_image_reference(line, &reference.markup(), cx);
        if let Some(popover) = self.image_popover.as_mut() {
            popover.reference = reference;
        }
    }

    /// Applies the "Reset size" action: the reference keeps only the name.
    pub fn reset_image_size(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let line = popover.line;
        let reference = popover.reference.with_size(None, None);
        self.replace_image_reference(line, &reference.markup(), cx);
        if let Some(popover) = self.image_popover.as_mut() {
            let max_width = images::text_width(popover.wrap_cols);
            let displayed_w = (popover.reference.display_size(popover.natural, max_width).0)
                .round()
                .max(1.0) as u32;
            let displayed_h = (popover.reference.display_size(popover.natural, max_width).1)
                .round()
                .max(1.0) as u32;
            popover.width.set_text(displayed_w.to_string());
            popover.height.set_text(displayed_h.to_string());
            popover.reference = reference;
        }
    }

    pub fn set_image_size_percent(&mut self, percent: u32, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let width = ((popover.natural.0 as f32) * percent as f32 / 100.0).round().max(1.0) as u32;
        let height = ((popover.natural.1 as f32) * percent as f32 / 100.0).round().max(1.0) as u32;
        if let Some(popover) = self.image_popover.as_mut() {
            popover.width.set_text(width.to_string());
            popover.height.set_text(height.to_string());
        }
        self.apply_image_size(cx);
    }

    pub fn set_image_radius(&mut self, percent: f32, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let path = popover.path.clone();
        if crate::images::set_corner_radius(&path, Some(percent)).is_ok() {
            cx.notify();
        }
    }

    pub fn reset_image_radius(&mut self, cx: &mut Context<Self>) {
        let Some(popover) = self.image_popover.as_ref() else {
            return;
        };
        let path = popover.path.clone();
        if crate::images::set_corner_radius(&path, None).is_ok() {
            cx.notify();
        }
    }

    pub(crate) fn replace_image_reference(&mut self, line: usize, markup: &str, cx: &mut Context<Self>) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        let start = doc.line_start_byte(line);
        let end = doc.line_end_byte(line);
        if start >= doc.len() {
            return;
        }
        let old_cursor = view.cursor_offset.min(doc.len());
        let old_len = end.saturating_sub(start);
        let new_len = markup.len();
        let new_cursor = if old_cursor > end {
            old_cursor + new_len - old_len
        } else if old_cursor >= start {
            start + new_len
        } else {
            old_cursor
        };
        let (_, note_edit, edit_event) = view.replace_range(doc, start..end, markup.to_string(), new_cursor);
        self.state.dispatch_edit_event(&edit_event, &note_edit);
        self.schedule_auto_save(cx);
        cx.notify();
    }

    pub fn delete_image_at_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.selected_image_line = None;
        self.hovered_image_line = None;
        self.image_popover = None;
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        let lines = doc.line_index.lines();
        let start = doc.line_start_byte(line);
        let end = if line + 1 < lines {
            doc.line_start_byte(line + 1)
        } else {
            doc.line_end_byte(line)
        };
        if start >= end {
            return;
        }
        let old_cursor = view.cursor_offset.min(doc.len());
        // Deleting a whole line is before, inside, or after the caret. Only the
        // "after" case may shift left by the removed length; a caret inside the
        // line must collapse to its start, otherwise the shifted byte offset
        // can land inside a multibyte character and panic in the rope.
        let new_cursor = if old_cursor >= end {
            old_cursor - (end - start)
        } else if old_cursor > start {
            start
        } else {
            old_cursor
        };
        let (_, note_edit, edit_event) = view.replace_range(doc, start..end, String::new(), new_cursor);
        self.state.dispatch_edit_event(&edit_event, &note_edit);
        self.schedule_auto_save(cx);
        cx.notify();
    }

    // ─────────────────────── Popover ───────────────────────

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

    fn render_size_popover_body(
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

    fn render_radius_popover_body(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
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


/// Paints the shared vector geometry with GPUI. `scale` converts original
/// image pixels to the displayed picture; `offset` is the crop origin.
fn paint_annotation_paths(
    window: &mut Window,
    annotations: &[crate::images::Annotation],
    scale: f32,
    offset: (f32, f32),
    origin: gpui::Point<gpui::Pixels>,
) {
    for annotation in annotations {
        let mut color = crate::images::parse_hex_color(&annotation.color);
        let mut stroke_width = annotation.width.max(0.5) * scale;
        if matches!(annotation.kind, crate::images::AnnotationKind::Highlight) {
            color[3] = color[3].min(140);
            stroke_width *= 3.0;
        }
        for shape in crate::images::annotation_geometry(annotation) {
            let mut builder = if shape.filled {
                gpui::PathBuilder::fill()
            } else {
                gpui::PathBuilder::stroke(px(stroke_width))
            };
            for (index, point) in shape.points.iter().enumerate() {
                let x = f32::from(origin.x) + (point.x - offset.0) * scale;
                let y = f32::from(origin.y) + (point.y - offset.1) * scale;
                if index == 0 {
                    builder.move_to(gpui::point(px(x), px(y)));
                } else {
                    builder.line_to(gpui::point(px(x), px(y)));
                }
            }
            if shape.closed {
                builder.close();
            }
            if let Ok(path) = builder.build() {
                window.paint_path(
                    path,
                    gpui::rgba((color[0] as u32) << 24 | (color[1] as u32) << 16 | (color[2] as u32) << 8 | color[3] as u32),
                );
            }
        }
    }
}
