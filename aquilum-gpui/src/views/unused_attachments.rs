//! "Unused attachments" modal: finds files no note references and moves them
//! to the attachment trash on request.

use std::path::PathBuf;

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, SharedString, Window};

use crate::images;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::notification::NotificationType;

pub struct UnusedAttachmentsState {
    pub paths: Vec<PathBuf>,
}

impl AppView {
    pub fn open_unused_attachments(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.state.workspace_path.clone() else {
            return;
        };
        let paths = images::unused_attachments(&root, &self.config.files.folder);
        if paths.is_empty() {
            self.push_notification(
                NotificationType::Info,
                None,
                crate::i18n::t("editor.image.unused_empty", &self.config.ui.language),
                Some(5000),
                cx,
            );
            return;
        }
        self.unused_attachments = Some(UnusedAttachmentsState { paths });
        cx.notify();
    }

    pub fn close_unused_attachments(&mut self, cx: &mut Context<Self>) {
        if self.unused_attachments.take().is_some() {
            cx.notify();
        }
    }

    pub fn trash_unused_attachments(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.state.workspace_path.clone() else {
            return;
        };
        if let Some(state) = self.unused_attachments.take() {
            let folder = self.config.files.folder.clone();
            for path in state.paths {
                let _ = images::trash_pair(&root, &folder, &path);
            }
            images::invalidate_resolution();
            cx.notify();
        }
    }

    pub(crate) fn handle_unused_attachments_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.close_unused_attachments(cx);
            window.focus(&self.focus_handle);
        }
    }

    pub fn render_unused_attachments(&self, theme: &Theme, cx: &Context<Self>) -> gpui::AnyElement {
        let Some(state) = self.unused_attachments.as_ref() else {
            return div().into_any_element();
        };
        let lang = self.config.ui.language.as_str();
        let mut list = div().flex().flex_col().gap(px(2.0)).max_h(px(260.0)).overflow_hidden();
        for path in state.paths.iter().take(40) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            list = list.child(
                div()
                    .h(px(22.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .bg(theme.bg_sidebar)
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(name),
            );
        }
        let panel = div()
            .id("unused-attachments-panel")
            .w(px(480.0))
            .flex()
            .flex_col()
            .gap(px(9.0))
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
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(crate::i18n::t("editor.image.unused_title", lang)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "{}: {}",
                                        crate::i18n::t("editor.image.unused_count", lang),
                                        state.paths.len()
                                    )),
                            )
                            .child(self.image_close_button(
                                "unused-close-x",
                                theme,
                                cx.listener(|this, _, _, cx| {
                                    this.close_unused_attachments(cx)
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(crate::i18n::t("editor.image.unused_remove_hint", lang)),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(6.0))
                    .child(self.unused_button(
                        "unused-close",
                        crate::i18n::t("editor.image.unused_close", lang),
                        false,
                        theme,
                        cx.listener(|this, _, _, cx| this.close_unused_attachments(cx)),
                    ))
                    .child(self.unused_button(
                        "unused-trash",
                        crate::i18n::t("editor.image.unused_move_trash", lang),
                        true,
                        theme,
                        cx.listener(|this, _, _, cx| this.trash_unused_attachments(cx)),
                    )),
            );
        div()
            .id("unused-attachments-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000059))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_unused_attachments(cx);
            }))
            .child(panel)
            .into_any_element()
    }

    fn unused_button(
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
