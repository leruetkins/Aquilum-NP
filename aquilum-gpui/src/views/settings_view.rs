use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    canvas, div, point, prelude::*, px, rgb, rgba, App, ClipboardItem, Context,
    DispatchPhase, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Render,
    ScrollHandle, ScrollWheelEvent, SharedString, StatefulInteractiveElement, Styled,
    Subscription, WeakEntity, Window, WindowControlArea, WindowOptions,
};

use crate::blink_manager::BlinkManager;
use crate::icons;
use crate::plugins::ui::{PluginInputFocusState, SelectOption, UiSink};
use crate::settings::{
    get_setting_by_id, search_settings, AppConfig, SettingsEvent, SettingsModel,
};
use crate::theme::Theme;
use super::app_view::AppView;

mod plugin_toolbar;
mod tab;
mod tabs;
mod ui;

pub use tab::*;
pub use ui::*;
use crate::views::tab_list::{TabEntry, TabListEvent, TabListPopup, ROW_HEIGHT};

impl AppView {
    /// Helper to get localized string using current UI language
    pub fn t(&self, key: &str) -> String {
        crate::i18n::t(key, &self.config.ui.language)
    }

    /// Helper to get localized string with positional `{}` formatting
    pub fn t_fmt(&self, key: &str, args: &[&str]) -> String {
        crate::i18n::t_fmt(key, &self.config.ui.language, args)
    }

    /// Toggles visibility of the Settings separate window.
    /// Validates the stored handle: if the OS window was closed via the
    /// native close button, the handle is stale — drop it and open fresh
    /// instead of requiring a second click.
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        let actually_open = self
            .settings_window_manager
            .window
            .as_ref()
            .map(|h| h.update(cx, |_, _, _| {}).is_ok())
            .unwrap_or(false);
        if actually_open {
            self.close_settings(cx);
        } else {
            self.settings_window_manager.window = None;
            self.open_settings(None, cx);
        }
    }

    /// Opens or focuses the Settings window, optionally navigating directly to the given tab.
    pub fn open_settings(&mut self, tab: Option<SettingsTab>, cx: &mut Context<Self>) {
        self.open_settings_with_id(tab, None, cx);
    }

    /// Opens or focuses the Settings window, navigating to a specific tab and highlighted setting id.
    pub fn open_settings_with_id(&mut self, tab: Option<SettingsTab>, setting_id: Option<String>, cx: &mut Context<Self>) {
        self.is_settings_open = true;
        let weak_self = cx.weak_entity();
        let model = self.settings_model.clone();
        let app_data_dir = crate::state::app_data_dir();
        let target_tab = tab.or_else(|| Some(self.settings_tab.clone()));

        cx.defer(move |cx| {
            // 1. If window is already open, activate and focus it without holding a long-lived borrow
            let already_open = weak_self.update(cx, |this, cx| {
                if let Some(existing) = &this.settings_window_manager.window {
                    if existing
                        .update(cx, |view, window, cx| {
                            if let Some(tab) = target_tab.clone() {
                                view.settings_tab = tab;
                            }
                            if let Some(id) = setting_id.clone() {
                                view.navigate_to_setting(&id, cx);
                            }
                            window.focus(&view.focus_handle);
                            window.activate_window();
                            cx.notify();
                        })
                        .is_ok()
                    {
                        return true;
                    }
                }
                false
            }).unwrap_or(false);

            if already_open {
                return;
            }

            // 2. Open new Settings window outside of AppView borrow to avoid double-lease panics during render
            let state_path = app_data_dir.join("settings-window.json");
            let saved_state = crate::state::WindowState::load_file(&state_path);
            let bounds = saved_state.to_validated_bounds(cx, 980.0, 720.0);

            let config = model.read(cx).config.clone();
            let is_ru = config.ui.language.starts_with("ru");
            let window_title = if is_ru {
                "Настройки — Vibecodium"
            } else {
                "Settings — Vibecodium"
            };

            let options = WindowOptions {
                window_bounds: Some(bounds),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(window_title.into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_min_size: Some(gpui::size(px(680.0), px(500.0))),
                ..Default::default()
            };

            let save_path = state_path.clone();
            let target_tab_init = target_tab.unwrap_or(SettingsTab::Ui);
            let model_clone = model.clone();
            let target_id_init = setting_id.clone();
            let weak_app = weak_self.clone();
            let weak_app_for_close = weak_app.clone();

            let handle_res = cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| {
                    let mut v = SettingsView::new(model_clone, weak_app, target_tab_init, cx);
                    if let Some(id) = target_id_init {
                        v.navigate_to_setting(&id, cx);
                    }
                    window.focus(&v.search_focus_handle);
                    v.search_filter.is_focused = true;
                    v.search_filter.enable_cursor_blink(cx);
                    v
                });

                window.on_window_should_close(cx, move |window, cx| {
                    let is_maximized = window.is_maximized();
                    let bounds = window.bounds();
                    let mut state = crate::state::WindowState::load_file(&save_path);
                    state.maximized = is_maximized;
                    if !is_maximized {
                        state.width = f32::from(bounds.size.width);
                        state.height = f32::from(bounds.size.height);
                        state.x = Some(f32::from(bounds.origin.x));
                        state.y = Some(f32::from(bounds.origin.y));
                    }
                    state.save_file(&save_path);
                    // The window is closed via the native close button: drop the
                    // stored handle and the active flag so the next click opens
                    // the window immediately instead of requiring two clicks.
                    if let Some(app) = weak_app_for_close.upgrade() {
                        let _ = app.update(cx, |this, cx| {
                            this.is_settings_open = false;
                            this.settings_window_manager.window = None;
                            cx.notify();
                        });
                    }
                    true
                });

                view
            });

            match handle_res {
                Ok(handle) => {
                    let _ = weak_self.update(cx, |this, _cx| {
                        this.settings_window_manager.window = Some(handle);
                    });
                }
                Err(e) => {
                    eprintln!("[SettingsWindowManager] Failed to open settings window: {:?}", e);
                }
            }
        });
    }

    /// Closes the Settings window if open.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = false;
        let weak_self = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = weak_self.update(cx, |this, cx| {
                this.settings_window_manager.close_if_open(cx);
            });
        });
        cx.notify();
    }

    /// Persists the modified AppConfig to disk via SettingsModel.
    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        for view in self.state.views.values_mut() {
            view.visual_cache_dirty = true;
        }
        let config = self.config.clone();
        self.settings_model.update(cx, |m, cx| {
            m.config = config;
            m.save(cx);
        });
        cx.notify();
    }

    /// Toggles a community plugin state (enable/disable) and manages its runtime lifecycle.
    pub fn toggle_community_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if let Some(vault) = &self.state.vault {
            let is_enabled = vault.is_community_plugin_enabled(plugin_id);
            if is_enabled {
                let _ = vault.disable_community_plugin(plugin_id);
                self.unload_plugin(plugin_id, cx);
            } else {
                let _ = vault.enable_community_plugin(plugin_id);
                self.load_installed_plugins_for_vault();
            }
            cx.notify();
        }
    }
}

