use crate::blink_manager::{BlinkManager, BlinkTarget};
use crate::icons;
use crate::state::{AppState, FileNode};
use crate::theme::Theme;
use crate::views::scrollbar::ScrollbarDrag;
use gpui::{
    div, point, prelude::*, px, rgb, rgba, ClipboardItem, Context, Div, Entity, FocusHandle,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Render, ScrollHandle, ScrollStrategy,
    SharedString, Subscription, Window, WindowControlArea,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub search_focus_handle: FocusHandle,
    pub editor_scroll_handle: ScrollHandle,
    pub sidebar_scroll_handle: ScrollHandle,
    pub settings_scroll_handle: ScrollHandle,
    pub need_scroll_sidebar_to_active: Cell<bool>,
    pub initial_sidebar_scroll_done: Cell<bool>,
    pub edit_mode: bool,
    pub editor_blink: BlinkManager,
    pub editor_is_focused: bool,
    /// Last observed OS window activation state. gpui tracks focus per window and never
    /// clears it when the window is deactivated, so this is the only way to notice that
    /// the app went to the background and stop blinking there.
    pub window_was_active: bool,
    pub is_mouse_selecting: bool,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub is_hovering_resizer: bool,
    pub right_sidebar_width: f32,
    pub right_sidebar_collapsed: bool,
    pub is_resizing_right_sidebar: bool,
    pub is_hovering_right_resizer: bool,
    /// Obsidian-style tabs popup; `Some` while it is shown.
    pub tabs_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the popup's event subscription alive; dropping it unsubscribes.
    pub tabs_popup_subscription: Option<Subscription>,
    /// Locale picker popup (same smart list, anchored to the language button).
    pub locale_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the locale picker's event subscription alive.
    pub locale_popup_subscription: Option<Subscription>,
    /// Editor font picker popup (same smart list, anchored to the font button).
    pub font_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the font picker's event subscription alive.
    pub font_popup_subscription: Option<Subscription>,
    /// Command Palette modal dialog (Ctrl+P, Obsidian-style).
    pub command_palette: crate::views::command_palette::CommandPaletteState,
    pub command_palette_focus_handle: FocusHandle,
    pub command_palette_blink: BlinkManager,
    pub command_palette_scroll_handle: ScrollHandle,
    /// Active declarative UI trees registered by plugins, keyed by (plugin_id, view_id).
    pub plugin_views: HashMap<(String, u64), crate::plugins::ui::UiNode>,
    /// Active plugin runtime handles keeping QuickJS worker threads and communication channels alive.
    pub plugin_runtimes: Vec<crate::plugins::runtime::PluginRuntimeHandle>,
    /// Custom declarative UI trees rendered for markdown code blocks, keyed by block_id.
    pub code_block_views: HashMap<usize, (String, crate::plugins::ui::UiNode)>,
    /// Active API sender channel connected to the GPUI plugin bridge background listener.
    pub plugin_api_tx: Option<futures::channel::mpsc::UnboundedSender<crate::plugins::api::ApiCall>>,
    pub word_wrap: bool,
    pub last_wrap_cols: usize,
    pub is_settings_open: bool,
    pub settings_tab: crate::views::settings_view::SettingsTab,
    pub is_dragging_scrollbar: bool,
    pub scrollbar_drag_start_y: f32,
    pub scrollbar_drag_start_scroll_y: f32,
    /// Drag state of the sidebar tree's overlay scrollbar.
    pub sidebar_scrollbar_drag: ScrollbarDrag,
    pub last_editor_h: Cell<f32>,
    pub config: crate::settings::AppConfig,
    pub last_saved_geometry: Option<(f32, f32, Option<f32>, Option<f32>, bool)>,
    pub initial_scroll_done: bool,
    pub initial_scroll_top: f32,
    pub is_hovering_close: bool,
    pub title_focus_handle: FocusHandle,
    pub title_edit_text: String,
    pub title_editing_path: Option<PathBuf>,
    pub title_cursor_pos: usize,
    pub was_title_focused: bool,
    pub title_selection_anchor: Option<usize>,
    pub title_is_mouse_selecting: bool,
    pub title_is_focused: bool,
    pub title_blink: BlinkManager,
    pub auto_save_epoch: usize,
    pub active_color_picker: Option<&'static str>,
    pub custom_hex_input: String,
    pub custom_hex_focus_handle: FocusHandle,
    pub hex_is_focused: bool,
    pub hex_blink: BlinkManager,
    pub hex_cursor_pos: usize,
    pub hex_selection_anchor: Option<usize>,
    pub hex_is_mouse_selecting: bool,
    pub hex_mouse_drag_start_x: Option<f32>,
    pub hex_box_origin_x: std::rc::Rc<Cell<f32>>,
    pub quick_filter: crate::views::sidebar::QuickFilterState,
    pub folder_context_menu: crate::views::sidebar::FolderContextMenuState,
    pub file_context_menu: crate::views::sidebar::FileContextMenuState,
    pub tab_context_menu: crate::views::tab_context_menu::TabContextMenuState,
    pub ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState,
    pub ribbon_dragged_id: Option<String>,
    pub ribbon_drag_start_y: Option<f32>,
    pub ribbon_is_dragging: bool,
    /// Sidebar content mode (file tree vs Obsidian-style search panel).
    pub sidebar_mode: crate::views::sidebar::SidebarMode,
    pub sidebar_search: crate::views::sidebar::SidebarSearchState,
    pub search_panel_focus_handle: FocusHandle,
    pub panel_scroll_handle: ScrollHandle,
    /// Drag state of the search panel results overlay scrollbar.
    pub panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    /// Go to file dialog ("Search the knowledge base").
    pub search_dialog: crate::views::goto_search::SearchDialogState,
    pub goto_focus_handle: FocusHandle,
    pub goto_blink: BlinkManager,
    pub goto_scroll_handle: ScrollHandle,
    /// Drag state of the search results overlay scrollbar.
    pub goto_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub folder_modal: crate::views::sidebar::FolderModal,
    pub folder_modal_focus_handle: FocusHandle,
    /// Vault switcher dialog ("Knowledge bases").
    pub vault_dialog_open: bool,
    pub vault_dialog_focus_handle: FocusHandle,
    /// Code block whose copy button was just pressed; shows the check mark until the
    /// reset timer fires.
    pub copied_code_block: Option<usize>,
    /// Community plugins catalog dialog.
    pub plugin_catalog: crate::views::plugin_catalog::PluginCatalogState,
    pub catalog_focus_handle: FocusHandle,
    pub catalog_scroll_handle: ScrollHandle,
    pub catalog_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub catalog_details_scroll_handle: ScrollHandle,
    pub catalog_details_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
}

impl AppView {
    /// Creates and initializes a new AppView instance with loaded configuration.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::new();
        let config = state.settings.get_config();
        let theme = Theme::from_config(&config.ui.theme, None);
        let ws = crate::state::WindowState::load(&crate::state::app_data_dir());
        let last_saved_geometry = Some((ws.width, ws.height, ws.x, ws.y, ws.maximized));

        let mut edit_mode = true;
        let mut word_wrap = true;
        let mut initial_scroll_top = 0.0;
        if let Some(ref note) = state.active_note {
            if let Some(settings) = state.note_states.get(&note.path) {
                edit_mode = settings.edit_mode;
                word_wrap = settings.word_wrap;
                initial_scroll_top = settings.scroll_top;
            }
        }

        let title_editing_path = state.active_note.as_ref().map(|n| n.path.clone());
        let title_edit_text = state.active_note.as_ref().map(|n| n.title.clone()).unwrap_or_default();
        let title_cursor_pos = title_edit_text.chars().count();

        // Restore the sidebar geometry persisted in the session. Old session
        // files predate these keys (width 0.0), so anything outside the
        // resize limits falls back to the default.
        let saved_width = state.session.sidebar_width;
        let sidebar_width = if (160.0..=600.0).contains(&saved_width) {
            saved_width
        } else {
            260.0
        };
        let sidebar_mode = state.session.sidebar_mode;

