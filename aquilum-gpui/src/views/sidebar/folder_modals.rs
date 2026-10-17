use std::path::PathBuf;
use gpui::{
    div, prelude::*, px, rgba, Context, IntoElement, KeyDownEvent, MouseButton,
    SharedString, Styled, Window,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::sidebar::folder_actions;

/// Modals triggered by folder context menu actions.
#[derive(Clone, Debug, PartialEq)]
pub enum FolderModal {
    None,
    Rename {
        folder_path: PathBuf,
        current_name: String,
        new_name: String,
        cursor_pos: usize,
        error_msg: Option<String>,
    },
    Delete {
        folder_path: PathBuf,
        folder_name: String,
    },
    Move {
        folder_path: PathBuf,
        folder_name: String,
        available_folders: Vec<PathBuf>,
        selected_target: Option<PathBuf>,
    },
}

impl Default for FolderModal {
    fn default() -> Self {
        Self::None
    }
}

impl AppView {
    /// Opens the rename folder modal.
    pub fn open_rename_folder_modal(&mut self, folder_path: PathBuf, cx: &mut Context<Self>) {
        let current_name = folder_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let len = current_name.chars().count();

        self.folder_modal = FolderModal::Rename {
            folder_path,
            current_name: current_name.clone(),
            new_name: current_name,
            cursor_pos: len,
            error_msg: None,
        };
        cx.notify();
    }

    /// Opens the delete folder confirmation modal.
    pub fn open_delete_folder_modal(&mut self, folder_path: PathBuf, cx: &mut Context<Self>) {
        let fallback_folder = self.t("common.folder");
        let folder_name = folder_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(fallback_folder);

        self.folder_modal = FolderModal::Delete {
            folder_path,
            folder_name,
        };
        cx.notify();
    }

    /// Opens the move folder modal.
    pub fn open_move_folder_modal(&mut self, folder_path: PathBuf, cx: &mut Context<Self>) {
        let fallback_folder = self.t("common.folder");
        let folder_name = folder_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(fallback_folder);

        let ws_root = self.state.workspace_path.clone().unwrap_or_default();
        let all_folders = folder_actions::collect_all_workspace_folders(&ws_root);
        // Exclude the folder itself and any nested children to prevent circular move
        let available_folders: Vec<PathBuf> = all_folders
            .into_iter()
            .filter(|p| !p.starts_with(&folder_path))
            .collect();

        self.folder_modal = FolderModal::Move {
            folder_path,
            folder_name,
            available_folders,
            selected_target: None,
        };
        cx.notify();
    }

    /// Closes any currently open folder modal.
    pub fn close_folder_modal(&mut self, cx: &mut Context<Self>) {
        self.folder_modal = FolderModal::None;
        cx.notify();
    }

    /// Commits the rename action on the folder.
    pub fn commit_folder_rename(&mut self, cx: &mut Context<Self>) {
        if let FolderModal::Rename {
            ref folder_path,
            ref new_name,
            ..
        } = self.folder_modal.clone()
        {
            match folder_actions::rename_folder(&folder_path, &new_name) {
                Ok(new_path) => {
                    self.state.handle_file_renamed(&folder_path, &new_path);
                    self.state.expanded_folders.remove(folder_path);
                    self.state.expanded_folders.insert(new_path);
                    self.state.reload_file_tree();
                    self.state.publish_snapshot();
                    self.state.status_text = self.t_fmt("sidebar.folder_renamed", &[new_name]);
                    self.folder_modal = FolderModal::None;
                }
                Err(e) => {
                    if let FolderModal::Rename { ref mut error_msg, .. } = self.folder_modal {
                        *error_msg = Some(e);
                    }
                }
            }
            cx.notify();
        }
    }

    /// Commits the deletion of the folder.
    pub fn commit_folder_delete(&mut self, cx: &mut Context<Self>) {
        if let FolderModal::Delete { ref folder_path, ref folder_name } = self.folder_modal.clone() {
            match folder_actions::delete_folder_recursive(&folder_path) {
                Ok(_) => {
                    let config = self.state.settings.get_config();
                    if config.files.close_deleted_tabs {
                        self.state.handle_file_deleted(&folder_path, config.files.keep_modified_on_delete);
                    }
                    self.state.expanded_folders.remove(folder_path);
                    self.state.reload_file_tree();
                    self.state.publish_snapshot();
                    self.state.status_text = self.t_fmt("sidebar.folder_deleted", &[folder_name]);
                    self.folder_modal = FolderModal::None;
                }
                Err(e) => {
                    self.state.status_text = self.t_fmt("sidebar.error_deleting", &[&e]);
                    self.folder_modal = FolderModal::None;
                }
            }
            cx.notify();
        }
    }

    /// Commits moving the folder to selected destination.
    pub fn commit_folder_move(&mut self, dest_parent: PathBuf, cx: &mut Context<Self>) {
        if let FolderModal::Move { ref folder_path, .. } = self.folder_modal.clone() {
            match folder_actions::move_folder(&folder_path, &dest_parent) {
                Ok(new_path) => {
                    self.state.handle_file_renamed(&folder_path, &new_path);
                    self.state.expanded_folders.remove(folder_path);
                    self.state.expanded_folders.insert(dest_parent);
                    self.state.expanded_folders.insert(new_path);
                    self.state.reload_file_tree();
                    self.state.publish_snapshot();
                    self.state.status_text = self.t("sidebar.folder_moved");
                    self.folder_modal = FolderModal::None;
                }
                Err(e) => {
                    self.state.status_text = self.t_fmt("sidebar.error_moving", &[&e]);
                    self.folder_modal = FolderModal::None;
                }
            }
            cx.notify();
        }
    }

    /// Handles keyboard events inside the rename modal.
    pub fn handle_folder_rename_keydown(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if let FolderModal::Rename {
            ref mut new_name,
            ref mut cursor_pos,
            ..
        } = self.folder_modal
        {
            let key = &event.keystroke.key;
            if key == "escape" {
                self.folder_modal = FolderModal::None;
                cx.notify();
                return;
            }
            if key == "enter" {
                self.commit_folder_rename(cx);
                return;
            }
            if key == "backspace" {
                if *cursor_pos > 0 {
                    let mut chars: Vec<char> = new_name.chars().collect();
                    chars.remove(*cursor_pos - 1);
                    *new_name = chars.into_iter().collect();
                    *cursor_pos -= 1;
                    cx.notify();
                }
                return;
            }
            if key == "delete" {
                let char_count = new_name.chars().count();
                if *cursor_pos < char_count {
                    let mut chars: Vec<char> = new_name.chars().collect();
                    chars.remove(*cursor_pos);
                    *new_name = chars.into_iter().collect();
                    cx.notify();
                }
                return;
            }
            if key == "left" {
                if *cursor_pos > 0 {
                    *cursor_pos -= 1;
                    cx.notify();
                }
                return;
            }
            if key == "right" {
                let char_count = new_name.chars().count();
                if *cursor_pos < char_count {
                    *cursor_pos += 1;
                    cx.notify();
                }
                return;
            }

            // Text typing
            let ch = if let Some(c) = &event.keystroke.key_char {
                c.as_str()
            } else if key.chars().count() == 1 {
                key.as_str()
            } else {
                ""
            };

            if !ch.is_empty() {
                for c in ch.chars() {
                    let mut chars: Vec<char> = new_name.chars().collect();
                    chars.insert(*cursor_pos, c);
                    *cursor_pos += 1;
                    *new_name = chars.into_iter().collect();
                }
                cx.notify();
            }
        }
    }

    /// Renders active folder modals.
    pub fn render_folder_modals(&self, theme: &Theme, window: &mut Window, cx: &Context<Self>) -> impl IntoElement {
        let modal = self.folder_modal.clone();

        div()
            .id("folder-modal-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            // Backdrop
            .child(
                div()
                    .id("folder-modal-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x000000aa))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_folder_modal(cx);
                    })),
            )
            // Modal dialog container
            .child(
                div()
                    .id("folder-modal-center")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match modal {
                        FolderModal::None => div().into_any_element(),
                        FolderModal::Rename {
                            new_name,
                            error_msg,
                            ..
                        } => {
                            window.focus(&self.folder_modal_focus_handle);

                            div()
                                .id("folder-rename-dialog")
                                .w(px(380.0))
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border)
                                .rounded_lg()
                                .shadow_xl()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .track_focus(&self.folder_modal_focus_handle)
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                    this.handle_folder_rename_keydown(event, cx);
                                }))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                // Title
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(self.t("sidebar.rename_folder")),
                                )
                                // Input Box
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .px_3()
                                        .py_1p5()
                                        .bg(theme.bg_sidebar)
                                        .border_1()
                                        .border_color(theme.accent)
                                        .rounded_md()
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.text_primary)
                                                .child(new_name),
                                        )
                                        .child(
                                            div()
                                                .w(px(1.5))
                                                .h(px(16.0))
                                                .bg(theme.accent)
                                                .ml_0p5(),
                                        ),
                                )
                                // Optional Error
                                .when_some(error_msg, |d, err| {
                                    d.child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::rgb(0xe05252))
                                            .child(err),
                                    )
                                })
                                // Actions
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .gap_2()
                                        .mt_2()
                                        .child(
                                            div()
                                                .id("folder-rename-btn-cancel")
                                                .px_3()
                                                .py_1()
                                                .rounded_md()
                                                .text_xs()
                                                .cursor_pointer()
                                                .text_color(theme.text_secondary)
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(self.t("common.cancel"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_folder_modal(cx);
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("folder-rename-btn-submit")
                                                .px_3()
                                                .py_1()
                                                .rounded_md()
                                                .text_xs()
                                                .cursor_pointer()
                                                .bg(theme.accent)
                                                .text_color(gpui::rgb(0xffffff))
                                                .hover(|h| h.opacity(0.9))
                                                .child(self.t("sidebar.rename"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.commit_folder_rename(cx);
                                                })),
                                        ),
                                )
                                .into_any_element()
                        }
                        FolderModal::Delete { folder_name, .. } => {
                            window.focus(&self.folder_modal_focus_handle);

                            div()
                                .id("folder-delete-dialog")
                                .w(px(380.0))
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border)
                                .rounded_lg()
                                .shadow_xl()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .track_focus(&self.folder_modal_focus_handle)
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "escape" {
                                        this.close_folder_modal(cx);
                                    } else if event.keystroke.key == "enter" {
                                        this.commit_folder_delete(cx);
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                // Title
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(gpui::rgb(0xe05252))
                                        .child(self.t("sidebar.delete_folder")),
                                )
                                // Description
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_primary)
                                        .child(self.t_fmt("sidebar.delete_folder_confirm", &[&folder_name])),
                                )
                                // Actions
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .gap_2()
                                        .mt_2()
                                        .child(
                                            div()
                                                .id("folder-delete-btn-cancel")
                                                .px_3()
                                                .py_1()
                                                .rounded_md()
                                                .text_xs()
                                                .cursor_pointer()
                                                .text_color(theme.text_secondary)
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(self.t("common.cancel"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_folder_modal(cx);
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("folder-delete-btn-submit")
                                                .px_3()
                                                .py_1()
                                                .rounded_md()
                                                .text_xs()
                                                .cursor_pointer()
                                                .bg(gpui::rgb(0xe05252))
                                                .text_color(gpui::rgb(0xffffff))
                                                .hover(|h| h.opacity(0.9))
                                                .child(self.t("sidebar.delete"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.commit_folder_delete(cx);
                                                })),
                                        ),
                                )
                                .into_any_element()
                        }
                        FolderModal::Move {
                            folder_name,
                            available_folders,
                            ..
                        } => {
                            window.focus(&self.folder_modal_focus_handle);
                            let ws_root = self.state.workspace_path.clone().unwrap_or_default();

                            div()
                                .id("folder-move-dialog")
                                .w(px(420.0))
                                .max_h(px(460.0))
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border)
                                .rounded_lg()
                                .shadow_xl()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .track_focus(&self.folder_modal_focus_handle)
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "escape" {
                                        this.close_folder_modal(cx);
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                // Title
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(self.t_fmt("sidebar.move_folder_to", &[&folder_name])),
                                )
                                // Folders scroll list
                                .child(
                                    div()
                                        .id("folder-move-list-scroll")
                                        .flex()
                                        .flex_col()
                                        .max_h(px(280.0))
                                        .overflow_scroll()
                                        .gap_1()
                                        .p_1()
                                        .bg(theme.bg_sidebar)
                                        .border_1()
                                        .border_color(theme.border)
                                        .rounded_md()
                                        .children(available_folders.into_iter().enumerate().map(|(idx, folder)| {
                                            let is_root = folder == ws_root;
                                            let label = if is_root {
                                                self.t("common.root")
                                            } else {
                                                folder_actions::get_relative_path_string(&ws_root, &folder)
                                            };
                                            let target_clone = folder.clone();

                                            div()
                                                .id(SharedString::from(format!("move-dest-{}", idx)))
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .px_2()
                                                .py_1p5()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(icons::ICON_FOLDER, 13.0, theme.accent))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme.text_primary)
                                                        .truncate()
                                                        .child(label),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.commit_folder_move(target_clone.clone(), cx);
                                                }))
                                        })),
                                )
                                // Cancel Button
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .mt_1()
                                        .child(
                                            div()
                                                .id("folder-move-btn-cancel")
                                                .px_3()
                                                .py_1()
                                                .rounded_md()
                                                .text_xs()
                                                .cursor_pointer()
                                                .text_color(theme.text_secondary)
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(self.t("common.cancel"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_folder_modal(cx);
                                                })),
                                        ),
                                )
                                .into_any_element()
                        }
                    }),
            )
    }
}