pub struct SettingsWindowManager {
    pub window: Option<gpui::WindowHandle<SettingsView>>,
}

impl Default for SettingsWindowManager {
    fn default() -> Self {
        Self { window: None }
    }
}

impl SettingsWindowManager {
    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub fn open_or_focus(
        &mut self,
        model: Entity<SettingsModel>,
        weak_app: WeakEntity<AppView>,
        target_tab: Option<SettingsTab>,
        target_setting_id: Option<String>,
        app_data_dir: &Path,
        cx: &mut App,
    ) {
        // 1. If window is already open, activate and focus it
        if let Some(existing) = &self.window {
            let target_tab = target_tab.clone();
            let target_setting_id = target_setting_id.clone();
            if existing
                .update(cx, |view, window, cx| {
                    window.activate_window();
                    if let Some(tab) = target_tab {
                        view.settings_tab = tab;
                    }
                    if let Some(id) = target_setting_id {
                        view.navigate_to_setting(&id, cx);
                    }
                    cx.notify();
                })
                .is_ok()
            {
                return;
            }
        }

        // 2. Validate saved window geometry
        let state_path = app_data_dir.join("settings-window.json");
        let saved_state = crate::state::WindowState::load_file(&state_path);
        let bounds = saved_state.to_validated_bounds(cx, 980.0, 720.0);

        let config = model.read(cx).config.clone();
        let is_ru = config.ui.language.starts_with("ru");
        let window_title = if is_ru {
            "Настройки — Vibecodium"
        } else {
            "Settings — Vibecodium"
        };

        let options = WindowOptions {
            window_bounds: Some(bounds),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(window_title.into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_min_size: Some(gpui::size(px(680.0), px(500.0))),
            ..Default::default()
        };

        let save_path = state_path.clone();
        let target_tab_init = target_tab.unwrap_or(SettingsTab::Ui);
        let model_clone = model.clone();
        let target_id_init = target_setting_id.clone();
        let weak_app_for_close = weak_app.clone();

        let handle_res = cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SettingsView::new(model_clone, weak_app, target_tab_init, cx);
                if let Some(id) = target_id_init {
                    v.navigate_to_setting(&id, cx);
                }
                window.focus(&v.search_focus_handle);
                v.search_filter.is_focused = true;
                v.search_filter.enable_cursor_blink(cx);
                v
            });