        let mut this = Self {
            state,
            theme,
            focus_handle: cx.focus_handle(),
            search_focus_handle: cx.focus_handle(),
            editor_scroll_handle: ScrollHandle::new(),
            sidebar_scroll_handle: ScrollHandle::new(),
            settings_scroll_handle: ScrollHandle::new(),
            need_scroll_sidebar_to_active: Cell::new(false),
            initial_sidebar_scroll_done: Cell::new(false),
            edit_mode,
            editor_blink: BlinkManager::new(),
            editor_is_focused: true,
            window_was_active: true,
            is_mouse_selecting: false,
            sidebar_width,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            is_hovering_resizer: false,
            right_sidebar_width: 260.0,
            right_sidebar_collapsed: true,
            is_resizing_right_sidebar: false,
            is_hovering_right_resizer: false,
            tabs_popup: None,
            tabs_popup_subscription: None,
            locale_popup: None,
            locale_popup_subscription: None,
            font_popup: None,
            font_popup_subscription: None,
            command_palette: crate::views::command_palette::CommandPaletteState::default(),
            command_palette_focus_handle: cx.focus_handle(),
            command_palette_blink: BlinkManager::new(),
            command_palette_scroll_handle: ScrollHandle::new(),
            word_wrap,
            last_wrap_cols: 80,
            is_settings_open: false,
            settings_tab: crate::views::settings_view::SettingsTab::Ui,
            is_dragging_scrollbar: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_scroll_y: 0.0,
            sidebar_scrollbar_drag: ScrollbarDrag::default(),
            last_editor_h: Cell::new(600.0),
            config,
            last_saved_geometry,
            initial_scroll_done: false,
            initial_scroll_top,
            is_hovering_close: false,
            title_focus_handle: cx.focus_handle(),
            title_edit_text,
            title_editing_path,
            title_cursor_pos,
            was_title_focused: false,
            title_selection_anchor: None,
            title_is_mouse_selecting: false,
            title_is_focused: false,
            title_blink: BlinkManager::new(),
            auto_save_epoch: 0,
            active_color_picker: None,
            custom_hex_input: String::new(),
            custom_hex_focus_handle: cx.focus_handle(),
            hex_is_focused: false,
            hex_blink: BlinkManager::new(),
            hex_cursor_pos: 0,
            hex_selection_anchor: None,
            hex_is_mouse_selecting: false,
            hex_mouse_drag_start_x: None,
            hex_box_origin_x: std::rc::Rc::new(Cell::new(0.0)),
            quick_filter: crate::views::sidebar::QuickFilterState::new(),
            folder_context_menu: crate::views::sidebar::FolderContextMenuState::default(),
            file_context_menu: crate::views::sidebar::FileContextMenuState::default(),
            tab_context_menu: crate::views::tab_context_menu::TabContextMenuState::default(),
            ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState::default(),
            ribbon_dragged_id: None,
            ribbon_drag_start_y: None,
            ribbon_is_dragging: false,
            sidebar_mode,
            sidebar_search: crate::views::sidebar::SidebarSearchState::default(),
            search_panel_focus_handle: cx.focus_handle(),
            panel_scroll_handle: ScrollHandle::new(),
            panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            search_dialog: crate::views::goto_search::SearchDialogState::default(),
            goto_focus_handle: cx.focus_handle(),
            goto_blink: BlinkManager::new(),
            goto_scroll_handle: ScrollHandle::new(),
            goto_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            folder_modal: crate::views::sidebar::FolderModal::default(),
            folder_modal_focus_handle: cx.focus_handle(),
            vault_dialog_open: false,
            vault_dialog_focus_handle: cx.focus_handle(),
            copied_code_block: None,
            plugin_catalog: crate::views::plugin_catalog::PluginCatalogState::default(),
            catalog_focus_handle: cx.focus_handle(),
            catalog_scroll_handle: ScrollHandle::new(),
            catalog_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            catalog_details_scroll_handle: ScrollHandle::new(),
            catalog_details_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            plugin_views: HashMap::new(),
            plugin_runtimes: Vec::new(),
            code_block_views: HashMap::new(),
            plugin_api_tx: None,
        };
        this.editor_blink.enable(BlinkTarget::Editor, cx);
        crate::commands_builtin::register_builtin_commands(&this.state.commands);
        this
    }

    /// Discovers and spawns all installed plugins located in `<vault>/.vault/plugins/`.
    pub fn load_installed_plugins_for_vault(&mut self) {
        let Some(api_tx) = self.plugin_api_tx.clone() else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            return;
        };
        // If safe mode is enabled, skip loading all community plugins.
        if self.config.community_plugins.safe_mode {
            return;
        }
        let plugins_dir = vault.root.join(".vault").join("plugins");
        let enabled = vault.load_community_plugins();
        let handles = crate::plugins::loader::load_installed_plugins(
            &plugins_dir,
            Some(&enabled),
            api_tx,
            self.state.snapshot.clone(),
            &self.state.plugin_host,
        );
        self.plugin_runtimes = handles;
    }

    /// Gracefully unloads all active plugins by sending HostEvent::Unload and clearing registrations.
    pub fn unload_all_plugins(&mut self) {
        for handle in &self.plugin_runtimes {
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
            self.state.plugin_host.unregister_plugin(&handle.manifest.id);
            self.state.commands.unregister_all_of(&handle.manifest.id);
            self.state.ribbon.unregister_all_of(&handle.manifest.id);
            self.state.status_bar.unregister_all_of(&handle.manifest.id);
        }
        self.state.code_block_processors.clear();
        self.code_block_views.clear();
        self.plugin_runtimes.clear();
        self.plugin_views.clear();
    }

    pub fn editor_font_family(&self) -> gpui::SharedString {
        let f = self.config.editor.font.font_family.trim();
        if f.is_empty() {
            "Roboto Mono".into()
        } else {
            f.to_string().into()
        }
    }

    pub fn hex_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.hex_selection_anchor {
            let pos = self.hex_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.pause_and_show(BlinkTarget::Hex, cx);
    }

    pub fn enable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.enable(BlinkTarget::Hex, cx);
    }

    pub fn disable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.disable(cx);
    }

    pub fn title_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.title_selection_anchor {
            let pos = self.title_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.pause_and_show(BlinkTarget::Title, cx);
    }

    pub fn enable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.enable(BlinkTarget::Title, cx);
    }

    pub fn disable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.disable(cx);
    }

    pub fn reset_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.pause_and_show(BlinkTarget::Editor, cx);
    }

    pub fn enable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.enable(BlinkTarget::Editor, cx);
    }

    pub fn disable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.disable(cx);
    }

    /// Freezes every focused caret visible and kills its blink timer. Used when the OS
    /// window loses activation: the caret stays on screen, but the view stops re-rendering.
    fn suspend_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.suspend_visible(cx);
        }
        if self.title_is_focused {
            self.title_blink.suspend_visible(cx);
        }
        if self.hex_is_focused {
            self.hex_blink.suspend_visible(cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.blink.suspend_visible(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.suspend_visible(cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.suspend_visible(cx);
        }
    }

    /// Re-arms blinking for whichever caret holds in-app focus. Used when the OS window
    /// becomes active again.
    fn resume_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.enable(BlinkTarget::Editor, cx);
        }
        if self.title_is_focused {
            self.title_blink.enable(BlinkTarget::Title, cx);
        }
        if self.hex_is_focused {
            self.hex_blink.enable(BlinkTarget::Hex, cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.enable_cursor_blink(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.enable(BlinkTarget::GotoSearch, cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.enable(BlinkTarget::SidebarSearch, cx);
        }
    }

    pub fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let epoch = self.auto_save_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            timer_executor.timer(std::time::Duration::from_millis(500)).await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                if this.auto_save_epoch == epoch {
                    if this.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
                        this.state.save_active_note();
                        cx.notify();
                    }
                }
            });
        }).detach();
    }

    pub fn current_scroll_top(&self) -> f32 {
        let offset = self.editor_scroll_handle.offset();
        f32::from(-offset.y).max(0.0)
    }

    pub fn apply_note_settings_for_path(&mut self, path: &Path) {
        if let Some(settings) = self.state.note_states.get(path).cloned() {
            self.edit_mode = settings.edit_mode;
            self.word_wrap = settings.word_wrap;
            if settings.scroll_top > 0.0 {
                self.set_editor_scroll_top(settings.scroll_top);
            } else if settings.cursor_offset > 0 {
                self.scroll_cursor_into_view();
            } else {
                self.set_editor_scroll_top(0.0);
            }
        } else {
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
        }
    }

    pub fn save_on_exit(&mut self, window: &Window) {
        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let mut window_state = crate::state::WindowState::load(&crate::state::app_data_dir());
        window_state.maximized = is_maximized;
        if !is_maximized {
            window_state.x = Some(bounds.origin.x.into());
            window_state.y = Some(bounds.origin.y.into());
            window_state.width = bounds.size.width.into();
            window_state.height = bounds.size.height.into();
        }
        window_state.save(&crate::state::app_data_dir());

        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }

        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.session.sidebar_width = self.sidebar_width;
        self.state.session.sidebar_mode = self.sidebar_mode;
        self.state.save_session_state();
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    /// Toggles word wrapping mode in the editor.
    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        cx.notify();
    }

    fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        let editor_left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let char_w = if char_width > 1.0 { char_width } else { 7.7 };
        // Доступная ширина текста: ширина окна минус левые колонки и отступы (контейнер 32px + строка 8px + номера строк 40px + правые 40px)
        let available_text_w = (window_width - editor_left_w - 120.0).max(100.0);
        ((available_text_w / char_w).floor() as usize).max(20)
    }

    fn move_cursor_up_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache_with_headings(self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        if curr_v_idx == 0 {
            note.cursor_offset = 0;
            note.sync_cursor();
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx - 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_down_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache_with_headings(self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        if curr_v_idx + 1 >= visual_rows.len() {
            note.cursor_offset = note.content.len();
            note.sync_cursor();
            if !shift {
                note.selection_anchor = None;
            }
            return;
        }

        let target_v = &visual_rows[curr_v_idx + 1];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = note.offset_for_line_col(target_v.line_idx, target_char);
        note.cursor_offset = offset;
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_home_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache_with_headings(self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        let target_char = if cursor_col == v.char_start && v.sub_idx > 0 {
            0
        } else {
            v.char_start
        };
        note.cursor_offset = note.offset_for_line_col(v.line_idx, target_char);
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    fn move_cursor_end_visual(&mut self, shift: bool) {
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache_with_headings(self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = note.line_col();
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && note.selection_anchor.is_none() {
            note.selection_anchor = Some(note.cursor_offset);
        }

        let line_len = note.cached_lines.get(v.line_idx).map(|l| l.chars().count()).unwrap_or(0);
        let sub_end = if v.is_last_sub {
            v.char_end
        } else {
            v.char_end.saturating_sub(1)
        };
        let target_char = if cursor_col >= sub_end && !v.is_last_sub {
            line_len
        } else {
            sub_end
        };
        note.cursor_offset = note.offset_for_line_col(v.line_idx, target_char);
        note.sync_cursor();
        if !shift {
            note.selection_anchor = None;
        }
    }

    pub fn heading_typography_metrics(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
    ) -> (f32, f32, f32, f32, f32) {
        Self::heading_typography_metrics_with_widths(classified, headings, base_line_h, base_char_w, None)
    }

    pub fn heading_typography_metrics_with_widths(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
        heading_char_widths: Option<&[f32; 6]>,
    ) -> (f32, f32, f32, f32, f32) {
        let (h_size, is_heading, h_idx) = match classified {
            crate::markdown::LineKind::Heading1(_) => (headings.h1.font_size as f32, true, 0),
            crate::markdown::LineKind::Heading2(_) => (headings.h2.font_size as f32, true, 1),
            crate::markdown::LineKind::Heading3(_) => (headings.h3.font_size as f32, true, 2),
            crate::markdown::LineKind::Heading4(_) => (headings.h4.font_size as f32, true, 3),
            crate::markdown::LineKind::Heading5(_) => (headings.h5.font_size as f32, true, 4),
            crate::markdown::LineKind::Heading6(_) => (headings.h6.font_size as f32, true, 5),
            _ => (14.0, false, 0),
        };
        if is_heading {
            let row_h = (h_size * 1.35).round().max(base_line_h);
            let char_w = heading_char_widths
                .and_then(|w| w.get(h_idx).copied())
                .unwrap_or_else(|| base_char_w * (h_size / 14.0));
            let cursor_h = (h_size * 0.88).round().max(16.0);
            let cursor_top = ((row_h - cursor_h) / 2.0).round().max(2.0);
            (row_h, char_w, h_size, cursor_h, cursor_top)
        } else {
            (base_line_h, base_char_w, 14.0, 16.0, 4.0)
        }
    }

    pub fn resolve_heading_char_widths(
        text_system: &gpui::TextSystem,
        editor_font: &gpui::SharedString,
        headings: &crate::settings::HeadingsSettings,
        base_char_w: f32,
    ) -> [f32; 6] {
        let heading_configs = [
            (&headings.h1, 0),
            (&headings.h2, 1),
            (&headings.h3, 2),
            (&headings.h4, 3),
            (&headings.h5, 4),
            (&headings.h6, 5),
        ];
        let mut widths = [base_char_w; 6];
        for (h_cfg, idx) in heading_configs {
            let mut font = gpui::font(editor_font.clone());
            if h_cfg.bold {
                font = font.bold();
            }
            if h_cfg.italic {
                font = font.italic();
            }
            let font_id = text_system.resolve_font(&font);
            let h_size = h_cfg.font_size as f32;
            let char_w = text_system
                .advance(font_id, gpui::px(h_size), 'x')
                .map(|s| f32::from(s.width))
                .unwrap_or(base_char_w * (h_size / 14.0));
            widths[idx] = char_w;
        }
        widths
    }

    pub fn scroll_cursor_into_view_with_strategy(&self, _strategy: ScrollStrategy) {
        if let Some(ref note) = self.state.active_note {
            let (cursor_line, cursor_col) = note.line_col();
            let visual_rows = &note.cached_visual_rows;
            if let Some((v_idx, _)) = crate::markdown::find_visual_row_for_cursor(visual_rows, cursor_line, cursor_col) {
                let row_top = note.cached_height_index.top_of(v_idx);
                let row_h = note.cached_height_index.height_of(v_idx);
                let row_bottom = row_top + row_h;
                let current_scroll = self.current_scroll_top();
                let editor_h = self.last_editor_h.get().max(100.0);

                if row_top < current_scroll {
                    self.set_editor_scroll_top((row_top - 24.0).max(0.0));
                } else if row_bottom > current_scroll + editor_h {
                    self.set_editor_scroll_top(row_bottom + 24.0 - editor_h);
                }
            }
        }
    }

    fn scroll_cursor_into_view(&self) {
        self.scroll_cursor_into_view_with_strategy(ScrollStrategy::Top);
    }

    pub(crate) fn set_editor_scroll_top(&self, scroll_y: f32) {
        let offset_x = self.editor_scroll_handle.offset().x;
        self.editor_scroll_handle.set_offset(point(offset_x, px(-scroll_y)));
    }

    pub fn editor_scroll_metrics(&self, total_content_h: f32) -> (f32, f32, f32) {
        let bounds = self.editor_scroll_handle.bounds();
        let bounds_h = f32::from(bounds.size.height);
        let editor_h = if bounds_h > 50.0 {
            bounds_h
        } else {
            self.last_editor_h.get().max(100.0)
        };
        let max_offset_y = f32::from(self.editor_scroll_handle.max_offset().height);
        let max_scroll_content = if max_offset_y > 0.0 {
            max_offset_y
        } else {
            (total_content_h - editor_h).max(0.0)
        };
        let track_top_y = if bounds.origin.y > px(0.0) {
            f32::from(bounds.origin.y)
        } else {
            132.0
        };
        (editor_h, max_scroll_content, track_top_y)
    }

    fn handle_scrollbar_drag(&mut self, current_y: f32, _window: &Window, cx: &mut Context<Self>) {
        if !self.is_dragging_scrollbar {
            return;
        }
        let Some(ref mut note) = self.state.active_note else {
            return;
        };
        note.ensure_visual_cache_with_headings(self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings));
        let total_content_h = note.cached_height_index.total() + 48.0;
        let (editor_h, max_scroll_content, _) = self.editor_scroll_metrics(total_content_h);
        let ratio = (editor_h / total_content_h.max(editor_h)).clamp(0.04, 1.0);
        let thumb_h = (editor_h * ratio).max(28.0);
        let track_scrollable = (editor_h - thumb_h).max(1.0);

        let delta_y = current_y - self.scrollbar_drag_start_y;
        let delta_fraction = delta_y / track_scrollable;
        let new_scroll_y = (self.scrollbar_drag_start_scroll_y + delta_fraction * max_scroll_content).clamp(0.0, max_scroll_content);
        self.set_editor_scroll_top(new_scroll_y);
        cx.notify();
    }

    pub(crate) fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(ref root) = self.state.workspace_path {
            dialog = dialog.set_directory(root);
        }
        cx.foreground_executor().spawn(async move {
            if let Some(folder_handle) = dialog.pick_folder().await {
                let folder = folder_handle.path().to_path_buf();
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.unload_all_plugins();
                    this.state.open_workspace(folder);
                    this.load_installed_plugins_for_vault();
                    if this.state.active_note.is_none() {
                        if let Some(first_file) = AppState::first_file_in_tree(&this.state.file_tree) {
                            this.state.open_note(first_file);
                        }
                    }
                    this.need_scroll_sidebar_to_active.set(true);
                    cx.notify();
                });
            }
        }).detach();
    }

    /// Opens the native folder picker asynchronously without blocking the GPUI update loop.
    pub(crate) fn open_folder_dialog_deferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_folder_dialog(window, cx);
    }

    pub(crate) fn create_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(_) = self.state.create_new_note() {
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            self.reset_editor_cursor_blink(cx);
            cx.notify();
        }
    }

    pub(crate) fn select_note(&mut self, path: PathBuf, center_in_sidebar: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Empty-tab sentinels carry no file: activate the New Tab page instead.
        if crate::state::is_empty_tab_path(&path) {
            self.select_empty_tab(path, center_in_sidebar, window, cx);
            return;
        }
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.open_note(path.clone());
        self.apply_note_settings_for_path(&path);
        if let Some(ref note) = self.state.active_note {
            self.title_editing_path = Some(note.path.clone());
            self.title_edit_text = note.title.clone();
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }
        if center_in_sidebar {
            self.need_scroll_sidebar_to_active.set(true);
        }
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.state.active_note.as_ref().map(|n| n.path == path).unwrap_or(false) {
            self.commit_title_rename_internal();
        }
        if self.state.active_note.as_ref().map(|n| n.is_dirty && n.path == path).unwrap_or(false) {
            self.state.save_active_note();
        }
        let is_closing_active = self.state.active_note.as_ref().map(|n| n.path == path).unwrap_or(false);
        if is_closing_active {
            let current_scroll = self.current_scroll_top();
            self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        }
        self.state.close_tab(path);
        if let Some((target_path, title)) = self.state.active_note.as_ref().map(|n| (n.path.clone(), n.title.clone())) {
            self.apply_note_settings_for_path(&target_path);
            self.title_editing_path = Some(target_path);
            self.title_edit_text = title;
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
            self.need_scroll_sidebar_to_active.set(true);
        } else {
            self.title_editing_path = None;
            self.title_edit_text.clear();
            self.title_cursor_pos = 0;
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }
        cx.notify();
    }

    fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_back() {
            self.apply_note_settings_for_path(&target);
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            } else {
                self.title_editing_path = None;
                self.title_edit_text.clear();
                self.title_cursor_pos = 0;
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_forward() {
            self.apply_note_settings_for_path(&target);
            if let Some(ref note) = self.state.active_note {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            } else {
                self.title_editing_path = None;
                self.title_edit_text.clear();
                self.title_cursor_pos = 0;
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn reload_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.reload_file_tree();
        cx.notify();
    }

    pub(crate) fn save_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.save_active_note();
        cx.notify();
    }

    fn handle_editor_mouse_down(&mut self, click_x: f32, click_y: f32, shift: bool, click_count: usize, window: &Window, cx: &mut Context<Self>) {
        if self.state.active_note.is_none() {
            return;
        }

        let editor_rows_top_y = 132.0;
        let current_scroll_y = self.current_scroll_top();
        let rel_y = click_y - editor_rows_top_y + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let text_start_x = editor_left_x + 32.0 + 8.0 + 40.0;
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some(ref mut note) = self.state.active_note else {
            return;
        };

        note.ensure_visual_cache_with_headings(word_wrap, wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            note.cached_height_index.v_idx_at_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = note.line_col();
        let selection_range = note.selection_range();
        let line = &note.cached_lines[v.line_idx];
        let line_start = note.cached_line_starts.get(v.line_idx).copied().unwrap_or(0);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = note.cached_code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = note.cached_code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = match note.cached_line_starts.binary_search(&sel.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };
                let sel_end_l = match note.cached_line_starts.binary_search(&sel.end) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };
                (sel_start_l..=sel_end_l.min(note.cached_code_states.len().saturating_sub(1))).any(|l_idx| {
                    note.cached_code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                })
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = note.cached_code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        // Hit-test against the shaped layout of the row rather than a
        // `column * char_width` cell model: wide glyphs (emoji, CJK) do not advance
        // by exactly one cell.
        let col_in_row = if click_x > text_start_x {
            let row_layout = shape_row(
                window.text_system(),
                line,
                &classified,
                v,
                is_active_line,
                &self.config.editor.headings,
                &editor_font,
                row_char_w,
            );
            let column = row_layout.column_for_x(click_x - text_start_x);
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = note.offset_for_line_col(v.line_idx, target_char);

        if click_count == 2 {
            let (w_start, w_end) = note.word_range_at(offset);
            note.selection_anchor = Some(w_start);
            note.cursor_offset = w_end;
        } else if click_count >= 3 {
            let (l_start, l_end) = note.line_range_at(offset);
            note.selection_anchor = Some(l_start);
            note.cursor_offset = l_end;
        } else if shift {
            if note.selection_anchor.is_none() {
                note.selection_anchor = Some(note.cursor_offset);
            }
            note.cursor_offset = offset;
        } else {
            note.cursor_offset = offset;
            note.selection_anchor = Some(offset);
        }
        note.sync_cursor();
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    fn update_mouse_selection(&mut self, pos_x: f32, pos_y: f32, viewport_h: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_mouse_selecting {
            return;
        }
        if self.state.active_note.is_none() {
            return;
        }

        // Top of rows area: Titlebar (38px) + Nav bar (40px) + note-title-panel (54px) = 132px
        let editor_rows_top_y = 132.0;
        let editor_bottom_y = (viewport_h - 28.0).max(editor_rows_top_y + 100.0);
        let scroll_y = self.current_scroll_top();

        // Edge autoscroll when dragging outside or near the top/bottom bounds
        if pos_y < editor_rows_top_y && scroll_y > 0.0 {
            let step = ((editor_rows_top_y - pos_y).max(1.0) * 0.8).min(30.0);
            self.set_editor_scroll_top((scroll_y - step).max(0.0));
        } else if pos_y > editor_bottom_y - 16.0 {
            let step = ((pos_y - (editor_bottom_y - 16.0)).max(1.0) * 0.8).min(30.0);
            self.set_editor_scroll_top(scroll_y + step);
        }

        let current_scroll_y = self.current_scroll_top();
        let rel_y = pos_y - editor_rows_top_y + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let text_start_x = editor_left_x + 32.0 + 8.0 + 40.0;
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some(ref mut note) = self.state.active_note else {
            return;
        };

        note.ensure_visual_cache_with_headings(word_wrap, wrap_cols, Some(&self.config.editor.headings));
        let visual_rows = Arc::clone(&note.cached_visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            note.cached_height_index.v_idx_at_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = note.line_col();
        let selection_range = note.selection_range();
        let line = &note.cached_lines[v.line_idx];
        let line_start = note.cached_line_starts.get(v.line_idx).copied().unwrap_or(0);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = note.cached_code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = note.cached_code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = match note.cached_line_starts.binary_search(&sel.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };
                let sel_end_l = match note.cached_line_starts.binary_search(&sel.end) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };
                (sel_start_l..=sel_end_l.min(note.cached_code_states.len().saturating_sub(1))).any(|l_idx| {
                    note.cached_code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                })
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = note.cached_code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        let col_in_row = if pos_x > text_start_x {
            let row_layout = shape_row(
                window.text_system(),
                line,
                &classified,
                v,
                is_active_line,
                &self.config.editor.headings,
                &editor_font,
                row_char_w,
            );
            let column = row_layout.column_for_x(pos_x - text_start_x);
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = note.offset_for_line_col(v.line_idx, target_char);
        if note.cursor_offset != offset {
            note.cursor_offset = offset;
            note.sync_cursor();
            cx.notify();
        }
    }

    /// Whether the render path paints line `line_idx` as the active line, i.e. with its
    /// markdown markers (`# `, `> `, task/bullet/numbered prefixes) visible.
    ///
    /// `shape_row` reproduces exactly the text the render paints, so every hit-test must
    /// answer this the same way the render does (`is_editing && (...)`). Dropping the
    /// `edit_mode` guard makes the hit-test count markers that read mode hides, and the
    /// clicked column then lands one marker length away from the glyph under the pointer.
    fn is_active_editor_line(
        edit_mode: bool,
        line_idx: usize,
        cursor_line: usize,
        is_line_in_selection: bool,
        is_in_active_block: bool,
    ) -> bool {
        edit_mode && (line_idx == cursor_line || is_line_in_selection || is_in_active_block)
    }


    pub fn handle_custom_hex_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        self.reset_hex_cursor_blink(cx);
        let key = &event.keystroke.key;
        let total_chars = self.custom_hex_input.chars().count();
        let sel_range = self.hex_selection_range();

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.hex_selection_anchor = Some(0);
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            let text_to_copy = if let Some((start, end)) = sel_range {
                self.custom_hex_input.chars().skip(start).take(end - start).collect::<String>()
            } else {
                self.custom_hex_input.clone()
            };
            if !text_to_copy.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text_to_copy));
            }
            return;
        }

        if key == "escape" {
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "enter" {
            self.apply_custom_hex_if_valid(cx);
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "arrowleft" || key == "left" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            } else if let Some((start, _)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = start;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            }
            cx.notify();
            return;
        }

        if key == "arrowright" || key == "right" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            } else if let Some((_, end)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = end;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            }
            cx.notify();
            return;
        }

        if key == "home" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = 0;
            cx.notify();
            return;
        }

        if key == "end" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        let delete_selection = |input: &mut String, start: usize, end: usize| {
            let before: String = input.chars().take(start).collect();
            let after: String = input.chars().skip(end).collect();
            *input = format!("{before}{after}");
        };

        if key == "backspace" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos > 0 {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos - 1);
                self.custom_hex_input = chars.into_iter().collect();
                self.hex_cursor_pos -= 1;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if key == "delete" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos < total_chars {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos);
                self.custom_hex_input = chars.into_iter().collect();
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text.trim().chars().filter(|c| c.is_ascii_hexdigit() || *c == '#').take(7).collect();
                    if !filtered.is_empty() {
                        let formatted = if filtered.starts_with('#') { filtered } else { format!("#{filtered}") };
                        self.custom_hex_input = formatted;
                        self.hex_cursor_pos = self.custom_hex_input.chars().count();
                        self.hex_selection_anchor = None;
                        self.apply_custom_hex_if_valid(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        let ch = if let Some(c) = &event.keystroke.key_char {
            c.as_str()
        } else if key.chars().count() == 1 {
            key.as_str()
        } else {
            ""
        };

        for c in ch.chars() {
            if c.is_ascii_hexdigit() || c == '#' {
                let upper = c.to_ascii_uppercase();
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut self.custom_hex_input, start, end);
                    self.hex_cursor_pos = start;
                    self.hex_selection_anchor = None;
                }
                if self.custom_hex_input.chars().count() < 7 {
                    let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                    chars.insert(self.hex_cursor_pos, upper);
                    self.custom_hex_input = chars.into_iter().collect();
                    self.hex_cursor_pos += 1;
                    self.apply_custom_hex_if_valid(cx);
                    cx.notify();
                }
            }
        }
    }

    pub fn apply_custom_hex_if_valid(&mut self, cx: &mut Context<Self>) {
        let Some(picker_id) = self.active_color_picker else {
            return;
        };
        if crate::markdown::parse_hex_color(&self.custom_hex_input).is_some() {
            let hex_val = if self.custom_hex_input.starts_with('#') {
                self.custom_hex_input.clone()
            } else {
                format!("#{}", self.custom_hex_input)
            };
            match picker_id {
                "h1-color" => self.config.editor.headings.h1.color_name = hex_val,
                "h2-color" => self.config.editor.headings.h2.color_name = hex_val,
                "h3-color" => self.config.editor.headings.h3.color_name = hex_val,
                "h4-color" => self.config.editor.headings.h4.color_name = hex_val,
                "h5-color" => self.config.editor.headings.h5.color_name = hex_val,
                "h6-color" => self.config.editor.headings.h6.color_name = hex_val,
                _ => {}
            }
            self.save_settings(cx);
        }
    }

    pub(crate) fn commit_title_rename_internal(&mut self) -> bool {
        let Some(ref note) = self.state.active_note else {
            return false;
        };
        let current_path = note.path.clone();
        let current_title = note.title.clone();
        let new_title = self.title_edit_text.trim();

        if new_title.is_empty() {
            self.title_edit_text = current_title;
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
            return false;
        }

        if new_title != current_title {
            match self.state.rename_note(&current_path, new_title) {
                Ok(new_path) => {
                    self.title_editing_path = Some(new_path);
                    self.title_edit_text = new_title.to_string();
                    self.title_cursor_pos = self.title_edit_text.chars().count();
                    self.title_selection_anchor = None;
                    self.title_is_mouse_selecting = false;
                    return true;
                }
                Err(err) => {
                    self.state.status_text = format!("Rename failed: {}", err);
                    self.title_edit_text = current_title;
                    self.title_cursor_pos = self.title_edit_text.chars().count();
                    self.title_selection_anchor = None;
                    self.title_is_mouse_selecting = false;
                    return false;
                }
            }
        }
        self.title_selection_anchor = None;
        self.title_is_mouse_selecting = false;
        false
    }

    pub fn commit_title_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_title_rename_internal() {
            cx.notify();
        }
    }

    fn handle_title_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        if key == "enter" || key == "tab" {
            self.title_selection_anchor = None;
            self.commit_title_rename(window, cx);
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "escape" {
            self.title_selection_anchor = None;
            if let Some(ref note) = self.state.active_note {
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "down" {
            self.title_selection_anchor = None;
            self.commit_title_rename(window, cx);
            if let Some(ref mut note) = self.state.active_note {
                note.cursor_offset = 0;
                note.selection_anchor = None;
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.title_selection_anchor = Some(0);
            self.title_cursor_pos = self.title_edit_text.chars().count();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s_start, s_end)) = self.title_selection_range() {
                let selected: String = self.title_edit_text.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s_start, s_end)) = self.title_selection_range() {
                let selected: String = self.title_edit_text.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                chars.drain(s_start..s_end);
                self.title_edit_text = chars.into_iter().collect();
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.reset_title_cursor_blink(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl + S (Save note)
        if ctrl && (key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы") {
            self.commit_title_rename(window, cx);
            self.save_note(window, cx);
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
                    let filtered: String = text
                        .chars()
                        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                        .collect();
                    if !filtered.is_empty() {
                        let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                        let pos = if let Some((s_start, s_end)) = self.title_selection_range() {
                            chars.drain(s_start..s_end);
                            self.title_selection_anchor = None;
                            s_start
                        } else {
                            self.title_cursor_pos.min(chars.len())
                        };
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        self.title_cursor_pos = pos + filtered.chars().count();
                        self.title_edit_text = chars.into_iter().collect();
                        self.reset_title_cursor_blink(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                if self.title_cursor_pos > 0 {
                    self.title_cursor_pos -= 1;
                }
            } else {
                if let Some((s_start, _)) = self.title_selection_range() {
                    self.title_cursor_pos = s_start;
                    self.title_selection_anchor = None;
                } else if self.title_cursor_pos > 0 {
                    self.title_cursor_pos -= 1;
                }
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            let total_chars = self.title_edit_text.chars().count();
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                if self.title_cursor_pos < total_chars {
                    self.title_cursor_pos += 1;
                }
            } else {
                if let Some((_, s_end)) = self.title_selection_range() {
                    self.title_cursor_pos = s_end;
                    self.title_selection_anchor = None;
                } else if self.title_cursor_pos < total_chars {
                    self.title_cursor_pos += 1;
                }
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                self.title_cursor_pos = 0;
            } else {
                self.title_selection_anchor = None;
                self.title_cursor_pos = 0;
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            let total_chars = self.title_edit_text.chars().count();
            if shift {
                if self.title_selection_anchor.is_none() {
                    self.title_selection_anchor = Some(self.title_cursor_pos);
                }
                self.title_cursor_pos = total_chars;
            } else {
                self.title_selection_anchor = None;
                self.title_cursor_pos = total_chars;
            }
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            let mut chars: Vec<char> = self.title_edit_text.chars().collect();
            if let Some((s_start, s_end)) = self.title_selection_range() {
                chars.drain(s_start..s_end);
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.title_edit_text = chars.into_iter().collect();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            } else {
                let pos = self.title_cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.title_cursor_pos = pos - 1;
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
            return;
        }

        // Delete
        if key == "delete" {
            let mut chars: Vec<char> = self.title_edit_text.chars().collect();
            if let Some((s_start, s_end)) = self.title_selection_range() {
                chars.drain(s_start..s_end);
                self.title_cursor_pos = s_start;
                self.title_selection_anchor = None;
                self.title_edit_text = chars.into_iter().collect();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            } else {
                let pos = self.title_cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
            let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
            let text_to_insert = if key == "space" {
                Some(" ")
            } else if let Some(ch) = &event.keystroke.key_char {
                Some(ch.as_str())
            } else if key.chars().count() == 1 {
                Some(key.as_str())
            } else {
                None
            };

            if let Some(text) = text_to_insert {
                let filtered: String = text
                    .chars()
                    .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                    .collect();
                if !filtered.is_empty() {
                    let mut chars: Vec<char> = self.title_edit_text.chars().collect();
                    let pos = if let Some((s_start, s_end)) = self.title_selection_range() {
                        chars.drain(s_start..s_end);
                        self.title_selection_anchor = None;
                        s_start
                    } else {
                        self.title_cursor_pos.min(chars.len())
                    };
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    self.title_cursor_pos = pos + filtered.chars().count();
                    self.title_edit_text = chars.into_iter().collect();
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn first_matching_file(&self, nodes: &[FileNode]) -> Option<PathBuf> {
        let query = self.state.search_query.trim().to_lowercase();
        self.first_matching_file_internal(nodes, &query, false)
    }

    fn first_matching_file_internal(&self, nodes: &[FileNode], query: &str, parent_matched: bool) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { name, path } => {
                    if query.is_empty() || parent_matched || name.to_lowercase().contains(query) {
                        return Some(path.clone());
                    }
                }
                FileNode::Folder { name, children, .. } => {
                    let folder_matched = parent_matched || (!query.is_empty() && name.to_lowercase().contains(query));
                    if let Some(p) = self.first_matching_file_internal(children, query, folder_matched) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.reset_editor_cursor_blink(cx);
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control;
        let alt = keystroke.modifiers.alt;
        let shift = keystroke.modifiers.shift;
        if alt && keystroke.key == "left" {
            self.navigate_back(_window, cx);
            return;
        }

        // Alt + Right (Navigate Forward in note history)
        if alt && keystroke.key == "right" {
            self.navigate_forward(_window, cx);
            return;
        }

        // Escape closes the tabs popup if open
        if keystroke.key == "escape" && self.tabs_popup.is_some() {
            self.close_tabs_popup(cx);
            return;
        }

        // Escape closes the command palette if open
        if keystroke.key == "escape" && self.command_palette.is_open {
            self.close_command_palette(_window, cx);
            return;
        }

        // Escape closes folder context menu if open
        if keystroke.key == "escape" && self.folder_context_menu.is_open {
            self.close_folder_context_menu(cx);
            return;
        }

        // Escape closes file context menu if open
        if keystroke.key == "escape" && self.file_context_menu.is_open {
            self.close_file_context_menu(cx);
            return;
        }

        // Escape closes folder modal if open
        if keystroke.key == "escape" && self.folder_modal != crate::views::sidebar::FolderModal::None {
            self.close_folder_modal(cx);
            return;
        }

        // Escape closes settings modal if open
        if keystroke.key == "escape" && self.is_settings_open {
            self.close_settings(cx);
            return;
        }

        // Ctrl + , (Toggle Settings modal)
        if ctrl && (keystroke.key == "," || keystroke.key == "<" || keystroke.key == "б" || keystroke.key == "Б") {
            self.toggle_settings(cx);
            return;
        }

        // Ctrl + \ или Ctrl + B (Свернуть / развернуть сайдбар)
        if ctrl && (keystroke.key == "\\" || keystroke.key.eq_ignore_ascii_case("b") || keystroke.key == "и" || keystroke.key == "И") {
            self.toggle_sidebar(cx);
            return;
        }

        // Ctrl + S (Сохранение)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("s") || keystroke.key == "ы" || keystroke.key == "Ы") {
            self.state.save_active_note();
            cx.notify();
            return;
        }

        // Ctrl + P (Палитра команд)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + Shift + P (Палитра команд)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + O (Поиск заметок / Меню быстрого перехода)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_search_dialog(_window, cx);
            return;
        }

        // Ctrl + Shift + O (Открыть хранилище)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_folder_dialog(_window, cx);
            return;
        }

        // Ctrl + N (Новая пустая вкладка: файл не создаётся, показывается New Tab)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("n") || keystroke.key == "т" || keystroke.key == "Т") {
            self.open_empty_tab(_window, cx);
            return;
        }

        // Ctrl + W (Закрыть текущую вкладку)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("w") || keystroke.key == "ц" || keystroke.key == "Ц") {
            if let Some(ref note) = self.state.active_note {
                let path = note.path.clone();
                self.close_tab(&path, cx);
            }
            return;
        }

        // Ctrl + F (Поиск / фильтр). Shift excluded: Ctrl+Shift+F opens
        // the sidebar search panel via the root listener below.
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("f") || keystroke.key == "а" || keystroke.key == "А") {
            _window.focus(&self.search_focus_handle);
            self.quick_filter.focus_and_select_all(&self.state.search_query, cx);
            return;
        }

        // Ctrl + Z (Undo)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            if let Some(ref mut note) = self.state.active_note {
                note.undo();
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + Y или Ctrl + Shift + Z (Redo)
        if (ctrl && (keystroke.key.eq_ignore_ascii_case("y") || keystroke.key == "н" || keystroke.key == "Н"))
            || (ctrl && shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я"))
        {
            if let Some(ref mut note) = self.state.active_note {
                note.redo();
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + A (Выделить всё)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("a") || keystroke.key == "ф" || keystroke.key == "Ф") {
            if let Some(ref mut note) = self.state.active_note {
                note.select_all();
                cx.notify();
            }
            return;
        }

        // Ctrl + C (Копировать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("c") || keystroke.key == "с" || keystroke.key == "С") {
            if let Some(ref note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            return;
        }

        // Ctrl + X (Вырезать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("x") || keystroke.key == "ч" || keystroke.key == "Ч") {
            if let Some(ref mut note) = self.state.active_note {
                if let Some(text) = note.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    note.delete_selection();
                    cx.notify();
                }
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Alt + Z (Переключение переноса строк)
        if keystroke.modifiers.alt && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            self.toggle_word_wrap(cx);
            return;
        }

        // Ctrl + V (Вставить)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("v") || keystroke.key == "м" || keystroke.key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    if let Some(ref mut note) = self.state.active_note {
                        note.insert_text(&text);
                        cx.notify();
                    }
                }
            }
            self.schedule_auto_save(cx);
            return;
        }

        if self.state.active_note.is_none() {
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => { if let Some(ref mut n) = self.state.active_note { n.delete_backwards(); } }
            "delete" => { if let Some(ref mut n) = self.state.active_note { n.delete_forwards(); } }
            "enter" => { if let Some(ref mut n) = self.state.active_note { n.insert_newline_auto_indent(); } }
            "tab" => { if let Some(ref mut n) = self.state.active_note { n.insert_text("    "); } }
            "space" => { if let Some(ref mut n) = self.state.active_note { n.insert_text(" "); } }
            "left" => { if let Some(ref mut n) = self.state.active_note { n.move_cursor_left(shift); } }
            "right" => { if let Some(ref mut n) = self.state.active_note { n.move_cursor_right(shift); } }
            "up" => self.move_cursor_up_visual(shift),
            "down" => self.move_cursor_down_visual(shift),
            "home" => self.move_cursor_home_visual(shift),
            "end" => self.move_cursor_end_visual(shift),
            _ => {
                if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                    if let Some(ref mut n) = self.state.active_note {
                        if let Some(ch) = &keystroke.key_char {
                            n.insert_text(ch);
                        } else if keystroke.key.chars().count() == 1 {
                            n.insert_text(&keystroke.key);
                        }
                    }
                }
            }
        }
        if self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false) {
            self.schedule_auto_save(cx);
        }
        self.scroll_cursor_into_view();
        cx.notify();
    }

    fn render_navigation_bar(&self, theme: &Theme, cx: &Context<Self>) -> gpui::Stateful<Div> {
        let can_go_back = self.state.can_navigate_back();
        let can_go_forward = self.state.can_navigate_forward();
        let breadcrumbs = self.state.note_breadcrumbs();
        let is_dirty = self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false);
        let is_editing = self.edit_mode;

        div()
            .id("editor-navigation-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(40.0))
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .bg(theme.bg_editor)
            // Left: Back/Forward buttons + Breadcrumbs
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .min_w_0()
                    .overflow_hidden()
                    // Back button
                    .child(
                        div()
                            .id("btn-nav-back")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_back, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_back(window, cx);
                                    }))
                            })
                            .when(!can_go_back, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.border))
                            }),
                    )
                    // Forward button
                    .child(
                        div()
                            .id("btn-nav-forward")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_forward, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_forward(window, cx);
                                    }))
                            })
                            .when(!can_go_forward, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.border))
                            }),
                    )
                    // Spacer between arrows and breadcrumbs
                    .child(div().w(px(10.0)))
                    // Breadcrumbs trail
                    .child({
                        let mut crumbs_bar = div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .min_w_0()
                            .overflow_hidden();

                        let count = breadcrumbs.len();
                        for (idx, part) in breadcrumbs.iter().enumerate() {
                            if idx > 0 {
                                crumbs_bar = crumbs_bar.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.text_muted)
                                        .px_1p5()
                                        .flex_shrink_0()
                                        .child("/"),
                                );
                            }
                            let is_last = idx == count.saturating_sub(1);
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_sm()
                                    .font_weight(if is_last {
                                        gpui::FontWeight::SEMIBOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(if is_last {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    })
                                    .truncate()
                                    .hover(|h| {
                                        if !is_last {
                                            h.text_color(theme.text_secondary)
                                        } else {
                                            h
                                        }
                                    })
                                    .child(part.clone()),
                            );
                        }

                        if is_dirty {
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.accent)
                                    .ml_1p5()
                                    .child("●"),
                            );
                        }

                        crumbs_bar
                    }),
            )
            // Right: Info and Obsidian-style action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .flex_shrink_0()
                    .when_some(self.state.active_note.as_ref(), |r, note| {
                        let words: usize = note.content.split_whitespace().count();
                        let chars = note.content.chars().count();
                        r.child(
                            div()
                                .text_sm()
                                .text_color(theme.text_muted)
                                .mr_2()
                                .child(format!("{}w • {}c", words, chars)),
                        )
                    })
                    // Toggle wrap
                    .child(
                        div()
                            .id("btn-toggle-wrap")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_CODE,
                                15.0,
                                if self.word_wrap { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_word_wrap(cx);
                            })),
                    )
                    // Toggle reader / edit view
                    .child(
                        div()
                            .id("btn-toggle-reading-mode")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_BOOK_OPEN,
                                16.0,
                                if !is_editing { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = !this.edit_mode;
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap);
                                if this.edit_mode {
                                    this.reset_editor_cursor_blink(cx);
                                }
                                cx.notify();
                            })),
                    )
                    // More options (3 vertical dots - saves active note)
                    .child(
                        div()
                            .id("btn-more-options")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MORE_VERTICAL, 16.0, theme.text_muted))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_note(window, cx);
                            })),
                    ),
            )
    }
}

