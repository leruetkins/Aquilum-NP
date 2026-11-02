//! Compression dialog: re-encodes the whole original (or a downscaled copy)
//! with a live size preview. The sidecar coordinates are scaled with the
//! pixels, so crop and annotations keep pointing at the same content.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, SharedString, Window};

use crate::image_render::{self, CompressFormat, CompressOptions};
use crate::images::{self, ImageEdit};
use crate::theme::Theme;
use crate::views::app_view::AppView;

pub struct CompressDialogState {
    pub line: usize,
    pub target: String,
    pub path: PathBuf,
    pub natural: (u32, u32),
    pub edit: ImageEdit,
    pub format: CompressFormat,
    pub quality: u8,
    pub scale: f32,
    pub optimize: bool,
    pub before_size: u64,
    pub after_size: Option<u64>,
    pub after_dims: Option<(u32, u32)>,
    pub preview: Option<Arc<gpui::Image>>,
    pub busy: bool,
    pub error: Option<String>,
    pub seq: u64,
}

impl CompressDialogState {
    pub fn options(&self) -> CompressOptions {
        CompressOptions {
            format: self.format,
            quality: self.quality,
            scale: self.scale,
            optimize: self.optimize,
        }
    }
}

fn format_for_path(path: &std::path::Path) -> CompressFormat {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => CompressFormat::Jpeg,
        Some("webp") => CompressFormat::WebP,
        _ => CompressFormat::Png,
    }
}

fn gpui_format(format: CompressFormat) -> gpui::ImageFormat {
    match format {
        CompressFormat::Png => gpui::ImageFormat::Png,
        CompressFormat::Jpeg => gpui::ImageFormat::Jpeg,
        CompressFormat::WebP => gpui::ImageFormat::Webp,
    }
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0} КБ", bytes as f64 / 1024.0)
    }
}

impl AppView {
    pub fn open_compress_dialog_at_line(
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
        self.open_compress_dialog(line, &image, window, cx);
    }

    pub fn open_compress_dialog(
        &mut self,
        line: usize,
        image: &images::ResolvedImage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_editor.is_some() {
            return;
        }
        self.compress_dialog = Some(CompressDialogState {
            line,
            target: image.reference.target.clone(),
            path: image.path.clone(),
            natural: image.natural,
            edit: image.edit.clone(),
            format: format_for_path(&image.path),
            quality: 85,
            scale: 1.0,
            optimize: false,
            before_size: std::fs::metadata(&image.path).map(|m| m.len()).unwrap_or(0),
            after_size: None,
            after_dims: None,
            preview: None,
            busy: false,
            error: None,
            seq: 0,
        });
        self.select_image_line(line);
        self.refresh_compress_preview(cx);
    }

    pub fn close_compress_dialog(&mut self, cx: &mut Context<Self>) {
        if self.compress_dialog.take().is_some() {
            self.clear_image_selection();
            cx.notify();
        }
    }

