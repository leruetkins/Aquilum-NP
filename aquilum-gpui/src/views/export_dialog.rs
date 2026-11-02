//! "Export MD" dialog: writes a zip with the note and only its attachments.
//! The dialog only chooses options; the zip work runs in the background.

use std::path::PathBuf;

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, SharedString, Window};

use crate::md_export::{self, CaptionFormat, ExportOptions};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::notification::NotificationType;

pub struct ExportDialogState {
    pub note_path: PathBuf,
    pub options: ExportOptions,
    pub busy: bool,
    pub status: Option<String>,
}

impl AppView {
    pub fn open_export_dialog(&mut self, target: Option<PathBuf>, cx: &mut Context<Self>) {
        let note_path = target.or_else(|| self.state.active_document().map(|doc| doc.path.clone()));
        let Some(note_path) = note_path else {
            return;
        };
        self.export_dialog = Some(ExportDialogState {
            note_path,
            options: ExportOptions {
                caption_template: self.config.images.caption_template.clone(),
                ..ExportOptions::default()
            },
            busy: false,
            status: None,
        });
        cx.notify();
    }

    pub fn close_export_dialog(&mut self, cx: &mut Context<Self>) {
        if self.export_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub fn set_export_caption_format(&mut self, format: CaptionFormat, cx: &mut Context<Self>) {
        if let Some(state) = self.export_dialog.as_mut() {
            state.options.caption_format = format;
            cx.notify();
        }
    }

    pub fn toggle_export_embed_caption(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.export_dialog.as_mut() {
            state.options.embed_caption = !state.options.embed_caption;
            cx.notify();
        }
    }

    pub fn toggle_export_round_corners(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.export_dialog.as_mut() {
            state.options.round_corners = !state.options.round_corners;
            cx.notify();
        }
    }

    pub fn toggle_export_keep_obsidian(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.export_dialog.as_mut() {
            state.options.keep_obsidian = !state.options.keep_obsidian;
            cx.notify();
        }
    }

    pub fn start_export(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.export_dialog.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let note_path = state.note_path.clone();
        let options = state.options.clone();
        let vault_root = self.state.workspace_path.clone();
        let folder = self.config.files.folder.clone();
        let default_name = note_path
            .file_stem()
            .map(|stem| format!("{}.zip", stem.to_string_lossy()))
            .unwrap_or_else(|| "note.zip".to_string());
        let start_dir = note_path.parent().map(|parent| parent.to_path_buf());
        if let Some(state) = self.export_dialog.as_mut() {
            state.busy = true;
            state.status = None;
        }
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(async move {
                let mut dialog = rfd::AsyncFileDialog::new()
                    .add_filter("Zip archive (*.zip)", &["zip"])
                    .set_file_name(&default_name);
                if let Some(dir) = start_dir {
                    dialog = dialog.set_directory(dir);
                }
                let Some(handle) = dialog.save_file().await else {
                    let _ = entity.update(&mut async_cx, |this, cx| {
                        if let Some(state) = this.export_dialog.as_mut() {
                            state.busy = false;
                        }
                        cx.notify();
                    });
                    return;
                };
                let destination = handle.path().to_path_buf();
                let result = background
                    .spawn(async move {
                        md_export::build_zip(
                            &note_path,
                            vault_root.as_deref(),
                            &folder,
                            &options,
                            &destination,
                        )
                    })
                    .await;
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.finish_export(result, cx);
                });
            })
            .detach();
        cx.notify();
    }

    fn finish_export(
        &mut self,
        result: Result<md_export::ExportReport, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(report) => {
                let language = self.config.ui.language.clone();
                let message = format!(
                    "{}: {} · {}: {}",
                    crate::i18n::t("editor.image.export_done", &language),
                    report.included,
                    crate::i18n::t("editor.image.export_missing", &language),
                    report.missing
                );
                self.push_notification(NotificationType::Success, None, message, Some(7000), cx);
                show_in_folder(&report.path);
                self.export_dialog = None;
            }
            Err(error) => {
                if let Some(state) = self.export_dialog.as_mut() {
                    state.busy = false;
                    state.status = Some(error);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn handle_export_dialog_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_export_dialog(cx);
                window.focus(&self.focus_handle);
            }
            "enter" => {
                self.start_export(cx);
            }
            _ => {}
        }
    }

    pub fn render_export_dialog(&self, theme: &Theme, cx: &Context<Self>) -> gpui::AnyElement {
        let Some(state) = self.export_dialog.as_ref() else {
            return div().into_any_element();
        };
        let lang = self.config.ui.language.as_str();
        let t = |key: &str| crate::i18n::t(key, lang);

        let caption_format = |format: CaptionFormat, label: String| {
            let selected = state.options.caption_format == format;
            div()
                .id(gpui::ElementId::Name(SharedString::from(format!(
                    "export-caption-{:?}",
                    format
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
                        this.set_export_caption_format(format, cx);
                    }),
                )
                .child(label)
        };

        let toggle = |id: &'static str, label: String, value: bool, action: fn(&mut AppView, &mut Context<AppView>)| {
            div()
                .id(gpui::ElementId::Name(SharedString::from(id)))
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded_md()
                .bg(if value { theme.bg_selected } else { theme.bg_sidebar })
                .border_1()
                .border_color(if value { theme.accent } else { theme.border })
                .text_xs()
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        action(this, cx);
                    }),
                )
                .child(label)
        };

        let panel = div()
            .id("export-dialog-panel")
            .w(px(470.0))
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
                            .child(t("editor.image.export_title")),
                    )
                    .child(self.image_close_button(
                        "export-close-x",
                        theme,
                        cx.listener(|this, _, _, cx| this.close_export_dialog(cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(110.0)).text_xs().text_color(theme.text_muted).child(t("editor.image.export_caption_format")))
                    .child(caption_format(
                        CaptionFormat::Italic,
                        t("editor.image.export_caption_italic"),
                    ))
                    .child(caption_format(
                        CaptionFormat::Html,
                        t("editor.image.export_caption_html"),
                    ))
                    .child(caption_format(
                        CaptionFormat::Alt,
                        t("editor.image.export_caption_alt"),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(toggle(
                        "export-embed-caption",
                        t("editor.image.export_embed_caption"),
                        state.options.embed_caption,
                        |this, cx| this.toggle_export_embed_caption(cx),
                    ))
                    .child(toggle(
                        "export-round-corners",
                        t("editor.image.export_round_corners"),
                        state.options.round_corners,
                        |this, cx| this.toggle_export_round_corners(cx),
                    ))
                    .child(toggle(
                        "export-keep-obsidian",
                        t("editor.image.export_keep_obsidian"),
                        state.options.keep_obsidian,
                        |this, cx| this.toggle_export_keep_obsidian(cx),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(t("editor.image.export_links_warning")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(if state.status.is_some() {
                        theme.marked_line_red()
                    } else {
                        theme.text_muted
                    })
                    .child(if state.busy {
                        t("editor.image.export_busy")
                    } else {
                        state.status.clone().unwrap_or_default()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(6.0))
                    .child(self.export_button(
                        "export-cancel",
                        t("editor.image.crop_cancel"),
                        false,
                        theme,
                        cx.listener(|this, _, _, cx| this.close_export_dialog(cx)),
                    ))
                    .child(self.export_button(
                        "export-start",
                        t("editor.image.export_button"),
                        true,
                        theme,
                        cx.listener(|this, _, _, cx| this.start_export(cx)),
                    )),
            );

        div()
            .id("export-dialog-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_export_dialog(cx);
            }))
            .child(panel)
            .into_any_element()
    }

    fn export_button(
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

fn show_in_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}