/// FPS-монитор — только для debug-сборки.
///
/// В release-сборке это пустая функция: в Windows-приложении с подсистемой "windows"
/// консоли нет, и писать в stdout/stderr некуда. Убираем последнего runtime-писателя
/// в консоль, чтобы исключить его влияние на CPU в простое.
#[cfg(debug_assertions)]
fn log_fps_tick() {
    static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static LAST_FPS: std::sync::OnceLock<std::sync::Mutex<std::time::Instant>> =
        std::sync::OnceLock::new();
    let mutex = LAST_FPS.get_or_init(|| std::sync::Mutex::new(std::time::Instant::now()));
    FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut last) = mutex.try_lock() {
        if last.elapsed() >= std::time::Duration::from_secs(1) {
            let frames = FRAME_COUNT.swap(0, std::sync::atomic::Ordering::Relaxed);
            let elapsed = last.elapsed();
            *last = std::time::Instant::now();
            eprintln!(
                "[FPS Monitor] {:?} elapsed, {} render calls (~{:.1} FPS)",
                elapsed,
                frames,
                frames as f32 / elapsed.as_secs_f32()
            );
        }
    }
}

/// В release-сборке монитор выключен — ни одной записи в консоль.
#[cfg(not(debug_assertions))]
fn log_fps_tick() {}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log_fps_tick();

        // Device-pixel grid for thin carets: GPUI paints quads at raw float
        // coordinates, so an unsnapped 1.5-2px caret visibly changes thickness
        // while typing. Captured once per frame, reused at every caret site.
        let pixel_scale = window.scale_factor();

        if !self.initial_scroll_done {
            self.initial_scroll_done = true;
            if self.initial_scroll_top > 0.0 {
                self.set_editor_scroll_top(self.initial_scroll_top);
            } else {
                self.scroll_cursor_into_view();
            }
        }

        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let geom = (
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            Some(f32::from(bounds.origin.x)),
            Some(f32::from(bounds.origin.y)),
            is_maximized,
        );
        if self.last_saved_geometry != Some(geom) {
            self.last_saved_geometry = Some(geom);
            let mut ws = crate::state::WindowState::load(&crate::state::app_data_dir());
            ws.maximized = is_maximized;
            if !is_maximized {
                ws.width = geom.0;
                ws.height = geom.1;
                ws.x = geom.2;
                ws.y = geom.3;
            }
            ws.save(&crate::state::app_data_dir());
        }

        let is_custom_hex_focused = self.custom_hex_focus_handle.is_focused(window);
        if !self.hex_is_focused && is_custom_hex_focused {
            self.hex_is_focused = true;
            self.enable_hex_cursor_blink(cx);
        } else if self.hex_is_focused && !is_custom_hex_focused {
            self.hex_is_focused = false;
            self.disable_hex_cursor_blink(cx);
        }

        let is_title_focused = self.title_focus_handle.is_focused(window);
        if !self.title_is_focused && is_title_focused {
            self.title_is_focused = true;
            self.enable_title_cursor_blink(cx);
        } else if self.title_is_focused && !is_title_focused {
            self.title_is_focused = false;
            self.disable_title_cursor_blink(cx);
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }

        let is_search_focused = self.search_focus_handle.is_focused(window);
        self.quick_filter.sync_focus(is_search_focused, cx);

        let is_editor_focused_now = self.focus_handle.is_focused(window)
            || (!is_title_focused
                && !is_search_focused
                && !is_custom_hex_focused
                && !self.folder_modal_focus_handle.is_focused(window)
                && !self.folder_context_menu.is_open
                && self.folder_modal == crate::views::sidebar::FolderModal::None
                && !self.is_settings_open);

        if !self.editor_is_focused && is_editor_focused_now {
            self.editor_is_focused = true;
            self.enable_editor_cursor_blink(cx);
        } else if self.editor_is_focused && !is_editor_focused_now {
            self.editor_is_focused = false;
            self.disable_editor_cursor_blink(cx);
        }

        // Everything above reacts to *in-window* focus, and gpui never clears `window.focus`
        // when the OS window is deactivated — so all of those flags stay true while the app
        // sits in the background. Left alone, the blink timers keep ticking there and every
        // tick calls `notify()`, i.e. a full re-render of this view twice a second, forever,
        // for a caret nobody can interact with (~0.5-1% of a core).
        //
        // Polling is sufficient: gpui's `on_active_status_change` calls `window.refresh()`,
        // which marks the window dirty and bypasses the root view's element cache, so this
        // block is guaranteed to run on both deactivation and re-activation.
        let window_active = window.is_window_active();
        if self.window_was_active != window_active {
            self.window_was_active = window_active;
            if window_active {
                self.resume_cursor_blinks(cx);
            } else {
                self.suspend_cursor_blinks(cx);
            }
        }

        if self.was_title_focused && !is_title_focused {
            self.commit_title_rename_internal();
        }
        self.was_title_focused = is_title_focused;

        if let Some(ref note) = self.state.active_note {
            if self.title_editing_path.as_ref() != Some(&note.path) {
                self.title_editing_path = Some(note.path.clone());
                self.title_edit_text = note.title.clone();
                self.title_cursor_pos = self.title_edit_text.chars().count();
                self.title_selection_anchor = None;
                self.title_is_mouse_selecting = false;
            }
        } else {
            self.title_editing_path = None;
            self.title_edit_text.clear();
            self.title_cursor_pos = 0;
            self.title_selection_anchor = None;
            self.title_is_mouse_selecting = false;
        }

        self.theme = Theme::from_config(&self.config.ui.theme, Some(window.appearance()));
        let cx = &*cx;
        let theme = &self.theme;
        let active_path = self.state.active_note.as_ref().map(|n| n.path.clone());
        let is_editing = self.edit_mode;
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        let viewport_w: f32 = window.viewport_size().width.into();
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;
        if let Some(ref mut note) = self.state.active_note {
            note.ensure_visual_cache_with_headings(self.word_wrap, wrap_cols, Some(&self.config.editor.headings));
        }

        let is_resizer_active = self.is_resizing_sidebar || self.is_hovering_resizer;
        let resizer_color = if is_resizer_active { theme.accent } else { theme.border };

        let sidebar_area_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let right_controls_w = 214.0;
        let drag_min_w = 24.0;
        let available_tab_strip_w = (viewport_w - sidebar_area_w - right_controls_w - drag_min_w).max(60.0);
        let active_tab_idx = self.state.active_note.as_ref().and_then(|note| {
            self.state.open_tabs.iter().position(|p| p == &note.path)
        });
        let tab_layout = crate::views::tabs_menu::calculate_tab_widths(
            self.state.open_tabs.len(),
            active_tab_idx,
            available_tab_strip_w,
            160.0,
            120.0,
            6.0,
            28.0,
            6.0,
            8.0,
        );
        let tab_widths = tab_layout.widths;
        let is_tabs_overflowing = tab_layout.is_overflowing;
        let effective_tab_gap = tab_layout.tab_gap;

        div()
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.is_settings_open {
                    return;
                }
                if this.is_resizing_sidebar {
                    let x: f32 = event.position.x.into();
                    if x < 100.0 {
                        this.sidebar_collapsed = true;
                    } else {
                        this.sidebar_collapsed = false;
                        this.sidebar_width = x.clamp(160.0, 600.0);
                    }
                    cx.notify();
                } else if this.is_resizing_right_sidebar {
                    let win_w: f32 = window.viewport_size().width.into();
                    let x: f32 = event.position.x.into();
                    let right_w = win_w - x;
                    if right_w < 100.0 {
                        this.right_sidebar_collapsed = true;
                    } else {
                        this.right_sidebar_collapsed = false;
                        this.right_sidebar_width = right_w.clamp(160.0, 600.0);
                    }
                    cx.notify();
                } else if this.is_dragging_scrollbar {
                    let y: f32 = event.position.y.into();
                    this.handle_scrollbar_drag(y, window, cx);
                } else if this.sidebar_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_sidebar_scrollbar_drag(y, cx);
                } else if this.title_is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let title_left_x = editor_left_x + 48.0;
                    let rel_x = (x - title_left_x).max(0.0);
                    let new_pos = if this.title_edit_text.is_empty() {
                        0
                    } else {
                        let run = gpui::TextRun {
                            len: this.title_edit_text.len(),
                            font: gpui::font("Segoe UI").bold(),
                            color: Default::default(),
                            underline: Default::default(),
                            strikethrough: None,
                            background_color: None,
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(this.title_edit_text.clone()),
                            px(28.0),
                            &[run],
                            None,
                        );
                        let byte_idx = shaped.closest_index_for_x(px(rel_x));
                        this.title_edit_text[..byte_idx.min(this.title_edit_text.len())].chars().count()
                    };
                    if this.title_cursor_pos != new_pos {
                        this.title_cursor_pos = new_pos;
                        this.reset_title_cursor_blink(cx);
                        cx.notify();
                    }
                } else if this.quick_filter.is_mouse_selecting {
                    this.quick_filter.handle_mouse_move(event, &this.state.search_query, window, cx);
                } else if this.panel_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_panel_scrollbar_drag(y, cx);
                } else if this.sidebar_search.is_mouse_selecting {
                    this.handle_panel_mouse_move(event, window, cx);
                } else if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                } else if this.goto_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_goto_scrollbar_drag(y, cx);
                } else if this.search_dialog.is_mouse_selecting {
                    this.handle_goto_mouse_move(event, window, cx);
                } else if this.is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    let viewport_h: f32 = window.viewport_size().height.into();
                    this.update_mouse_selection(x, y, viewport_h, window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.is_settings_open {
                    return;
                }
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.catalog_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                if this.is_resizing_sidebar {
                    // Persist the dragged width right away (crash-safe;
                    // exit save refreshes it again).
                    this.state.session.sidebar_width = this.sidebar_width;
                    this.state.save_session_state();
                }
                if this.title_is_mouse_selecting {
                    this.title_is_mouse_selecting = false;
                    if this.title_selection_anchor == Some(this.title_cursor_pos) {
                        this.title_selection_anchor = None;
                    }
                    cx.notify();
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
                this.sidebar_scrollbar_drag.end();
                if let Some(ref mut n) = this.state.active_note {
                    if n.selection_anchor == Some(n.cursor_offset) {
                        n.selection_anchor = None;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.is_settings_open {
                    return;
                }
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                if this.title_is_mouse_selecting {
                    this.title_is_mouse_selecting = false;
                    if this.title_selection_anchor == Some(this.title_cursor_pos) {
                        this.title_selection_anchor = None;
                    }
                    cx.notify();
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.is_hovering_resizer = false;
                this.is_hovering_right_resizer = false;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
                this.sidebar_scrollbar_drag.end();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                // Ctrl+P opens Command Palette from anywhere (leaves don't handle it,
                // so this root listener fires exactly once; the modal input
                // stops propagation, so an open modal never re-triggers it).
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                    && !this.command_palette.is_open
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+O opens Go to file (Search Dialog) from anywhere.
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                    && !this.search_dialog.is_open
                {
                    this.open_search_dialog(window, cx);
                }
                // Ctrl+Shift+O opens folder dialog from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                {
                    this.open_folder_dialog(window, cx);
                }
                // Ctrl+Shift+P also opens Command Palette for muscle memory compatibility.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+Shift+F opens the sidebar search panel from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("f")
                        || keystroke.key == "а"
                        || keystroke.key == "А")
                {
                    this.open_sidebar_search(window, cx);
                }
            }))
            // ================= WINDOW TITLEBAR & TAB BAR (CUSTOM CSD FRAME) =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(38.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .bg(theme.bg_sidebar)
                    // Left area: Sidebar header column in titlebar (width matches sidebar)
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .border_b_1()
                                .border_color(theme.border)
                                // 1. Rail header cell (44px) matching the rail below
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(44.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .border_r_1()
                                        .border_color(theme.border)
                                        .child(
                                            div()
                                                .id("btn-toggle-sidebar")
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .p_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(
                                                    icons::ICON_PANEL_LEFT,
                                                    15.0,
                                                    theme.accent,
                                                ))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_sidebar(cx);
                                                })),
                                        ),
                                )
                                // 2. File tree header (VIBECODIUM badge and drag region)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .flex_1()
                                        .h_full()
                                        .px_3()
                                        .child(
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .bg(theme.accent)
                                                .text_color(theme.bg_sidebar)
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .rounded_md()
                                                .text_xs()
                                                .child("VIBECODIUM"),
                                        )
                                        // Files / Search view toggle (Obsidian-style)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .mx_1()
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-files")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Files,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_FOLDER,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Files {
                                                                theme.accent
                                                            } else {
                                                                theme.text_muted
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.close_sidebar_search(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-search")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Search,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_SEARCH,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
                                                                theme.accent
                                                            } else {
                                                                theme.text_muted
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.open_sidebar_search(window, cx);
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-titlebar-drag-region")
                                                .flex_1()
                                                .h_full()
                                                .window_control_area(WindowControlArea::Drag),
                                        ),
                                ),
                        )
                    })
                    .when(self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(44.0))
                                .h_full()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_b_1()
                                .border_color(theme.border)
                                .bg(theme.bg_sidebar)
                                .child(
                                    div()
                                        .id("btn-toggle-sidebar")
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .p_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(icons::icon(
                                            icons::ICON_PANEL_LEFT,
                                            15.0,
                                            theme.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_sidebar(cx);
                                        })),
                                ),
                        )
                    })
                    // Vertical divider (1px border line + 7px transparent hitbox) matching the sidebar-resizer below exactly
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(resizer_color)
                                .border_b_1()
                                .border_color(theme.border)
                                .when(is_resizer_active, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(px(-1.0))
                                            .w(px(3.0))
                                            .bg(theme.accent),
                                    )
                                })
                                .child(
                                    div()
                                        .id("titlebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            this.is_hovering_resizer = *hovered;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // Right area: Editor header column containing Tab Bar, Drag Area, and Window Controls
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_row()
                            .items_center()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            // Continuous bottom border line running across the editor header column
                            .child(
                                div()
                                    .id("titlebar-editor-bottom-border")
                                    .absolute()
                                    .bottom_0()
                                    .left_0()
                                    .right_0()
                                    .h(px(1.0))
                                    .bg(theme.border),
                            )
                            // Browser-style Tabs Area starting directly at the boundary
                            .child({
                                let active_path = self.state.active_note.as_ref().map(|n| n.path.clone())
                                    .or_else(|| self.state.active_empty_tab.clone());
                                let is_active_dirty = self.state.active_note.as_ref().map(|n| n.is_dirty).unwrap_or(false);

                                let is_overflowing = is_tabs_overflowing;

                                let mut tabs_strip = div()
                                    .id("tabs-strip-inner")
                                    .flex()
                                    .flex_row()
                                    .items_end()
                                    .gap(px(effective_tab_gap))
                                    .h_full()
                                    .pl(px(8.0))
                                    .flex_shrink_0();

                                for (idx, tab_path) in self.state.open_tabs.iter().enumerate() {
                                    let is_active = active_path.as_ref() == Some(tab_path);
                                    let is_next_active = active_path.as_ref() == self.state.open_tabs.get(idx + 1);
                                    let show_divider = !is_active && (idx + 1 < self.state.open_tabs.len()) && !is_next_active;

                                    let title = crate::views::new_tab::tab_display_title(
                                        tab_path,
                                        &self.t("tabs.newTab"),
                                        &self.t("common.untitled"),
                                    );
                                    let tab_path_for_select = tab_path.clone();
                                    let tab_path_for_close = tab_path.clone();
                                    let tab_path_for_middle = tab_path.clone();
                                    let tab_path_for_middle_select = tab_path.clone();
                                    let tab_path_for_context = tab_path.clone();

                                    let tab_w = tab_widths.get(idx).copied().unwrap_or(160.0);

                                    let mut tab_item = div()
                                        .id(gpui::ElementId::from(("tab-item", idx)))
                                        .relative()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .h(px(34.0))
                                        .w(px(tab_w))
                                        .flex_shrink_0()
                                        .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.close_tab(&tab_path_for_middle, cx);
                                        }))
                                        .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            let x: f32 = event.position.x.into();
                                            let y: f32 = event.position.y.into();
                                            this.open_tab_context_menu(tab_path_for_context.clone(), x, y, cx);
                                        }))
                                        .when(!is_active, |tab| {
                                            tab.bg(theme.bg_sidebar)
                                                .rounded_t_md()
                                                .border_b_1()
                                                .border_color(theme.border)
                                                .hover(|h| h.bg(theme.bg_hover))
                                        });

                                    if show_divider {
                                        let sep_right = -(effective_tab_gap / 2.0).floor();
                                        let divider_color = if theme.is_dark {
                                            rgba(0xcdd6f440)
                                        } else {
                                            rgba(0x1e203030)
                                        };
                                        tab_item = tab_item.child(
                                            div()
                                                .id(gpui::ElementId::from(("tab-separator", idx)))
                                                .absolute()
                                                .top(px(9.0))
                                                .h(px(16.0))
                                                .w(px(1.0))
                                                .right(px(sep_right))
                                                .rounded_full()
                                                .bg(divider_color),
                                        );
                                    }

                                    if is_active {
                                        // 1. Solid center body fill
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-body-fill")
                                                .absolute()
                                                .top(px(1.0))
                                                .bottom_0()
                                                .left(px(6.0))
                                                .right(px(6.0))
                                                .bg(theme.bg_editor),
                                        );
                                        // 2. Solid fill mask at the baseline connecting active tab into the editor
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-bottom-cover")
                                                .absolute()
                                                .bottom(px(-2.0))
                                                .left(px(6.0))
                                                .right(px(6.0))
                                                .h(px(3.0))
                                                .bg(theme.bg_editor),
                                        );
                                        // 3. Continuous top border - stops safely within the flat SVG lead-in
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-border-top")
                                                .absolute()
                                                .top_0()
                                                .left(px(7.0))
                                                .right(px(7.0))
                                                .h(px(1.0))
                                                .bg(theme.border),
                                        );
                                        // 4. Left cap
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-cap-left")
                                                .absolute()
                                                .top_0()
                                                .left(px(-8.0))
                                                .w(px(16.0))
                                                .h(px(36.0))
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_FILL, 16.0, 36.0, theme.bg_editor)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                )
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_STROKE, 16.0, 36.0, theme.border)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                ),
                                        );
                                        // 5. Right cap
                                        tab_item = tab_item.child(
                                            div()
                                                .id("tab-cap-right")
                                                .absolute()
                                                .top_0()
                                                .right(px(-8.0))
                                                .w(px(16.0))
                                                .h(px(36.0))
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_FILL, 16.0, 36.0, theme.bg_editor)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                )
                                                .child(
                                                    icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_STROKE, 16.0, 36.0, theme.border)
                                                        .absolute()
                                                        .top_0()
                                                        .left_0(),
                                                ),
                                        );
                                    }

                                    tabs_strip = tabs_strip.child(
                                        tab_item
                                            // Tab selection area (icon + title + dirty dot)
                                            .child(
                                                div()
                                                    .id(gpui::ElementId::from(("tab-select", idx)))
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap(if tab_w < 48.0 { px(2.0) } else { px(6.0) })
                                                    .h_full()
                                                    .pl(if tab_w < 40.0 { px(2.0) } else { px(10.0) })
                                                    .pr(if tab_w < 40.0 { px(2.0) } else { px(4.0) })
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .cursor_pointer()
                                                    .child(icons::icon(
                                                        icons::ICON_FILE_TEXT,
                                                        12.0,
                                                        if is_active { theme.accent } else { theme.text_muted },
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(if is_active {
                                                                gpui::FontWeight::SEMIBOLD
                                                            } else {
                                                                gpui::FontWeight::NORMAL
                                                            })
                                                            .text_color(if is_active {
                                                                theme.text_primary
                                                            } else {
                                                                theme.text_secondary
                                                            })
                                                            .truncate()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(title),
                                                    )
                                                    .when(is_active && is_active_dirty, |tab| {
                                                        tab.child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.accent)
                                                                .child("●"),
                                                        )
                                                    })
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        if this.state.active_note.as_ref().map(|n| &n.path != &tab_path_for_select).unwrap_or(true) {
                                                            this.select_note(tab_path_for_select.clone(), true, window, cx);
                                                        } else {
                                                            this.need_scroll_sidebar_to_active.set(true);
                                                            this.state.reveal_path_in_tree(&tab_path_for_select);
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        this.close_tab(&tab_path_for_middle_select, cx);
                                                    })),
                                            )
                                            // Tab close button (isolated sibling)
                                            .when(is_active || tab_w >= 56.0, |tab| {
                                                tab.child(
                                                    div()
                                                        .id(gpui::ElementId::from(("btn-close-tab", idx)))
                                                        .p_0p5()
                                                        .mr_1p5()
                                                        .flex_shrink_0()
                                                        .rounded_sm()
                                                        .cursor_pointer()
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                                                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                                            cx.stop_propagation();
                                                        })
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.close_tab(&tab_path_for_close, cx);
                                                        })),
                                                )
                                            }),
                                    );
                                }

                                let btn_new_tab = div()
                                    .id("btn-new-tab-inline")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(34.0))
                                    .w(px(32.0))
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .id("btn-new-tab-inline-inner")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(28.0))
                                            .h(px(28.0))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_PLUS, 15.0, theme.text_muted))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_empty_tab(window, cx);
                                            })),
                                    );

                                div()
                                    .id("tabs-scroll-viewport")
                                    .relative()
                                    .flex()
                                    .flex_row()
                                    .items_end()
                                    .h_full()
                                    .gap(px(6.0))
                                    .when(!is_overflowing, |d| d.flex_shrink_0())
                                    .when(is_overflowing, |d| d.w(px(available_tab_strip_w)).flex_shrink_0())
                                    .child(tabs_strip)
                                    .child(btn_new_tab)
                            })
                            // Drag Area (fills remaining space between tabs and window controls)
                            .child(
                                div()
                                    .id("titlebar-drag-region")
                                    .flex_1()
                                    .min_w(px(24.0))
                                    .h_full()
                                    .window_control_area(WindowControlArea::Drag),
                            )
                            // Unified right controls group permanently anchored to the right window edge
                            .child(
                                div()
                                    .id("titlebar-right-controls-group")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .h_full()
                                    .flex_shrink_0()
                                    // 1. Right action buttons ([v], [|])
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .h_full()
                                            .flex_shrink_0()
                                            .gap_1()
                                            .px_1()
                                            .child(self.render_tabs_dropdown_button(theme, cx))
                                            .child(self.render_right_sidebar_toggle_button(theme, cx)),
                                    )
                                    // 2. Native Windows Titlebar Control Buttons (Minimize, Maximize/Restore, Close)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .h_full()
                                            .flex_shrink_0()
                                            .child(
                                                div()
                                                    .id("win-ctrl-min")
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .w(px(46.0))
                                                    .h_full()
                                                    .cursor_pointer()
                                                    .window_control_area(WindowControlArea::Min)
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_MINIMIZE, 12.0, theme.text_secondary))
                                                    .on_click(cx.listener(|_, _, window, _| {
                                                        window.minimize_window();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("win-ctrl-max")
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .w(px(46.0))
                                                    .h_full()
                                                    .cursor_pointer()
                                                    .window_control_area(WindowControlArea::Max)
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(
                                                        if is_maximized { icons::ICON_RESTORE } else { icons::ICON_MAXIMIZE },
                                                        12.0,
                                                        theme.text_secondary,
                                                    ))
                                                    .on_click(cx.listener(|_, _, window, _| {
                                                        window.zoom_window();
                                                    })),
                                            )
                                            .child({
                                                let is_hovering_close = self.is_hovering_close;
                                                div()
                                                    .id("win-ctrl-close")
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .w(px(46.0))
                                                    .h_full()
                                                    .cursor_pointer()
                                                    .window_control_area(WindowControlArea::Close)
                                                    .hover(|h| h.bg(rgb(0xe81123)))
                                                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                                        if this.is_hovering_close != *hovered {
                                                            this.is_hovering_close = *hovered;
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .child(icons::icon(
                                                        icons::ICON_WINDOW_CLOSE,
                                                        12.0,
                                                        if is_hovering_close { rgb(0xffffff) } else { theme.text_secondary },
                                                    ))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.save_on_exit(window);
                                                        cx.quit();
                                                    }))
                                            }),
                                    ),
                            ),
                    ),
            )
            // ================= MAIN BODY =================
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_row()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    // ----- Left Action Rail / Ribbon (Obsidian-style) -----
                    .child(self.render_ribbon_rail(theme, cx))
                    // ----- Сайдбар файлов (Дерево каталогов) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_col()
                                .w(px((self.sidebar_width - 44.0).max(100.0)))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .pt_2()
                                .px_2()
                                .pb_0()
                                .gap_2()
                                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                    if this.is_resizing_sidebar {
                                        let x: f32 = event.position.x.into();
                                        if x < 100.0 {
                                            this.sidebar_collapsed = true;
                                        } else {
                                            this.sidebar_collapsed = false;
                                            this.sidebar_width = x.clamp(160.0, 600.0);
                                        }
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                    if this.is_resizing_sidebar {
                                        this.is_resizing_sidebar = false;
                                        cx.notify();
                                    }
                                }))
                            // Vault Header
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .truncate()
                                                    .child(self.state.workspace_name.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .id("btn-new-note")
                                                    .p_1()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_PLUS, 13.0, theme.text_primary))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.create_new_note(window, cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("btn-open-vault")
                                                    .p_1()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_primary))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.open_folder_dialog_deferred(window, cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Search Filter Bar + Tree (Files mode) or Search Panel
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Files,
                                |d| {
                                    d.child(self.render_quick_filter(window, cx))
                                        // Tree Scroll — виртуализированный список (см. views/sidebar/tree.rs)
                                        .child(self.render_sidebar_tree(
                                            active_path.as_ref(),
                                            theme,
                                            window,
                                            cx,
                                        ))
                                },
                            )
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Search,
                                |d| d.child(self.render_search_panel(theme, window, cx)),
                            )
                            // Sidebar Footer (Vault switcher button + Reload button)
                            .child(
                                div()
                                    .id("sidebar-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1p5()
                                    .border_t_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("btn-switch-vault")
                                            .flex()
                                            .flex_1()
                                            .min_w_0()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .px_1p5()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .child(self.state.workspace_name.clone()),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_vault_dialog(cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("btn-reload-vault")
                                            .p_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_muted))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reload_files(window, cx);
                                            })),
                                    ),
                            )
                        )
                    })
                    // ----- Sidebar Resizer Divider (1px border line + 7px transparent hitbox) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("sidebar-resizer")
                                .relative()
                                .flex_shrink_0()
                                .w(px(1.0))
                                .h_full()
                                .bg(resizer_color)
                                .when(is_resizer_active, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(px(-1.0))
                                            .w(px(3.0))
                                            .bg(theme.accent),
                                    )
                                })
                                .child(
                                    div()
                                        .id("sidebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            // A scrollbar drag may pass over this hitbox; the
                                            // divider must not light up as the active target.
                                            this.is_hovering_resizer =
                                                *hovered && !this.sidebar_scrollbar_drag.active;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            // One gesture at a time: the scrollbar already
                                            // owns the pointer, so the border must not steal it.
                                            if this.sidebar_scrollbar_drag.active {
                                                return;
                                            }
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // ----- Центральная область редактора -----
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .bg(theme.bg_editor)
                            // Панель навигации (Obsidian-style) сразу под вкладками
                            .child(self.render_navigation_bar(theme, cx))
                            .when_some(self.state.active_note.as_ref(), |this, note| {
                                let note_title = note
                                    .path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "Untitled".to_string());

                                let lines_arc = Arc::clone(&note.cached_lines);
                                let visual_rows_arc = Arc::clone(&note.cached_visual_rows);
                                let line_starts_arc = Arc::clone(&note.cached_line_starts);
                                let code_states_arc = Arc::clone(&note.cached_code_states);
                                let height_index_arc = Arc::clone(&note.cached_height_index);

                                let (cursor_line, cursor_col) = note.line_col();
                                let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows_arc, cursor_line, cursor_col)
                                    .unwrap_or((0, 0));

                                let selection_range = note.selection_range();
                                let mut active_block_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
                                if is_editing {
                                    if let Some(s) = code_states_arc.get(cursor_line).and_then(|s| s.as_ref()) {
                                        active_block_ids.insert(s.block_id);
                                    }
                                    if let Some(ref sel) = selection_range {
                                        let sel_start_line = match line_starts_arc.binary_search(&sel.start) {
                                            Ok(idx) => idx,
                                            Err(idx) => idx.saturating_sub(1),
                                        };
                                        let sel_end_line = match line_starts_arc.binary_search(&sel.end) {
                                            Ok(idx) => idx,
                                            Err(idx) => idx.saturating_sub(1),
                                        };
                                        for l_idx in sel_start_line..=sel_end_line.min(code_states_arc.len().saturating_sub(1)) {
                                            let l_start = line_starts_arc[l_idx];
                                            let l_end = l_start + lines_arc[l_idx].len();
                                            let is_in_sel = if sel.start == sel.end {
                                                false
                                            } else if l_start == l_end {
                                                sel.start <= l_start && sel.end >= l_end
                                            } else {
                                                sel.end > l_start && sel.start < l_end
                                            };
                                            if is_in_sel {
                                                if let Some(s) = code_states_arc.get(l_idx).and_then(|s| s.as_ref()) {
                                                    active_block_ids.insert(s.block_id);
                                                }
                                            }
                                        }
                                    }
                                }
                                let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
                                let text_start_x = editor_left_x + 32.0 + 8.0 + 40.0;

                                let theme_copy = theme.clone();
                                let theme_scrollbar = theme.clone();

                                let total_content_h = height_index_arc.total() + 48.0;
                                let (editor_h, max_scroll_content, track_top_y) = self.editor_scroll_metrics(total_content_h);
                                self.last_editor_h.set(editor_h);
                                let show_scrollbar = max_scroll_content > 1.0;
                                let (thumb_top, thumb_h) = if show_scrollbar {
                                    let ratio = (editor_h / total_content_h.max(editor_h)).clamp(0.04, 1.0);
                                    let th = (editor_h * ratio).max(28.0);
                                    let scroll_y = {
                                        let offset = self.editor_scroll_handle.offset();
                                        f32::from(-offset.y).clamp(0.0, max_scroll_content)
                                    };
                                    let scroll_fraction = (scroll_y / max_scroll_content).clamp(0.0, 1.0);
                                    let tt = scroll_fraction * (editor_h - th);
                                    (tt, th)
                                } else {
                                    (0.0, 28.0)
                                };
                                let is_scrollbar_dragging = self.is_dragging_scrollbar;
                                let scroll_y_current = {
                                    let offset = self.editor_scroll_handle.offset();
                                    f32::from(-offset.y).clamp(0.0, max_scroll_content)
                                };

                                let is_title_focused = self.title_focus_handle.is_focused(window);
                                let title_text = &self.title_edit_text;
                                let title_chars: Vec<char> = title_text.chars().collect();
                                let cursor_pos = self.title_cursor_pos.min(title_chars.len());
                                let selection_range = self.title_selection_range();

                                let run = gpui::TextRun {
                                    len: title_text.len(),
                                    font: gpui::font("Segoe UI").bold(),
                                    color: Default::default(),
                                    underline: Default::default(),
                                    strikethrough: None,
                                    background_color: None,
                                };
                                let shaped = window.text_system().shape_line(
                                    SharedString::from(title_text.clone()),
                                    px(28.0),
                                    &[run],
                                    None,
                                );

                                let cursor_x = if title_text.is_empty() {
                                    0.0
                                } else {
                                    let byte_index: usize = title_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                                    crate::snap::snap_x(
                                        f32::from(shaped.x_for_index(byte_index)),
                                        pixel_scale,
                                    )
                                };

                                let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
                                    if !title_text.is_empty() {
                                        let b1: usize = title_chars[..s_start.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                                        let b2: usize = title_chars[..s_end.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                                        // Same device-pixel snapping as the caret above: an
                                        // unsnapped antialiased edge shifts its blend ratio with
                                        // every sub-pixel move and reads as a stuttering edge.
                                        crate::snap::snapped_span(
                                            f32::from(shaped.x_for_index(b1)),
                                            f32::from(shaped.x_for_index(b2)),
                                            pixel_scale,
                                        )
                                    } else {
                                        (0.0, 0.0)
                                    }
                                } else {
                                    (0.0, 0.0)
                                };

                                let display_title = if title_text.is_empty() {
                                    if is_title_focused {
                                        String::new()
                                    } else {
                                        note_title.clone()
                                    }
                                } else {
                                    title_text.clone()
                                };

                                this.child(
                                    div()
                                        .id("note-title-panel")
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .items_start()
                                        .pl(px(48.0))
                                        .pr_4()
                                        .pt(px(2.0))
                                        .pb(px(16.0))
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .bg(theme.bg_editor)
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                            window.focus(&this.title_focus_handle);
                                            this.title_is_focused = true;
                                            this.reset_title_cursor_blink(cx);
                                            this.title_cursor_pos = this.title_edit_text.chars().count();
                                            this.title_selection_anchor = None;
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .id("note-title-input")
                                                .relative()
                                                .flex_1()
                                                .min_w_0()
                                                .h(px(36.0))
                                                .track_focus(&self.title_focus_handle)
                                                .cursor(gpui::CursorStyle::IBeam)
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    window.focus(&this.title_focus_handle);
                                                    this.title_is_focused = true;
                                                    this.reset_title_cursor_blink(cx);

                                                    let click_x: f32 = event.position.x.into();
                                                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                                    let title_left_x = editor_left_x + 48.0;
                                                    let rel_x = (click_x - title_left_x).max(0.0);

                                                    let clicked_pos = if this.title_edit_text.is_empty() {
                                                        0
                                                    } else {
                                                        let run = gpui::TextRun {
                                                            len: this.title_edit_text.len(),
                                                            font: gpui::font("Segoe UI").bold(),
                                                            color: Default::default(),
                                                            underline: Default::default(),
                                                            strikethrough: None,
                                                            background_color: None,
                                                        };
                                                        let shaped = window.text_system().shape_line(
                                                            SharedString::from(this.title_edit_text.clone()),
                                                            px(28.0),
                                                            &[run],
                                                            None,
                                                        );
                                                        let byte_idx = shaped.closest_index_for_x(px(rel_x));
                                                        this.title_edit_text[..byte_idx.min(this.title_edit_text.len())].chars().count()
                                                    };

                                                    if event.click_count == 2 {
                                                        this.title_selection_anchor = Some(0);
                                                        this.title_cursor_pos = this.title_edit_text.chars().count();
                                                        this.title_is_mouse_selecting = false;
                                                    } else if event.modifiers.shift {
                                                        if this.title_selection_anchor.is_none() {
                                                            this.title_selection_anchor = Some(this.title_cursor_pos);
                                                        }
                                                        this.title_cursor_pos = clicked_pos;
                                                        this.title_is_mouse_selecting = true;
                                                    } else {
                                                        this.title_cursor_pos = clicked_pos;
                                                        this.title_selection_anchor = Some(clicked_pos);
                                                        this.title_is_mouse_selecting = true;
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    this.handle_title_key(event, window, cx);
                                                }))
                                                .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left(px(sel_x1))
                                                            .top(px(4.0))
                                                            .w(px(sel_w))
                                                            .h(px(28.0))
                                                            .bg(theme.selection_bg)
                                                            .rounded_xs(),
                                                    )
                                                })
                                                .child(
                                                    div()
                                                        .font_family("Segoe UI")
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_size(px(28.0))
                                                        .line_height(px(36.0))
                                                        .text_color(rgb(0xf44336))
                                                        .child(display_title),
                                                )
                                                .when(title_text.is_empty() && is_title_focused, |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .h(px(36.0))
                                                            .font_family("Segoe UI")
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .text_size(px(28.0))
                                                            .line_height(px(36.0))
                                                            .text_color(rgba(0xf4433666))
                                                            .child("Untitled"),
                                                    )
                                                })
                                                .when(is_title_focused && self.title_blink.visible(), |s| {
                                                    s.child(
                                                        div()
                                                            .absolute()
                                                            .left(px(cursor_x))
                                                            .top(px(5.0))
                                                            .w(px(2.0))
                                                            .h(px(26.0))
                                                            .bg(rgb(0xf44336))
                                                            .rounded_xs(),
                                                    )
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("note-editor-container")
                                        .relative()
                                        .track_focus(&self.focus_handle)
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .w_full()
                                        .min_w_0()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .when(self.is_settings_open || !show_scrollbar, |d| {
                                            d.on_scroll_wheel(|_, _, cx| {
                                                cx.stop_propagation();
                                            })
                                        })
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                return;
                                            }
                                            this.commit_title_rename(window, cx);
                                            window.focus(&this.focus_handle);
                                            this.is_mouse_selecting = true;
                                            let click_x: f32 = event.position.x.into();
                                            let click_y: f32 = event.position.y.into();
                                            this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                                        }))
                                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                                            if this.is_resizing_sidebar {
                                                let x: f32 = event.position.x.into();
                                                if x < 100.0 {
                                                    this.sidebar_collapsed = true;
                                                } else {
                                                    this.sidebar_collapsed = false;
                                                    this.sidebar_width = x.clamp(160.0, 600.0);
                                                }
                                                cx.notify();
                                            } else if this.is_dragging_scrollbar {
                                                let y: f32 = event.position.y.into();
                                                this.handle_scrollbar_drag(y, window, cx);
                                            } else if this.is_mouse_selecting {
                                                let x: f32 = event.position.x.into();
                                                let y: f32 = event.position.y.into();
                                                let viewport_h: f32 = window.viewport_size().height.into();
                                                this.update_mouse_selection(x, y, viewport_h, window, cx);
                                            }
                                        }))
                                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_resizing_sidebar = false;
                                            this.is_dragging_scrollbar = false;
                                            this.sidebar_scrollbar_drag.end();
                                            this.is_mouse_selecting = false;
                                            if let Some(ref mut n) = this.state.active_note {
                                                if n.selection_anchor == Some(n.cursor_offset) {
                                                    n.selection_anchor = None;
                                                }
                                            }
                                            cx.notify();
                                        }))
                                        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            this.is_resizing_sidebar = false;
                                            this.is_dragging_scrollbar = false;
                                            this.sidebar_scrollbar_drag.end();
                                            this.is_mouse_selecting = false;
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                            this.handle_key(event, window, cx);
                                        }))
                                        .child({
                                            let theme = &theme_copy;
                                            let is_editing = self.edit_mode;
                                            let is_editor_focused = self.editor_is_focused;
                                            let selection_range = self.state.active_note.as_ref().and_then(|n| n.selection_range());
                                            let (cursor_line, _) = self.state.active_note.as_ref().map(|n| n.line_col()).unwrap_or((0, 0));
                                            let headings = &self.config.editor.headings;
                                            let editor_font = self.editor_font_family();
                                            let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, headings, line_char_width);
                                            // Row geometry is derived from shaped text; keep an owned
                                            // handle so the row closures below can shape without
                                            // borrowing the window again.
                                            let text_system = window.text_system().clone();
                                            let hide_current_line_on_selection = self.config.editor.hide_current_line_on_selection;
                                            let has_selection = selection_range.as_ref().map_or(false, |sel| sel.start < sel.end);

                                            let (visible_range, top_spacer_h, bottom_spacer_h) = if !show_scrollbar {
                                                if self.current_scroll_top() > 0.0 {
                                                    self.set_editor_scroll_top(0.0);
                                                }
                                                (0..visual_rows_arc.len(), 0.0, 0.0)
                                            } else {
                                                let raw_scroll_y = self.current_scroll_top();
                                                let scroll_y = raw_scroll_y.clamp(0.0, max_scroll_content);
                                                if (raw_scroll_y - scroll_y).abs() > 0.5 {
                                                    self.set_editor_scroll_top(scroll_y);
                                                }
                                                let vr = height_index_arc.visible_range(scroll_y, editor_h, 300.0);
                                                let top_h = height_index_arc.top_of(vr.start);
                                                let rendered_bottom = height_index_arc.top_of(vr.end);
                                                let bot_h = (height_index_arc.total() - rendered_bottom).max(0.0);
                                                (vr, top_h, bot_h)
                                            };

                                            crate::perf::PERF.shape_calls.fetch_add(visible_range.len() as u64, std::sync::atomic::Ordering::Relaxed);
                                            let mut items = Vec::with_capacity(visible_range.len() + 2);

                                            if top_spacer_h > 0.0 {
                                                items.push(
                                                    div()
                                                        .id("top-spacer")
                                                        .w_full()
                                                        .h(px(top_spacer_h))
                                                        .flex_shrink_0()
                                                        .cursor(gpui::CursorStyle::IBeam)
                                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                            cx.stop_propagation();
                                                            this.commit_title_rename(window, cx);
                                                            window.focus(&this.focus_handle);
                                                            this.is_mouse_selecting = true;
                                                            let click_x: f32 = event.position.x.into();
                                                            let click_y: f32 = event.position.y.into();
                                                            this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                                                        }))
                                                );
                                            }

                                            let calc_v_row_sel_range = |target_v_idx: usize| -> Option<(usize, usize)> {
                                                let sel = selection_range.as_ref()?;
                                                if sel.start >= sel.end {
                                                    return None;
                                                }
                                                let v = visual_rows_arc.get(target_v_idx)?;
                                                let line = &lines_arc[v.line_idx];
                                                let line_start = line_starts_arc[v.line_idx];
                                                let line_end = line_start + line.len();

                                                if line.is_empty() {
                                                    if sel.start <= line_start && sel.end > line_start {
                                                        return Some((0, 1));
                                                    } else {
                                                        return None;
                                                    }
                                                }

                                                let is_line_in_sel = if sel.start == sel.end {
                                                    false
                                                } else if line_start == line_end {
                                                    sel.start <= line_start && sel.end > line_start
                                                } else {
                                                    sel.end > line_start && sel.start < line_end
                                                };
                                                let is_in_act_block = code_states_arc[v.line_idx]
                                                    .as_ref()
                                                    .map(|s| active_block_ids.contains(&s.block_id))
                                                    .unwrap_or(false);
                                                let is_act_line = is_editing && (v.line_idx == cursor_line || is_line_in_sel || is_in_act_block);
                                                let code_state = code_states_arc[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                                                let classified = crate::markdown::classify_line(line, code_state);
                                                let (_, row_char_w, _, _, _) =
                                                    Self::heading_typography_metrics_with_widths(&classified, headings, 24.0, line_char_width, Some(&heading_widths));
                                                // Selection columns come from the shaped row, i.e. the
                                                // same model the mouse handlers hit-test with.
                                                let row_layout = shape_row(&text_system, line, &classified, v, is_act_line, headings, &editor_font, row_char_w);

                                                let mut byte_s = sel.start.saturating_sub(line_start).min(line.len());
                                                let mut byte_e = sel.end.saturating_sub(line_start).min(line.len());
                                                while byte_s > 0 && !line.is_char_boundary(byte_s) {
                                                    byte_s -= 1;
                                                }
                                                while byte_e > 0 && !line.is_char_boundary(byte_e) {
                                                    byte_e -= 1;
                                                }
                                                let char_s = line[..byte_s].chars().count();
                                                let char_e = char_s + line[byte_s..byte_e].chars().count();

                                                let includes_newline = v.is_last_sub && sel.end > line_end && sel.start <= line_end;

                                                if byte_s < byte_e {
                                                    let intersect_s = char_s.max(v.char_start);
                                                    let intersect_e = char_e.min(v.char_end);
                                                    if intersect_s <= intersect_e {
                                                        let vis_s = row_layout.column_for_line_char(intersect_s);
                                                        let mut vis_e = row_layout.column_for_line_char(intersect_e);
                                                        if includes_newline {
                                                            vis_e += 1;
                                                        }
                                                        if vis_s < vis_e {
                                                            Some((vis_s, vis_e))
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
                                                    }
                                                } else if includes_newline && char_s == v.char_end {
                                                    let vis_s = row_layout.column_for_line_char(v.char_end);
                                                    Some((vis_s, vis_s + 1))
                                                } else {
                                                    None
                                                }
                                            };

                                            for v_idx in visible_range {
                                                let v = &visual_rows_arc[v_idx];
                                                let line = &lines_arc[v.line_idx];
                                                let line_start = line_starts_arc[v.line_idx];
                                                let line_end = line_start + line.len();
                                                let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
                                                    if sel.start == sel.end {
                                                        false
                                                    } else if line_start == line_end {
                                                        sel.start <= line_start && sel.end > line_start
                                                    } else {
                                                        sel.end > line_start && sel.start < line_end
                                                    }
                                                });
                                                let is_current = is_editing && (v_idx == active_v_idx);
                                                let show_current_line_highlight = is_current && !(hide_current_line_on_selection && has_selection);
                                                let is_in_active_block = code_states_arc[v.line_idx]
                                                    .as_ref()
                                                    .map(|s| active_block_ids.contains(&s.block_id))
                                                    .unwrap_or(false);
                                                let is_active_line = is_editing && (v.line_idx == cursor_line || is_line_in_selection || is_in_active_block);
                                                let code_state = code_states_arc[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                                                let classified = crate::markdown::classify_line(line, code_state);
                                                let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                                                let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                                                let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                                                let row_len = v.char_end.saturating_sub(v.char_start);

                                                let (row_h, row_char_w, _row_font_size, _cursor_h, _cursor_top) =
                                                    Self::heading_typography_metrics_with_widths(&classified, headings, 24.0, line_char_width, Some(&heading_widths));
                                                // One shaped layout per row drives the selection
                                                // rectangle, the caret and the row's own mouse handlers.
                                                let row_layout = shape_row(&text_system, line, &classified, v, is_active_line, headings, &editor_font, row_char_w);
                                                let row_layout_down = row_layout.clone();
                                                let row_layout_move = row_layout.clone();

                                                let line_number_text = if v.sub_idx == 0 {
                                                    format!("{:>3} ", v.line_idx + 1)
                                                } else {
                                                    "    ".to_string()
                                                };

                                                let row_sel_chars = calc_v_row_sel_range(v_idx);

                                                // Rectangles the row paints behind its text. Both the chip and
                                                // the selection are painted here, before the text, so the glyphs
                                                // stay on top of them: gpui paints a text-run background from
                                                // inside the text element, i.e. above everything the row draws
                                                // itself, and a chip drawn that way would cover the selection and
                                                // hide the very thing the user is selecting. The chip comes first,
                                                // so the selection is drawn over it.
                                                let chip_inset = code_chip_inset(row_h);
                                                let mut backdrop_rects = Vec::new();
                                                // The row's own current-line highlight is painted by the row
                                                // element, i.e. *under* the code block's background, so inside a
                                                // block it would only show in the line-number gutter. Repaint it
                                                // here, first, so it covers the block and still sits under the
                                                // chips and the selection.
                                                if show_current_line_highlight {
                                                    backdrop_rects.push(
                                                        div()
                                                            .absolute()
                                                            .left(px(0.0))
                                                            .top_0()
                                                            .h(px(row_h))
                                                            .w_full()
                                                            .bg(theme.bg_hover),
                                                    );
                                                }
                                                for &(code_start, code_end) in row_layout.code_ranges() {
                                                    // The chip is a hair shorter than its row and rounded, so that
                                                    // it reads as a chip rather than as a band across the line.
                                                    let (chip_x, chip_w) = row_layout.selection_rect(code_start, code_end, pixel_scale);
                                                    backdrop_rects.push(
                                                        div()
                                                            .absolute()
                                                            .left(px(chip_x))
                                                            .top(px(chip_inset))
                                                            .h(px((row_h - 2.0 * chip_inset).max(1.0)))
                                                            .w(px(chip_w))
                                                            .bg(theme.code_bg)
                                                            .rounded_sm(),
                                                    );
                                                }
                                                if let Some((sel_s, sel_e)) = row_sel_chars {
                                                    let sel_above = if v_idx > 0 { calc_v_row_sel_range(v_idx - 1) } else { None };
                                                    let sel_below = if v_idx + 1 < visual_rows_arc.len() { calc_v_row_sel_range(v_idx + 1) } else { None };

                                                    let round_tl = sel_above.map_or(true, |(as_s, as_e)| sel_s < as_s || sel_s >= as_e);
                                                    let round_tr = sel_above.map_or(true, |(as_s, as_e)| sel_e > as_e || sel_e <= as_s);
                                                    let round_bl = sel_below.map_or(true, |(bs_s, bs_e)| sel_s < bs_s || sel_s >= bs_e);
                                                    let round_br = sel_below.map_or(true, |(bs_s, bs_e)| sel_e > bs_e || sel_e <= bs_s);

                                                    let (sel_x, sel_w) = row_layout.selection_rect(sel_s, sel_e, pixel_scale);
                                                    let mut sel_rect = div()
                                                        .absolute()
                                                        .left(px(sel_x))
                                                        .top_0()
                                                        .h(px(row_h))
                                                        .w(px(sel_w))
                                                        .bg(theme.selection_bg);

                                                    if round_tl {
                                                        sel_rect = sel_rect.rounded_tl_xs();
                                                    }
                                                    if round_tr {
                                                        sel_rect = sel_rect.rounded_tr_xs();
                                                    }
                                                    if round_bl {
                                                        sel_rect = sel_rect.rounded_bl_xs();
                                                    }
                                                    if round_br {
                                                        sel_rect = sel_rect.rounded_br_xs();
                                                    }

                                                    backdrop_rects.push(sel_rect);
                                                }

                                                let v_char_start = v.char_start;
                                                let v_char_end = v.char_end;
                                                let v_line_idx = v.line_idx;
                                                let v_sub_idx = v.sub_idx;

                                                // A collapsed code block paints its language label in place of
                                                // the opening fence, so the copy button belongs to that row. The
                                                // payload is the block's body: the `InsideCode` lines that follow
                                                // the fence, up to the closing fence (exclusive).
                                                let copy_payload: Option<(usize, String)> =
                                                    if is_opening_fence && !is_in_active_block {
                                                        code_states_arc[v.line_idx].as_ref().map(|st| {
                                                            let mut body = String::new();
                                                            let mut i = v.line_idx + 1;
                                                            while let Some(next) = code_states_arc.get(i).and_then(|s| s.as_ref()) {
                                                                if next.block_id != st.block_id
                                                                    || next.kind != crate::markdown::CodeBlockKind::InsideCode
                                                                {
                                                                    break;
                                                                }
                                                                body.push_str(&lines_arc[i]);
                                                                body.push('\n');
                                                                i += 1;
                                                            }
                                                            (st.block_id, body)
                                                        })
                                                    } else {
                                                        None
                                                    };

                                                // Trigger custom code block processor if registered by a plugin and not yet cached
                                                if is_opening_fence {
                                                    if let Some(st) = code_states_arc[v.line_idx].as_ref() {
                                                        if let Some(lang) = &st.lang {
                                                            if let Some(owner_plugin) = self.state.code_block_processors.get_plugin_for_lang(lang) {
                                                                if !self.code_block_views.contains_key(&st.block_id) {
                                                                    let mut body = String::new();
                                                                    let mut i = v.line_idx + 1;
                                                                    while let Some(next) = code_states_arc.get(i).and_then(|s| s.as_ref()) {
                                                                        if next.block_id != st.block_id || next.kind != crate::markdown::CodeBlockKind::InsideCode {
                                                                            break;
                                                                        }
                                                                        body.push_str(&lines_arc[i]);
                                                                        body.push('\n');
                                                                        i += 1;
                                                                    }
                                                                    let _ = self.state.plugin_host.send_event(
                                                                        &owner_plugin,
                                                                        crate::plugins::api::HostEvent::ProcessCodeBlock {
                                                                            language: lang.clone(),
                                                                            source: body,
                                                                            block_id: st.block_id,
                                                                        },
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                let (custom_plugin_id, custom_ui_node, is_first_code_line) = match &code_states_arc[v.line_idx] {
                                                    Some(st) => {
                                                        let view = self.code_block_views.get(&st.block_id);
                                                        let pid = view.as_ref().map(|(p, _)| p.as_str());
                                                        let tree = view.as_ref().map(|(_, t)| t);
                                                        let first_line = v.line_idx > 0 && code_states_arc[v.line_idx - 1].as_ref().map(|s| s.kind) == Some(crate::markdown::CodeBlockKind::OpeningFence);
                                                        (pid, tree, first_line)
                                                    }
                                                    None => (None, None, false),
                                                };

                                                items.push(
                                                    div()
                                                        .id(gpui::ElementId::from(("v-row", v_idx)))
                                                        .w_full()
                                                        .flex_shrink_0()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .h(px(row_h))
                                                        .py_0()
                                                        .px_2()
                                                        .rounded_sm()
                                                        .cursor(gpui::CursorStyle::IBeam)
                                                        .when(show_current_line_highlight, |s| s.bg(theme.bg_hover))
                                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                            cx.stop_propagation();
                                                            if this.is_resizing_sidebar {
                                                                return;
                                                            }
                                                            this.is_mouse_selecting = true;
                                                            let click_x: f32 = event.position.x.into();

                                                            let col_in_row = if click_x > text_start_x {
                                                                let column = row_layout_down.column_for_x(click_x - text_start_x);
                                                                row_layout_down.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                                                            } else {
                                                                0
                                                            };
                                                            let target_char = v_char_start + col_in_row;
                                                            if let Some(ref mut n) = this.state.active_note {
                                                                let offset = n.offset_for_line_col(v_line_idx, target_char);
                                                                if event.click_count == 2 {
                                                                    let (w_start, w_end) = n.word_range_at(offset);
                                                                    n.selection_anchor = Some(w_start);
                                                                    n.cursor_offset = w_end;
                                                                } else if event.click_count >= 3 {
                                                                    let (l_start, l_end) = n.line_range_at(offset);
                                                                    n.selection_anchor = Some(l_start);
                                                                    n.cursor_offset = l_end;
                                                                } else if event.modifiers.shift {
                                                                    if n.selection_anchor.is_none() {
                                                                        n.selection_anchor = Some(n.cursor_offset);
                                                                    }
                                                                    n.cursor_offset = offset;
                                                                } else {
                                                                    // Single click anywhere on row places cursor and CLEARS existing selection
                                                                    n.cursor_offset = offset;
                                                                    n.selection_anchor = Some(offset);
                                                                }
                                                                n.sync_cursor();
                                                            }
                                                            window.focus(&this.focus_handle);
                                                            this.reset_editor_cursor_blink(cx);
                                                            cx.notify();
                                                        }))
                                                        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                                                            if this.is_mouse_selecting {
                                                                cx.stop_propagation();
                                                                let click_x: f32 = event.position.x.into();
                                                                let col_in_row = if click_x > text_start_x {
                                                                    let column = row_layout_move.column_for_x(click_x - text_start_x);
                                                                    row_layout_move.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                                                                } else {
                                                                    0
                                                                };
                                                                let target_char = v_char_start + col_in_row;
                                                                if let Some(ref mut n) = this.state.active_note {
                                                                    let new_offset = n.offset_for_line_col(v_line_idx, target_char);
                                                                    if n.selection_anchor.is_none() {
                                                                        n.selection_anchor = Some(n.cursor_offset);
                                                                    }
                                                                    if n.cursor_offset != new_offset {
                                                                        n.cursor_offset = new_offset;
                                                                        n.sync_cursor();
                                                                        cx.notify();
                                                                    }
                                                                }
                                                            }
                                                        }))
                                                        .child(
                                                            div()
                                                                .id(gpui::ElementId::from(("line-num", v_idx)))
                                                                .w(px(40.0))
                                                                .h(px(row_h))
                                                                .flex()
                                                                .items_center()
                                                                .text_xs()
                                                                .text_color(if is_current { theme.accent } else { theme.text_muted })
                                                                .font_family(editor_font.clone())
                                                                .child(line_number_text),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(gpui::ElementId::from(("line-content", v_idx)))
                                                                .relative()
                                                                .flex()
                                                                .flex_row()
                                                                .items_center()
                                                                .flex_1()
                                                                .h(px(row_h))
                                                                .py_0()
                                                                .font_family(editor_font.clone())
                                                                .text_sm()
                                                                // gpui paints a `TextRun` background at the text
                                                                // element's line height, and the default (1.618 x
                                                                // font size) is taller than a heading row
                                                                // (1.35 x font size), so an inline-code chip would
                                                                // spill into the rows above and below. Pin the line
                                                                // height to the row so the chip fits the line; the
                                                                // glyph baseline does not move, because the text
                                                                // element stays centred in the row either way.
                                                                .line_height(px(row_h))
                                                                .cursor(gpui::CursorStyle::IBeam)
                                                                .when(is_opening_fence, |s| s.bg(theme.code_bg).rounded_t_md())
                                                                .when(is_closing_fence, |s| s.bg(theme.code_bg).rounded_b_md())
                                                                .when(is_code_line, |s| s.bg(theme.code_bg))
                                                                .children(backdrop_rects)
                                                                .child(
                                                                    div()
                                                                        .h_full()
                                                                        .flex()
                                                                        .flex_row()
                                                                        .items_center()
                                                                        .child(render_live_styled_row(
                                                                            line,
                                                                            &classified,
                                                                            v_char_start,
                                                                            v_char_end,
                                                                            v_sub_idx,
                                                                            is_active_line,
                                                                            is_in_active_block,
                                                                            headings,
                                                                            theme,
                                                                            &editor_font,
                                                                            custom_ui_node,
                                                                            custom_plugin_id,
                                                                            is_first_code_line,
                                                                            cx,
                                                                        )),
                                                                )
                                                                .when(is_current && is_editor_focused && self.editor_blink.visible(), |s| {
                                                                    let cursor_col = row_layout.column_for_line_char(v_char_start + active_col_in_row);
                                                                    let cursor_x = crate::snap::snap_x(
                                                                        row_layout.x_for_column(cursor_col),
                                                                        pixel_scale,
                                                                    );
                                                                    s.child(
                                                                        div()
                                                                            .absolute()
                                                                            .left(px(cursor_x))
                                                                            .top_0()
                                                                            .h_full()
                                                                            .w(px(2.0))
                                                                            .bg(theme.accent)
                                                                            .rounded_xs(),
                                                                    )
                                                                })
                                                                // Obsidian-style copy affordance: it sits at the right end
                                                                // of the block header and copies the block body to the
                                                                // clipboard, flashing a check mark for a moment.
                                                                .when(copy_payload.is_some(), |s| {
                                                                    let (block_id, body) = copy_payload.clone().unwrap();
                                                                    let just_copied = self.copied_code_block == Some(block_id);
                                                                    s.child(
                                                                        div()
                                                                            .id(gpui::ElementId::from(("code-copy", v_idx)))
                                                                            .absolute()
                                                                            .right(px(6.0))
                                                                            .top_0()
                                                                            .h(px(row_h))
                                                                            .flex()
                                                                            .items_center()
                                                                            .px(px(4.0))
                                                                            .rounded_sm()
                                                                            .cursor(gpui::CursorStyle::PointingHand)
                                                                            .hover(|h| h.bg(theme.bg_hover))
                                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                                                                // Without this the row would also move the caret
                                                                                // into the block.
                                                                                cx.stop_propagation();
                                                                                cx.write_to_clipboard(ClipboardItem::new_string(body.clone()));
                                                                                this.copied_code_block = Some(block_id);
                                                                                cx.notify();

                                                                                let entity = cx.weak_entity();
                                                                                let mut async_cx = cx.to_async();
                                                                                let timer_executor = cx.background_executor().clone();
                                                                                cx.foreground_executor()
                                                                                    .spawn(async move {
                                                                                        timer_executor
                                                                                            .timer(std::time::Duration::from_millis(2000))
                                                                                            .await;
                                                                                        entity
                                                                                            .update(&mut async_cx, |this, cx| {
                                                                                                if this.copied_code_block == Some(block_id) {
                                                                                                    this.copied_code_block = None;
                                                                                                    cx.notify();
                                                                                                }
                                                                                            })
                                                                                            .ok();
                                                                                    })
                                                                                    .detach();
                                                                            }))
                                                                            .child(icons::icon(
                                                                                if just_copied { icons::ICON_CHECK } else { icons::ICON_COPY },
                                                                                14.0,
                                                                                if just_copied { theme.accent } else { theme.text_muted },
                                                                            )),
                                                                    )
                                                                }),
                                                        ),
                                                );
                                            }

                                            if bottom_spacer_h > 0.0 {
                                                items.push(
                                                    div()
                                                        .id("bottom-spacer")
                                                        .w_full()
                                                        .h(px(bottom_spacer_h))
                                                        .flex_shrink_0()
                                                        .cursor(gpui::CursorStyle::IBeam)
                                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                            cx.stop_propagation();
                                                            this.commit_title_rename(window, cx);
                                                            window.focus(&this.focus_handle);
                                                            this.is_mouse_selecting = true;
                                                            let click_x: f32 = event.position.x.into();
                                                            let click_y: f32 = event.position.y.into();
                                                            this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                                                        }))
                                                );
                                            }

                                            div()
                                                .id("note-editor-list")
                                                .track_scroll(&self.editor_scroll_handle)
                                                .flex()
                                                .flex_col()
                                                .when(show_scrollbar, |d| d.overflow_y_scroll())
                                                .size_full()
                                                .px_8()
                                                .pt_0()
                                                .pb(px(48.0))
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    this.commit_title_rename(window, cx);
                                                    window.focus(&this.focus_handle);
                                                    this.is_mouse_selecting = true;
                                                    let click_x: f32 = event.position.x.into();
                                                    let click_y: f32 = event.position.y.into();
                                                    this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                                                }))
                                                .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                                                    let delta_y = match event.delta {
                                                        gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                        gpui::ScrollDelta::Lines(l) => l.y,
                                                    };
                                                    if !show_scrollbar {
                                                        cx.stop_propagation();
                                                        return;
                                                    }
                                                    let current_scroll = this.current_scroll_top();
                                                    if delta_y < 0.0 {
                                                        if current_scroll >= max_scroll_content - 1.0 {
                                                            this.set_editor_scroll_top(max_scroll_content);
                                                            cx.stop_propagation();
                                                            return;
                                                        }
                                                        let line_h = 24.0;
                                                        let px_delta_y = match event.delta {
                                                            gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                            gpui::ScrollDelta::Lines(l) => l.y * line_h,
                                                        };
                                                        if current_scroll - px_delta_y >= max_scroll_content {
                                                            this.set_editor_scroll_top(max_scroll_content);
                                                            cx.stop_propagation();
                                                            cx.notify();
                                                            return;
                                                        }
                                                    } else if delta_y > 0.0 {
                                                        if current_scroll <= 1.0 {
                                                            this.set_editor_scroll_top(0.0);
                                                            cx.stop_propagation();
                                                            return;
                                                        }
                                                        let line_h = 24.0;
                                                        let px_delta_y = match event.delta {
                                                            gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                            gpui::ScrollDelta::Lines(l) => l.y * line_h,
                                                        };
                                                        if current_scroll - px_delta_y <= 0.0 {
                                                            this.set_editor_scroll_top(0.0);
                                                            cx.stop_propagation();
                                                            cx.notify();
                                                            return;
                                                        }
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .id("note-editor-scroll-content")
                                                        .w_full()
                                                        .flex()
                                                        .flex_col()
                                                        .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                                                            let delta_y = match event.delta {
                                                                gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                                gpui::ScrollDelta::Lines(l) => l.y,
                                                            };
                                                            if !show_scrollbar {
                                                                cx.stop_propagation();
                                                                return;
                                                            }
                                                            let current_scroll = this.current_scroll_top();
                                                            if delta_y < 0.0 {
                                                                if current_scroll >= max_scroll_content - 1.0 {
                                                                    this.set_editor_scroll_top(max_scroll_content);
                                                                    cx.stop_propagation();
                                                                    return;
                                                                }
                                                                let line_h = 24.0;
                                                                let px_delta_y = match event.delta {
                                                                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                                                                };
                                                                if current_scroll - px_delta_y >= max_scroll_content {
                                                                    this.set_editor_scroll_top(max_scroll_content);
                                                                    cx.stop_propagation();
                                                                    cx.notify();
                                                                    return;
                                                                }
                                                            } else if delta_y > 0.0 {
                                                                if current_scroll <= 1.0 {
                                                                    this.set_editor_scroll_top(0.0);
                                                                    cx.stop_propagation();
                                                                    return;
                                                                }
                                                                let line_h = 24.0;
                                                                let px_delta_y = match event.delta {
                                                                    gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                                                                    gpui::ScrollDelta::Lines(l) => l.y * line_h,
                                                                };
                                                                if current_scroll - px_delta_y <= 0.0 {
                                                                    this.set_editor_scroll_top(0.0);
                                                                    cx.stop_propagation();
                                                                    cx.notify();
                                                                    return;
                                                                }
                                                            }
                                                        }))
                                                        .children(items)
                                                )
                                        })
                                        .when(show_scrollbar, |c| {
                                            c.child(
                                                div()
                                                    .id("editor-scrollbar-track")
                                                    .absolute()
                                                    .top_0()
                                                    .bottom_0()
                                                    .right_0()
                                                    .w(px(14.0))
                                                    .h_full()
                                                    .cursor_pointer()
                                                    .hover(|h| h.bg(rgba(0x00000015)))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let click_y: f32 = event.position.y.into();
                                                        let click_rel_y = (click_y - track_top_y).max(0.0);
                                                        let target_fraction = (click_rel_y / editor_h).clamp(0.0, 1.0);
                                                        let target_scroll_y = target_fraction * max_scroll_content;
                                                        this.set_editor_scroll_top(target_scroll_y);
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        div()
                                                            .id("editor-scrollbar-thumb")
                                                            .absolute()
                                                            .top(px(thumb_top))
                                                            .right(px(3.0))
                                                            .w(px(if is_scrollbar_dragging { 8.0 } else { 6.0 }))
                                                            .h(px(thumb_h))
                                                            .rounded_full()
                                                            .bg(if is_scrollbar_dragging {
                                                                theme_scrollbar.accent
                                                            } else {
                                                                theme_scrollbar.border
                                                            })
                                                            .hover(|h| h.bg(theme_scrollbar.text_secondary).w(px(8.0)))
                                                            .cursor_pointer()
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                                                cx.stop_propagation();
                                                                this.is_dragging_scrollbar = true;
                                                                this.scrollbar_drag_start_y = event.position.y.into();
                                                                this.scrollbar_drag_start_scroll_y = scroll_y_current;
                                                                cx.notify();
                                                            }))
                                                    )
                                            )
                                        })
                                )
                            })
                            .when(self.state.active_empty_tab.is_some(), |this| {
                                this.child(self.render_new_tab_page(theme, cx))
                            })
                            .when(
                                self.state.active_note.is_none()
                                    && self.state.active_empty_tab.is_none(),
                                |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .size_full()
                                        .justify_center()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_3xl()
                                                .child("🦅"),
                                        )
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child("Welcome to Vibecodium"),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.text_muted)
                                                .child("Select a note from the sidebar or click 'Open Folder' to choose a workspace."),
                                        ),
                                )
                            }),
                    )
                    // ----- Right Sidebar Resizer & Right Sidebar -----
                    .when(!self.right_sidebar_collapsed, |s| {
                        s.child(self.render_right_resizer(theme, cx))
                            .child(self.render_right_sidebar(theme, cx))
                    }),
            )
            // ================= FOOTER / STATUS BAR =================
            .child(
                div()
                    .id("app-status-bar")
                    .flex()
                    .flex_shrink_0()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(28.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.state.status_text.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap_4()
                            .when_some(self.state.active_note.as_ref(), |s, note| {
                                let (line, col) = note.line_col();
                                s.child(format!("Ln {}, Col {}", line + 1, col + 1))
                            })
                            .children(self.state.status_bar.all().into_iter().map(|item| {
                                div()
                                    .id(gpui::SharedString::from(item.id.clone()))
                                    .child(item.text)
                            }))
                            .child("120 FPS"),
                    ),
            )
            // ================= TABS POPUP (Obsidian-style switcher) =================
            .when_some(self.tabs_popup.clone(), |d, popup| {
                d.child(
                    gpui::deferred(
                        div()
                            .id("tabs-popup-anchor")
                            .absolute()
                            // Below the tab strip, with a small gap: the "+" and the
                            // chevron that opens this popup sit at the right end of that
                            // row, so the panel hangs from under them instead of
                            // covering them. Same corner the old tabs dropdown used.
                            .top(px(
                                crate::views::tab_list::WINDOW_ANCHOR_TOP
                                    + crate::views::tab_list::POPUP_GAP,
                            ))
                            .right(px(crate::views::tab_list::POPUP_RIGHT_OFFSET))
                            .child(popup),
                    )
                    .with_priority(999),
                )
            })
            // ================= COMMAND PALETTE MODAL =================
            .when(self.command_palette.is_open, |d| {
                d.child(self.render_command_palette(theme, window, cx))
            })
            // ================= SETTINGS MODAL =================
            .when(self.is_settings_open, |d| {
                d.child(self.render_settings_modal(cx))
            })
            // ================= FOLDER CONTEXT MENU =================
            .when(self.folder_context_menu.is_open, |d| {
                d.child(self.render_folder_context_menu(theme, window, cx))
            })
            // ================= FILE CONTEXT MENU =================
            .when(self.file_context_menu.is_open, |d| {
                d.child(self.render_file_context_menu(theme, window, cx))
            })
            // ================= TAB CONTEXT MENU (Obsidian-style) =================
            .when(self.tab_context_menu.is_open, |d| {
                d.child(self.render_tab_context_menu(theme, window, cx))
            })
            // ================= RIBBON CONTEXT MENU (Obsidian-style) =================
            .when(self.ribbon_context_menu.is_open, |d| {
                d.child(self.render_ribbon_context_menu(theme, window, cx))
            })
            // ================= GO TO FILE DIALOG =================
            .when(self.search_dialog.is_open, |d| {
                d.child(self.render_search_dialog(theme, window, cx))
            })
            // ================= FOLDER MODALS =================
            .when(self.folder_modal != crate::views::sidebar::FolderModal::None, |d| {
                d.child(self.render_folder_modals(theme, window, cx))
            })
            // ================= VAULT SWITCHER DIALOG =================
            .when(self.vault_dialog_open, |d| {
                d.child(self.render_vault_dialog(theme, window, cx))
            })
            // ================= COMMUNITY PLUGINS CATALOG =================
            .when(self.plugin_catalog.is_open, |d| {
                d.child(self.render_plugin_catalog_modal(cx))
            })
    }
}