    pub fn refresh_compress_preview(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.compress_dialog.as_mut() else {
            return;
        };
        state.seq = state.seq.wrapping_add(1);
        let seq = state.seq;
        state.busy = true;
        state.after_size = None;
        state.error = None;
        let path = state.path.clone();
        let options = state.options();
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                let result = background
                    .spawn(async move { image_render::compress_preview(&path, &options) })
                    .await;
                let _ = entity.update(&mut async_cx, |this, cx| {
                    let Some(state) = this.compress_dialog.as_mut() else {
                        return;
                    };
                    if state.seq != seq {
                        return;
                    }
                    state.busy = false;
                    match result {
                        Ok((bytes, width, height, format)) => {
                            state.after_size = Some(bytes.len() as u64);
                            state.after_dims = Some((width, height));
                            state.preview = Some(Arc::new(gpui::Image::from_bytes(
                                gpui_format(format),
                                bytes,
                            )));
                        }
                        Err(error) => state.error = Some(error),
                    }
                    cx.notify();
                });
            })
            .detach();
        cx.notify();
    }

    pub fn set_compress_format(&mut self, format: CompressFormat, cx: &mut Context<Self>) {
        if let Some(state) = self.compress_dialog.as_mut() {
            state.format = format;
        }
        self.refresh_compress_preview(cx);
    }

    pub fn set_compress_quality(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some(state) = self.compress_dialog.as_mut() {
            state.quality = (state.quality as i32 + delta).clamp(20, 100) as u8;
        }
        self.refresh_compress_preview(cx);
    }

    pub fn set_compress_scale(&mut self, scale: f32, cx: &mut Context<Self>) {
        if let Some(state) = self.compress_dialog.as_mut() {
            state.scale = scale.clamp(0.1, 1.0);
        }
        self.refresh_compress_preview(cx);
    }

    pub fn toggle_compress_optimize(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.compress_dialog.as_mut() {
            state.optimize = !state.optimize;
        }
        self.refresh_compress_preview(cx);
    }

    pub fn apply_compress_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.compress_dialog.as_ref() else {
            return;
        };
        let line = state.line;
        let old_path = state.path.clone();
        let options = state.options();
        let mut edit = state.edit.clone();
        match image_render::apply_compression(&old_path, &mut edit, &options) {
            Ok(new_path) => {
                if new_path != old_path {
                    if let Some(name) = new_path.file_name().map(|name| name.to_string_lossy().into_owned()) {
                        if let Some(doc) = self.state.active_document() {
                            let text = doc.line_text(line);
                            if let Some(reference) = images::parse_image_reference(&text) {
                                let updated = reference.with_target(name);
                                self.replace_image_reference(line, &updated.markup(), cx);
                            }
                        }
                    }
                }
                if let Some((view, _)) = self.state.active_view_and_doc_mut() {
                    view.visual_cache_dirty = true;
                }
                images::invalidate_resolved_images();
                image_render::invalidate_caches();
                self.compress_dialog = None;
                self.clear_image_selection();
                cx.notify();
            }
            Err(error) => {
                if let Some(state) = self.compress_dialog.as_mut() {
                    state.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn handle_compress_dialog_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_compress_dialog(cx);
                window.focus(&self.focus_handle);
            }
            "enter" => {
                self.apply_compress_dialog(cx);
                window.focus(&self.focus_handle);
            }
            _ => {}
        }
    }

    pub fn render_compress_dialog(&self, theme: &Theme, cx: &Context<Self>) -> gpui::AnyElement {
        let Some(state) = self.compress_dialog.as_ref() else {
            return div().into_any_element();
        };
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);

        let format_button = |format: CompressFormat| {
            let selected = state.format == format;
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "compress-format-{}",
                    format.label()
                ))))
                .h(px(26.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(if selected { theme.bg_selected } else { theme.bg_sidebar })
                .border_1()
                .border_color(if selected { theme.accent } else { theme.border })
                .text_xs()
                .text_color(if selected { theme.accent } else { theme.text_secondary })
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.set_compress_format(format, cx);
                    }),
                )
                .child(format.label())
        };

        let preset = |label: &str, scale: f32| {
            let selected = (state.scale - scale).abs() < 0.01;
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "compress-scale-{}",
                    label
                ))))
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded_sm()
                .bg(if selected { theme.bg_selected } else { theme.bg_sidebar })
                .border_1()
                .border_color(if selected { theme.accent } else { theme.border })
                .text_xs()
                .text_color(if selected { theme.accent } else { theme.text_secondary })
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.set_compress_scale(scale, cx);
                    }),
                )
                .child(label.to_string())
        };

        let quality_enabled = state.format == CompressFormat::Jpeg;
        let stronger_enabled = state.format == CompressFormat::Png;

        let preview = div()
            .w_full()
            .h(px(210.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(theme.bg_sidebar)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(match state.preview.as_ref() {
                Some(preview) => gpui::img(Arc::clone(preview))
                    .max_w(px(480.0))
                    .max_h(px(200.0))
                    .object_fit(gpui::ObjectFit::Contain)
                    .into_any_element(),
                None if state.error.is_some() => div()
                    .text_xs()
                    .text_color(theme.marked_line_red())
                    .child(state.error.clone().unwrap_or_default())
                    .into_any_element(),
                None => div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(t("editor.image.compress_busy"))
                    .into_any_element(),
            });

        let panel = div()
            .id("compress-dialog-panel")
            .w(px(520.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(16.0))
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
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
                            .child(t("editor.image.compress_title")),
                    )
                    .child(self.image_close_button(
                        "compress-close-x",
                        theme,
                        cx.listener(|this, _, _, cx| this.close_compress_dialog(cx)),
                    )),
            )
            .child(preview)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .child(
                        div()
                            .text_color(theme.text_secondary)
                            .child(format!(
                                "{}: {}",
                                t("editor.image.compress_before"),
                                human_size(state.before_size)
                            )),
                    )
                    .child(
                        div()
                            .text_color(theme.text_secondary)
                            .child(match state.after_size {
                                Some(size) => format!(
                                    "{}: {} · {}×{}",
                                    t("editor.image.compress_after"),
                                    human_size(size),
                                    state.after_dims.map(|d| d.0).unwrap_or(0),
                                    state.after_dims.map(|d| d.1).unwrap_or(0)
                                ),
                                None => format!("{}: …", t("editor.image.compress_after")),
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(86.0)).text_xs().text_color(theme.text_muted).child(t("editor.image.compress_format")))
                    .child(format_button(CompressFormat::Png))
                    .child(format_button(CompressFormat::Jpeg))
                    .child(format_button(CompressFormat::WebP)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(86.0)).text_xs().text_color(theme.text_muted).child(t("editor.image.compress_quality")))
                    .child(self.compress_step_button("-", state.quality > 20, cx, -5))
                    .child(
                        div()
                            .w(px(34.0))
                            .text_xs()
                            .text_center()
                            .text_color(if quality_enabled { theme.text_primary } else { theme.text_muted })
                            .child(state.quality.to_string()),
                    )
                    .child(self.compress_step_button("+", quality_enabled && state.quality < 100, cx, 5)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(86.0)).text_xs().text_color(theme.text_muted).child(t("editor.image.compress_resolution")))
                    .child(preset("100%", 1.0))
                    .child(preset("75%", 0.75))
                    .child(preset("50%", 0.5))
                    .child(self.compress_step_button("-", state.scale > 0.11, cx, 0))
                    .child(
                        div()
                            .w(px(38.0))
                            .text_xs()
                            .text_center()
                            .text_color(theme.text_primary)
                            .child(format!("{:.0}%", state.scale * 100.0)),
                    )
                    .child(self.compress_step_button("+", state.scale < 1.0, cx, 1)),
            )
            .child(
                div()
                    .id("compress-stronger")
                    .h(px(26.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(if state.optimize { theme.bg_selected } else { theme.bg_sidebar })
                    .border_1()
                    .border_color(if state.optimize { theme.accent } else { theme.border })
                    .text_xs()
                    .text_color(if stronger_enabled { theme.text_secondary } else { theme.text_muted })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_compress_optimize(cx);
                        }),
                    )
                    .child(t("editor.image.compress_stronger")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(t("editor.image.compress_warning")),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(6.0))
                    .child(self.compress_text_button(
                        "compress-cancel",
                        t("editor.image.crop_cancel"),
                        false,
                        theme,
                        cx.listener(|this, _, _, cx| this.close_compress_dialog(cx)),
                    ))
                    .child(self.compress_text_button(
                        "compress-apply",
                        t("editor.image.apply"),
                        true,
                        theme,
                        cx.listener(|this, _, _, cx| this.apply_compress_dialog(cx)),
                    )),
            );

        div()
            .id("compress-dialog-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_compress_dialog(cx);
            }))
            .child(panel)
            .into_any_element()
    }

    fn compress_step_button(
        &self,
        label: &'static str,
        enabled: bool,
        cx: &Context<Self>,
        delta: i32,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = &self.theme;
        div()
            .id(gpui::ElementId::Name(SharedString::from(format!(
                "compress-step-{}-{}",
                label, delta
            ))))
            .w(px(22.0))
            .h(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .bg(theme.bg_sidebar)
            .border_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(if enabled { theme.text_secondary } else { theme.text_muted })
            .when(enabled, |d| {
                d.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.compress_step(delta, cx);
                    }),
                )
            })
            .child(label)
    }

    fn compress_step(&mut self, delta: i32, cx: &mut Context<Self>) {
        match delta {
            -5 | 5 => self.set_compress_quality(delta, cx),
            0 => {
                if let Some(state) = self.compress_dialog.as_mut() {
                    state.scale = (state.scale - 0.05).clamp(0.1, 1.0);
                }
                self.refresh_compress_preview(cx);
            }
            1 => {
                if let Some(state) = self.compress_dialog.as_mut() {
                    state.scale = (state.scale + 0.05).clamp(0.1, 1.0);
                }
                self.refresh_compress_preview(cx);
            }
            _ => {}
        }
    }

    fn compress_text_button(
        &self,
        id: &'static str,
        label: String,
        primary: bool,
        theme: &Theme,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(gpui::ElementId::Name(SharedString::from(id)))
            .h(px(27.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded_md()
            .bg(if primary { theme.accent } else { theme.bg_sidebar })
            .border_1()
            .border_color(if primary { theme.accent } else { theme.border })
            .text_xs()
            .text_color(if primary { theme.text_on_accent } else { theme.text_secondary })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, listener)
            .child(label)
    }
}