            window.on_window_should_close(cx, move |window, cx| {
                let is_maximized = window.is_maximized();
                let bounds = window.bounds();
                let mut state = crate::state::WindowState::load_file(&save_path);
                state.maximized = is_maximized;
                if !is_maximized {
                    state.width = f32::from(bounds.size.width);
                    state.height = f32::from(bounds.size.height);
                    state.x = Some(f32::from(bounds.origin.x));
                    state.y = Some(f32::from(bounds.origin.y));
                }
                state.save_file(&save_path);
                // Native close must clear the stored handle and the active
                // flag, otherwise the next toggle/open sees a stale handle.
                if let Some(app) = weak_app_for_close.upgrade() {
                    let _ = app.update(cx, |this, cx| {
                        this.is_settings_open = false;
                        this.settings_window_manager.window = None;
                        cx.notify();
                    });
                }
                true
            });

            view
        });

        match handle_res {
            Ok(handle) => {
                self.window = Some(handle);
            }
            Err(e) => {
                eprintln!("[SettingsWindowManager] Failed to open settings window: {:?}", e);
            }
        }
    }

    pub fn close_if_open(&mut self, cx: &mut App) {
        if let Some(handle) = self.window.take() {
            let _ = handle.update(cx, |_view, window, _cx| {
                window.remove_window();
            });
        }
    }

    pub fn toggle(
        &mut self,
        model: Entity<SettingsModel>,
        weak_app: WeakEntity<AppView>,
        target_tab: Option<SettingsTab>,
        app_data_dir: &Path,
        cx: &mut App,
    ) {
        // Validate the handle: a window closed via the native close button
        // leaves a stale handle behind until the close callback clears it.
        let actually_open = self
            .window
            .as_ref()
            .map(|h| h.update(cx, |_, _, _| {}).is_ok())
            .unwrap_or(false);
        if actually_open {
            self.close_if_open(cx);
        } else {
            self.window = None;
            self.open_or_focus(model, weak_app, target_tab, None, app_data_dir, cx);
        }
    }
}

/// Independent view entity hosting all settings tabs in a standalone OS window.
pub struct SettingsView {
    pub model: Entity<SettingsModel>,
    pub weak_app: WeakEntity<AppView>,
    pub settings_tab: SettingsTab,
    pub config: AppConfig,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub settings_scroll_handle: ScrollHandle,
    pub settings_nav_scroll_handle: ScrollHandle,
    pub active_color_picker: Option<&'static str>,
    pub custom_hex_input: String,
    pub hex_is_focused: bool,
    pub hex_cursor_pos: usize,
    pub hex_selection_anchor: Option<usize>,
    pub custom_hex_focus_handle: FocusHandle,
    pub hex_box_origin_x: Rc<Cell<f32>>,
    pub hex_is_mouse_selecting: bool,
    pub hex_mouse_drag_start_x: Option<f32>,
    pub hex_blink: BlinkManager,
    pub locale_popup: Option<gpui::Entity<TabListPopup>>,
    pub locale_popup_subscription: Option<Subscription>,
    pub font_popup: Option<gpui::Entity<TabListPopup>>,
    pub font_popup_subscription: Option<Subscription>,
    pub active_plugin_input: Option<PluginInputFocusState>,
    pub plugin_input_blink: BlinkManager,
    pub plugin_input_focus_handle: FocusHandle,
    pub active_plugin_select: Option<(String, u64, String)>,
    pub plugin_select_popup: Option<gpui::Entity<TabListPopup>>,
    pub plugin_select_subscription: Option<Subscription>,
    pub word_wrap: bool,
    // Step 4 & 7: Window and Search state
    pub is_hovering_close: bool,
    pub search_query: String,
    pub search_focus_handle: FocusHandle,
    pub search_filter: SearchFilterState,
    pub highlighted_setting_id: Option<String>,
    pub copied_setting_url_id: Option<String>,
    // Editor toolbar customization state
    pub toolbar_add_button_open: bool,
    pub toolbar_add_dropdown_open: bool,
    pub toolbar_target_dropdown_id: Option<String>,
    pub toolbar_command_filter: String,
    pub toolbar_filter_focus_handle: FocusHandle,
    pub toolbar_command_dropdown_open: bool,
    pub toolbar_icon_dropdown_open: bool,
    pub toolbar_icon_filter: String,
    pub toolbar_icon_filter_focus_handle: FocusHandle,
    pub toolbar_selected_command: Option<String>,
    pub toolbar_selected_icon: Option<String>,
    pub toolbar_custom_tooltip: String,
    pub toolbar_status_msg: Option<String>,
    pub toolbar_cmd_scroll_handle: ScrollHandle,
    pub toolbar_cmd_scroll_y: f32,
    pub toolbar_cmd_scroll_drag: crate::views::scrollbar::ScrollbarDrag,
    pub toolbar_icon_scroll_handle: ScrollHandle,
    pub toolbar_icon_scroll_y: f32,
    pub toolbar_icon_scroll_drag: crate::views::scrollbar::ScrollbarDrag,
    pub toolbar_command_popup: Option<gpui::Entity<TabListPopup>>,
    pub toolbar_command_popup_subscription: Option<Subscription>,
    pub toolbar_icon_popup: Option<gpui::Entity<TabListPopup>>,
    pub toolbar_icon_popup_subscription: Option<Subscription>,
}