/// Weight inline code is painted at.
///
/// Roboto Mono is bundled here in Regular and Bold only, and a request for a weight in
/// between resolves to whichever face the platform deems closest — so the one step up
/// that is guaranteed to show is Bold. `markdown::PaintedSegment::code` carries the same
/// trait into the row's shaped layout, so geometry and paint stay in parity.
const INLINE_CODE_WEIGHT: gpui::FontWeight = gpui::FontWeight::BOLD;

/// Fraction of the row height left as a gap above and below the inline-code chip, i.e.
/// the chip is `1 - 2 * CODE_CHIP_INSET_RATIO` of the row. Scaled with the row so that a
/// heading's chip keeps the proportions of a paragraph's.
const CODE_CHIP_INSET_RATIO: f32 = 1.0 / 12.0;

/// Weight the inline-code face takes in a row whose base weight is `base`: the heavier
/// of the two, so that code is never lighter than the text around it and a bold heading
/// keeps its bold.
fn inline_code_weight(base: gpui::FontWeight) -> gpui::FontWeight {
    if base.0 > INLINE_CODE_WEIGHT.0 { base } else { INLINE_CODE_WEIGHT }
}

/// Vertical gap above and below an inline-code chip, in logical px, for a row `row_h`
/// tall. At least one pixel, so that the chip is always visibly shorter than its row.
fn code_chip_inset(row_h: f32) -> f32 {
    (row_h * CODE_CHIP_INSET_RATIO).round().max(1.0)
}

