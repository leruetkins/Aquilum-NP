//! Vault switcher dialog ("Knowledge bases", Obsidian-style).
//!
//! Opened from the workspace button in the sidebar footer. Lists known
//! vaults with a check mark on the current one; clicking switches.
//! The accent button at the bottom opens the native folder picker.

use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, Context, IntoElement, KeyDownEvent, MouseButton, SharedString, Styled,
    Window,
};

use crate::icons;
use crate::state::AppState;
use crate::theme::Theme;
use crate::views::app_view::AppView;

impl AppView {
    /// Opens the vault switcher, dropping vaults whose folders are gone.
    pub fn open_vault_dialog(&mut self, cx: &mut Context<Self>) {
        self.state.known_vaults.retain(|p| p.is_dir());
        self.vault_dialog_open = true;
        cx.notify();
    }

    /// Closes the vault switcher.
    pub fn close_vault_dialog(&mut self, cx: &mut Context<Self>) {
        if self.vault_dialog_open {
            self.vault_dialog_open = false;
            cx.notify();
        }
    }

    /// Switches to another known vault, keeping the editor consistent:
    /// a note left over from the previous vault is replaced with the new
    /// vault's first file (or cleared when the vault has no notes).
    pub(crate) fn switch_vault(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !path.is_dir() {
            self.state.known_vaults.retain(|p| p.is_dir());
            self.state.save_session_state();
            cx.notify();
            return;
        }
        self.close_vault_dialog(cx);
        if self.state.workspace_path.as_deref() == Some(path.as_path()) {
            return;
        }
        self.unload_all_plugins();
        self.state.open_workspace(path.clone());
        self.load_installed_plugins_for_vault();
        let still_valid = self
            .state
            .active_document()
            .map(|d| d.path.starts_with(&path))
            .unwrap_or(false);
        if !still_valid {
            if let Some(first) = AppState::first_file_in_tree(&self.state.file_tree) {
                self.select_note(first, true, window, cx);
                return;
            }
            self.state.active_empty_tab = None;
        }
        self.need_scroll_sidebar_to_active.set(true);
        cx.notify();
    }

    /// Renders the vault switcher dialog overlay.
    pub fn render_vault_dialog(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_w: f32 = window_size.width.into();
        let window_h: f32 = window_size.height.into();
        let card_w = (480.0f32).min(window_w - 64.0).max(300.0);
        let card_max_h = (window_h - 96.0).max(300.0);
        let current = self.state.workspace_path.clone();

        div()
            .id("vault-dialog-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("vault-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(gpui::rgba(0x00000066))
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_vault_dialog(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_vault_dialog(cx);
                    })),
            )
            .child(
                div()
                    .id("vault-dialog-center")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("vault-dialog-card")
                            .w(px(card_w))
                            .max_h(px(card_max_h))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_xl()
                            .shadow_xl()
                            .cursor(gpui::CursorStyle::Arrow)
                            .track_focus(&self.vault_dialog_focus_handle)
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                cx.stop_propagation();
                                if event.keystroke.key == "escape" {
                                    this.close_vault_dialog(cx);
                                }
                            }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // ---- Header ----
                            .child({
                                window.focus(&self.vault_dialog_focus_handle);
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_4()
                                    .h(px(52.0))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(self.t("sidebar.vaults_title")),
                                    )
                                    .child(
                                        div()
                                            .id("vault-dialog-close")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(28.0))
                                            .h(px(28.0))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(
                                                icons::ICON_CLOSE,
                                                13.0,
                                                theme.text_secondary,
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_vault_dialog(cx);
                                            })),
                                    )
                            })
                            // ---- Vault list ----
                            .child({
                                let mut list = div()
                                    .id("vault-dialog-list")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_scroll()
                                    .p_2()
                                    .gap_1();
                                if self.state.known_vaults.is_empty() {
                                    list = list.child(
                                        div()
                                            .px_3()
                                            .py_4()
                                            .text_sm()
                                            .text_color(theme.text_muted)
                                            .child(self.t("sidebar.no_vaults")),
                                    );
                                } else {
                                    for (ix, vault) in
                                        self.state.known_vaults.iter().enumerate()
                                    {
                                        let is_current = current.as_deref() == Some(vault.as_path());
                                        let vault_clone = vault.clone();
                                        let name = vault
                                            .file_name()
                                            .map(|s| s.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| vault.to_string_lossy().into_owned());
                                        list = list.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "vault-row-{}",
                                                    ix
                                                )))
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2p5()
                                                .px_3()
                                                .py_2()
                                                .rounded_lg()
                                                .cursor_pointer()
                                                .when(is_current, |d| d.bg(theme.bg_hover))
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(
                                                    div()
                                                        .w(px(16.0))
                                                        .flex_shrink_0()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .when(is_current, |d| {
                                                            d.child(icons::icon(
                                                                icons::ICON_CHECK,
                                                                14.0,
                                                                theme.accent,
                                                            ))
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .flex()
                                                        .flex_col()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .truncate()
                                                                .text_color(theme.text_primary)
                                                                .child(name),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .truncate()
                                                                .text_color(theme.text_muted)
                                                                .child(
                                                                    vault.to_string_lossy().into_owned(),
                                                                ),
                                                        ),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.switch_vault(
                                                            vault_clone.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        );
                                    }
                                }
                                list
                            })
                            // ---- Footer ----
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .px_4()
                                    .py_3()
                                    .flex_shrink_0()
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .id("vault-dialog-open-folder")
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .px_4()
                                            .py_2()
                                            .rounded_lg()
                                            .cursor_pointer()
                                            .bg(theme.accent)
                                            .hover(|h| h.opacity(0.9))
                                            .child(icons::icon(
                                                icons::ICON_FOLDER_OPEN,
                                                15.0,
                                                gpui::rgb(0xffffff),
                                            ))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(gpui::rgb(0xffffff))
                                                    .child(self.t("sidebar.open_folder")),
                                            )
                                            .on_click(cx.listener(
                                                |this, _, window, cx| {
                                                    this.close_vault_dialog(cx);
                                                    this.open_folder_dialog_deferred(window, cx);
                                                },
                                            )),
                                    ),
                            ),
                    ),
            )
    }
}
