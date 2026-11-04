use std::path::{Path, PathBuf};
use gpui::{
    div, prelude::*, px, AnyElement, Context, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, Window,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use super::data::*;

const BOOKMARK_ROW_H: f32 = 29.0;

impl AppView {
    /// Opens multiple files either in the active leaf (as tabs) or creates a split leaf to the right.
    pub fn open_files_batch(
        &mut self,
        files: &[PathBuf],
        split_right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if files.is_empty() {
            self.state.status_text = self.t("sidebar.no_files_in_folder");
            cx.notify();
            return;
        }

        self.tab_context_menu.leaf_id = None;
        let first_file = files[0].clone();

        if split_right {
            self.split_tab_right(&first_file, window, cx);
            for f in &files[1..] {
                self.select_note(f.clone(), false, window, cx);
            }
            self.select_note(first_file, true, window, cx);
        } else {
            for f in files {
                self.select_note(f.clone(), false, window, cx);
            }
            self.select_note(first_file, true, window, cx);
        }

        let count_str = files.len().to_string();
        self.state.status_text = self.t_fmt("sidebar.opened_files_count", &[&count_str]);
        cx.notify();
    }
    /// Checks whether any bookmark group is currently expanded.
    pub fn is_any_bookmark_group_expanded(&self) -> bool {
        self.state.bookmarks.items.iter().any(|item| {
            if let BookmarkItem::Group { title, .. } = item {
                self.expanded_bookmark_groups.contains(title)
            } else {
                false
            }
        })
    }

    /// Toggles expand / collapse for all bookmark groups.
    pub fn toggle_expand_all_bookmark_groups(&mut self, cx: &mut Context<Self>) {
        if self.is_any_bookmark_group_expanded() {
            self.expanded_bookmark_groups.clear();
        } else {
            for item in &self.state.bookmarks.items {
                if let BookmarkItem::Group { title, .. } = item {
                    self.expanded_bookmark_groups.insert(title.clone());
                }
            }
        }
        cx.notify();
    }

    /// Creates a new bookmark group and immediately enters inline edit mode (Image 2).
    pub fn create_bookmark_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }

        let default_title = self.t("sidebar.untitled_group");

        let new_group_title = self.state.bookmarks.create_group(&default_title);
        let group_idx = self.state.bookmarks.items.len().saturating_sub(1);
        self.expanded_bookmark_groups.insert(new_group_title.clone());
        self.save_bookmarks();

        self.start_bookmark_rename(
            BookmarkRenameTarget::Group(group_idx),
            &new_group_title,
            window,
            cx,
        );
    }

    /// Starts inline rename mode with pre-selected text matching Image 2.
    pub fn start_bookmark_rename(
        &mut self,
        target: BookmarkRenameTarget,
        initial_title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }
        self.bookmark_rename = Some(BookmarkRenameState::new(
            target,
            initial_title.to_string(),
            self.bookmark_rename_focus_handle.clone(),
        ));
        self.bookmark_rename_blink
            .enable(crate::blink_manager::BlinkTarget::SidebarRename, cx);
        window.focus(&self.bookmark_rename_focus_handle);
        self.arm_inline_rename_blur(window, cx);
        cx.notify();
    }

    /// Commits the rename when focus leaves its input for another control.
    ///
    /// A window deactivation also moves focus away; that must not commit a
    /// half-typed name, so the callback only acts while the window is active.
    pub(crate) fn arm_inline_rename_blur(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bookmark_rename_blur_subscription = Some(cx.on_blur(
            &self.bookmark_rename_focus_handle,
            window,
            |this, window, cx| {
                if !window.is_window_active() {
                    return;
                }
                if this.bookmark_rename.is_some() {
                    this.commit_bookmark_rename(cx);
                }
            },
        ));
    }

    /// Commits the active inline rename (bookmarks or the sidebar explorer tree).
    pub fn commit_bookmark_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.bookmark_rename.take() else {
            return;
        };
        self.bookmark_rename_blink.disable(cx);
        let new_name = rename.input.text.trim().to_string();
        if new_name.is_empty() {
            // Keep the input open so the user can type a name instead of losing the
            // gesture.
            self.bookmark_rename = Some(rename);
            self.bookmark_rename_blink
                .enable(crate::blink_manager::BlinkTarget::SidebarRename, cx);
            cx.notify();
            return;
        }

        match rename.target.clone() {
            BookmarkRenameTarget::Group(group_idx) => {
                if group_idx < self.state.bookmarks.items.len() {
                    let old_title = rename.original_title;
                    if self.expanded_bookmark_groups.remove(&old_title) {
                        self.expanded_bookmark_groups.insert(new_name.clone());
                    }
                    self.state.bookmarks.rename_group(group_idx, new_name);
                    self.save_bookmarks();
                }
            }
            BookmarkRenameTarget::File {
                group_index,
                item_index,
            } => {
                self.state
                    .bookmarks
                    .rename_file_bookmark(group_index, item_index, new_name);
                self.save_bookmarks();
            }
            BookmarkRenameTarget::Folder(item_index) => {
                self.state.bookmarks.rename_folder_bookmark(item_index, new_name);
                self.save_bookmarks();
            }
            BookmarkRenameTarget::TreePath { path, is_folder } => {
                match crate::views::sidebar::folder_actions::rename_folder(&path, &new_name) {
                    Ok(new_path) if new_path == path => {
                        // Unchanged name: the inline field simply closes.
                    }
                    Ok(new_path) => {
                        self.state.handle_file_renamed(&path, &new_path);
                        if is_folder {
                            self.state.expanded_folders.remove(&path);
                            self.state.expanded_folders.insert(new_path.clone());
                            self.selected_sidebar_folder = Some(new_path);
                            self.state.status_text =
                                self.t_fmt("sidebar.folder_renamed", &[&new_name]);
                        } else if self.selected_sidebar_folder.as_deref() == Some(path.as_path()) {
                            self.selected_sidebar_folder = None;
                        }
                        self.state.reload_file_tree();
                        self.state.publish_snapshot();
                    }
                    Err(e) => {
                        // Keep the input open so the name can be corrected.
                        self.bookmark_rename = Some(rename);
                        self.bookmark_rename_blink
                            .enable(crate::blink_manager::BlinkTarget::SidebarRename, cx);
                        self.push_notification(
                            crate::views::notification::NotificationType::Error,
                            None,
                            e,
                            Some(6000),
                            cx,
                        );
                    }
                }
            }
        }
        if self.bookmark_rename.is_none() {
            self.bookmark_rename_blur_subscription = None;
        }
        cx.notify();
    }

    /// Cancels active inline rename without modifying bookmarks.
    pub fn cancel_bookmark_rename(&mut self, cx: &mut Context<Self>) {
        self.bookmark_rename = None;
        self.bookmark_rename_blur_subscription = None;
        self.bookmark_rename_blink.disable(cx);
        cx.notify();
    }

    /// Opens the bookmarks right-click context menu (Image 1).
    pub fn open_bookmark_context_menu(
        &mut self,
        target: BookmarkContextTarget,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if self.bookmark_rename.is_some() {
            self.commit_bookmark_rename(cx);
        }
        self.bookmark_context_menu = Some(BookmarkContextMenuState { target, x, y });
        cx.notify();
    }

    /// Closes the bookmarks right-click context menu.
    pub fn close_bookmark_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.bookmark_context_menu.is_some() {
            self.bookmark_context_menu = None;
            cx.notify();
        }
    }

    /// Opens the sidebar in Bookmarks mode.
    pub fn open_sidebar_bookmarks(&mut self, cx: &mut Context<Self>) {
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Bookmarks;
        self.sidebar_collapsed = false;
        self.state.session.sidebar_mode = crate::views::sidebar::SidebarMode::Bookmarks;
        self.state.save_session_state();
        cx.notify();
    }

    /// Checks if a file path is in the bookmarks.
    pub fn is_file_bookmarked(&self, path: &Path) -> bool {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        self.state.bookmarks.contains_file(&str_val)
    }

    /// Checks if a folder path is in the bookmarks.
    pub fn is_folder_bookmarked(&self, path: &Path) -> bool {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        self.state.bookmarks.contains_folder(&str_val)
    }

    /// Toggles bookmark status for a file.
    pub fn toggle_file_bookmark(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if self.state.bookmarks.contains_file(&str_val) {
            self.state.bookmarks.remove_file(&str_val);
            self.state.status_text = self.t_fmt("sidebar.file_unbookmarked", &[&name]);
        } else {
            self.state.bookmarks.add_file(str_val, None, None);
            self.state.status_text = self.t_fmt("sidebar.file_bookmarked", &[&name]);
        }
        self.save_bookmarks();
        cx.notify();
    }

    /// Adds a file to bookmarks.
    pub fn add_file_to_bookmarks(
        &mut self,
        path: &Path,
        group_title: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.add_file(str_val, None, group_title);
        self.state.status_text = self.t_fmt("sidebar.file_bookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Removes a file from bookmarks.
    pub fn remove_file_from_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.remove_file(&str_val);
        self.state.status_text = self.t_fmt("sidebar.file_unbookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Adds a folder to bookmarks.
    pub fn add_folder_to_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.add_folder(str_val, None);
        self.state.status_text = self.t_fmt("sidebar.folder_bookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Removes a folder from bookmarks.
    pub fn remove_folder_from_bookmarks(&mut self, path: &Path, cx: &mut Context<Self>) {
        let str_val = path_to_bookmark_str(self.state.workspace_path.as_deref(), path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.state.bookmarks.remove_folder(&str_val);
        self.state.status_text = self.t_fmt("sidebar.folder_unbookmarked", &[&name]);
        self.save_bookmarks();
        cx.notify();
    }

    /// Saves bookmarks to `.vault/bookmarks.json` (if workspace is open) and session state.
    pub fn save_bookmarks(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let vault_dir = ws.join(".vault");
            let _ = std::fs::create_dir_all(&vault_dir);
            let path = vault_dir.join("bookmarks.json");
            if let Ok(json) = serde_json::to_string_pretty(&self.state.bookmarks) {
                let _ = std::fs::write(path, json);
            }
        }
        self.state.session.bookmarks = self.state.bookmarks.clone();
        self.state.save_session_state();
    }

    /// Loads bookmarks from `.vault/bookmarks.json` (or `.obsidian/bookmarks.json` as fallback) or session state.
    pub fn load_bookmarks(&mut self) {
        if let Some(ref ws) = self.state.workspace_path {
            let vault_bm = ws.join(".vault").join("bookmarks.json");
            let obs_bm = ws.join(".obsidian").join("bookmarks.json");
            let bm_path = if vault_bm.exists() {
                Some(vault_bm)
            } else if obs_bm.exists() {
                Some(obs_bm)
            } else {
                None
            };
            if let Some(path) = bm_path {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<BookmarksData>(&content) {
                        self.state.bookmarks = data;
                        return;
                    }
                }
            }
        }
        if !self.state.session.bookmarks.is_empty() {
            self.state.bookmarks = self.state.session.bookmarks.clone();
        }
    }

    /// Reveals a folder in the file explorer sidebar.
    pub fn reveal_folder_in_sidebar(&mut self, folder_path: PathBuf, cx: &mut Context<Self>) {
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        self.sidebar_collapsed = false;
        self.selected_sidebar_folder = Some(folder_path.clone());
        self.state.expanded_folders.insert(folder_path.clone());
        self.state.reveal_path_in_tree(&folder_path);
        self.need_scroll_sidebar_to_active.set(true);
        cx.notify();
    }

    /// Executes moving a dragged favorite note into/out of a group or reordering.
    fn execute_bookmark_move(
        &mut self,
        source: BookmarkDragSource,
        target: BookmarkDropTarget,
        cx: &mut Context<Self>,
    ) {
        // 1. Extract item from source
        let item = match source.group_index {
            None => {
                if source.item_index < self.state.bookmarks.items.len() {
                    self.state.bookmarks.items.remove(source.item_index)
                } else {
                    return;
                }
            }
            Some(g_idx) => {
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(g_idx)
                {
                    if source.item_index < items.len() {
                        items.remove(source.item_index)
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
        };

        // 2. Insert item into target
        match target {
            BookmarkDropTarget::IntoGroup(target_g_idx) => {
                let actual_g_idx = if source.group_index.is_none() && source.item_index < target_g_idx {
                    target_g_idx.saturating_sub(1)
                } else {
                    target_g_idx
                };
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(actual_g_idx)
                {
                    items.push(item);
                }
            }
            BookmarkDropTarget::GroupItem(target_g_idx, target_item_idx) => {
                let actual_g_idx = if source.group_index.is_none() && source.item_index < target_g_idx {
                    target_g_idx.saturating_sub(1)
                } else {
                    target_g_idx
                };
                if let Some(BookmarkItem::Group { ref mut items, .. }) =
                    self.state.bookmarks.items.get_mut(actual_g_idx)
                {
                    let actual_item_idx = if source.group_index == Some(actual_g_idx)
                        && source.item_index < target_item_idx
                    {
                        target_item_idx.saturating_sub(1)
                    } else {
                        target_item_idx
                    };
                    items.insert(actual_item_idx.min(items.len()), item);
                }
            }
            BookmarkDropTarget::RootItem(target_idx) => {
                let actual_idx = if source.group_index.is_none() && source.item_index < target_idx {
                    target_idx.saturating_sub(1)
                } else {
                    target_idx
                };
                self.state
                    .bookmarks
                    .items
                    .insert(actual_idx.min(self.state.bookmarks.items.len()), item);
            }
            BookmarkDropTarget::RootEnd => {
                self.state.bookmarks.items.push(item);
            }
        }

        self.save_bookmarks();
        cx.notify();
    }

    /// Floating preview of the dragged favorite note, drawn at the pointer so the
    /// gesture stays visible even when the source row scrolls out of view.
    pub fn render_bookmark_drag_ghost(&self, theme: &Theme) -> Option<AnyElement> {
        let drag = self
            .bookmark_drag
            .as_ref()
            .filter(|drag| drag.is_dragging)?;

        Some(crate::views::sidebar::drag_ghost::sidebar_drag_ghost(
            drag.source.display_name.clone(),
            icons::ICON_FILE_TEXT,
            13.0,
            drag.pointer_x,
            drag.pointer_y,
            theme,
        ))
    }

    /// Renders the inline rename input box shared by bookmarks and the explorer tree.
    pub fn render_bookmark_rename_input(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(ref rename) = self.bookmark_rename else {
            return div().into_any_element();
        };

        let pixel_scale = window.scale_factor();
        // Render with the view-level blink manager: the input's own manager is not
        // the one the handlers drive, so the caret would never blink.
        let mut input = rename.input.clone();
        input.blink = self.bookmark_rename_blink.clone();
        let input_div = input.render("", theme, window, pixel_scale, 12.0, "Segoe UI");

        div()
            .id("bookmark-inline-rename-box")
            .track_focus(&self.bookmark_rename_focus_handle)
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w_0()
            .h(px(24.0))
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(theme.accent)
            .bg(theme.bg_card)
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.bookmark_rename_focus_handle);
                    if let Some(ref mut r) = this.bookmark_rename {
                        r.input.handle_mouse_down(event, window, 12.0, "Segoe UI");
                    }
                    this.bookmark_rename_blink
                        .pause_and_show(crate::blink_manager::BlinkTarget::SidebarRename, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if let Some(ref mut r) = this.bookmark_rename {
                    if r.input.is_mouse_selecting {
                        r.input.handle_mouse_move(event, window, 12.0, "Segoe UI");
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(ref mut r) = this.bookmark_rename {
                        r.input.handle_mouse_up();
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "enter" {
                    this.commit_bookmark_rename(cx);
                    // On an invalid name the field stays open; keep focus in it so
                    // the blur listener does not retry the same failed commit.
                    if this.bookmark_rename.is_none() {
                        window.focus(&this.focus_handle);
                    }
                    return;
                }
                if key == "escape" {
                    this.cancel_bookmark_rename(cx);
                    window.focus(&this.focus_handle);
                    return;
                }
                let changed = if let Some(ref mut r) = this.bookmark_rename {
                    r.input.handle_key_down(event, cx)
                } else {
                    false
                };
                if changed {
                    this.bookmark_rename_blink
                        .pause_and_show(crate::blink_manager::BlinkTarget::SidebarRename, cx);
                }
            }))
            .child(input_div)
            .into_any_element()
    }

    /// Renders the Obsidian-style Bookmarks (Favorites) sidebar tree view.
    pub fn render_sidebar_bookmarks(
        &self,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let query = self.state.search_query.trim().to_lowercase();
        let is_filtering = !query.is_empty();

        let mut scroll_container = div()
            .id("sidebar-bookmarks-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py_1()
            .px_1()
            .gap_0p5()
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();
                this.open_bookmark_context_menu(BookmarkContextTarget::EmptyArea, x, y, cx);
            }))
            // Keeps the drag ghost glued to the pointer over gaps between rows, where
            // no row listener runs.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(ref mut drag) = this.bookmark_drag {
                    drag.pointer_x = event.position.x.into();
                    drag.pointer_y = event.position.y.into();
                    if drag.is_dragging {
                        cx.notify();
                    }
                }
            }))
            // A drag released over empty space (not a row, not the root zone) has no
            // destination: drop the state instead of leaving the gesture armed. Rows run
            // first in the bubble phase and take the state themselves, so this only sees
            // gestures that ended outside every drop target.
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.bookmark_drag.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.bookmark_drag.take().is_some() {
                    cx.notify();
                }
            }));

        if self.state.bookmarks.is_empty() {
            let display_text = self.t("sidebar.no_bookmarks");

            return div()
                .id("sidebar-bookmarks-empty")
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .flex_1()
                .p_4()
                .gap_2()
                .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_bookmark_context_menu(BookmarkContextTarget::EmptyArea, x, y, cx);
                }))
                .child(icons::icon(icons::ICON_BOOKMARK, 24.0, theme.text_muted))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(display_text.to_string()),
                )
                .into_any_element();
        }

        let is_dragging_active = self.bookmark_drag.as_ref().map(|d| d.is_dragging).unwrap_or(false);

        for (item_idx, item) in self.state.bookmarks.items.iter().enumerate() {
            match item {
                BookmarkItem::File { path, title, .. } => {
                    let display_name = title.clone().unwrap_or_else(|| {
                        let filename = Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                    });

                    if is_filtering && !display_name.to_lowercase().contains(&query) {
                        continue;
                    }

                    scroll_container = scroll_container.child(self.render_bookmark_file_row(
                        path,
                        &display_name,
                        title.clone(),
                        None,
                        item_idx,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
                BookmarkItem::Folder { path, title } => {
                    let display_name = title.clone().unwrap_or_else(|| {
                        Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone())
                    });

                    if is_filtering && !display_name.to_lowercase().contains(&query) {
                        continue;
                    }

                    scroll_container = scroll_container.child(self.render_bookmark_folder_row(
                        path,
                        &display_name,
                        title.clone(),
                        item_idx,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
                BookmarkItem::Group { title, items } => {
                    let group_matches = is_filtering && title.to_lowercase().contains(&query);
                    let matching_children: Vec<(usize, &BookmarkItem)> = items
                        .iter()
                        .enumerate()
                        .filter(|(_, child)| match child {
                            BookmarkItem::File { path, title, .. } => {
                                let name = title.clone().unwrap_or_else(|| {
                                    let filename = Path::new(path)
                                        .file_name()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| path.clone());
                                    filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                                });
                                !is_filtering || group_matches || name.to_lowercase().contains(&query)
                            }
                            _ => !is_filtering || group_matches,
                        })
                        .collect();

                    if is_filtering && !group_matches && matching_children.is_empty() {
                        continue;
                    }

                    let is_expanded = (is_filtering && !matching_children.is_empty())
                        || self.expanded_bookmark_groups.contains(title);

                    scroll_container = scroll_container.child(self.render_bookmark_group_row(
                        title,
                        item_idx,
                        is_expanded,
                        &matching_children,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
                _ => {}
            }
        }

        // Empty area at bottom acting as drop target to move notes out of groups to root level
        if is_dragging_active {
            let is_hovered = matches!(
                self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()),
                Some(BookmarkDropTarget::RootEnd)
            );

            scroll_container = scroll_container.child(
                div()
                    .id("bookmark-root-drop-zone")
                    .relative()
                    .h(px(40.0))
                    .w_full()
                    // An explicit square rectangle instead of a top border: a border
                    // picks up rounded caps at the ends of the line.
                    .when(is_hovered, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .right(px(0.0))
                                .h(px(2.0))
                                .bg(theme.accent),
                        )
                    })
                    .on_mouse_move(cx.listener(|this, _, _, cx| {
                        if let Some(ref mut drag) = this.bookmark_drag {
                            if drag.is_dragging {
                                drag.target = Some(BookmarkDropTarget::RootEnd);
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        if let Some(drag) = this.bookmark_drag.take() {
                            if drag.is_dragging {
                                this.execute_bookmark_move(drag.source, BookmarkDropTarget::RootEnd, cx);
                            }
                        }
                    })),
            );
        }

        scroll_container.into_any_element()
    }

    /// Renders a single favorite note row.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_file_row(
        &self,
        path: &str,
        display_name: &str,
        custom_title: Option<String>,
        group_index: Option<usize>,
        item_index: usize,
        active_path: Option<&PathBuf>,
        is_dragging_active: bool,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_menu_open = self.bookmark_context_menu.is_some();
        let abs_path = bookmark_str_to_path(self.state.workspace_path.as_deref(), path);
        let is_selected = active_path.map(|p| p == &abs_path).unwrap_or(false);

        let is_being_dragged = self.bookmark_drag.as_ref().map(|d| {
            d.is_dragging && d.source.group_index == group_index && d.source.item_index == item_index
        }).unwrap_or(false);

        let is_drop_target = match self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()) {
            Some(BookmarkDropTarget::RootItem(idx)) => group_index.is_none() && *idx == item_index,
            Some(BookmarkDropTarget::GroupItem(g_idx, idx)) => {
                group_index == Some(*g_idx) && *idx == item_index
            }
            _ => false,
        };

        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| {
                r.target
                    == BookmarkRenameTarget::File {
                        group_index,
                        item_index,
                    }
            })
            .unwrap_or(false);

        let file_svg = icons::icon(
            icons::ICON_FILE_TEXT,
            13.0,
            if is_selected { theme.accent } else { theme.text_muted },
        );

        let path_owned = path.to_string();
        let name_owned = display_name.to_string();
        let title_owned = custom_title;
        let abs_path_click = abs_path.clone();

        let mut row = div()
            .id(SharedString::from(format!("bm-file-{}-{}", group_index.unwrap_or(999), item_index)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(is_selected, |s| {
                s.bg(theme.bg_selected)
                    .text_color(theme.accent)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .when(!is_selected && !is_menu_open && !is_dragging_active, |s| {
                s.hover(|h| h.bg(theme.bg_hover)).text_color(theme.text_primary)
            })
            // The grabbed row keeps an accent frame while the ghost follows the pointer,
            // the same as the sidebar file tree.
            .when(is_being_dragged, |s| {
                s.border_1()
                    .border_color(theme.accent)
                    .text_color(theme.accent)
            })
            .child(file_svg);

        if is_editing {
            row = row.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            row = row
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(if is_selected { theme.accent } else { theme.text_primary })
                        .child(display_name.to_string()),
                )
                // Left mouse down initiates drag tracking
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        this.close_bookmark_context_menu(cx);
                        return;
                    }
                    this.bookmark_drag = Some(BookmarkDragState {
                        source: BookmarkDragSource {
                            group_index,
                            item_index,
                            path: path_owned.clone(),
                            display_name: name_owned.clone(),
                        },
                        start_y: event.position.y.into(),
                        pointer_x: event.position.x.into(),
                        pointer_y: event.position.y.into(),
                        is_dragging: false,
                        target: None,
                    });
                    cx.notify();
                }))
                // Mouse move updates drag threshold and hover drop target
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(ref mut drag) = this.bookmark_drag {
                        let cur_y: f32 = event.position.y.into();
                        if (cur_y - drag.start_y).abs() > 4.0 {
                            drag.is_dragging = true;
                        }
                        if drag.is_dragging {
                            drag.target = Some(if let Some(g) = group_index {
                                BookmarkDropTarget::GroupItem(g, item_index)
                            } else {
                                BookmarkDropTarget::RootItem(item_index)
                            });
                            cx.notify();
                        }
                    }
                }))
                // Mouse up completes drag or opens note if was plain click
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(drag) = this.bookmark_drag.take() {
                        if drag.is_dragging {
                            if let Some(target) = drag.target {
                                this.execute_bookmark_move(drag.source, target, cx);
                            }
                        } else {
                            this.select_note(abs_path_click.clone(), false, window, cx);
                        }
                    }
                }))
                // Right click opens Obsidian-style context menu for this note
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let path = path.to_string();
                    let title = title_owned;
                    move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_bookmark_context_menu(
                            BookmarkContextTarget::File {
                                path: path.clone(),
                                title: title.clone(),
                                group_index,
                                item_index,
                            },
                            x,
                            y,
                            cx,
                        );
                    }
                }));
        }

        // Only the insertion line marks the destination; the row itself is not
        // highlighted while a note is being reordered. The line is absolute so the
        // target row does not shift by a border width as it changes.
        if is_drop_target && !is_being_dragged {
            let mut line = div()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .top(px(-1.0))
                .h(px(2.0))
                .bg(theme.accent);
            // Root-level insertions are a plain square strip; only items inside a
            // group keep the pill-shaped line.
            if group_index.is_some() {
                line = line.rounded_full();
            }
            row = row.relative().child(line);
        }

        row.into_any_element()
    }

    /// Renders a single favorite folder row.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_folder_row(
        &self,
        path: &str,
        display_name: &str,
        custom_title: Option<String>,
        item_index: usize,
        is_dragging_active: bool,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let abs_path = bookmark_str_to_path(self.state.workspace_path.as_deref(), path);
        let folder_svg = icons::icon(icons::ICON_FOLDER, 14.0, theme.accent);
        let abs_path_click = abs_path.clone();

        let is_menu_open = self.bookmark_context_menu.is_some();
        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| r.target == BookmarkRenameTarget::Folder(item_index))
            .unwrap_or(false);

        let mut row = div()
            .id(SharedString::from(format!("bm-fld-{}", path)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(!is_menu_open && !is_dragging_active, |s| {
                s.hover(|h| h.bg(theme.bg_hover))
            })
            .child(folder_svg);

        if is_editing {
            row = row.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            let path_owned = path.to_string();
            let title_owned = custom_title;
            row = row
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(theme.text_primary)
                        .child(display_name.to_string()),
                )
                // Clicking folder reveals it in the file explorer sidebar
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        this.close_bookmark_context_menu(cx);
                        return;
                    }
                    if this.bookmark_rename.is_some() {
                        this.commit_bookmark_rename(cx);
                    }
                    this.reveal_folder_in_sidebar(abs_path_click.clone(), cx);
                }))
                // Right click opens folder context menu
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_bookmark_context_menu(
                        BookmarkContextTarget::Folder {
                            path: path_owned.clone(),
                            title: title_owned.clone(),
                            item_index,
                        },
                        x,
                        y,
                        cx,
                    );
                }));
        }

        row.into_any_element()
    }

    /// Renders a bookmark group row and its children.
    #[allow(clippy::too_many_arguments)]
    fn render_bookmark_group_row(
        &self,
        title: &str,
        group_idx: usize,
        is_expanded: bool,
        matching_children: &[(usize, &BookmarkItem)],
        active_path: Option<&PathBuf>,
        is_dragging_active: bool,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_menu_open = self.bookmark_context_menu.is_some();
        let is_drop_target_group = matches!(
            self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()),
            Some(BookmarkDropTarget::IntoGroup(idx)) if idx == &group_idx
        );
        let is_drop_target_item = matches!(
            self.bookmark_drag.as_ref().and_then(|d| d.target.as_ref()),
            Some(BookmarkDropTarget::GroupItem(idx, _)) if idx == &group_idx
        );
        // A note moved here from the root or another group highlights the whole group:
        // the header and the files inside it share one frame. Reordering within the
        // group keeps only the insertion line.
        let source_outside_group = self
            .bookmark_drag
            .as_ref()
            .map(|drag| drag.source.group_index != Some(group_idx))
            .unwrap_or(false);
        let frame_group = source_outside_group && (is_drop_target_group || is_drop_target_item);

        let chevron = if is_expanded {
            icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)
        } else {
            icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted)
        };

        let title_owned = title.to_string();
        let is_editing = self
            .bookmark_rename
            .as_ref()
            .map(|r| r.target == BookmarkRenameTarget::Group(group_idx))
            .unwrap_or(false);

        let mut header = div()
            .id(SharedString::from(format!("bm-group-hdr-{}", group_idx)))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(BOOKMARK_ROW_H))
            .flex_shrink_0()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            // No row highlight for the group while reordering: the insertion line at
            // the end of the group is the only marker (same as the ribbon rail).
            .when(!is_menu_open && !is_dragging_active, |s| {
                s.hover(|h| h.bg(theme.bg_hover))
            })
            .child(
                div()
                    .id(SharedString::from(format!("bm-group-chevron-{}", group_idx)))
                    .cursor_pointer()
                    .p_0p5()
                    .child(chevron)
                    .on_click(cx.listener({
                        let title = title_owned.clone();
                        move |this, _, _, cx| {
                            if this.expanded_bookmark_groups.contains(&title) {
                                this.expanded_bookmark_groups.remove(&title);
                            } else {
                                this.expanded_bookmark_groups.insert(title.clone());
                            }
                            cx.notify();
                        }
                    })),
            );

        if is_editing {
            header = header.child(self.render_bookmark_rename_input(theme, window, cx));
        } else {
            header = header
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::NORMAL)
                        .truncate()
                        .text_color(theme.text_primary)
                        .child(title.to_string()),
                )
                .on_click(cx.listener({
                    let title = title_owned.clone();
                    move |this, _, _, cx| {
                        if this.bookmark_context_menu.is_some() {
                            this.close_bookmark_context_menu(cx);
                            return;
                        }
                        if this.bookmark_rename.is_some() {
                            this.commit_bookmark_rename(cx);
                        }
                        if this.expanded_bookmark_groups.contains(&title) {
                            this.expanded_bookmark_groups.remove(&title);
                        } else {
                            this.expanded_bookmark_groups.insert(title.clone());
                        }
                        cx.notify();
                    }
                }))
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(ref mut drag) = this.bookmark_drag {
                        if drag.is_dragging {
                            drag.target = Some(BookmarkDropTarget::IntoGroup(group_idx));
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    if this.bookmark_context_menu.is_some() {
                        return;
                    }
                    if let Some(drag) = this.bookmark_drag.take() {
                        if drag.is_dragging {
                            this.execute_bookmark_move(
                                drag.source,
                                BookmarkDropTarget::IntoGroup(group_idx),
                                cx,
                            );
                        }
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let title = title_owned.clone();
                    move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_bookmark_context_menu(
                            BookmarkContextTarget::Group {
                                group_index: group_idx,
                                title: title.clone(),
                            },
                            x,
                            y,
                            cx,
                        );
                    }
                }));
        }

        let mut group_elem = div()
            .id(SharedString::from(format!("bm-group-{}", group_idx)))
            .flex()
            .flex_col()
            .child(header);

        // If expanded, render children with tree guide line
        if is_expanded {
            let mut children_container = div()
                .id(SharedString::from(format!("bm-group-children-{}", group_idx)))
                .flex()
                .flex_col()
                .pl(px(14.0))
                .border_l_1()
                .border_color(theme.border_subtle)
                .ml(px(8.0))
                .my_0p5();

            for (child_idx, child_item) in matching_children {
                if let BookmarkItem::File { path, title, .. } = child_item {
                    let display_name = title.clone().unwrap_or_else(|| {
                        let filename = Path::new(path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                    });

                    children_container = children_container.child(self.render_bookmark_file_row(
                        path,
                        &display_name,
                        title.clone(),
                        Some(group_idx),
                        *child_idx,
                        active_path,
                        is_dragging_active,
                        theme,
                        window,
                        cx,
                    ));
                }
            }

            group_elem = group_elem.child(children_container);
        }

        if frame_group {
            // Drawn as an overlay instead of a real border: a border would take a pixel
            // from the layout and shift the header's title down and right.
            group_elem = group_elem.relative().child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.accent),
            );
        } else if is_drop_target_group {
            // Appending inside the same group: only the line at the end of the group.
            group_elem = group_elem.relative().child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom_0()
                    .h(px(2.0))
                    .rounded_full()
                    .bg(theme.accent),
            );
        }

        group_elem.into_any_element()
    }

    /// Renders the floating context menu for bookmarks matching Image 1.
    pub fn render_bookmark_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(ref menu_state) = self.bookmark_context_menu else {
            return div().into_any_element();
        };

        let bounds = window.bounds();
        let win_w = f32::from(bounds.size.width);
        let win_h = f32::from(bounds.size.height);

        let menu_w = 260.0;
        let menu_h = 245.0;
        let clamped_x = menu_state.x.clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
        let clamped_y = menu_state.y.clamp(8.0, (win_h - menu_h - 8.0).max(8.0));

        let target = menu_state.target.clone();

        let render_item = |id: &'static str,
                           icon_svg: &'static str,
                           label: String,
                           is_destructive: bool,
                           action: Box<dyn Fn(&mut AppView, &mut Window, &mut Context<AppView>)>| {
            let icon_color = if is_destructive {
                theme.text_secondary
            } else {
                theme.text_secondary
            };
            let text_color = if is_destructive {
                theme.text_primary
            } else {
                theme.text_primary
            };

            div()
                .id(SharedString::from(id))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_move(|_, _, cx| {
                    cx.stop_propagation();
                })
                .child(icons::icon(icon_svg, 13.0, icon_color))
                .child(
                    div()
                        .text_xs()
                        .text_color(text_color)
                        .child(label),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    action(this, window, cx);
                }))
        };

        let render_divider = || {
            div()
                .h(px(1.0))
                .my_0p5()
                .mx_1()
                .bg(theme.border_subtle)
        };

        let target_open_tab = target.clone();
        let target_open_right = target.clone();
        let target_open_window = target.clone();
        let target_rename = target.clone();
        let target_bookmark = target.clone();
        let target_delete = target.clone();

        div()
            .id("bookmark-context-menu-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("bookmark-context-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bookmark_context_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bookmark_context_menu(cx);
                    }))
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            .child(
                div()
                    .id("bookmark-context-panel")
                    .absolute()
                    .left(px(clamped_x))
                    .top(px(clamped_y))
                    .w(px(menu_w))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    // 1. Open in a new tab
                    .child(render_item(
                        "bm-item-open-new-tab",
                        icons::ICON_NOTE_CREATE,
                        self.t("sidebar.open_in_new_tab"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_tab {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.select_note(abs_path, true, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.select_note(p, true, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // 2. Open in the area to the right
                    .child(render_item(
                        "bm-item-open-right",
                        icons::ICON_SPLIT,
                        self.t("sidebar.open_in_right"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_right {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.split_tab_right(&abs_path, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, true, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, true, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.split_tab_right(&p, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // 3. Open in a new window
                    .child(render_item(
                        "bm-item-open-new-window",
                        icons::ICON_APP_WINDOW,
                        self.t("sidebar.open_in_new_window"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_open_window {
                                BookmarkContextTarget::File { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    this.select_note(abs_path, true, window, cx);
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    let abs_path = bookmark_str_to_path(this.state.workspace_path.as_deref(), path);
                                    let files = collect_files_in_folder(&abs_path);
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::Group { group_index, title } => {
                                    let files = if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.get(*group_index) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else if let Some(BookmarkItem::Group { items, .. }) = this.state.bookmarks.items.iter().find(|i| matches!(i, BookmarkItem::Group { title: t, .. } if t == title)) {
                                        collect_files_in_bookmark_group(this.state.workspace_path.as_deref(), items)
                                    } else {
                                        Vec::new()
                                    };
                                    this.open_files_batch(&files, false, window, cx);
                                }
                                BookmarkContextTarget::EmptyArea => {
                                    if let Some((_, doc)) = this.state.active_view_and_doc() {
                                        let p = doc.path.clone();
                                        this.select_note(p, true, window, cx);
                                    }
                                }
                            }
                        }),
                    ))
                    // Divider
                    .child(render_divider())
                    // 4. Rename
                    .child(render_item(
                        "bm-item-rename",
                        icons::ICON_PENCIL,
                        self.t("sidebar.rename"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_rename {
                                BookmarkContextTarget::Group { group_index, title } => {
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::Group(*group_index),
                                        title,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::File {
                                    path,
                                    title,
                                    group_index,
                                    item_index,
                                } => {
                                    let cur_name = title.clone().unwrap_or_else(|| {
                                        let filename = Path::new(path)
                                            .file_name()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.clone());
                                        filename.strip_suffix(".md").unwrap_or(&filename).to_string()
                                    });
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::File {
                                            group_index: *group_index,
                                            item_index: *item_index,
                                        },
                                        &cur_name,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::Folder {
                                    path,
                                    title,
                                    item_index,
                                } => {
                                    let cur_name = title.clone().unwrap_or_else(|| {
                                        Path::new(path)
                                            .file_name()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.clone())
                                    });
                                    this.start_bookmark_rename(
                                        BookmarkRenameTarget::Folder(*item_index),
                                        &cur_name,
                                        window,
                                        cx,
                                    );
                                }
                                BookmarkContextTarget::EmptyArea => {}
                            }
                        }),
                    ))
                    // 5. Bookmark the current tab...
                    .child(render_item(
                        "bm-item-bookmark-current",
                        icons::ICON_BOOKMARK,
                        self.t("sidebar.bookmark_current_tab"),
                        false,
                        Box::new(move |this, _, cx| {
                            this.close_bookmark_context_menu(cx);
                            let target_group = if let BookmarkContextTarget::Group { ref title, .. } = target_bookmark {
                                Some(title.clone())
                            } else {
                                None
                            };
                            if let Some((_, doc)) = this.state.active_view_and_doc() {
                                let p = doc.path.clone();
                                this.add_file_to_bookmarks(&p, target_group.as_deref(), cx);
                            }
                        }),
                    ))
                    // 6. New bookmark group
                    .child(render_item(
                        "bm-item-new-group",
                        icons::ICON_FOLDER_PLUS,
                        self.t("sidebar.new_bookmark_group"),
                        false,
                        Box::new(move |this, window, cx| {
                            this.close_bookmark_context_menu(cx);
                            this.create_bookmark_group(window, cx);
                        }),
                    ))
                    // Divider
                    .child(render_divider())
                    // 7. Delete
                    .child(render_item(
                        "bm-item-delete",
                        icons::ICON_TRASH_2,
                        self.t("sidebar.delete"),
                        true,
                        Box::new(move |this, _, cx| {
                            this.close_bookmark_context_menu(cx);
                            match &target_delete {
                                BookmarkContextTarget::File { path, .. } => {
                                    this.state.bookmarks.remove_file(path);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::Folder { path, .. } => {
                                    this.state.bookmarks.remove_folder(path);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::Group { group_index, .. } => {
                                    this.state.bookmarks.delete_group(*group_index);
                                    this.save_bookmarks();
                                    cx.notify();
                                }
                                BookmarkContextTarget::EmptyArea => {}
                            }
                        }),
                    )),
            )
            .into_any_element()
    }
}