/// Encapsulates the complete state of the settings search input (based on quick_filter.rs).
#[derive(Clone)]
pub struct SearchFilterState {
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub is_focused: bool,
    pub blink: BlinkManager,
    pub box_origin_x: Rc<Cell<f32>>,
    pub box_width: Rc<Cell<f32>>,
    pub scroll_offset: Rc<Cell<f32>>,
}

impl Default for SearchFilterState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchFilterState {
    pub fn new() -> Self {
        Self {
            cursor_pos: 0,
            selection_anchor: None,
            is_mouse_selecting: false,
            is_focused: false,
            blink: BlinkManager::new(),
            box_origin_x: Rc::new(Cell::new(0.0)),
            box_width: Rc::new(Cell::new(0.0)),
            scroll_offset: Rc::new(Cell::new(0.0)),
        }
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            let pos = self.cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_cursor_blink(&mut self, cx: &mut Context<SettingsView>) {
        self.blink.pause_and_show_view(|s| &mut s.search_filter.blink, cx);
    }

    pub fn enable_cursor_blink(&mut self, cx: &mut Context<SettingsView>) {
        self.blink.enable_view(|s| &mut s.search_filter.blink, cx);
    }

    pub fn disable_cursor_blink(&mut self, cx: &mut Context<SettingsView>) {
        self.blink.disable_view(cx);
    }

    #[inline]
    pub fn cursor_visible(&self) -> bool {
        self.blink.visible()
    }

    pub fn sync_focus(&mut self, is_focused: bool, cx: &mut Context<SettingsView>) {
        if !self.is_focused && is_focused {
            self.is_focused = true;
            self.enable_cursor_blink(cx);
        } else if self.is_focused && !is_focused {
            self.is_focused = false;
            self.disable_cursor_blink(cx);
            self.selection_anchor = None;
            self.is_mouse_selecting = false;
        }
    }

    pub fn handle_mouse_move(&mut self, event: &MouseMoveEvent, query: &str, window: &Window, cx: &mut Context<SettingsView>) {
        if !self.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let box_x = self.box_origin_x.get();
        let scroll_x = self.scroll_offset.get();
        let rel_x = (x - box_x + scroll_x).max(0.0);
        let new_pos = if query.is_empty() {
            0
        } else {
            let run = gpui::TextRun {
                len: query.len(),
                font: gpui::font("Segoe UI"),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(query.to_string()),
                px(12.0),
                &[run],
                None,
            );
            let byte_idx = shaped.closest_index_for_x(px(rel_x));
            query[..byte_idx.min(query.len())].chars().count()
        };
        if self.cursor_pos != new_pos {
            self.cursor_pos = new_pos;
            self.reset_cursor_blink(cx);
            cx.notify();
        }
    }

    pub fn handle_mouse_up(&mut self, cx: &mut Context<SettingsView>) {
        if self.is_mouse_selecting {
            self.is_mouse_selecting = false;
            if self.selection_anchor == Some(self.cursor_pos) {
                self.selection_anchor = None;
            }
            cx.notify();
        }
    }

    pub fn clear(&mut self, query: &mut String, cx: &mut Context<SettingsView>) {
        query.clear();
        self.cursor_pos = 0;
        self.selection_anchor = None;
        self.is_mouse_selecting = false;
        self.scroll_offset.set(0.0);
        self.reset_cursor_blink(cx);
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_search_focused = self.search_focus_handle.is_focused(window);
        self.search_filter.sync_focus(is_search_focused, cx);
        self.render_settings_modal(window, cx)
    }
}

#[cfg(test)]
mod tests;