/// Colour a syntax marker is painted with, or `None` when it is not painted at all on
/// this line.
///
/// Every marker except an inline-code delimiter only shows on the active line. A code
/// delimiter is painted even off the active line so that the code chip keeps a constant
/// width — and with it the selection rectangle, which otherwise jumps by two cells the
/// moment the line becomes active. Off the active line the delimiter carries no ink at
/// all, which is exactly what reads as the chip's left/right padding: the chip itself is
/// a rectangle the editor paints behind the row's text, not a text-run background, so a
/// blank delimiter is enough to open the padding. `markdown::row_painted_text`
/// must agree: it keeps the delimiter's cell on an idle line for the same reason.
fn marker_paint(m: &str, is_active_line: bool, theme: &Theme) -> Option<gpui::Hsla> {
    let is_code_delim = crate::markdown::is_inline_code_delim(m);
    if !is_active_line && !is_code_delim {
        return None;
    }
    if is_active_line {
        Some(theme.text_muted.into())
    } else {
        // No ink: the cell is part of the chip, and the chip shows through.
        Some(gpui::rgba(0x00000000).into())
    }
}

fn render_syntax_spans(
    spans: Vec<crate::markdown::SyntaxSpan>,
    is_active_line: bool,
    theme: &Theme,
    text_color_override: Option<gpui::Rgba>,
    base_weight: Option<gpui::FontWeight>,
    base_style: Option<gpui::FontStyle>,
    font_family: &gpui::SharedString,
) -> gpui::StyledText {
    let main_color = text_color_override.unwrap_or(theme.text_primary);
    let base_weight = base_weight.unwrap_or(gpui::FontWeight::NORMAL);
    let base_style = base_style.unwrap_or(gpui::FontStyle::Normal);
    let mut full_text = String::new();
    let mut runs = Vec::new();

    let reg_font = gpui::Font {
        family: font_family.clone(),
        weight: base_weight,
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::BOLD,
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };
    let italic_font = gpui::Font {
        family: font_family.clone(),
        weight: base_weight,
        style: gpui::FontStyle::Italic,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_italic_font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::BOLD,
        style: gpui::FontStyle::Italic,
        features: Default::default(),
        fallbacks: None,
    };
    // Inline code is set one weight step heavier than the body it sits in. The chip
    // background is painted by the editor as part of the row's backdrop, not by this run,
    // only thing that marks code out here is this face plus the row's chip rectangle.
    let code_font = gpui::Font {
        family: font_family.clone(),
        weight: inline_code_weight(base_weight),
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };

    for span in spans {
        match span {
            crate::markdown::SyntaxSpan::Marker(m) => {
                if let Some(color) = marker_paint(&m, is_active_line, theme) {
                    let len = m.len();
                    let font = if crate::markdown::is_inline_code_delim(&m) { code_font.clone() } else { reg_font.clone() };
                    full_text.push_str(&m);
                    runs.push(gpui::TextRun {
                        len,
                        font,
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
            }
            crate::markdown::SyntaxSpan::Text(t) => {
                let len = t.len();
                full_text.push_str(&t);
                runs.push(gpui::TextRun {
                    len,
                    font: reg_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::Bold(b) => {
                let len = b.len();
                full_text.push_str(&b);
                runs.push(gpui::TextRun {
                    len,
                    font: bold_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::Italic(i) => {
                let len = i.len();
                full_text.push_str(&i);
                runs.push(gpui::TextRun {
                    len,
                    font: italic_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::BoldItalic(bi) => {
                let len = bi.len();
                full_text.push_str(&bi);
                runs.push(gpui::TextRun {
                    len,
                    font: bold_italic_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::InlineCode(c) => {
                // Inline code keeps the surrounding text colour and is marked out by the
                // chip the editor paints behind the row plus its heavier face, so its
                // colour never changes as the caret moves in and out of the line. The
                // chip is not a run background on purpose: a run background is painted
                // by the text element, i.e. above the selection rectangle, which would
                // hide the selection over the code.
                let len = c.len();
                full_text.push_str(&c);
                runs.push(gpui::TextRun {
                    len,
                    font: code_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::Strikethrough(s) => {
                let len = s.len();
                full_text.push_str(&s);
                runs.push(gpui::TextRun {
                    len,
                    font: reg_font.clone(),
                    color: theme.text_muted.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: Some(gpui::StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(theme.text_muted.into()),
                    }),
                });
            }
            crate::markdown::SyntaxSpan::LinkText(l) => {
                let len = l.len();
                full_text.push_str(&l);
                runs.push(gpui::TextRun {
                    len,
                    font: reg_font.clone(),
                    color: theme.accent.into(),
                    background_color: None,
                    underline: Some(gpui::UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme.accent.into()),
                        wavy: false,
                    }),
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::Underline(u) => {
                let len = u.len();
                full_text.push_str(&u);
                runs.push(gpui::TextRun {
                    len,
                    font: reg_font.clone(),
                    color: main_color.into(),
                    background_color: None,
                    underline: Some(gpui::UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(main_color.into()),
                        wavy: false,
                    }),
                    strikethrough: None,
                });
            }
            crate::markdown::SyntaxSpan::Highlight(h) => {
                let hl_bg = if theme.is_dark { gpui::rgba(0xf9e2af44) } else { gpui::rgba(0xdf8e1d33) };
                let len = h.len();
                full_text.push_str(&h);
                runs.push(gpui::TextRun {
                    len,
                    font: reg_font.clone(),
                    color: main_color.into(),
                    background_color: Some(hl_bg.into()),
                    underline: None,
                    strikethrough: None,
                });
            }
        }
    }

    if full_text.is_empty() {
        gpui::StyledText::new(" ").with_runs(vec![gpui::TextRun {
            len: 1,
            font: reg_font,
            color: gpui::rgba(0x00000000).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }])
    } else {
        gpui::StyledText::new(full_text).with_runs(runs)
    }
}

/// Shaped geometry of one painted visual row.
///
/// The editor paints rows from `SyntaxSpan` slices, so nothing downstream knew where
/// the glyphs actually landed: hit-testing and the caret/selection rectangles used a
/// naive `column * char_width` cell model instead. That model is exact for monospace
/// runs of narrow glyphs, but drifts for wide ones (emoji, CJK, VS16/ZWJ sequences),
/// which made the selection edge twitch while dragging across them.
///
/// This follows Zed's editor: the row's painted text is shaped once and every
/// pixel <-> column conversion goes through the real glyph positions
/// (`LineLayout::closest_index_for_x` and `ShapedGlyph::position`).
#[derive(Clone)]
pub(crate) struct RowShapedLayout {
    layout: Arc<gpui::LineLayout>,
    text: SharedString,
    /// `line_chars[k]` = line char painted at visual column `k`, plus one trailing
    /// entry for the line char just past the last painted char.
    line_chars: Vec<usize>,
    /// Visual columns the row covers with the inline-code chip, merged and in order.
    /// The editor paints the chip and the selection from these, so they must come from
    /// the same painted text the renderer walks (`markdown::row_painted_text`).
    code_ranges: Vec<(usize, usize)>,
    /// Monospace cell width. Only used for the virtual cell past the last painted
    /// char, where a soft-wrapped newline still occupies one cell.
    cell_w: f32,
}

impl RowShapedLayout {
    /// Number of painted character cells in this row.
    pub fn columns(&self) -> usize {
        self.line_chars.len().saturating_sub(1)
    }

    /// Byte offset of painted column `col` in the shaped text.
    fn byte_for_column(&self, col: usize) -> usize {
        self.text
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }

    /// Pixel offset of painted column `col`, measured from the left edge of the row's
    /// text. Columns past the end fall back to the monospace cell width.
    pub fn x_for_column(&self, col: usize) -> f32 {
        let columns = self.columns();
        if col >= columns {
            return f32::from(self.layout.width) + (col - columns) as f32 * self.cell_w;
        }
        let byte = self.byte_for_column(col);
        // Walk the glyphs rather than calling `x_for_index`, so that a column landing
        // inside a multi-glyph cluster (a ZWJ emoji, say) still resolves to the left
        // edge of that cluster instead of jumping to the next one.
        let mut cluster_x = 0.0f32;
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index == byte {
                    return f32::from(glyph.position.x);
                }
                if glyph.index > byte {
                    return cluster_x;
                }
                cluster_x = f32::from(glyph.position.x);
            }
        }
        cluster_x
    }

    /// Painted column closest to the given pixel offset.
    pub fn column_for_x(&self, x: f32) -> usize {
        let columns = self.columns();
        let end_x = f32::from(self.layout.width);
        if x >= end_x {
            return columns;
        }
        let byte = self.layout.closest_index_for_x(px(x));
        let col = self.text.get(..byte).map(|s| s.chars().count()).unwrap_or(0).min(columns);
        // `closest_index_for_x` weighs the boundaries that own a glyph, and the position
        // past the last painted character owns none: it snaps that one to the last
        // glyph's *left edge* while every other boundary sits in the middle of its cell.
        // Left alone, dragging onto the last character makes the selection edge lag by
        // half a glyph and then jump a whole advance (up to ~19px on an emoji) as the
        // pointer crosses it — which reads as stuttering at the end of the line.
        if col == columns && columns > 0 {
            let mid = (self.x_for_column(columns - 1) + end_x) / 2.0;
            if x < mid {
                return columns - 1;
            }
        }
        col
    }

    /// Line char painted at column `col`.
    pub fn line_char_for_column(&self, col: usize) -> usize {
        self.line_chars[col.min(self.columns())]
    }

    /// Column that paints line char `lc`: the first column at or past it.
    pub fn column_for_line_char(&self, lc: usize) -> usize {
        self.line_chars
            .partition_point(|&line_char| line_char < lc)
            .min(self.columns())
    }

    /// Visual columns covered by the inline-code chip, merged and in order.
    pub fn code_ranges(&self) -> &[(usize, usize)] {
        &self.code_ranges
    }

    /// Left edge and width of the selection rectangle covering columns
    /// `sel_s..sel_e`, in logical px from the row's text origin.
    ///
    /// Both edges go through the same device-pixel snapping as the caret
    /// (`crate::snap::snapped_span`). The rectangle carries `rounded_*_xs()` corners, so
    /// unlike a plain quad it is painted by the signed-distance path and its edge is
    /// antialiased: at a fractional x the partially covered pixel column shifts its
    /// blend ratio with every sub-pixel mouse move, while the snapped caret sitting
    /// on the same boundary stays crisp — so the painted edge and the caret disagree
    /// by up to a device pixel and the edge appears to breathe while dragging.
    pub fn selection_rect(&self, sel_s: usize, sel_e: usize, scale_factor: f32) -> (f32, f32) {
        crate::snap::snapped_span(
            self.x_for_column(sel_s),
            self.x_for_column(sel_e),
            scale_factor,
        )
    }
}

/// Shape the text one painted visual row draws, so that pixel <-> column conversion
/// can use real glyph positions instead of a `column * char_width` cell model.
///
/// `cell_w` is only used for the virtual cell past the last painted char, so callers
/// pass the row's own monospace char width.
pub(crate) fn shape_row(
    text_system: &gpui::WindowTextSystem,
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    v: &crate::markdown::VisualRow,
    is_active_line: bool,
    headings: &crate::settings::HeadingsSettings,
    font_family: &SharedString,
    cell_w: f32,
) -> RowShapedLayout {
    let painted = crate::markdown::row_painted_text(line, classified, v, is_active_line, headings);
    let code_ranges = painted.code_ranges();
    // Heading rows paint at the heading size, everything else inherits `text_sm()`.
    let (_, _, font_size, _, _) = AppView::heading_typography_metrics_with_widths(classified, headings, 24.0, cell_w, None);

    let mut text = String::new();
    let mut runs = Vec::new();
    // Colours never move glyphs, so one flat colour is enough for geometry. That holds
    // only because the text system resolves a font per run: if it merged runs by colour
    // alone, a flat colour would shape every run with the first segment's face (see the
    // LOCAL PATCH in `vendor/gpui/src/text_system.rs::layout_line`).
    let color = rgba(0x000000ff).into();
    for segment in &painted.segments {
        // Inline code is painted heavier than the row's body, so the shaped layout has
        // to ask for the same face the renderer uses or the chip would be measured with
        // the wrong advances.
        let weight = if segment.code {
            inline_code_weight(if segment.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
        } else if segment.bold {
            gpui::FontWeight::BOLD
        } else {
            gpui::FontWeight::NORMAL
        };
        text.push_str(&segment.text);
        runs.push(gpui::TextRun {
            len: segment.text.len(),
            font: gpui::Font {
                family: font_family.clone(),
                weight,
                style: if segment.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal },
                features: Default::default(),
                fallbacks: None,
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let text: SharedString = text.into();
    let layout = text_system.layout_line(text.as_ref(), px(font_size), &runs, None);
    RowShapedLayout {
        layout,
        text,
        line_chars: painted.line_chars,
        code_ranges,
        cell_w,
    }
}

fn render_live_styled_row(
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    char_start: usize,
    char_end: usize,
    sub_idx: usize,
    is_active_line: bool,
    show_fences: bool,
    headings: &crate::settings::HeadingsSettings,
    theme: &Theme,
    font_family: &gpui::SharedString,
    custom_ui_node: Option<&crate::plugins::ui::UiNode>,
    custom_plugin_id: Option<&str>,
    is_first_code_line: bool,
    cx: &Context<AppView>,
) -> gpui::Div {
    if line.is_empty() || char_start >= char_end {
        return div().h_full().flex().items_center().child(" ");
    }

    match classified {
        crate::markdown::LineKind::Heading1(_) => {
            let rest = line.strip_prefix("# ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h1.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h1.color_name);
            let h_weight = if headings.h1.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h1.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("# "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Heading2(_) => {
            let rest = line.strip_prefix("## ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h2.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h2.color_name);
            let h_weight = if headings.h2.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h2.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(3));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("## "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(3),
                    char_end.saturating_sub(3),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Heading3(_) => {
            let rest = line.strip_prefix("### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h3.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h3.color_name);
            let h_weight = if headings.h3.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h3.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(4));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(4),
                    char_end.saturating_sub(4),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Heading4(_) => {
            let rest = line.strip_prefix("#### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h4.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h4.color_name);
            let h_weight = if headings.h4.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h4.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(5));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("#### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(5),
                    char_end.saturating_sub(5),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Heading5(_) => {
            let rest = line.strip_prefix("##### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h5.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h5.color_name);
            let h_weight = if headings.h5.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h5.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(6));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("##### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(6),
                    char_end.saturating_sub(6),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Heading6(_) => {
            let rest = line.strip_prefix("###### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h6.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h6.color_name);
            let h_weight = if headings.h6.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h6.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(7));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("###### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(7),
                    char_end.saturating_sub(7),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_syntax_spans(row_spans, is_active_line, theme, Some(h_color), Some(h_weight), Some(h_style), font_family)),
                    )
            }
        }
        crate::markdown::LineKind::Quote(_) => {
            let rest = line.strip_prefix("> ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.accent).child("> "))
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, is_active_line, theme, Some(theme.text_secondary), None, None, font_family)))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.border_subtle).child("  "))
                    .child(div().text_color(theme.text_secondary).child(render_syntax_spans(row_spans, is_active_line, theme, Some(theme.text_secondary), None, None, font_family)))
            }
        }
        crate::markdown::LineKind::Task { checked, .. } => {
            let prefix_len = if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            };
            let rest = if line.len() >= prefix_len { &line[prefix_len..] } else { "" };
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let prefix_color = if *checked { theme.success } else { theme.text_muted };
            let prefix_str = if line.len() >= prefix_len { &line[..prefix_len] } else { "" };
            let task_color = if *checked { Some(theme.text_muted) } else { None };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(prefix_color).child(prefix_str.to_string()))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_syntax_spans(row_spans, is_active_line, theme, task_color, None, None, font_family)),
                    )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child("      "))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_syntax_spans(row_spans, is_active_line, theme, task_color, None, None, font_family)),
                    )
            }
        }
        crate::markdown::LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let (bullet, rest) = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                (&trimmed[..2], &trimmed[2..])
            } else {
                ("", trimmed)
            };
            let prefix_len = indent_len + bullet.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(bullet.to_string()))
                    .child(render_syntax_spans(row_spans, is_active_line, theme, None, None, None, font_family))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(format!("{}  ", indent)))
                    .child(render_syntax_spans(row_spans, is_active_line, theme, None, None, None, font_family))
            }
        }
        crate::markdown::LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let num_prefix_len = num.len() + 2;
            let (num_part, rest) = if trimmed.len() >= num_prefix_len {
                (&trimmed[..num_prefix_len], &trimmed[num_prefix_len..])
            } else {
                ("", trimmed)
            };
            let prefix_len = indent_len + num_part.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(num_part.to_string()))
                    .child(render_syntax_spans(row_spans, is_active_line, theme, None, None, None, font_family))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                let spaces = " ".repeat(prefix_len);
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(spaces))
                    .child(render_syntax_spans(row_spans, is_active_line, theme, None, None, None, font_family))
            }
        }
        crate::markdown::LineKind::HorizontalRule => {
            if is_active_line {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(theme.border_subtle),
                    )
            }
        }
        crate::markdown::LineKind::CodeFence { is_opening, lang } => {
            if *is_opening {
                if show_fences || is_active_line {
                    // Editing the block: paint the raw fence so the caret and the
                    // backticks line up with the source.
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h_full()
                        .text_color(theme.text_muted)
                        .child(line.to_string())
                } else {
                    // Collapsed: the language label stands in for the fence, and the row
                    // loop overlays the copy button at the right end of this same row.
                    let lang_label = lang.unwrap_or("CODE").to_uppercase();
                    let mut header = div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h_full()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child(lang_label),
                        );
                    if let Some(pid) = custom_plugin_id {
                        header = header.child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text_muted)
                                .child(format!("[{}]", pid)),
                        );
                    }
                    header.child(icons::icon(icons::ICON_CODE, 12.0, theme.text_muted))
                }
            } else if show_fences || is_active_line {
                // Editing the block: paint the raw closing fence.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                // Collapsed: the fence is hidden. The row stays empty and only carries
                // the block's background and its rounded bottom corners, so the panel
                // still ends in a visible edge.
                div().h_full()
            }
        }
        crate::markdown::LineKind::CodeLine(_) => {
            if !show_fences && !is_active_line && custom_ui_node.is_some() {
                if is_first_code_line {
                    let tree = custom_ui_node.unwrap();
                    let pid = custom_plugin_id.unwrap_or("plugin");
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .py_1()
                        .child(crate::plugins::ui::render_ui_node(tree, pid, 0, theme, cx))
                } else {
                    div().h_0().overflow_hidden()
                }
            } else {
                let row_slice: String = line.chars().skip(char_start).take(char_end - char_start).collect();
                // Plain foreground: the block background alone marks this as code. The
                // green that used to sit here read as an accent colour, not as text.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_primary)
                    .child(row_slice)
            }
        }
        crate::markdown::LineKind::Paragraph(_) => {
            let spans = crate::markdown::parse_inline_syntax_spans(line);
            let row_spans = crate::markdown::slice_syntax_spans(&spans, char_start, char_end);
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(render_syntax_spans(row_spans, is_active_line, theme, None, None, None, font_family))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{classify_line, row_painted_text, VisualRow};

    /// Every mouse position must resolve to the **nearest** painted column, including
    /// the boundary past the last painted character. That one boundary owns no glyph,
    /// so it is the one a shaper's `closest_index_for_x` cannot weigh and the one a
    /// right-to-left drag lands on at the end of a line.
    #[test]
    fn column_boundaries_sit_mid_cell_including_end_of_line() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                for line in ["abcdef", "abc", "ab 😀", "текст", "x🏳️‍🌈", "warn ⚠️ tail"] {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);
                    let columns = layout.columns();
                    let end_x = layout.x_for_column(columns);

                    let mut prev = layout.column_for_x(-1.0);
                    assert_eq!(prev, 0, "the left edge must map to column 0 for {line:?}");
                    let mut x = -1.0f32;
                    while x <= end_x + 2.0 {
                        let col = layout.column_for_x(x);
                        if col != prev {
                            let mid = (layout.x_for_column(prev) + layout.x_for_column(col)) / 2.0;
                            assert!(
                                (x - mid).abs() <= 0.02,
                                "{line:?}: boundary {prev}->{col} flipped at x={x:.3} but the middle of \
                                 that cell is {mid:.3} (off by {:+.3}px)",
                                x - mid
                            );
                            prev = col;
                        }
                        x += 0.01;
                    }
                    assert_eq!(prev, columns, "sweep never reached the last column for {line:?}");
                    // And the position past the end must stay reachable far to the right.
                    assert_eq!(layout.column_for_x(end_x + 100.0), columns);
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }


    /// Row geometry must come from real glyph positions rather than from a
    /// `column * char_width` cell model: a wide glyph (emoji, CJK) does not advance by
    /// exactly one cell, so the cell model drifts and the selection edge twitches while
    /// dragging across it. This fails if `shape_row` is ever replaced by the old model.
    #[test]
    fn row_geometry_follows_shaped_glyph_positions() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                let cases = [
                    "plain ascii line",
                    "кириллица без эмодзи",
                    "emoji 😀 test 🚀 done",
                    "warn ⚠️ love ❤️ flag 🏳️‍🌈 done",
                ];

                for line in cases {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);

                    // Independently shape the very same painted text and read the glyph
                    // positions straight out of the layout.
                    let painted = row_painted_text(line, &kind, &v, false, &headings);
                    let text: String = painted.segments.iter().map(|s| s.text.as_str()).collect();
                    let run = gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(font_family.clone()),
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let reference = window
                        .text_system()
                        .shape_line(text.clone().into(), px(14.0), &[run], None);

                    let mut shaped_error = 0.0f32;
                    let mut naive_error = 0.0f32;
                    let mut cluster_starts = 0usize;
                    let mut byte = 0usize;
                    for (column, ch) in text.chars().enumerate() {
                        // The glyph that starts exactly at this char, and the glyph that
                        // starts its cluster (VS16 / ZWJ continuations have neither).
                        let mut cluster_x = 0.0f32;
                        let mut exact_x = None;
                        for run in &reference.runs {
                            for glyph in &run.glyphs {
                                if glyph.index == byte {
                                    exact_x = Some(f32::from(glyph.position.x));
                                }
                                if glyph.index <= byte {
                                    cluster_x = f32::from(glyph.position.x);
                                }
                            }
                        }
                        let actual = layout.x_for_column(column);
                        match exact_x {
                            Some(expected) => {
                                cluster_starts += 1;
                                shaped_error = shaped_error.max((actual - expected).abs());
                                naive_error = naive_error.max((column as f32 * cell_w - expected).abs());
                            }
                            // A continuation char has no glyph of its own: the caret must
                            // stay on its cluster's left edge, not jump to the next one.
                            None => assert_eq!(
                                actual, cluster_x,
                                "column {column} ({ch:?}) left its cluster in {line:?}"
                            ),
                        }
                        byte += ch.len_utf8();
                    }
                    assert!(cluster_starts > 0, "no glyph positions checked for {line:?}");
                    println!("{line:?}: {cluster_starts} glyph columns, shaped max error {shaped_error:.3}px, naive cell model {naive_error:.3}px");

                    assert!(
                        shaped_error <= 0.01,
                        "shaped geometry drifted {shaped_error}px for {line:?}"
                    );
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// The painted selection edge has to sit on the same device-pixel grid as the
    /// painted caret, and it has to move monotonically while the mouse drags.
    ///
    /// The rectangle carries `rounded_*_xs()` corners, so it is painted by the
    /// signed-distance path and its edge is antialiased; the caret is snapped by
    /// `crate::snap::snap_x`. Leave the rectangle unsnapped and the two disagree by up
    /// to a device pixel — the partially covered pixel column changes its blend ratio
    /// with every sub-pixel mouse move, which is what reads as the selection edge
    /// stuttering at the end of a line while dragging right to left.
    #[test]
    fn painted_selection_edge_sits_on_the_device_grid() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                for line in ["abcdef", "ab 😀", "x🏳️‍🌈", "warn ⚠️ tail", "текст"] {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);
                    let columns = layout.columns();
                    let end_x = layout.x_for_column(columns);

                    for scale in [1.0f32, 1.25, 1.5, 1.75, 2.0] {
                        // Right to left, the direction the stutter was reported in, with
                        // the anchor parked at the end of the line.
                        let mut prev_edge = f32::INFINITY;
                        let mut x = end_x + 2.0;
                        while x >= -1.0 {
                            // Exactly what the app does: the mouse picks a column, that
                            // column becomes a line char / offset, and the selection range
                            // is painted back from the column of that line char.
                            let col = layout.column_for_x(x);
                            let painted_col = layout.column_for_line_char(layout.line_char_for_column(col));
                            let (edge, width) = layout.selection_rect(painted_col, columns, scale);

                            assert!(width >= 0.0, "{line:?} @{scale}x: negative width at x={x:.3}");

                            // On the device grid, like the caret.
                            let device = edge * scale;
                            assert!(
                                (device - device.round()).abs() <= 0.0005,
                                "{line:?} @{scale}x: painted edge {edge:.4}px is off the device grid \
                                 ({device:.4} device px) at x={x:.3}"
                            );

                            // Dragging left must never push the painted edge right.
                            assert!(
                                edge <= prev_edge + 0.0005,
                                "{line:?} @{scale}x: painted edge moved backwards while dragging left, \
                                 {prev_edge:.4} -> {edge:.4} at x={x:.3}"
                            );

                            // Snapping only rounds, so the edge stays within half a device
                            // pixel of the true glyph boundary.
                            let raw = layout.x_for_column(painted_col);
                            assert!(
                                (edge - raw).abs() <= 0.5 / scale + 0.0005,
                                "{line:?} @{scale}x: snapping moved the edge {:.4}px from {raw:.4}",
                                edge - raw
                            );

                            prev_edge = edge;
                            x -= 0.01;
                        }
                    }
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// Row geometry is built from shaped glyph positions, so every run must be shaped
    /// with its *own* face. `TextSystem::layout_line` used to merge runs on the
    /// decoration alone (colour/underline/strikethrough) and never re-resolve the font,
    /// so the editor's flat-colour geometry measured an italic span with the upright
    /// advance. Roboto Mono's italic is 1202/2048 em against the upright 1229/2048 em,
    /// so the caret and the selection edge drifted ~2% right of the glyphs and the
    /// caret ended up inside the glyph it should sit in front of.
    #[test]
    fn geometry_shapes_each_run_with_its_own_face() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Roboto Mono".into();
                let upright = gpui::Font {
                    family: font_family.clone(),
                    weight: gpui::FontWeight::BOLD,
                    style: gpui::FontStyle::Normal,
                    features: Default::default(),
                    fallbacks: None,
                };
                let bold_italic = gpui::Font { style: gpui::FontStyle::Italic, ..upright.clone() };
                let run = |font: gpui::Font, len: usize| gpui::TextRun {
                    len,
                    font,
                    color: rgba(0x000000ff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let advance = |font: gpui::Font| -> f32 {
                    let text: SharedString = "A".into();
                    let runs = [run(font, 1)];
                    f32::from(window.text_system().layout_line(&text, px(40.0), &runs, None).width)
                };
                let upright_adv = advance(upright.clone());
                let italic_adv = advance(bold_italic.clone());
                assert!(
                    italic_adv < upright_adv,
                    "the shipped editor font must have a narrower italic advance, \
                     otherwise this test cannot detect the merged-run regression"
                );

                // Two runs that share one colour must still split into per-font runs.
                let text: SharedString = "AB".into();
                let runs = [run(upright.clone(), 1), run(bold_italic.clone(), 1)];
                let shaped = window.text_system().layout_line(&text, px(40.0), &runs, None);
                assert_eq!(
                    shaped.runs.len(),
                    2,
                    "runs with different fonts must not be merged into one font run"
                );
                assert!(
                    (f32::from(shaped.width) - (upright_adv + italic_adv)).abs() < 0.05,
                    "expected the two faces' own advances to be summed, got {} for {upright_adv} + {italic_adv}",
                    f32::from(shaped.width)
                );

                // ...and the row geometry the caret is derived from must do the same.
                let line = "# *Курсив со звёздочками*";
                let v = VisualRow { line_idx: 0, sub_idx: 0, is_last_sub: true, char_start: 0, char_end: line.chars().count() };
                let kind = classify_line(line, None);
                let mut headings = crate::settings::HeadingsSettings::default();
                headings.h1.font_size = 40;
                headings.h1.bold = true;
                headings.h1.italic = false;
                let geom = shape_row(window.text_system(), line, &kind, &v, true, &headings, &font_family, 24.0);
                // Column 0 is the upright "# " marker, column 3 the first italic char.
                let up_cell = geom.x_for_column(1) - geom.x_for_column(0);
                let it_cell = geom.x_for_column(4) - geom.x_for_column(3);
                assert!((up_cell - upright_adv).abs() < 0.05, "upright cell {up_cell} vs {upright_adv}");
                assert!((it_cell - italic_adv).abs() < 0.05, "italic cell {it_cell} vs {italic_adv}");

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// The inline-code chip keeps a constant width, so the marker delimiting it must stay
    /// painted off the active line — with no ink at all, which is what reads as the
    /// chip's padding. Every other marker collapses entirely when the line is idle.
    #[test]
    fn code_delimiters_stay_painted_off_the_active_line() {
        let theme = Theme::light();
        // `Hsla` is not `PartialEq`, and the Rgba -> Hsla -> Rgba round trip is not
        // exact, so compare the components with a tolerance.
        let comps = |c: gpui::Rgba| (c.r, c.g, c.b, c.a);
        let same = |a: gpui::Hsla, b: gpui::Rgba| {
            let a = comps(gpui::Rgba::from(a));
            let b = comps(b);
            (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3 && (a.2 - b.2).abs() < 1e-3 && (a.3 - b.3).abs() < 1e-3
        };

        assert!(marker_paint("**", false, &theme).is_none(), "a hidden `**` must not be painted");
        assert!(marker_paint("~~", false, &theme).is_none(), "a hidden `~~` must not be painted");

        let muted = marker_paint("**", true, &theme).expect("an active-line marker is painted");
        assert!(same(muted, theme.text_muted), "an active-line marker is muted: {:?}", comps(gpui::Rgba::from(muted)));

        // The chip is painted by the row, so an idle delimiter only has
        // to carry no ink: the chip shows through it, and so does a selection.
        let idle_delim = marker_paint("`", false, &theme).expect("a code delimiter is always painted");
        assert_eq!(comps(gpui::Rgba::from(idle_delim)).3, 0.0, "an idle delimiter must be invisible: {:?}", comps(gpui::Rgba::from(idle_delim)));

        let active_delim = marker_paint("`", true, &theme).expect("a code delimiter is always painted");
        assert!(same(active_delim, theme.text_muted), "an active delimiter is muted: {:?}", comps(gpui::Rgba::from(active_delim)));
    }

    /// Inline code is painted one step heavier than the body text, and the row's shaped
    /// layout has to ask for the same face — otherwise the chip and the caret would be
    /// measured with the wrong advances.
    #[test]
    fn inline_code_is_heavier_than_its_body_but_never_lighter_than_a_bold_one() {
        assert_eq!(inline_code_weight(gpui::FontWeight::NORMAL).0, INLINE_CODE_WEIGHT.0);
        assert!(INLINE_CODE_WEIGHT.0 > gpui::FontWeight::NORMAL.0);
        // A bold row keeps its bold: there is no heavier bundled face to move up to.
        assert_eq!(inline_code_weight(gpui::FontWeight::BOLD).0, gpui::FontWeight::BOLD.0);
        assert!(inline_code_weight(gpui::FontWeight::LIGHT).0 >= INLINE_CODE_WEIGHT.0);
    }

    /// The chip has to be visibly shorter than its row at every font size, and never
    /// collapse to nothing.
    #[test]
    fn code_chip_is_shorter_than_its_row_at_every_size() {
        // Paragraph rows and heading rows, as `heading_typography_metrics` computes them.
        for row_h in [18.0f32, 24.0, 27.0, 40.5, 47.0, 54.0, 108.0] {
            let inset = code_chip_inset(row_h);
            let chip_h = row_h - 2.0 * inset;
            assert!(inset >= 1.0, "row_h {row_h}: inset {inset} must leave a visible gap");
            assert!(chip_h < row_h, "row_h {row_h}: chip {chip_h} is not shorter than the row");
            assert!(chip_h > row_h * 0.7, "row_h {row_h}: chip {chip_h} lost too much height");
        }
        // A tiny row still gets a one-pixel gap rather than a negative chip.
        assert_eq!(code_chip_inset(4.0), 1.0);
        assert!(4.0 - 2.0 * code_chip_inset(4.0) > 0.0);
    }
}
