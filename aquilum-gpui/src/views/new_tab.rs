use gpui::{div, prelude::*, Context, IntoElement, KeyDownEvent, Styled, Window};

use crate::icons;
use crate::state::is_empty_tab_path;
use crate::theme::Theme;
use crate::views::app_view::AppView;

impl AppView {
    /// Opens a new empty tab (no file is created on disk).
    ///
    /// Mirrors `newTab` in aquilum-app: the tab strip gains a "New tab"
    /// entry and the editor area shows the New Tab page with
    /// `Create note / Go to file / Close` actions.
    pub(crate) fn open_empty_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state
            .snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        self.state.open_empty_tab();
        self.title_editing_path = None;
        self.title_edit_text.clear();
        self.title_cursor_pos = 0;
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    /// Activates an already open empty tab.
    pub(crate) fn select_empty_tab(
        &mut self,
        path: std::path::PathBuf,
        center_in_sidebar: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state
            .snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.activate_empty_tab(path);
        self.title_editing_path = None;
        self.title_edit_text.clear();
        self.title_cursor_pos = 0;
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        if center_in_sidebar {
            self.need_scroll_sidebar_to_active.set(true);
        }
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    /// Creates a real note file, replacing the active empty tab with it.
    ///
    /// Used by "Create note" on the New Tab page: unlike the sidebar
    /// actions (which keep their tab), the empty sentinel is dropped so
    /// the tab strip does not accumulate blank entries.
    pub(crate) fn create_note_from_empty_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(empty) = self.state.active_empty_tab.clone() {
            self.state.open_tabs.retain(|p| p != &empty);
            self.state.active_empty_tab = None;
        }
        self.create_new_note(window, cx);
    }

    /// Renders the New Tab page: centered icon actions, like aquilum-app's
    /// `NewTab` (`Create a note / Go to the file / Close`), but with icons.
    pub fn render_new_tab_page(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let action = |id: &'static str, icon_path: &'static str, label: String| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_2p5()
                .px_4()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .child(icons::icon(icon_path, 16.0, theme.accent))
                .child(
                    div()
                        .text_base()
                        .text_color(theme.accent)
                        .child(label),
                )
        };

        div()
            .id("new-tab-page")
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(theme.bg_editor)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // Ctrl+N stacks another empty tab instead of doing nothing here
                // (the editor key handler is not mounted without an open note).
                if event.keystroke.modifiers.control
                    && (event.keystroke.key.eq_ignore_ascii_case("n")
                        || event.keystroke.key == "т"
                        || event.keystroke.key == "Т")
                {
                    this.open_empty_tab(window, cx);
                }
            }))
            .child(
                action(
                    "new-tab-create",
                    icons::ICON_NOTE_CREATE,
                    self.t("editor.create_note"),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.create_note_from_empty_tab(window, cx);
                })),
            )
            .child(
                action("new-tab-goto", icons::ICON_SEARCH, self.t("editor.open_file")).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.open_search_dialog(window, cx);
                    }),
                ),
            )
            .child(
                action("new-tab-close", icons::ICON_CLOSE, self.t("editor.close")).on_click(
                    cx.listener(|this, _, _, cx| {
                        if let Some(empty) = this.state.active_empty_tab.clone() {
                            this.close_tab(&empty, cx);
                        }
                    }),
                ),
            )
    }
}

/// Title shown for a tab: the file stem, the localized "New tab" label for
/// empty-tab sentinels, or the "untitled" placeholder as a last resort.
pub fn tab_display_title(path: &std::path::Path, new_tab: &str, untitled: &str) -> String {
    if is_empty_tab_path(path) {
        return new_tab.to_string();
    }
    match path.file_stem() {
        Some(stem) => stem.to_string_lossy().into_owned(),
        None => untitled.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_shows_new_tab_title() {
        let sentinel = crate::state::empty_tab_path();
        assert_eq!(tab_display_title(&sentinel, "New tab", "Untitled"), "New tab");
    }

    #[test]
    fn real_file_shows_stem() {
        let path = std::path::Path::new("/vault/My note.md");
        assert_eq!(tab_display_title(path, "New tab", "Untitled"), "My note");
    }
}
