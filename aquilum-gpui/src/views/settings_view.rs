use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    canvas, div, point, prelude::*, px, rgb, rgba, App, ClipboardItem, Context,
    DispatchPhase, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Render, ScrollDelta,
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
use crate::views::tab_list::{TabEntry, TabListEvent, TabListPopup, ROW_HEIGHT};

/// Line-based wheel ticks (mice) are divided down by this: one notch moves
/// roughly one UI line instead of the system default (usually 3 lines ≈
/// half a settings card). Pixel deltas (trackpads) pass through untouched.
const SETTINGS_WHEEL_LINES_DIVISOR: f32 = 3.0;

/// Scales a raw wheel delta for the settings list into logical pixels.
fn scaled_wheel_dy(delta: ScrollDelta, line_h: f32) -> f32 {
    match delta {
        ScrollDelta::Pixels(p) => f32::from(p.y),
        ScrollDelta::Lines(l) => l.y * line_h / SETTINGS_WHEEL_LINES_DIVISOR,
    }
}

/// Represents the navigation tabs available in the Settings modal window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTab {
    Ui,
    Editor,
    Reader,
    Search,
    Templates,
    Files,
    Analysis,
    Mcp,
    History,
    Trash,
    System,
    Shortcuts,
    CorePlugins,
    CommunityPlugins,
    PluginSettings(String),
}

impl SettingsTab {
    /// Returns the i18n key for this settings tab.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Ui => "settings.tabs.ui",
            Self::Editor => "settings.tabs.editor",
            Self::Reader => "settings.tabs.reader",
            Self::Search => "settings.tabs.search",
            Self::Templates => "settings.tabs.templates",
            Self::Files => "settings.tabs.files",
            Self::Analysis => "settings.tabs.analysis",
            Self::Mcp => "settings.tabs.mcp",
            Self::History => "settings.tabs.history",
            Self::Trash => "settings.tabs.trash",
            Self::System => "settings.tabs.system",
            Self::Shortcuts => "settings.tabs.shortcuts",
            Self::CorePlugins => "settings.tabs.core_plugins",
            Self::CommunityPlugins => "settings.tabs.community_plugins",
            Self::PluginSettings(_) => "settings.tabs.plugin_settings",
        }
    }

    /// Returns the user-facing localized display title for the tab.
    pub fn title(&self, lang: &str) -> String {
        match self {
            Self::CorePlugins => {
                if lang.starts_with("ru") {
                    "Встроенные плагины".to_string()
                } else {
                    "Core plugins".to_string()
                }
            }
            Self::CommunityPlugins => {
                if lang.starts_with("ru") {
                    "Сторонние плагины".to_string()
                } else {
                    "Community plugins".to_string()
                }
            }
            Self::PluginSettings(name) => name.clone(),
            _ => crate::i18n::t(self.key(), lang),
        }
    }

    /// Returns the SVG icon path corresponding to this settings tab.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Ui => icons::ICON_MONITOR,
            Self::Editor => icons::ICON_TYPE,
            Self::Reader => icons::ICON_BOOK_OPEN,
            Self::Search => icons::ICON_SEARCH,
            Self::Templates => icons::ICON_FILE_TEXT,
            Self::Files => icons::ICON_PAPERCLIP,
            Self::Analysis => icons::ICON_ANALYSIS,
            Self::Mcp => icons::ICON_PLUG,
            Self::History => icons::ICON_HISTORY,
            Self::Trash => icons::ICON_TRASH_2,
            Self::System => icons::ICON_POWER,
            Self::Shortcuts => icons::ICON_KEYBOARD,
            Self::CorePlugins => icons::ICON_PLUG,
            Self::CommunityPlugins => icons::ICON_GLOBE,
            Self::PluginSettings(_) => icons::ICON_SETTINGS,
        }
    }

    /// Returns a slice containing all settings tabs in display order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            Self::Ui,
            Self::Editor,
            Self::Reader,
            Self::Search,
            Self::Templates,
            Self::Files,
            Self::Analysis,
            Self::Mcp,
            Self::History,
            Self::Trash,
            Self::System,
            Self::Shortcuts,
            Self::CorePlugins,
            Self::CommunityPlugins,
        ]
    }
}

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

impl SettingsView {
    pub fn new(
        model: Entity<SettingsModel>,
        weak_app: WeakEntity<AppView>,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> Self {
        let config = model.read(cx).config.clone();
        let theme = Theme::from_config(&config.ui.theme, None);
        let word_wrap = config.editor.word_wrap;

        let model_sub = cx.subscribe(&model, |this, model, event: &SettingsEvent, cx| {
            match event {
                SettingsEvent::ConfigChanged => {
                    this.config = model.read(cx).config.clone();
                    cx.notify();
                }
                SettingsEvent::ThemeChanged => {
                    let theme_name = model.read(cx).config.ui.theme.clone();
                    this.theme = Theme::from_config(&theme_name, None);
                    cx.notify();
                }
                SettingsEvent::WordWrapToggled(wrap) => {
                    this.word_wrap = *wrap;
                    cx.notify();
                }
                _ => {}
            }
        });
        model_sub.detach();

        Self {
            model,
            weak_app,
            settings_tab: tab,
            config,
            theme,
            focus_handle: cx.focus_handle(),
            settings_scroll_handle: ScrollHandle::new(),
            settings_nav_scroll_handle: ScrollHandle::new(),
            active_color_picker: None,
            custom_hex_input: String::new(),
            hex_is_focused: false,
            hex_cursor_pos: 0,
            hex_selection_anchor: None,
            custom_hex_focus_handle: cx.focus_handle(),
            hex_box_origin_x: Rc::new(Cell::new(0.0)),
            hex_is_mouse_selecting: false,
            hex_mouse_drag_start_x: None,
            hex_blink: BlinkManager::new(),
            locale_popup: None,
            locale_popup_subscription: None,
            font_popup: None,
            font_popup_subscription: None,
            active_plugin_input: None,
            plugin_input_blink: BlinkManager::new(),
            plugin_input_focus_handle: cx.focus_handle(),
            active_plugin_select: None,
            plugin_select_popup: None,
            plugin_select_subscription: None,
            word_wrap,
            is_hovering_close: false,
            search_query: String::new(),
            search_focus_handle: cx.focus_handle(),
            search_filter: SearchFilterState::new(),
            highlighted_setting_id: None,
            copied_setting_url_id: None,
        }
    }

    #[inline]
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Helper to get localized string using current UI language
    pub fn t(&self, key: &str) -> String {
        crate::i18n::t(key, &self.config.ui.language)
    }

    /// Helper to get localized string with positional `{}` formatting
    pub fn t_fmt(&self, key: &str, args: &[&str]) -> String {
        crate::i18n::t_fmt(key, &self.config.ui.language, args)
    }

    /// Closes the Settings window.
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window(window, cx);
    }

    /// Closes the Settings OS window and resets the AppView flags, so the
    /// next click on the settings button opens the window immediately.
    /// Must be used for every programmatic close inside SettingsView:
    /// `remove_window()` alone does not trigger `on_window_should_close`.
    pub fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            let _ = app.update(cx, |this, cx| {
                this.is_settings_open = false;
                this.settings_window_manager.window = None;
                cx.notify();
            });
        }
        window.remove_window();
    }

    /// Persists the modified AppConfig to disk via SettingsModel.
    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        self.theme = crate::theme::Theme::from_config(&self.config.ui.theme, None);
        let config = self.config.clone();
        let old_theme = self.model.read(cx).config.ui.theme.clone();
        let theme_changed = old_theme != config.ui.theme;
        let old_wrap = self.model.read(cx).config.editor.word_wrap;
        let wrap_changed = old_wrap != config.editor.word_wrap;

        self.model.update(cx, |model, cx| {
            model.config = config;
            model.save(cx);
            if theme_changed {
                cx.emit(SettingsEvent::ThemeChanged);
            }
            if wrap_changed {
                cx.emit(SettingsEvent::WordWrapToggled(model.config.editor.word_wrap));
            }
        });
        cx.notify();
    }

    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        let word_wrap = self.word_wrap;
        self.model.update(cx, |model, cx| {
            model.set_word_wrap(word_wrap, cx);
        });
        cx.notify();
    }

    pub fn navigate_to_setting(&mut self, setting_id: &str, cx: &mut Context<Self>) {
        if let Some(setting) = get_setting_by_id(setting_id) {
            match setting.tab_id {
                "ui" => self.settings_tab = SettingsTab::Ui,
                "editor" => self.settings_tab = SettingsTab::Editor,
                "reader" => self.settings_tab = SettingsTab::Reader,
                "search" => self.settings_tab = SettingsTab::Search,
                "templates" => self.settings_tab = SettingsTab::Templates,
                "files" => self.settings_tab = SettingsTab::Files,
                "analysis" => self.settings_tab = SettingsTab::Analysis,
                "mcp" => self.settings_tab = SettingsTab::Mcp,
                "history" => self.settings_tab = SettingsTab::History,
                "trash" => self.settings_tab = SettingsTab::Trash,
                "system" => self.settings_tab = SettingsTab::System,
                "shortcuts" => self.settings_tab = SettingsTab::Shortcuts,
                "core-plugins" => self.settings_tab = SettingsTab::CorePlugins,
                "community-plugins" => self.settings_tab = SettingsTab::CommunityPlugins,
                tab => self.settings_tab = SettingsTab::PluginSettings(tab.to_string()),
            }
            self.highlighted_setting_id = Some(setting.id.to_string());
            let target_id = setting.id.to_string();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let _ = this.update(cx, |view, cx| {
                    if view.highlighted_setting_id.as_deref() == Some(&target_id) {
                        view.highlighted_setting_id = None;
                        cx.notify();
                    }
                });
            }).detach();
            cx.notify();
        }
    }

    pub fn copy_setting_link(&mut self, setting_id: &str, cx: &mut Context<Self>) {
        if let Some(setting) = get_setting_by_id(setting_id) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(setting.url()));
            self.copied_setting_url_id = Some(setting.id.to_string());
            let target_id = setting.id.to_string();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let _ = this.update(cx, |view, cx| {
                    if view.copied_setting_url_id.as_deref() == Some(&target_id) {
                        view.copied_setting_url_id = None;
                        cx.notify();
                    }
                });
            }).detach();
            cx.notify();
        }
    }

    pub fn unload_all_plugins(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            app.update(cx, |app, cx| {
                app.unload_all_plugins();
                cx.notify();
            });
        }
    }

    pub fn load_installed_plugins_for_vault(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            app.update(cx, |app, cx| {
                app.load_installed_plugins_for_vault();
                cx.notify();
            });
        }
    }

    pub fn uninstall_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            let pid = plugin_id.to_string();
            app.update(cx, |app, cx| {
                app.uninstall_plugin_from_catalog(&pid, cx);
                app.load_installed_plugins_for_vault();
                cx.notify();
            });
        }
        if matches!(&self.settings_tab, SettingsTab::PluginSettings(id) if id == plugin_id) {
            self.settings_tab = SettingsTab::CommunityPlugins;
        }
    }

    pub fn handle_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.locale_popup.is_some() {
            self.close_locale_popup(cx);
            return true;
        }
        if self.font_popup.is_some() {
            self.close_font_popup(cx);
            return true;
        }
        if self.plugin_select_popup.is_some() {
            self.close_plugin_select(cx);
            return true;
        }
        if self.active_color_picker.is_some() {
            self.active_color_picker = None;
            self.hex_is_focused = false;
            cx.notify();
            return true;
        }
        if self.active_plugin_input.is_some() {
            self.active_plugin_input = None;
            self.plugin_input_blink.disable_view(cx);
            cx.notify();
            return true;
        }
        false
    }

    pub fn editor_font_family(&self) -> SharedString {
        SharedString::from(self.config.editor.font.font_family.clone())
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
        self.hex_blink.pause_and_show_view(|s| &mut s.hex_blink, cx);
    }

    pub fn enable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.enable_view(|s| &mut s.hex_blink, cx);
    }

    pub fn disable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.disable_view(cx);
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

    pub fn open_locale_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::i18n::{LocaleInfo, SUPPORTED_LOCALES};

        let current = LocaleInfo::normalize(&self.config.ui.language);
        let tabs: Vec<TabEntry> = SUPPORTED_LOCALES
            .iter()
            .enumerate()
            .map(|(id, loc)| TabEntry {
                id,
                title: SharedString::from(loc.native_name),
                subtitle: Some(SharedString::from(loc.english_name)),
                icon: icons::ICON_GLOBE,
                font_family: None,
                dot_color: None,
            })
            .collect();
        let active_ix = SUPPORTED_LOCALES
            .iter()
            .position(|loc| loc.code == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                active_ix,
                theme,
                None,
                Some(px(ROW_HEIGHT * 10.0)),
                Some(50.0),
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(loc) = SUPPORTED_LOCALES.get(*id) {
                            this.config.ui.language = loc.code.to_string();
                            this.save_settings(cx);
                        }
                        this.close_locale_popup(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_locale_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.locale_popup = Some(popup);
        self.locale_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_locale_popup(&mut self, cx: &mut Context<Self>) {
        if self.locale_popup.take().is_none() {
            return;
        }
        self.locale_popup_subscription = None;
        cx.notify();
    }

    pub fn locale_popup_is_open(&self) -> bool {
        self.locale_popup.is_some()
    }

    pub fn open_font_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.config.editor.font.font_family.clone();
        let tabs: Vec<TabEntry> = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .enumerate()
            .map(|(id, name)| TabEntry {
                id,
                title: SharedString::from(*name),
                subtitle: None,
                icon: icons::ICON_TYPE,
                font_family: Some(SharedString::from(*name)),
                dot_color: None,
            })
            .collect();
        let active_ix = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .position(|name| *name == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| TabListPopup::new(tabs, active_ix, theme, None, None, None, cx));
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(name) = crate::fonts::EDITOR_FONT_CHOICES.get(*id) {
                            this.config.editor.font.font_family = name.to_string();
                            this.save_settings(cx);
                        }
                        this.close_font_popup(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_font_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.font_popup = Some(popup);
        self.font_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_font_popup(&mut self, cx: &mut Context<Self>) {
        if self.font_popup.take().is_none() {
            return;
        }
        self.font_popup_subscription = None;
        cx.notify();
    }

    pub fn font_popup_is_open(&self) -> bool {
        self.font_popup.is_some()
    }

    pub fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<SelectOption>,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_ix = options.iter().position(|o| o.value == current_value).unwrap_or(0);
        let tabs: Vec<TabEntry> = options
            .iter()
            .enumerate()
            .map(|(id, opt)| TabEntry {
                id,
                title: SharedString::from(opt.label.clone()),
                subtitle: None,
                icon: icons::ICON_CHEVRON_RIGHT,
                font_family: None,
                dot_color: None,
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = options.len().clamp(1, 10) as f32;
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                active_ix,
                theme,
                None,
                Some(px(ROW_HEIGHT * max_rows)),
                Some(50.0),
                cx,
            )
        });

        let p_id_activate = plugin_id.clone();
        let n_id_activate = node_id.clone();
        let opts_activate = options.clone();

        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(opt) = opts_activate.get(*id) {
                            let chosen_val = opt.value.clone();
                            if let Some(app) = this.weak_app.upgrade() {
                                let pid = p_id_activate.clone();
                                let nid = n_id_activate.clone();
                                let cval = chosen_val.clone();
                                app.update(cx, |app, cx| {
                                    if let Some(tree) = app.plugin_views.get_mut(&(pid.clone(), view_id)) {
                                        tree.update_input_value(&nid, &cval);
                                    }
                                    let _ = app.state.plugin_host.send_event(
                                        &pid,
                                        crate::plugins::api::HostEvent::UiEvent {
                                            view_id,
                                            node_id: nid,
                                            event: "change".to_string(),
                                            value: Some(cval),
                                        },
                                    );
                                    cx.notify();
                                });
                            }
                        }
                        this.close_plugin_select(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_plugin_select(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.active_plugin_select = Some((plugin_id, view_id, node_id));
        self.plugin_select_popup = Some(popup);
        self.plugin_select_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        if self.plugin_select_popup.take().is_none() {
            return;
        }
        self.active_plugin_select = None;
        self.plugin_select_subscription = None;
        cx.notify();
    }

    pub fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.active_plugin_select.as_ref() == Some(&(plugin_id.to_string(), view_id, node_id.to_string()))
            && self.plugin_select_popup.is_some()
    }

    pub fn toggle_community_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let mut now_disabled = false;
        if let Some(app) = self.weak_app.upgrade() {
            let pid = plugin_id.to_string();
            app.update(cx, |app, cx| {
                app.toggle_community_plugin(&pid, cx);
                if let Some(vault) = &app.state.vault {
                    now_disabled = !vault.is_community_plugin_enabled(&pid);
                }
            });
        }
        if now_disabled && matches!(&self.settings_tab, SettingsTab::PluginSettings(id) if id == plugin_id) {
            self.settings_tab = SettingsTab::CommunityPlugins;
        }
        cx.notify();
    }

    pub fn uninstall_community_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            let pid = plugin_id.to_string();
            app.update(cx, |app, cx| {
                app.uninstall_plugin_from_catalog(&pid, cx);
                app.load_installed_plugins_for_vault();
                cx.notify();
            });
        }
        if matches!(&self.settings_tab, SettingsTab::PluginSettings(id) if id == plugin_id) {
            self.settings_tab = SettingsTab::CommunityPlugins;
        }
        cx.notify();
    }

    pub fn open_plugin_catalog(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = self.weak_app.upgrade() {
            app.update(cx, |app, cx| {
                app.open_plugin_catalog(window, cx);
            });
        }
    }

    pub fn close_active_plugin_modal(&mut self, cx: &mut Context<Self>) {
        if let SettingsTab::PluginSettings(ref pid) = self.settings_tab {
            let p_id = pid.clone();
            if let Some(app) = self.weak_app.upgrade() {
                app.update(cx, |app, cx| {
                    let _ = app.state.plugin_host.send_event(
                        &p_id,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id: 1,
                            node_id: "modal".to_string(),
                            event: "close".to_string(),
                            value: None,
                        },
                    );
                    app.plugin_views.remove(&(p_id, 1));
                    cx.notify();
                });
            }
        }
    }

    /// Renders the full Settings modal dialog including backdrop and content.
    pub fn render_settings_modal(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let caret_visible = self.plugin_input_blink.visible();

        let plugin_modal_overlay = if let SettingsTab::PluginSettings(ref pid) = self.settings_tab {
            let p_id_query = pid.clone();
            let modal_tree = self.weak_app.upgrade().and_then(|app| {
                app.read(cx).plugin_views.get(&(p_id_query, 1)).cloned()
            });
            modal_tree.and_then(|modal_tree| {
                if let crate::plugins::ui::UiNode::Text { ref value, .. } = modal_tree {
                    if value.is_empty() {
                        return None;
                    }
                }
                let is_ru = self.config.ui.language.starts_with("ru");
                let modal_title = if let crate::plugins::ui::UiNode::Column { ref children, .. } = modal_tree {
                    children.iter().find_map(|c| {
                        if let crate::plugins::ui::UiNode::Text { value, style: crate::plugins::ui::TextStyle::Heading, .. } = c {
                            if !value.trim().is_empty() {
                                return Some(value.clone());
                            }
                        }
                        None
                    })
                } else {
                    None
                }.unwrap_or_else(|| if is_ru { "Редактирование".to_string() } else { "Edit".to_string() });

                let rendered_modal = crate::plugins::ui::render_ui_node(
                    &modal_tree,
                    pid,
                    1,
                    theme,
                    self.active_plugin_input.as_ref(),
                    &self.plugin_input_focus_handle,
                    self.active_plugin_select.as_ref(),
                    self.plugin_select_popup.as_ref(),
                    caret_visible,
                    window,
                    cx,
                    None,
                );
                Some(
                    div()
                        .id("plugin-modal-backdrop-overlay")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(rgba(0x000000cc))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(gpui::CursorStyle::Arrow)
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_active_plugin_modal(cx);
                        }))
                        .on_mouse_up(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(|_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .id("plugin-modal-dialog-box")
                                .relative()
                                .flex()
                                .flex_col()
                                .w(px(660.0))
                                .max_w(px(760.0))
                                .max_h(px(640.0))
                                .rounded_xl()
                                .overflow_hidden()
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border)
                                .shadow_xl()
                                .cursor(gpui::CursorStyle::Arrow)
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(|_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_scroll_wheel(|_, _, cx| {
                                    cx.stop_propagation();
                                })
                                // Title header with close button
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .px_5()
                                        .py_3()
                                        .border_b_1()
                                        .border_color(theme.border_subtle)
                                        .bg(theme.bg_sidebar)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .child(icons::icon(icons::ICON_SETTINGS, 15.0, theme.accent))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_color(theme.text_primary)
                                                        .child(modal_title),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("plugin-modal-close-x")
                                                .cursor(gpui::CursorStyle::PointingHand)
                                                .p_1()
                                                .rounded_md()
                                                .hover(|s| s.bg(theme.bg_hover))
                                                .text_color(theme.text_muted)
                                                .child(icons::icon(icons::ICON_CLOSE, 14.0, theme.text_muted))
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.close_active_plugin_modal(cx);
                                                }))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.close_active_plugin_modal(cx);
                                                })),
                                        ),
                                )
                                // Scrollable body
                                .child(
                                    div()
                                        .id("plugin-modal-body-scroll")
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_h_0()
                                        .p_6()
                                        .gap_4()
                                        .overflow_scroll()
                                        .child(rendered_modal),
                                ),
                        ),
                )
            })
        } else {
            None
        };

        div()
            .id("settings-window")
            .key_context("settings_window")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_editor)
            .text_color(theme.text_primary)
            .font_family("Segoe UI")
            .cursor(gpui::CursorStyle::Arrow)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_window_key(event, window, cx);
            }))
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.render_settings_nav(window, cx))
                    .child(self.render_settings_content(window, cx)),
            )
            .when_some(plugin_modal_overlay, |this, overlay| this.child(overlay))
    }

    /// Renders the standalone title bar for the Settings window.
    fn render_titlebar(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_maximized = window.is_maximized();
        let is_ru = self.config.ui.language.starts_with("ru");
        let title = if is_ru {
            "Настройки — Vibecodium"
        } else {
            "Settings — Vibecodium"
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(34.0))
            .w_full()
            .bg(theme.bg_sidebar)
            .border_b_1()
            .border_color(theme.border_subtle)
            .flex_shrink_0()
            // Draggable area with title and icon
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .items_center()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag)
                    // `occlude()` keeps the full-window root out of the hit test
                    // over this region. The root carries `track_focus`, and a
                    // tracked element calls `Window::prevent_default()` on
                    // mouse-down; the Windows backend treats a prevented-default
                    // non-client mouse-down as handled and therefore skips the
                    // OS window move, so the title bar stopped dragging. With the
                    // root blocked behind this hitbox `default_prevented` stays
                    // false and the platform handles the drag itself.
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count >= 2 {
                            // The click reaches DefWindowProc now, so it would
                            // toggle maximize on its own. Claim the event to keep
                            // our zoom the only toggle (and only on the 2nd click).
                            window.prevent_default();
                            if event.click_count == 2 {
                                window.zoom_window();
                            }
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w(px(138.0))
                            .px_3()
                            .gap_2()
                            .h_full()
                            .child(icons::icon(icons::ICON_SETTINGS, 14.0, theme.accent))
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_secondary)
                                    .child(title)
                            )
                    )
            )
            // Window controls (Minimize, Maximize/Restore, Close). Marking them as
            // platform control areas gives the OS its own non-client handling of
            // these buttons — the Snap Layouts flyout over "maximize" in particular —
            // matching the main and catalog windows.
            //
            // The `on_mouse_down`/`stop_propagation` on each button is load-bearing:
            // the platform sets its own `nc_button_pressed` only when the non-client
            // mouse-down is NOT handled, and then repeats the action on mouse-up.
            // Claiming the down is what keeps the action single — otherwise a
            // maximize would toggle once here and once in the platform, cancelling out.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .w(px(138.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("set-win-ctrl-min")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor(gpui::CursorStyle::PointingHand)
                            .window_control_area(WindowControlArea::Min)
                            .hover(|s| s.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MINIMIZE, 11.0, theme.text_secondary))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|_, _, window, _| {
                                window.minimize_window();
                            }))
                    )
                    .child(
                        div()
                            .id("set-win-ctrl-max")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor(gpui::CursorStyle::PointingHand)
                            .window_control_area(WindowControlArea::Max)
                            .hover(|s| s.bg(theme.bg_hover))
                            .child(icons::icon(
                                if is_maximized { icons::ICON_RESTORE } else { icons::ICON_MAXIMIZE },
                                11.0,
                                theme.text_secondary,
                            ))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|_, _, window, _| {
                                window.zoom_window();
                            }))
                    )
                    .child(
                        div()
                            .id("set-win-ctrl-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(46.0))
                            .h_full()
                            .cursor(gpui::CursorStyle::PointingHand)
                            .window_control_area(WindowControlArea::Close)
                            .hover(|s| s.bg(rgb(0xe81123)))
                            .child(icons::icon(icons::ICON_CLOSE, 13.0, theme.text_secondary))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_window(window, cx);
                            }))
                    )
            )
    }

    /// Handles keyboard events for the Settings window.
    pub fn handle_window_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            if self.active_plugin_select.is_some() || self.plugin_select_popup.is_some() {
                self.close_plugin_select(cx);
            } else if self.active_color_picker.is_some() {
                self.active_color_picker = None;
                cx.notify();
            } else if !self.search_query.is_empty() {
                self.search_filter.clear(&mut self.search_query, cx);
            } else {
                self.close_window(window, cx);
            }
            return;
        }

        if event.keystroke.modifiers.control && (event.keystroke.key == "," || event.keystroke.key == "<" || event.keystroke.key == "б" || event.keystroke.key == "Б") {
            self.close_window(window, cx);
            return;
        }

        if !self.search_focus_handle.is_focused(window)
            && self.active_color_picker.is_none()
            && self.active_plugin_input.is_none()
        {
            window.focus(&self.search_focus_handle);
            self.search_filter.is_focused = true;
            self.handle_search_key(event, window, cx);
        }
    }

    /// Dispatches keyboard events for the settings search input (based on quick_filter.rs).
    pub fn handle_search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.search_filter.reset_cursor_blink(cx);
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Escape: clear query or close window if already empty
        if key == "escape" {
            if !self.search_query.is_empty() {
                self.search_filter.clear(&mut self.search_query, cx);
            } else {
                self.close_window(window, cx);
            }
            cx.notify();
            return;
        }

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.search_filter.selection_anchor = Some(0);
            self.search_filter.cursor_pos = self.search_query.chars().count();
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                let selected: String = self.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                let selected: String = self.search_query.chars().skip(s_start).take(s_end - s_start).collect();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                let mut chars: Vec<char> = self.search_query.chars().collect();
                chars.drain(s_start..s_end);
                self.search_query = chars.into_iter().collect();
                self.search_filter.cursor_pos = s_start;
                self.search_filter.selection_anchor = None;
                cx.notify();
            }
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        let mut chars: Vec<char> = self.search_query.chars().collect();
                        let pos = if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                            chars.drain(s_start..s_end);
                            self.search_filter.selection_anchor = None;
                            s_start
                        } else {
                            self.search_filter.cursor_pos.min(chars.len())
                        };
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        self.search_filter.cursor_pos = pos + filtered.chars().count();
                        self.search_query = chars.into_iter().collect();

                        // If user pastes a deep link like vibecodium://settings/ui.theme, navigate to it!
                        let trimmed = self.search_query.trim();
                        let target_id = if let Some(stripped) = trimmed.strip_prefix("vibecodium://settings/") {
                            Some(stripped)
                        } else if get_setting_by_id(trimmed).is_some() {
                            Some(trimmed)
                        } else {
                            None
                        };
                        if let Some(id) = target_id {
                            if let Some(setting) = get_setting_by_id(id) {
                                self.navigate_to_setting(&setting.id, cx);
                            }
                        }

                        cx.notify();
                    }
                }
            }
            return;
        }

        // Enter key (navigate to first match if any)
        if key == "enter" {
            let query = self.search_query.trim();
            let results = search_settings(query);
            if let Some(first) = results.first() {
                let id = first.id;
                self.navigate_to_setting(id, cx);
            }
            return;
        }

        if ctrl {
            if key == "," || key == "<" || key == "б" || key == "Б" {
                window.remove_window();
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            if shift {
                if self.search_filter.selection_anchor.is_none() {
                    self.search_filter.selection_anchor = Some(self.search_filter.cursor_pos);
                }
                if self.search_filter.cursor_pos > 0 {
                    self.search_filter.cursor_pos -= 1;
                }
            } else if let Some((s_start, _)) = self.search_filter.selection_range() {
                self.search_filter.cursor_pos = s_start;
                self.search_filter.selection_anchor = None;
            } else if self.search_filter.cursor_pos > 0 {
                self.search_filter.cursor_pos -= 1;
            }
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            let total_chars = self.search_query.chars().count();
            if shift {
                if self.search_filter.selection_anchor.is_none() {
                    self.search_filter.selection_anchor = Some(self.search_filter.cursor_pos);
                }
                if self.search_filter.cursor_pos < total_chars {
                    self.search_filter.cursor_pos += 1;
                }
            } else if let Some((_, s_end)) = self.search_filter.selection_range() {
                self.search_filter.cursor_pos = s_end;
                self.search_filter.selection_anchor = None;
            } else if self.search_filter.cursor_pos < total_chars {
                self.search_filter.cursor_pos += 1;
            }
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            if shift {
                if self.search_filter.selection_anchor.is_none() {
                    self.search_filter.selection_anchor = Some(self.search_filter.cursor_pos);
                }
                self.search_filter.cursor_pos = 0;
            } else {
                self.search_filter.selection_anchor = None;
                self.search_filter.cursor_pos = 0;
            }
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            let total_chars = self.search_query.chars().count();
            if shift {
                if self.search_filter.selection_anchor.is_none() {
                    self.search_filter.selection_anchor = Some(self.search_filter.cursor_pos);
                }
                self.search_filter.cursor_pos = total_chars;
            } else {
                self.search_filter.selection_anchor = None;
                self.search_filter.cursor_pos = total_chars;
            }
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            let mut chars: Vec<char> = self.search_query.chars().collect();
            if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                chars.drain(s_start..s_end);
                self.search_filter.cursor_pos = s_start;
                self.search_filter.selection_anchor = None;
                self.search_query = chars.into_iter().collect();
                cx.notify();
            } else {
                let pos = self.search_filter.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.search_filter.cursor_pos = pos - 1;
                    self.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
            return;
        }

        // Delete
        if key == "delete" {
            let mut chars: Vec<char> = self.search_query.chars().collect();
            if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                chars.drain(s_start..s_end);
                self.search_filter.cursor_pos = s_start;
                self.search_filter.selection_anchor = None;
                self.search_query = chars.into_iter().collect();
                cx.notify();
            } else {
                let pos = self.search_filter.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    self.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
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
                    .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                    .collect();
                if !filtered.is_empty() {
                    let mut chars: Vec<char> = self.search_query.chars().collect();
                    let pos = if let Some((s_start, s_end)) = self.search_filter.selection_range() {
                        chars.drain(s_start..s_end);
                        self.search_filter.selection_anchor = None;
                        s_start
                    } else {
                        self.search_filter.cursor_pos.min(chars.len())
                    };
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    self.search_filter.cursor_pos = pos + filtered.chars().count();
                    self.search_query = chars.into_iter().collect();
                    cx.notify();
                }
            }
        }
    }

    fn render_search_bar(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let is_search_focused = self.search_focus_handle.is_focused(window);
        let pixel_scale = window.scale_factor();
        let is_ru = self.config.ui.language.starts_with("ru");
        let placeholder = if is_ru { "Поиск параметров..." } else { "Search settings..." };

        div()
            .id("settings-search-box")
            .track_focus(&self.search_focus_handle)
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .px_2()
            .py_1()
            .bg(theme.bg_editor)
            .border_1()
            .border_color(if is_search_focused { theme.accent } else { theme.border_subtle })
            .rounded_md()
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                window.focus(&this.search_focus_handle);
                this.search_filter.is_focused = true;
                this.search_filter.reset_cursor_blink(cx);

                let click_x: f32 = event.position.x.into();
                let box_x = this.search_filter.box_origin_x.get();
                let scroll_x = this.search_filter.scroll_offset.get();
                let rel_x = (click_x - box_x + scroll_x).max(0.0);

                let clicked_pos = if this.search_query.is_empty() {
                    0
                } else {
                    let run = gpui::TextRun {
                        len: this.search_query.len(),
                        font: gpui::font("Segoe UI"),
                        color: Default::default(),
                        underline: Default::default(),
                        strikethrough: None,
                        background_color: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(this.search_query.clone()),
                        px(12.0),
                        &[run],
                        None,
                    );
                    let byte_idx = shaped.closest_index_for_x(px(rel_x));
                    this.search_query[..byte_idx.min(this.search_query.len())].chars().count()
                };

                let total_chars = this.search_query.chars().count();
                if event.click_count == 2 {
                    let chars: Vec<char> = this.search_query.chars().collect();
                    if !chars.is_empty() {
                        let pos = clicked_pos.min(chars.len().saturating_sub(1));
                        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
                        let target_is_word = is_word_char(chars[pos]);
                        let mut start = pos;
                        while start > 0 && is_word_char(chars[start - 1]) == target_is_word && !chars[start - 1].is_whitespace() {
                            start -= 1;
                        }
                        let mut end = pos + 1;
                        while end < chars.len() && is_word_char(chars[end]) == target_is_word && !chars[end].is_whitespace() {
                            end += 1;
                        }
                        this.search_filter.selection_anchor = Some(start);
                        this.search_filter.cursor_pos = end;
                    }
                    this.search_filter.is_mouse_selecting = false;
                } else if event.click_count >= 3 {
                    this.search_filter.selection_anchor = Some(0);
                    this.search_filter.cursor_pos = total_chars;
                    this.search_filter.is_mouse_selecting = false;
                } else if event.modifiers.shift {
                    if this.search_filter.selection_anchor.is_none() {
                        this.search_filter.selection_anchor = Some(this.search_filter.cursor_pos);
                    }
                    this.search_filter.cursor_pos = clicked_pos;
                    this.search_filter.is_mouse_selecting = true;
                } else {
                    this.search_filter.cursor_pos = clicked_pos;
                    this.search_filter.selection_anchor = Some(clicked_pos);
                    this.search_filter.is_mouse_selecting = true;
                }
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let query = this.search_query.clone();
                this.search_filter.handle_mouse_move(event, &query, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.search_filter.handle_mouse_up(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_search_key(event, window, cx);
            }))
            .child(icons::icon(icons::ICON_SEARCH, 13.0, theme.text_muted))
            .child({
                let query = &self.search_query;
                let query_chars: Vec<char> = query.chars().collect();
                let cursor_pos = self.search_filter.cursor_pos.min(query_chars.len());
                let selection_range = self.search_filter.selection_range();

                let run = gpui::TextRun {
                    len: query.len(),
                    font: gpui::font("Segoe UI"),
                    color: Default::default(),
                    underline: Default::default(),
                    strikethrough: None,
                    background_color: None,
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(query.clone()),
                    px(12.0),
                    &[run],
                    None,
                );

                let cursor_x = if query.is_empty() {
                    0.0
                } else {
                    let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                    crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
                };

                let visible_w = self.search_filter.box_width.get();
                let mut scroll_offset = self.search_filter.scroll_offset.get();
                if visible_w > 10.0 {
                    let text_w = f32::from(shaped.width);
                    if text_w + 4.0 <= visible_w {
                        scroll_offset = 0.0;
                    } else if cursor_x < scroll_offset {
                        scroll_offset = (cursor_x - 4.0).max(0.0);
                    } else if cursor_x > scroll_offset + visible_w - 6.0 {
                        scroll_offset = (cursor_x - visible_w + 6.0).max(0.0);
                    }
                } else if query.is_empty() {
                    scroll_offset = 0.0;
                }
                self.search_filter.scroll_offset.set(scroll_offset);

                let (sel_x1, sel_w) = if let Some((s_start, s_end)) = selection_range {
                    if !query.is_empty() {
                        let b1: usize = query_chars[..s_start.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                        let b2: usize = query_chars[..s_end.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
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

                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(16.0))
                    .overflow_hidden()
                    .child(
                        gpui::canvas(
                            {
                                let cell_x = self.search_filter.box_origin_x.clone();
                                let cell_w = self.search_filter.box_width.clone();
                                move |bounds, _, _| {
                                    cell_x.set(f32::from(bounds.origin.x));
                                    cell_w.set(f32::from(bounds.size.width));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    // 1. Selection background
                    .when(selection_range.is_some() && sel_w > 0.0, |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(sel_x1 - scroll_offset))
                                .top(px(0.0))
                                .w(px(sel_w))
                                .h(px(16.0))
                                .bg(theme.selection_bg)
                                .rounded_xs(),
                        )
                    })
                    // 2. Text or placeholder
                    .child(
                        if query.is_empty() {
                            div()
                                .font_family("Segoe UI")
                                .text_xs()
                                .line_height(px(16.0))
                                .text_color(theme.text_muted)
                                .whitespace_nowrap()
                                .child(placeholder.to_string())
                        } else {
                            div()
                                .absolute()
                                .left(px(-scroll_offset))
                                .top(px(0.0))
                                .font_family("Segoe UI")
                                .text_xs()
                                .line_height(px(16.0))
                                .text_color(theme.text_primary)
                                .whitespace_nowrap()
                                .child(query.clone())
                        }
                    )
                    // 3. Caret
                    .when(is_search_focused && self.search_filter.cursor_visible(), |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(cursor_x - scroll_offset))
                                .top(px(0.0))
                                .w(px(1.5))
                                .h(px(16.0))
                                .bg(theme.accent)
                                .rounded_xs(),
                        )
                    })
            })
            .when(!self.search_query.is_empty(), |s| {
                s.child(
                    div()
                        .id("btn-clear-settings-search")
                        .cursor_pointer()
                        .p_0p5()
                        .hover(|h| h.bg(theme.bg_hover))
                        .rounded_xs()
                        .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                            cx.stop_propagation();
                        }))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search_filter.clear(&mut this.search_query, cx);
                            window.focus(&this.search_focus_handle);
                        }))
                )
            })
    }

    /// Renders the vertical tab navigation sidebar inside the Settings window with search.
    fn render_settings_nav(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active_tab = &self.settings_tab;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.search_query.trim();
        let search_results = if !query.is_empty() {
            search_settings(query)
        } else {
            Vec::new()
        };

        div()
            .id("settings-nav-sidebar")
            .flex()
            .flex_col()
            .w(px(220.0))
            .min_w(px(220.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .bg(theme.bg_sidebar)
            // Top: Search input
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(self.render_search_bar(window, cx))
            )
            // Tab list or search results
            .child(
                div()
                    .id("settings-nav-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .p_2()
                    .gap_1()
                    .when(!query.is_empty(), |this| {
                        if search_results.is_empty() {
                            this.child(
                                div()
                                    .p_3()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(if is_ru { "Ничего не найдено" } else { "No settings found" })
                            )
                        } else {
                            this.children(search_results.into_iter().enumerate().map(|(idx, setting)| {
                                let is_active = match setting.tab_id {
                                    "ui" => *active_tab == SettingsTab::Ui,
                                    "editor" => *active_tab == SettingsTab::Editor,
                                    "reader" => *active_tab == SettingsTab::Reader,
                                    "search" => *active_tab == SettingsTab::Search,
                                    "templates" => *active_tab == SettingsTab::Templates,
                                    "files" => *active_tab == SettingsTab::Files,
                                    "analysis" => *active_tab == SettingsTab::Analysis,
                                    "mcp" => *active_tab == SettingsTab::Mcp,
                                    "history" => *active_tab == SettingsTab::History,
                                    "trash" => *active_tab == SettingsTab::Trash,
                                    "system" => *active_tab == SettingsTab::System,
                                    "shortcuts" => *active_tab == SettingsTab::Shortcuts,
                                    "core-plugins" => *active_tab == SettingsTab::CorePlugins,
                                    "community-plugins" => *active_tab == SettingsTab::CommunityPlugins,
                                    other => *active_tab == SettingsTab::PluginSettings(other.to_string()),
                                };
                                let text_color = if is_active { theme.text_primary } else { theme.text_secondary };
                                let s_id = setting.id;

                                div()
                                    .id(gpui::ElementId::from(("search-res", idx)))
                                    .flex()
                                    .flex_col()
                                    .px_2()
                                    .py_1p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .bg(if is_active { theme.bg_hover } else { rgba(0x00000000) })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.navigate_to_setting(s_id, cx);
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(text_color)
                                            .child(setting.title(is_ru))
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child(format!("{}: {}", setting.tab_id, setting.id))
                                    )
                            }))
                        }
                    })
                    .when(query.is_empty(), |this| {
                        this.children(SettingsTab::all().iter().enumerate().map(|(idx, tab)| {
                            let is_active = tab == active_tab;
                            let text_color = if is_active { theme.text_primary } else { theme.text_secondary };
                            let icon_color = if is_active { theme.accent } else { theme.text_muted };
                            let bg_color = if is_active { theme.bg_hover } else { rgba(0x00000000) };
                            let tab_clone = tab.clone();

                            div()
                                .id(gpui::ElementId::from(("nav-tab", idx)))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(bg_color)
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(icons::icon(tab.icon(), 15.0, icon_color))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(if is_active { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                                        .text_color(text_color)
                                        .child(tab.title(&self.config.ui.language)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings_tab = tab_clone.clone();
                                    cx.notify();
                                }))
                        }))
                    })
            )
    }

    /// Renders the active tab's settings content container.
    fn render_settings_content(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let scroll_handle = self.settings_scroll_handle.clone();
        let is_catalog_open = false;

        div()
            .id("settings-content-scroll")
            .relative()
            .track_scroll(&self.settings_scroll_handle)
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_scroll()
            .bg(theme.bg_editor)
            .rounded_br_xl()
            .cursor(gpui::CursorStyle::Arrow)
            .p_6()
            .gap_5()
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                if this.active_color_picker.is_some() {
                    this.active_color_picker = None;
                    cx.notify();
                }
            }))
            .child(match self.settings_tab {
                SettingsTab::Ui => self.render_ui_tab(cx),
                SettingsTab::Editor => self.render_editor_tab(cx),
                SettingsTab::Reader => self.render_reader_tab(cx),
                SettingsTab::Search => self.render_search_tab(cx),
                SettingsTab::Templates => self.render_templates_tab(cx),
                SettingsTab::Files => self.render_files_tab(cx),
                SettingsTab::Analysis => self.render_analysis_tab(cx),
                SettingsTab::Mcp => self.render_mcp_tab(cx),
                SettingsTab::History => self.render_history_tab(cx),
                SettingsTab::Trash => self.render_trash_tab(cx),
                SettingsTab::System => self.render_system_tab(cx),
                SettingsTab::Shortcuts => self.render_shortcuts_tab(cx),
                SettingsTab::CorePlugins => self.render_core_plugins_tab(cx),
                SettingsTab::CommunityPlugins => self.render_community_plugins_tab(cx),
                SettingsTab::PluginSettings(ref id) => self.render_plugin_settings_tab(id, window, cx),
            })
            .child(
                canvas(
                    |viewport, _, _| viewport,
                    move |viewport, _, window, _cx| {
                        // Capture-перехват колеса раньше дефолтного скролла div:
                        // делим Lines-тики (мышь) на 3 — один тик ≈ одна строка,
                        // пиксельные дельты тачпада идут как есть.
                        let current_view = window.current_view();
                        window.on_mouse_event(
                            move |event: &ScrollWheelEvent, phase, window, cx| {
                                if !is_catalog_open
                                    && phase == DispatchPhase::Capture
                                    && viewport.contains(&event.position)
                                {
                                    let dy = scaled_wheel_dy(
                                        event.delta,
                                        f32::from(window.line_height()),
                                    );
                                    if dy != 0.0 {
                                        let cur = f32::from(scroll_handle.offset().y);
                                        let max =
                                            f32::from(scroll_handle.max_offset().height);
                                        let next = (cur + dy).clamp(-max, 0.0);
                                        if next != cur {
                                            scroll_handle
                                                .set_offset(point(px(0.0), px(next)));
                                            cx.notify(current_view);
                                        }
                                    }
                                    cx.stop_propagation();
                                }
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
    }

    // =========================================================================
    // Reusable UI Building Blocks
    // =========================================================================

    /// Wraps a collection of rows inside a titled card container.
    fn render_settings_card(&self, title: &str, rows: Vec<gpui::Div>) -> gpui::Div {
        let theme = &self.theme;
        let row_count = rows.len();
        let formatted_rows = rows.into_iter().enumerate().map(|(idx, row)| {
            if idx + 1 == row_count {
                row.border_b_0()
            } else {
                row
            }
        });

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_secondary)
                    .child(title.to_uppercase()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .bg(theme.bg_card)
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px_4()
                    .children(formatted_rows),
            )
    }

    /// Renders an individual setting row with deep link copy button, highlight effect, label, optional hint, and control widget.
    fn render_setting_row_with_id(
        &self,
        setting_id: Option<&'static str>,
        label: &str,
        hint: Option<&str>,
        control: impl IntoElement,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let theme = &self.theme;
        let is_highlighted = setting_id
            .map(|id| self.highlighted_setting_id.as_deref() == Some(id))
            .unwrap_or(false);
        let is_copied = setting_id
            .map(|id| self.copied_setting_url_id.as_deref() == Some(id))
            .unwrap_or(false);

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .py_3()
            .px_2()
            .rounded_md()
            .border_b_1()
            .border_color(if is_highlighted { theme.accent } else { theme.border_subtle })
            .bg(if is_highlighted {
                gpui::Rgba { a: 0.12, ..theme.accent }
            } else {
                rgba(0x00000000)
            })
            .when_some(setting_id, |this, s_id| {
                this.child(
                    div()
                        .w(px(24.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .mr_2()
                        .child(
                            div()
                                .id(gpui::ElementId::from(gpui::SharedString::from(format!("copy-link-{}", s_id))))
                                .p_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|h| h.bg(theme.bg_hover))
                                .child(if is_copied {
                                    icons::icon(icons::ICON_CHECK, 13.0, theme.accent)
                                } else {
                                    icons::icon(icons::ICON_LINK, 13.0, theme.text_muted)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.copy_setting_link(s_id, cx);
                                }))
                        )
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .mr_4()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .child(label.to_string()),
                    )
                    .when_some(hint, |this, h| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(h.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .child(control),
            )
    }

    /// Renders an individual setting row with a label, optional hint, and control widget.
    fn render_setting_row(&self, label: &str, hint: Option<&str>, control: impl IntoElement) -> gpui::Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .mr_4()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .child(label.to_string()),
                    )
                    .when_some(hint, |this, h| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(h.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .child(control),
            )
    }

    /// Renders an interactive toggle switch widget.
    fn render_toggle_switch(&self, checked: bool) -> gpui::Div {
        let theme = &self.theme;
        let track_bg = if checked { theme.accent } else { theme.border };
        let knob_left = if checked { px(21.0) } else { px(3.0) };

        div()
            .flex_none()
            .relative()
            .w(px(40.0))
            .h(px(22.0))
            .rounded_full()
            .bg(track_bg)
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .top(px(3.0))
                    .left(knob_left)
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .shadow_sm(),
            )
    }

    /// Renders a numeric increment/decrement stepper control.
    fn render_stepper<F1, F2>(
        &self,
        id: &'static str,
        value_display: &str,
        on_dec: F1,
        on_inc: F2,
        cx: &Context<Self>,
    ) -> gpui::Div
    where
        F1: Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
        F2: Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
    {
        let theme = &self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .bg(theme.bg_sidebar)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .p_0p5()
            .child(
                div()
                    .id(gpui::ElementId::from((id, 0usize)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(24.0))
                    .h(px(24.0))
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(div().text_sm().text_color(theme.text_secondary).child("−"))
                    .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
                        on_dec(this, cx);
                    })),
            )
            .child(
                div()
                    .w(px(56.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(value_display.to_string()),
            )
            .child(
                div()
                    .id(gpui::ElementId::from((id, 1usize)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(24.0))
                    .h(px(24.0))
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(div().text_sm().text_color(theme.text_secondary).child("+"))
                    .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
                        on_inc(this, cx);
                    })),
            )
    }

    /// Renders bold (B) and italic (I) toggle buttons for a heading level.
    fn render_heading_format_toggles<F1, F2>(
        &self,
        prefix: &'static str,
        bold: bool,
        italic: bool,
        on_toggle_bold: F1,
        on_toggle_italic: F2,
        cx: &Context<Self>,
    ) -> gpui::Div
    where
        F1: Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
        F2: Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
    {
        let theme = &self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .id(gpui::ElementId::from((prefix, 10usize)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(26.0))
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .when(bold, |s| {
                        s.bg(gpui::Rgba { a: 0.18, ..theme.accent })
                            .border_color(theme.accent)
                            .text_color(theme.accent)
                    })
                    .when(!bold, |s| {
                        s.bg(theme.bg_sidebar)
                            .border_color(theme.border)
                            .text_color(theme.text_muted)
                            .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_secondary))
                    })
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("B"),
                    )
                    .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
                        on_toggle_bold(this, cx);
                    })),
            )
            .child(
                div()
                    .id(gpui::ElementId::from((prefix, 11usize)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(26.0))
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .when(italic, |s| {
                        s.bg(gpui::Rgba { a: 0.18, ..theme.accent })
                            .border_color(theme.accent)
                            .text_color(theme.accent)
                    })
                    .when(!italic, |s| {
                        s.bg(theme.bg_sidebar)
                            .border_color(theme.border)
                            .text_color(theme.text_muted)
                            .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_secondary))
                    })
                    .child(
                        div()
                            .text_xs()
                            .italic()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("I"),
                    )
                    .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
                        on_toggle_italic(this, cx);
                    })),
            )
    }

    /// Renders a compact borderless color dot widget with an attached deferred context menu.
    fn render_color_picker_button(
        &self,
        picker_id: &'static str,
        active_color: &str,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let is_open = self.active_color_picker == Some(picker_id);
        let current_color_rgb = crate::markdown::heading_color_to_rgb(active_color);

        div()
            .relative()
            .child(
                div()
                    .id(gpui::ElementId::from((picker_id, 0usize)))
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(current_color_rgb)
                    .cursor_pointer()
                    .hover(|h| h.opacity(0.85))
                    .on_click(cx.listener(move |this: &mut SettingsView, _, window, cx| {
                        if this.active_color_picker == Some(picker_id) {
                            this.active_color_picker = None;
                            this.hex_is_focused = false;
                        } else {
                            this.active_color_picker = Some(picker_id);
                            this.hex_is_focused = true;
                            this.reset_hex_cursor_blink(cx);
                            let curr = match picker_id {
                                "h1-color" => this.config.editor.headings.h1.color_name.as_str(),
                                "h2-color" => this.config.editor.headings.h2.color_name.as_str(),
                                "h3-color" => this.config.editor.headings.h3.color_name.as_str(),
                                "h4-color" => this.config.editor.headings.h4.color_name.as_str(),
                                "h5-color" => this.config.editor.headings.h5.color_name.as_str(),
                                "h6-color" => this.config.editor.headings.h6.color_name.as_str(),
                                _ => "blue",
                            };
                            this.custom_hex_input = crate::markdown::heading_color_to_hex(curr);
                            this.hex_selection_anchor = None;
                            this.hex_cursor_pos = this.custom_hex_input.chars().count();
                            window.focus(&this.custom_hex_focus_handle);
                        }
                        cx.notify();
                    }))
            )
            .when(is_open, |d| {
                d.child(
                    gpui::deferred(
                        self.render_color_context_menu(picker_id, active_color, cx)
                    ).with_priority(999)
                )
            })
    }

    /// Renders a context menu palette right below the color circle with Okabe-Ito matrix & Hex input.
    fn render_color_context_menu(
        &self,
        picker_id: &'static str,
        active_color: &str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = &self.theme;
        let preview_rgb = crate::markdown::parse_hex_color(&self.custom_hex_input)
            .unwrap_or_else(|| crate::markdown::heading_color_to_rgb(active_color));
        let font_id = cx.text_system().resolve_font(&gpui::font("Consolas"));
        let char_w = cx
            .text_system()
            .advance(font_id, px(13.0), '0')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.15);

        div()
            .id(gpui::ElementId::from((picker_id, 1usize)))
            .absolute()
            .top(px(28.0))
            .right_0()
            .w(px(252.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .rounded_xl()
            .shadow_lg()
            .p_2p5()
            .flex()
            .flex_col()
            .gap_2()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.active_color_picker = None;
                this.hex_is_focused = false;
                cx.notify();
            }))
            // 1. Okabe-Ito colors matrix in a row (no borders!)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .children(crate::markdown::HEADING_COLOR_PRESETS.iter().enumerate().map(|(idx, preset)| {
                        let is_sel = preset.id.eq_ignore_ascii_case(active_color)
                            || preset.label.eq_ignore_ascii_case(active_color)
                            || preset.hex.eq_ignore_ascii_case(active_color);
                        let (pr, pg, pb) = preset.rgb;
                        let prgb = gpui::rgb(((pr as u32) << 16) | ((pg as u32) << 8) | (pb as u32));
                        let pid = preset.id;
                        let phex = preset.hex;

                        div()
                            .id(gpui::ElementId::from((picker_id, 10usize + idx)))
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded_full()
                            .bg(prgb)
                            .cursor_pointer()
                            .hover(|h| h.opacity(0.85))
                            .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
                                this.active_color_picker = None;
                                this.hex_is_focused = false;
                                this.custom_hex_input = phex.to_string();
                                match picker_id {
                                    "h1-color" => this.config.editor.headings.h1.color_name = pid.to_string(),
                                    "h2-color" => this.config.editor.headings.h2.color_name = pid.to_string(),
                                    "h3-color" => this.config.editor.headings.h3.color_name = pid.to_string(),
                                    "h4-color" => this.config.editor.headings.h4.color_name = pid.to_string(),
                                    "h5-color" => this.config.editor.headings.h5.color_name = pid.to_string(),
                                    "h6-color" => this.config.editor.headings.h6.color_name = pid.to_string(),
                                    _ => {}
                                }
                                this.save_settings(cx);
                                cx.notify();
                            }))
                            .when(is_sel, |s| {
                                s.flex().items_center().justify_center().child(
                                    icons::icon(icons::ICON_CHECK, 12.0, theme.text_on_accent)
                                )
                            })
                    }))
            )
            // 2. Divider
            .child(
                div().h_px().w_full().bg(theme.border_subtle)
            )
            // 3. Hex input & custom color picker row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    // Preview circle of custom hex (no borders!)
                    .child(
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded_full()
                            .bg(preview_rgb)
                            .shadow_sm()
                            .flex_shrink_0()
                    )
                    // Hex text input box
                    .child(
                        div()
                            .id("custom-hex-input-box")
                            .track_focus(&self.custom_hex_focus_handle)
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h(px(26.0))
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .focus(|f| f.border_color(theme.accent))
                            .rounded_md()
                            .cursor(gpui::CursorStyle::IBeam)
                            .overflow_hidden()
                            .child(
                                gpui::canvas(
                                    {
                                        let cell = self.hex_box_origin_x.clone();
                                        move |bounds, _, _| {
                                            cell.set(f32::from(bounds.origin.x));
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                window.focus(&this.custom_hex_focus_handle);
                                this.hex_is_focused = true;
                                this.reset_hex_cursor_blink(cx);

                                let box_x = this.hex_box_origin_x.get();
                                let click_x: f32 = event.position.x.into();
                                let rel_x = (click_x - box_x - 8.0).max(0.0);
                                let total_len = this.custom_hex_input.chars().count();
                                let clicked_pos = ((rel_x / char_w).round() as usize).min(total_len);

                                if event.click_count == 2 {
                                    this.hex_selection_anchor = Some(0);
                                    this.hex_cursor_pos = total_len;
                                    this.hex_is_mouse_selecting = false;
                                } else {
                                    this.hex_cursor_pos = clicked_pos;
                                    this.hex_selection_anchor = Some(clicked_pos);
                                    this.hex_mouse_drag_start_x = Some(click_x);
                                    this.hex_is_mouse_selecting = true;
                                }
                                cx.notify();
                            }))
                            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                                if !this.hex_is_mouse_selecting {
                                    return;
                                }
                                let box_x = this.hex_box_origin_x.get();
                                let curr_x: f32 = event.position.x.into();
                                let rel_x = (curr_x - box_x - 8.0).max(0.0);
                                let total_len = this.custom_hex_input.chars().count();
                                let target_pos = ((rel_x / char_w).round() as usize).min(total_len);

                                if this.hex_cursor_pos != target_pos {
                                    this.hex_cursor_pos = target_pos;
                                    cx.notify();
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.hex_is_mouse_selecting = false;
                                this.hex_mouse_drag_start_x = None;
                                if this.hex_selection_anchor == Some(this.hex_cursor_pos) {
                                    this.hex_selection_anchor = None;
                                }
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                this.handle_custom_hex_key(event, cx);
                            }))
                            // 1. Selection background
                            .when_some(self.hex_selection_range(), |s, (start, end)| {
                                let sel_x = 8.0 + (start as f32) * char_w;
                                let sel_w = ((end - start) as f32) * char_w;
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(sel_x))
                                        .top(px(4.0))
                                        .w(px(sel_w))
                                        .h(px(18.0))
                                        .bg(theme.selection_bg)
                                        .rounded_xs()
                                )
                            })
                            // 2. Text or placeholder
                            .child(
                                div()
                                    .absolute()
                                    .left(px(8.0))
                                    .top(px(4.0))
                                    .h(px(18.0))
                                    .font_family("Consolas")
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(if self.custom_hex_input.is_empty() { theme.text_muted } else { theme.text_primary })
                                    .child(if self.custom_hex_input.is_empty() { "#HEX".to_string() } else { self.custom_hex_input.clone() })
                            )
                            // 3. Caret (only shown when focused and not selecting)
                            .when(self.hex_is_focused && self.hex_blink.visible() && self.hex_selection_range().is_none(), |s| {
                                let cursor_x = 8.0 + (self.hex_cursor_pos as f32) * char_w;
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(cursor_x))
                                        .top(px(4.0))
                                        .w(px(1.5))
                                        .h(px(18.0))
                                        .bg(theme.accent)
                                        .rounded_xs()
                                )
                            })
                    )
                    // Eyedropper / Color Picker (Пипетка) button
                    .child(
                        div()
                            .id("btn-eyedropper-picker")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(26.0))
                            .h(px(26.0))
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_PIPETTE, 13.0, theme.text_secondary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(chosen_hex) = crate::markdown::open_system_color_picker(&this.custom_hex_input) {
                                    this.custom_hex_input = chosen_hex;
                                    this.apply_custom_hex_if_valid(cx);
                                    cx.notify();
                                }
                            }))
                            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                                if let Some(sampled_hex) = crate::markdown::sample_screen_color_at_cursor() {
                                    this.custom_hex_input = sampled_hex;
                                    this.apply_custom_hex_if_valid(cx);
                                    cx.notify();
                                }
                            }))
                    )
            )
    }

    /// Renders a clickable styled pill dropdown or selector button.
    pub fn render_select_pill(&self, label: &str) -> gpui::Div {
        render_select_pill_styled(label, &self.theme)
    }
}

/// Renders a clickable styled pill dropdown or selector button with given theme.
pub fn render_select_pill_styled(label: &str, theme: &crate::theme::Theme) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .bg(theme.bg_sidebar)
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .text_xs()
        .text_color(theme.text_primary)
        .child(label.to_string())
        .child(icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted))
}

impl SettingsView {

    // =========================================================================
    // 1. Interface (Ui) Tab
    // =========================================================================

    /// Renders the UI and Display settings tab.
    fn render_ui_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let display_rows = vec![
            self.render_setting_row_with_id(
                Some("ui.theme"),
                &self.t("settings.ui.theme"),
                Some(&self.t("settings.ui.theme_desc")),
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .bg(theme.bg_sidebar)
                    .p_0p5()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .id("btn-theme-system")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.ui.theme == "system", |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.ui.theme != "system", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.ui.theme_system"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.ui.theme = "system".to_string();
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-theme-light")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.ui.theme == "light", |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.ui.theme != "light", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.ui.theme_light"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.ui.theme = "light".to_string();
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-theme-dark")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.ui.theme == "dark", |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.ui.theme != "dark", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.ui.theme_dark"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.ui.theme = "dark".to_string();
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.language"),
                &self.t("settings.ui.language"),
                Some(&self.t("settings.ui.language_desc")),
                {
                    let current_code = crate::i18n::LocaleInfo::normalize(&config.ui.language);
                    let current_loc = crate::i18n::LocaleInfo::for_code(current_code)
                        .unwrap_or(&crate::i18n::SUPPORTED_LOCALES[0]);
                    let pill_label = format!("{} ({})", current_loc.native_name, current_loc.english_name);

                    div()
                        .relative()
                        .child(
                            div()
                                .id("select-language-btn")
                                .cursor_pointer()
                                .child(self.render_select_pill(&pill_label))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.locale_popup_is_open() {
                                        this.close_locale_popup(cx);
                                    } else {
                                        this.open_locale_popup(window, cx);
                                    }
                                })),
                        )
                        // The smart locale picker hangs from this button, same
                        // anchor the old dropdown menu used.
                        .when_some(self.locale_popup.clone(), |parent, popup| {
                            parent.child(
                                gpui::deferred(
                                    div()
                                        .id("language-dropdown-menu")
                                        .absolute()
                                        .top_full()
                                        .right_0()
                                        .mt_1()
                                        .child(popup),
                                )
                                .with_priority(999),
                            )
                        })
                },
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.primaryColor"),
                &self.t("settings.ui.accent_color"),
                Some(&self.t("settings.ui.accent_color_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2p5()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .child(div().w(px(14.0)).h(px(14.0)).rounded_xs().bg(theme.accent))
                    .child(div().text_xs().text_color(theme.text_primary).child(config.ui.primary_color.clone())),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.sidebarSmartScroll"),
                &self.t("settings.ui.sidebar_smart_scroll"),
                Some(&self.t("settings.ui.sidebar_smart_scroll_desc")),
                div()
                    .id("row-sidebar-smart-scroll")
                    .child(self.render_toggle_switch(config.ui.sidebar_smart_scroll))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.ui.sidebar_smart_scroll = !this.config.ui.sidebar_smart_scroll;
                        // Сбрасываем курсорное состояние, чтобы при включении
                        // список не прыгал от устаревшей позиции мыши.
                        if let Some(app) = this.weak_app.upgrade() {
                            app.update(cx, |app, _| {
                                app.sidebar_smart_last_mouse_y.set(None);
                                app.sidebar_smart_target_y.set(None);
                            });
                        }
                        this.save_settings(cx);
                    })),
                cx,
            ),
        ];

        let font_rows = vec![
            self.render_setting_row(
                &self.t("settings.ui.font_family"),
                Some(&self.t("settings.ui.font_family_desc")),
                self.render_select_pill(&config.ui.font.font_family),
            ),
            self.render_setting_row(
                &self.t("settings.ui.font_weight"),
                Some(&self.t("settings.ui.font_weight_desc")),
                self.render_select_pill(if config.ui.font.font_weight >= 600 { "Semibold" } else { "Regular" }),
            ),
            self.render_setting_row(
                &self.t("settings.ui.font_size"),
                Some(&self.t("settings.ui.font_size_desc")),
                self.render_stepper(
                    "ui-font-size",
                    &format!("{} px", config.ui.font.font_size_base),
                    |this: &mut SettingsView, cx| {
                        if this.config.ui.font.font_size_base > 11 {
                            this.config.ui.font.font_size_base -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.ui.font.font_size_base < 24 {
                            this.config.ui.font.font_size_base += 1;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.ui.display_card"), display_rows))
            .child(self.render_settings_card(&self.t("settings.ui.font_card"), font_rows))
    }

    // =========================================================================
    // 2. Editor Tab
    // =========================================================================

    /// Renders the Editor layout, typography and behaviors settings tab.
    fn render_editor_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let heading_rows = vec![
            self.render_setting_row(
                &self.t("settings.editor.h1_title"),
                Some(&self.t("settings.editor.h1_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h1-size",
                        &format!("{} px", config.editor.headings.h1.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h1.font_size > 14 {
                                this.config.editor.headings.h1.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h1.font_size < 40 {
                                this.config.editor.headings.h1.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h1-color",
                        &config.editor.headings.h1.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h1",
                        config.editor.headings.h1.bold,
                        config.editor.headings.h1.italic,
                        |this, cx| {
                            this.config.editor.headings.h1.bold = !this.config.editor.headings.h1.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h1.italic = !this.config.editor.headings.h1.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
            self.render_setting_row(
                &self.t("settings.editor.h2_title"),
                Some(&self.t("settings.editor.h2_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h2-size",
                        &format!("{} px", config.editor.headings.h2.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h2.font_size > 13 {
                                this.config.editor.headings.h2.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h2.font_size < 36 {
                                this.config.editor.headings.h2.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h2-color",
                        &config.editor.headings.h2.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h2",
                        config.editor.headings.h2.bold,
                        config.editor.headings.h2.italic,
                        |this, cx| {
                            this.config.editor.headings.h2.bold = !this.config.editor.headings.h2.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h2.italic = !this.config.editor.headings.h2.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
            self.render_setting_row(
                &self.t("settings.editor.h3_title"),
                Some(&self.t("settings.editor.h3_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h3-size",
                        &format!("{} px", config.editor.headings.h3.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h3.font_size > 12 {
                                this.config.editor.headings.h3.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h3.font_size < 32 {
                                this.config.editor.headings.h3.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h3-color",
                        &config.editor.headings.h3.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h3",
                        config.editor.headings.h3.bold,
                        config.editor.headings.h3.italic,
                        |this, cx| {
                            this.config.editor.headings.h3.bold = !this.config.editor.headings.h3.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h3.italic = !this.config.editor.headings.h3.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
            self.render_setting_row(
                &self.t("settings.editor.h4_title"),
                Some(&self.t("settings.editor.h4_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h4-size",
                        &format!("{} px", config.editor.headings.h4.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h4.font_size > 11 {
                                this.config.editor.headings.h4.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h4.font_size < 28 {
                                this.config.editor.headings.h4.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h4-color",
                        &config.editor.headings.h4.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h4",
                        config.editor.headings.h4.bold,
                        config.editor.headings.h4.italic,
                        |this, cx| {
                            this.config.editor.headings.h4.bold = !this.config.editor.headings.h4.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h4.italic = !this.config.editor.headings.h4.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
            self.render_setting_row(
                &self.t("settings.editor.h5_title"),
                Some(&self.t("settings.editor.h5_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h5-size",
                        &format!("{} px", config.editor.headings.h5.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h5.font_size > 10 {
                                this.config.editor.headings.h5.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h5.font_size < 26 {
                                this.config.editor.headings.h5.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h5-color",
                        &config.editor.headings.h5.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h5",
                        config.editor.headings.h5.bold,
                        config.editor.headings.h5.italic,
                        |this, cx| {
                            this.config.editor.headings.h5.bold = !this.config.editor.headings.h5.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h5.italic = !this.config.editor.headings.h5.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
            self.render_setting_row(
                &self.t("settings.editor.h6_title"),
                Some(&self.t("settings.editor.h6_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h6-size",
                        &format!("{} px", config.editor.headings.h6.font_size),
                        |this, cx| {
                            if this.config.editor.headings.h6.font_size > 9 {
                                this.config.editor.headings.h6.font_size -= 1;
                                this.save_settings(cx);
                            }
                        },
                        |this, cx| {
                            if this.config.editor.headings.h6.font_size < 24 {
                                this.config.editor.headings.h6.font_size += 1;
                                this.save_settings(cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_color_picker_button(
                        "h6-color",
                        &config.editor.headings.h6.color_name,
                        cx,
                    ))
                    .child(self.render_heading_format_toggles(
                        "h6",
                        config.editor.headings.h6.bold,
                        config.editor.headings.h6.italic,
                        |this, cx| {
                            this.config.editor.headings.h6.bold = !this.config.editor.headings.h6.bold;
                            this.save_settings(cx);
                        },
                        |this, cx| {
                            this.config.editor.headings.h6.italic = !this.config.editor.headings.h6.italic;
                            this.save_settings(cx);
                        },
                        cx,
                    )),
            ),
        ];

        let font_rows = vec![
            {
                let current_font = config.editor.font.font_family.clone();
                self.render_setting_row(
                    &self.t("settings.editor.font_family"),
                    Some(&self.t("settings.editor.font_family_desc")),
                    div()
                        .relative()
                        .child(
                            div()
                                .id("select-editor-font-btn")
                                .cursor_pointer()
                                .child(self.render_select_pill(&current_font))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.font_popup_is_open() {
                                        this.close_font_popup(cx);
                                    } else {
                                        this.open_font_popup(window, cx);
                                    }
                                })),
                        )
                        // The smart font picker hangs from this button, same
                        // anchor the old dropdown menu used.
                        .when_some(self.font_popup.clone(), |parent, popup| {
                            parent.child(
                                gpui::deferred(
                                    div()
                                        .id("editor-font-dropdown-menu")
                                        .absolute()
                                        .top_full()
                                        .right_0()
                                        .mt_1()
                                        .child(popup),
                                )
                                .with_priority(999),
                            )
                        }),
                )
            },
            self.render_setting_row(
                &self.t("settings.editor.font_size"),
                Some(&self.t("settings.editor.font_size_desc")),
                self.render_stepper(
                    "editor-font-size",
                    &format!("{} px", config.editor.font.font_size_base),
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.font.font_size_base > 12 {
                            this.config.editor.font.font_size_base -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.font.font_size_base < 32 {
                            this.config.editor.font.font_size_base += 1;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                &self.t("settings.editor.line_height"),
                Some(&self.t("settings.editor.line_height_desc")),
                self.render_stepper(
                    "editor-line-height",
                    &format!("{:.2}", config.editor.line_height),
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.line_height > 1.2 {
                            this.config.editor.line_height = (this.config.editor.line_height - 0.05).max(1.2);
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.line_height < 2.5 {
                            this.config.editor.line_height = (this.config.editor.line_height + 0.05).min(2.5);
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        let tabs_display = self.t_fmt("settings.editor.live_tabs_unit", &[&config.editor.live_tabs.to_string()]);
        let layout_rows = vec![
            self.render_setting_row(
                &self.t("settings.editor.max_width"),
                Some(&self.t("settings.editor.max_width_desc")),
                self.render_stepper(
                    "editor-max-width",
                    &format!("{} ch", config.editor.max_width_ch),
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.max_width_ch > 40 {
                            this.config.editor.max_width_ch -= 5;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.max_width_ch < 140 {
                            this.config.editor.max_width_ch += 5;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row_with_id(
                Some("editor.wordWrap"),
                &self.t("settings.editor.word_wrap"),
                Some(&self.t("settings.editor.word_wrap_desc")),
                div()
                    .id("row-word-wrap")
                    .child(self.render_toggle_switch(self.word_wrap))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_word_wrap(cx);
                    })),
                cx,
            ),
            self.render_setting_row(
                &self.t("settings.editor.live_tabs"),
                Some(&self.t("settings.editor.live_tabs_desc")),
                self.render_stepper(
                    "editor-tabs",
                    &tabs_display,
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.live_tabs > 1 {
                            this.config.editor.live_tabs -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.live_tabs < 8 {
                            this.config.editor.live_tabs += 1;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row_with_id(
                Some("editor.wheelSmoothScroll"),
                &self.t("settings.editor.wheel_smooth_scroll"),
                Some(&self.t("settings.editor.wheel_smooth_scroll_desc")),
                div()
                    .id("row-wheel-smooth-scroll")
                    .child(self.render_toggle_switch(config.editor.wheel_smooth_scroll))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.wheel_smooth_scroll = !this.config.editor.wheel_smooth_scroll;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.scrollbarRuler"),
                &self.t("settings.editor.scrollbar_ruler"),
                Some(&self.t("settings.editor.scrollbar_ruler_desc")),
                div()
                    .id("row-scrollbar-ruler")
                    .child(self.render_toggle_switch(config.editor.scrollbar_ruler))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.scrollbar_ruler = !this.config.editor.scrollbar_ruler;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.wideScrollbar"),
                &self.t("settings.editor.wide_scrollbar"),
                Some(&self.t("settings.editor.wide_scrollbar_desc")),
                div()
                    .id("row-wide-scrollbar")
                    .child(self.render_toggle_switch(config.editor.wide_scrollbar))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.wide_scrollbar = !this.config.editor.wide_scrollbar;
                        this.save_settings(cx);
                    })),
                cx,
            ),
        ];

        let debounce_display = self.t_fmt("settings.editor.debounce_unit", &[&config.editor.save_debounce_ms.to_string()]);
        let input_rows = vec![
            self.render_setting_row(
                &self.t("settings.editor.smart_dashes"),
                Some(&self.t("settings.editor.smart_dashes_desc")),
                div()
                    .id("row-smart-dashes")
                    .child(self.render_toggle_switch(config.editor.smart_dashes))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.smart_dashes = !this.config.editor.smart_dashes;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.editor.list_callouts"),
                Some(&self.t("settings.editor.list_callouts_desc")),
                div()
                    .id("row-list-callouts")
                    .child(self.render_toggle_switch(config.editor.list_callouts))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.list_callouts = !this.config.editor.list_callouts;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.editor.link_suggest"),
                Some(&self.t("settings.editor.link_suggest_desc")),
                div()
                    .id("row-link-suggest")
                    .child(self.render_toggle_switch(config.editor.link_suggest))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.link_suggest = !this.config.editor.link_suggest;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.editor.hide_current_line_on_selection"),
                Some(&self.t("settings.editor.hide_current_line_on_selection_desc")),
                div()
                    .id("row-hide-current-line-on-selection")
                    .child(self.render_toggle_switch(config.editor.hide_current_line_on_selection))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.hide_current_line_on_selection = !this.config.editor.hide_current_line_on_selection;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.editor.debounce"),
                Some(&self.t("settings.editor.debounce_desc")),
                self.render_stepper(
                    "editor-debounce",
                    &debounce_display,
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.save_debounce_ms > 250 {
                            this.config.editor.save_debounce_ms -= 250;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.save_debounce_ms < 5000 {
                            this.config.editor.save_debounce_ms += 250;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.editor.headings_card"), heading_rows))
            .child(self.render_settings_card(&self.t("settings.editor.font_card"), font_rows))
            .child(self.render_settings_card(&self.t("settings.editor.layout_card"), layout_rows))
            .child(self.render_settings_card(&self.t("settings.editor.input_card"), input_rows))
    }

    // =========================================================================
    // 3. Reader Tab
    // =========================================================================

    /// Renders the Reader and Presentation mode settings tab.
    fn render_reader_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let reader_rows = vec![
            self.render_setting_row(
                &self.t("settings.reader.font_family"),
                Some(&self.t("settings.reader.font_family_desc")),
                self.render_select_pill(&config.reader.font.font_family),
            ),
            self.render_setting_row(
                &self.t("settings.reader.line_height"),
                Some(&self.t("settings.reader.line_height_desc")),
                self.render_stepper(
                    "reader-line-height",
                    &format!("{:.2}", config.reader.line_height),
                    |this: &mut SettingsView, cx| {
                        this.config.reader.line_height = (this.config.reader.line_height - 0.05).max(1.2);
                        this.save_settings(cx);
                    },
                    |this: &mut SettingsView, cx| {
                        this.config.reader.line_height = (this.config.reader.line_height + 0.05).min(2.5);
                        this.save_settings(cx);
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                &self.t("settings.reader.justify"),
                Some(&self.t("settings.reader.justify_desc")),
                div()
                    .id("row-justify")
                    .child(self.render_toggle_switch(config.reader.justify))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.reader.justify = !this.config.reader.justify;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.reader.hyphenate"),
                Some(&self.t("settings.reader.hyphenate_desc")),
                div()
                    .id("row-hyphenate")
                    .child(self.render_toggle_switch(config.reader.hyphenate))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.reader.hyphenate = !this.config.reader.hyphenate;
                        this.save_settings(cx);
                    })),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.reader.title"), reader_rows))
    }

    // =========================================================================
    // 4. Search Tab
    // =========================================================================

    /// Renders Tantivy full-text search index parameters tab.
    fn render_search_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let search_rows = vec![
            self.render_setting_row(
                &self.t("settings.search.candidate_pool"),
                Some(&self.t("settings.search.candidate_pool_desc")),
                self.render_stepper(
                    "search-pool",
                    &format!("{}", config.search.candidate_pool_size),
                    |this: &mut SettingsView, cx| {
                        if this.config.search.candidate_pool_size > 64 {
                            this.config.search.candidate_pool_size -= 32;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.search.candidate_pool_size < 1024 {
                            this.config.search.candidate_pool_size += 32;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                &self.t("settings.search.max_terms"),
                Some(&self.t("settings.search.max_terms_desc")),
                self.render_stepper(
                    "search-terms",
                    &format!("{}", config.search.max_query_terms),
                    |this: &mut SettingsView, cx| {
                        if this.config.search.max_query_terms > 8 {
                            this.config.search.max_query_terms -= 8;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.search.max_query_terms < 128 {
                            this.config.search.max_query_terms += 8;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.search.title"), search_rows))
    }

    // =========================================================================
    // 5. Templates Tab
    // =========================================================================

    /// Renders note template configuration and generator tab.
    fn render_templates_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let template_rows = vec![
            self.render_setting_row(
                &self.t("settings.templates.folder"),
                Some(&self.t("settings.templates.folder_desc")),
                div()
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_primary)
                    .child(config.templates.folder.clone()),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.templates.title"), template_rows))
    }

    // =========================================================================
    // 6. Files & Attachments Tab
    // =========================================================================

    /// Renders file attachment paths and storage configuration tab.
    fn render_files_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let file_rows = vec![
            self.render_setting_row(
                &self.t("settings.files.attachments_folder"),
                Some(&self.t("settings.files.attachments_folder_desc")),
                div()
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_primary)
                    .child(config.files.folder.clone()),
            ),
            self.render_setting_row(
                &self.t("settings.files.close_deleted_tabs"),
                Some(&self.t("settings.files.close_deleted_tabs_desc")),
                div()
                    .id("row-close-deleted-tabs")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.files.close_deleted_tabs))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.files.close_deleted_tabs = !this.config.files.close_deleted_tabs;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.files.keep_modified_on_delete"),
                Some(&self.t("settings.files.keep_modified_on_delete_desc")),
                div()
                    .id("row-keep-modified-on-delete")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.files.keep_modified_on_delete))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.files.keep_modified_on_delete = !this.config.files.keep_modified_on_delete;
                        this.save_settings(cx);
                    })),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.files.title"), file_rows))
    }

    // =========================================================================
    // 7. Graph Analysis Tab
    // =========================================================================

    /// Renders graph analysis and BM25F ranking algorithms settings tab.
    fn render_analysis_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let analysis_rows = vec![
            self.render_setting_row(
                &self.t("settings.analysis.bm25f"),
                Some(&self.t("settings.analysis.bm25f_desc")),
                div()
                    .id("row-bm25f")
                    .child(self.render_toggle_switch(config.analysis.enable_bm25f))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.analysis.enable_bm25f = !this.config.analysis.enable_bm25f;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.analysis.adamic_adar"),
                Some(&self.t("settings.analysis.adamic_adar_desc")),
                div()
                    .id("row-adamic")
                    .child(self.render_toggle_switch(config.analysis.enable_adamic_adar))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.analysis.enable_adamic_adar = !this.config.analysis.enable_adamic_adar;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.analysis.wikixiv"),
                Some(&self.t("settings.analysis.wikixiv_desc")),
                div()
                    .id("row-wikixiv")
                    .child(self.render_toggle_switch(config.analysis.enable_wikixiv))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.analysis.enable_wikixiv = !this.config.analysis.enable_wikixiv;
                        this.save_settings(cx);
                    })),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.analysis.title"), analysis_rows))
    }

    // =========================================================================
    // 8. AI Agents (MCP) Tab
    // =========================================================================

    /// Renders Model Context Protocol local server settings tab.
    fn render_mcp_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let mcp_rows = vec![
            self.render_setting_row(
                &self.t("settings.mcp.enable"),
                Some(&self.t("settings.mcp.enable_desc")),
                div()
                    .id("row-mcp")
                    .child(self.render_toggle_switch(config.mcp.enabled))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.mcp.enabled = !this.config.mcp.enabled;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.mcp.port"),
                Some(&self.t("settings.mcp.port_desc")),
                div()
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_primary)
                    .child(format!("{}", config.mcp.port)),
            ),
            self.render_setting_row(
                &self.t("settings.mcp.allow_write"),
                Some(&self.t("settings.mcp.allow_write_desc")),
                div()
                    .id("row-mcp-write")
                    .child(self.render_toggle_switch(config.mcp.allow_write))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.mcp.allow_write = !this.config.mcp.allow_write;
                        this.save_settings(cx);
                    })),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.mcp.title"), mcp_rows))
    }

    // =========================================================================
    // 9. History Tab
    // =========================================================================

    /// Renders note versioning and revision history retention settings tab.
    fn render_history_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let history_rows = vec![
            self.render_setting_row(
                &self.t("settings.history.retention"),
                Some(&self.t("settings.history.retention_desc")),
                div()
                    .id("row-history-retention")
                    .cursor_pointer()
                    .child(self.render_select_pill(&match config.history.retention_days {
                        0 => self.t("common.always"),
                        30 => self.t("common.days_30"),
                        90 => self.t("common.days_90"),
                        365 => self.t("common.year_1"),
                        _ => self.t("common.always"),
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.history.retention_days = match this.config.history.retention_days {
                            0 => 30,
                            30 => 90,
                            90 => 365,
                            _ => 0,
                        };
                        this.save_settings(cx);
                    })),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.history.title"), history_rows))
    }

    // =========================================================================
    // 10. Trash Tab
    // =========================================================================

    /// Renders deleted notes management and trash retention tab.
    fn render_trash_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let trash_rows = vec![
            self.render_setting_row(
                &self.t("settings.trash.auto_delete"),
                Some(&self.t("settings.trash.auto_delete_desc")),
                div()
                    .id("row-trash-retention")
                    .cursor_pointer()
                    .child(self.render_select_pill(&match config.trash.retention_days {
                        30 => self.t("common.after_30_days"),
                        90 => self.t("common.after_90_days"),
                        365 => self.t("common.after_1_year"),
                        _ => self.t("common.never"),
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.trash.retention_days = match this.config.trash.retention_days {
                            30 => 90,
                            90 => 365,
                            365 => 0,
                            _ => 30,
                        };
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.trash.folder"),
                Some(&self.t("settings.trash.folder_desc")),
                div()
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child(".trash"),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.trash.title"), trash_rows))
    }

    // =========================================================================
    // 11. System Tab
    // =========================================================================

    /// Renders desktop app version info and system integration tab.
    fn render_system_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let system_rows = vec![
            self.render_setting_row(
                &self.t("settings.system.version"),
                Some(&self.t("settings.system.arch")),
                div()
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.accent)
                    .child("v0.1.0 GPUI Native DirectX 120 FPS"),
            ),
            self.render_setting_row(
                &self.t("settings.system.storage"),
                Some(&self.t("settings.system.storage_desc")),
                div()
                    .text_xs()
                    .text_color(theme.text_secondary)
                    .child("SQLite WAL + Tantivy v2"),
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.system.title"), system_rows))
    }

    // =========================================================================
    // 12. Shortcuts Tab
    // =========================================================================

    /// Renders the keyboard shortcuts quick reference catalog tab.
    fn render_shortcuts_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let render_shortcut_item = |name: String, kbd: &str| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .py_2()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(div().text_sm().text_color(theme.text_primary).child(name))
                .child(
                    div()
                        .px_2()
                        .py_0p5()
                        .bg(theme.bg_sidebar)
                        .border_1()
                        .border_color(theme.border)
                        .rounded_sm()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child(kbd.to_string()),
                )
        };

        let global_rows = vec![
            render_shortcut_item(self.t("settings.shortcuts.quick_open"), "Ctrl + O"),
            render_shortcut_item(self.t("settings.shortcuts.open_command_palette"), "Ctrl + P"),
            render_shortcut_item(self.t("settings.shortcuts.open_folder"), "Ctrl + Shift + O"),
            render_shortcut_item(self.t("settings.shortcuts.new_note"), "Ctrl + N"),
            render_shortcut_item(self.t("settings.shortcuts.search_files"), "Ctrl + Shift + F"),
            render_shortcut_item(self.t("settings.shortcuts.save_note"), "Ctrl + S"),
            render_shortcut_item(self.t("settings.shortcuts.quick_search"), "Ctrl + F"),
            render_shortcut_item(self.t("settings.shortcuts.settings"), "Ctrl + ,"),
            render_shortcut_item(self.t("settings.shortcuts.toggle_sidebar"), "Ctrl + \\"),
            render_shortcut_item(self.t("settings.shortcuts.toggle_word_wrap"), "Alt + Z"),
        ];

        let editor_rows = vec![
            render_shortcut_item(self.t("settings.shortcuts.bold"), "Ctrl + B"),
            render_shortcut_item(self.t("settings.shortcuts.italic"), "Ctrl + I"),
            render_shortcut_item(self.t("settings.shortcuts.select_all"), "Ctrl + A"),
            render_shortcut_item(self.t("settings.shortcuts.copy"), "Ctrl + C"),
            render_shortcut_item(self.t("settings.shortcuts.cut"), "Ctrl + X"),
            render_shortcut_item(self.t("settings.shortcuts.paste"), "Ctrl + V"),
            render_shortcut_item(self.t("settings.shortcuts.undo"), "Ctrl + Z"),
            render_shortcut_item(self.t("settings.shortcuts.redo"), "Ctrl + Y"),
            render_shortcut_item(self.t("settings.shortcuts.home_end"), "Home / End"),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&self.t("settings.shortcuts.global_header"), global_rows))
            .child(self.render_settings_card(&self.t("settings.shortcuts.editing_header"), editor_rows))
    }

    /// Renders the Core Plugins settings tab (Obsidian-style built-in tools).
    fn render_core_plugins_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let _theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");

        let header_title = if is_ru {
            "Встроенные модули приложения"
        } else {
            "Application Core Plugins"
        };

        // Built-in modules list
        let plugins = vec![
            (
                "command-palette",
                if is_ru { "Палитра команд" } else { "Command palette" },
                if is_ru { "Быстрый поиск и выполнение команд приложения (Ctrl+Shift+P)" } else { "Quickly search and execute application actions (Ctrl+Shift+P)" },
                true, // Command palette is always on
            ),
            (
                "word-counter",
                if is_ru { "Подсчёт слов" } else { "Word counter" },
                if is_ru { "Отображает количество слов и символов в строке состояния" } else { "Displays character and word count in the status bar" },
                true,
            ),
            (
                "file-recovery",
                if is_ru { "Восстановление файлов" } else { "File recovery" },
                if is_ru { "Создание регулярных локальных снэпшотов для защиты от потери данных" } else { "Automatic local snapshotting to protect against data loss" },
                false,
            ),
        ];

        let rows: Vec<gpui::Div> = plugins
            .into_iter()
            .map(|(id, name, desc, default_val)| {
                let p_id = id.to_string();
                let is_enabled = self
                    .weak_app
                    .upgrade()
                    .and_then(|app| {
                        app.read(cx)
                            .state
                            .vault
                            .as_ref()
                            .map(|v| v.is_core_plugin_enabled(id))
                    })
                    .unwrap_or(default_val);

                let toggle_id = p_id.clone();
                let toggle = div()
                    .id(gpui::ElementId::from(gpui::SharedString::from(format!("core-toggle-{}", id))))
                    .cursor_pointer()
                    .child(self.render_toggle_switch(is_enabled))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(app) = this.weak_app.upgrade() {
                            let tid = toggle_id.clone();
                            app.update(cx, |app, cx| {
                                if let Some(vault) = &app.state.vault {
                                    if is_enabled {
                                        let _ = vault.disable_core_plugin(&tid);
                                    } else {
                                        let _ = vault.enable_core_plugin(&tid);
                                    }
                                    app.state.status_text = format!("Core plugin '{}' updated", tid);
                                    cx.notify();
                                }
                            });
                            cx.notify();
                        }
                    }));

                self.render_setting_row(name, Some(desc), toggle)
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(header_title, rows))
    }

    /// Renders the Community Plugins settings tab (Obsidian-style third-party plugins management).
    fn render_community_plugins_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");

        let header_title = if is_ru {
            "Сторонние плагины (Community Plugins)"
        } else {
            "Community Plugins"
        };

        let safe_mode_enabled = self.config.community_plugins.safe_mode;
        let safe_mode_toggle = div()
            .id("toggle-community-safe-mode")
            .cursor_pointer()
            .child(self.render_toggle_switch(safe_mode_enabled))
            .on_click(cx.listener(|this, _, _, cx| {
                this.config.community_plugins.safe_mode = !this.config.community_plugins.safe_mode;
                if this.config.community_plugins.safe_mode {
                    // Safe mode turned ON -> unload all plugins immediately
                    this.unload_all_plugins(cx);
                } else {
                    // Safe mode turned OFF -> reload enabled plugins
                    this.load_installed_plugins_for_vault(cx);
                }
                this.save_settings(cx);
            }));

        let safe_mode_row = self.render_setting_row(
            if is_ru { "Безопасный режим (Safe Mode)" } else { "Safe Mode" },
            Some(if is_ru {
                "Блокирует выполнение сторонних плагинов для защиты данных и стабильности"
            } else {
                "Blocks community plugins execution for system stability and data protection"
            }),
            safe_mode_toggle,
        );

        let browse_btn = div()
            .id("btn-browse-community-plugins")
            .flex()
            .items_center()
            .justify_center()
            .px_4()
            .py_2()
            .bg(if safe_mode_enabled { theme.bg_hover } else { theme.accent })
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.bg(theme.accent_hover))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if safe_mode_enabled { theme.text_muted } else { theme.text_on_accent })
                    .child(if is_ru { "Обзор каталога плагинов" } else { "Browse community plugins" }),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_plugin_catalog(window, cx);
            }));

        let browse_row = self.render_setting_row(
            if is_ru { "Каталог плагинов" } else { "Community plugins catalog" },
            Some(if is_ru { "Поиск, установка и обновление плагинов от сообщества" } else { "Discover, install and update community-created plugins" }),
            browse_btn,
        );

        // Scan installed plugins in the vault
        let installed_plugins = if let Some(app) = self.weak_app.upgrade() {
            if let Some(vault) = &app.read(cx).state.vault {
                let plugins_dir = vault.root.join(".vault").join("plugins");
                crate::plugins::loader::discover_plugins(&plugins_dir)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let mut plugin_rows = Vec::new();
        for plugin in &installed_plugins {
            let p_id = plugin.manifest.id.clone();
            let is_enabled = self.weak_app.upgrade().map(|app| {
                app.read(cx).state.vault.as_ref().map(|v| v.is_community_plugin_enabled(&p_id)).unwrap_or(false)
            }).unwrap_or(false);

            let toggle_id = p_id.clone();
            let toggle = div()
                .id(gpui::ElementId::from(gpui::SharedString::from(format!("comm-toggle-{}", plugin.manifest.id))))
                .cursor_pointer()
                .child(self.render_toggle_switch(is_enabled))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_community_plugin(&toggle_id, cx);
                }));

            let p_name = plugin.manifest.name.clone();
            let details_btn_id = p_id.clone();
            let settings_btn_id = p_id.clone();
            let action_group = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .when(is_enabled, |parent| {
                    parent.child(
                        div()
                            .id(gpui::ElementId::from(gpui::SharedString::from(format!("comm-gear-{}", plugin.manifest.id))))
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_SETTINGS, 14.0, theme.text_muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings_tab = SettingsTab::PluginSettings(settings_btn_id.clone());
                                if let Some(app) = this.weak_app.upgrade() {
                                    let s_id = settings_btn_id.clone();
                                    app.update(cx, |app, _cx| {
                                        let _ = app.state.plugin_host.send_event(
                                            &s_id,
                                            crate::plugins::api::HostEvent::UiEvent {
                                                view_id: 0,
                                                node_id: "__refresh_setting_tab__".to_string(),
                                                event: "refresh".to_string(),
                                                value: None,
                                            },
                                        );
                                    });
                                }
                                cx.notify();
                            })),
                    )
                })
                .child(toggle);

            let hint = format!("v{} • {}", plugin.manifest.version, plugin.manifest.description);
            // Calculate compatibility status
            let script_src = std::fs::read_to_string(&plugin.main_js_path).unwrap_or_default();
            let compat_report = crate::plugins::compat::analyze_plugin_script(&script_src);
            let (badge_bg, badge_fg, badge_text) = match compat_report.status {
                crate::plugins::compat::CompatibilityStatus::Full => (
                    rgba(0x2ea44f22),
                    rgba(0x3fb950ff),
                    if is_ru { "100% Совместим".to_string() } else { "100% Compatible".to_string() },
                ),
                crate::plugins::compat::CompatibilityStatus::Partial(score) => (
                    rgba(0xdbab0922),
                    rgba(0xd29922ff),
                    if is_ru { format!("{:.0}% Совместим", score * 100.0) } else { format!("{:.0}% Compatible", score * 100.0) },
                ),
                crate::plugins::compat::CompatibilityStatus::Incompatible => (
                    rgba(0xf8514922),
                    rgba(0xf85149ff),
                    if is_ru { "Не поддерживается".to_string() } else { "Incompatible".to_string() },
                ),
                crate::plugins::compat::CompatibilityStatus::Unknown => (
                    rgba(0x8b949e22),
                    rgba(0x8b949eff),
                    if is_ru { "Совместимость неизвестна".to_string() } else { "Compatibility unknown".to_string() },
                ),
            };

            let row_header = div()
                .id(gpui::ElementId::from(gpui::SharedString::from(format!("comm-item-title-{}", plugin.manifest.id))))
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .mr_4()
                .gap_0p5()
                .cursor_pointer()
                .hover(|h| h.opacity(0.85))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings_tab = SettingsTab::PluginSettings(details_btn_id.clone());
                    if let Some(app) = this.weak_app.upgrade() {
                        let d_id = details_btn_id.clone();
                        app.update(cx, |app, _cx| {
                            let _ = app.state.plugin_host.send_event(
                                &d_id,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id: 0,
                                    node_id: "__refresh_setting_tab__".to_string(),
                                    event: "refresh".to_string(),
                                    value: None,
                                },
                            );
                        });
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(p_name),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(badge_bg)
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(badge_fg)
                                .child(badge_text),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.accent)
                                .child("↗"),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(hint),
                );

            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .py_3()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(row_header)
                .child(
                    div()
                        .flex_none()
                        .child(action_group),
                );
            plugin_rows.push(row);
        }

        let installed_card_title = if is_ru {
            format!("Установленные плагины ({})", installed_plugins.len())
        } else {
            format!("Installed plugins ({})", installed_plugins.len())
        };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(header_title, vec![safe_mode_row, browse_row]))
            .when(!plugin_rows.is_empty(), |this| {
                this.child(self.render_settings_card(&installed_card_title, plugin_rows))
            })
    }

    /// Converts parsed inline spans into a StyledText or InteractiveText element that wraps across lines
    fn render_markdown_inlines(
        &self,
        spans: Vec<crate::markdown::InlineSpan>,
        line_idx: usize,
    ) -> gpui::AnyElement {
        let theme = &self.theme;
        let mut full_text = String::new();
        let mut runs = Vec::new();
        let mut ranges = Vec::new();
        let mut urls = Vec::new();

        let reg_font = gpui::font("Segoe UI");
        let bold_font = gpui::font("Segoe UI").bold();
        let italic_font = gpui::font("Segoe UI").italic();
        let bold_italic_font = gpui::font("Segoe UI").bold().italic();
        let code_font = gpui::font(self.editor_font_family());

        for span in spans {
            match span {
                crate::markdown::InlineSpan::Text(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: theme.text_secondary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::InlineSpan::Bold(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_font.clone(),
                        color: theme.text_primary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::InlineSpan::Italic(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: italic_font.clone(),
                        color: theme.text_secondary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::InlineSpan::BoldItalic(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_italic_font.clone(),
                        color: theme.text_primary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::InlineSpan::InlineCode(t) => {
                    let display_code = format!(" {} ", t);
                    let len = display_code.len();
                    full_text.push_str(&display_code);
                    runs.push(gpui::TextRun {
                        len,
                        font: code_font.clone(),
                        color: theme.accent.into(),
                        background_color: Some(theme.code_bg.into()),
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::InlineSpan::Link { label, url } => {
                    let start = full_text.len();
                    let len = label.len();
                    full_text.push_str(&label);
                    let end = full_text.len();
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
                    ranges.push(start..end);
                    urls.push(url);
                }
                crate::markdown::InlineSpan::Strikethrough(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
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
            }
        }

        if full_text.is_empty() {
            return div().into_any_element();
        }

        let styled_text = gpui::StyledText::new(full_text).with_runs(runs);
        if !ranges.is_empty() {
            gpui::InteractiveText::new(
                gpui::ElementId::from(("readme-md-interactive", line_idx)),
                styled_text,
            )
            .on_click(ranges, move |range_ix, _window, cx| {
                if let Some(url) = urls.get(range_ix) {
                    cx.open_url(url);
                }
            })
            .into_any_element()
        } else {
            styled_text.into_any_element()
        }
    }

    /// Parses and renders multi-line markdown content into styled GPUI elements
    pub(crate) fn render_markdown_content(
        &self,
        text: &str,
        _cx: &Context<Self>,
    ) -> Vec<gpui::Div> {
        let theme = &self.theme;
        let mut rendered_markdown_lines: Vec<gpui::Div> = Vec::new();
        let mut lines_iter = text.lines().peekable();
        let mut line_counter = 0usize;

        while let Some(line) = lines_iter.next() {
            let trimmed = line.trim();

            // 1. Fenced code block (``` or ~~~)
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                let lang = trimmed.trim_start_matches(fence).trim().to_string();
                let lang_opt = if !lang.is_empty() { Some(lang) } else { None };

                let mut code_lines = Vec::new();
                while let Some(next_line) = lines_iter.peek() {
                    let trimmed_peek = next_line.trim();
                    if trimmed_peek.starts_with(fence) {
                        lines_iter.next(); // consume closing fence
                        break;
                    }
                    code_lines.push(lines_iter.next().unwrap());
                }

                let code_str = code_lines.join("\n");
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .overflow_x_hidden()
                        .my_2()
                        .rounded_md()
                        .bg(theme.bg_editor)
                        .border_1()
                        .border_color(theme.border)
                        .overflow_hidden()
                        .when_some(lang_opt, |parent, l| {
                            parent.child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .px_3()
                                    .py_1()
                                    .bg(theme.bg_hover)
                                    .border_b_1()
                                    .border_color(theme.border_subtle)
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_muted)
                                    .child(l),
                            )
                        })
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .p_3()
                                .font_family(self.editor_font_family())
                                .text_xs()
                                .line_height(gpui::relative(1.4))
                                .text_color(theme.text_primary)
                                .whitespace_normal()
                                .child(code_str),
                        ),
                );
                continue;
            }

            // 2. Empty line
            if trimmed.is_empty() {
                rendered_markdown_lines.push(div().h_2());
                continue;
            }

            // 3. Headings
            if let Some(h1) = trimmed.strip_prefix("# ") {
                rendered_markdown_lines.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .whitespace_normal()
                        .mt_3()
                        .mb_1()
                        .child(h1.to_string()),
                );
                continue;
            }
            if let Some(h2) = trimmed.strip_prefix("## ") {
                rendered_markdown_lines.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_base()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .whitespace_normal()
                        .mt_2p5()
                        .mb_1()
                        .child(h2.to_string()),
                );
                continue;
            }
            if let Some(h3) = trimmed.strip_prefix("### ") {
                rendered_markdown_lines.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .whitespace_normal()
                        .mt_2()
                        .mb_0p5()
                        .child(h3.to_string()),
                );
                continue;
            }
            if let Some(h4) = trimmed.strip_prefix("#### ") {
                rendered_markdown_lines.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_secondary)
                        .whitespace_normal()
                        .mt_1p5()
                        .child(h4.to_string()),
                );
                continue;
            }

            // 4. Horizontal rule
            if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                rendered_markdown_lines.push(
                    div()
                        .h(px(1.0))
                        .w_full()
                        .bg(theme.border)
                        .my_2(),
                );
                continue;
            }

            // 5. Blockquote
            if let Some(quote) = trimmed.strip_prefix("> ") {
                let spans = crate::markdown::parse_inline_spans(quote);
                let inlines = self.render_markdown_inlines(spans, line_counter);
                line_counter += 1;
                rendered_markdown_lines.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .border_l_2()
                        .border_color(theme.accent)
                        .pl_3()
                        .py_0p5()
                        .my_1()
                        .italic()
                        .text_sm()
                        .text_color(theme.text_muted)
                        .whitespace_normal()
                        .child(inlines),
                );
                continue;
            }

            // 6. Task list item
            let task_opt = if let Some(rest) = trimmed.strip_prefix("- [x] ").or_else(|| trimmed.strip_prefix("- [X] ")).or_else(|| trimmed.strip_prefix("* [x] ")).or_else(|| trimmed.strip_prefix("* [X] ")) {
                Some((true, rest))
            } else if let Some(rest) = trimmed.strip_prefix("- [ ] ").or_else(|| trimmed.strip_prefix("* [ ] ")) {
                Some((false, rest))
            } else {
                None
            };
            if let Some((checked, task_text)) = task_opt {
                let spans = crate::markdown::parse_inline_spans(task_text);
                let inlines = self.render_markdown_inlines(spans, line_counter);
                line_counter += 1;
                let checkbox = div()
                    .flex_shrink_0()
                    .w_3p5()
                    .h_3p5()
                    .mt_0p5()
                    .rounded_sm()
                    .border_1()
                    .border_color(if checked { theme.accent } else { theme.border })
                    .bg(if checked { theme.accent } else { theme.bg_editor })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |c| c.child(icons::icon(icons::ICON_CHECK, 10.0, theme.text_on_accent)));
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .my_0p5()
                        .child(checkbox)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .line_height(gpui::relative(1.4))
                                .whitespace_normal()
                                .child(inlines),
                        ),
                );
                continue;
            }

            // 7. Bullet list item
            if let Some(bullet) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
                let spans = crate::markdown::parse_inline_spans(bullet);
                let inlines = self.render_markdown_inlines(spans, line_counter);
                line_counter += 1;
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .my_0p5()
                        .child(div().flex_shrink_0().text_color(theme.accent).child("•"))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .line_height(gpui::relative(1.4))
                                .whitespace_normal()
                                .child(inlines),
                        ),
                );
                continue;
            }

            // 8. Numbered list item
            let digits_len = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits_len > 0 && trimmed[digits_len..].starts_with(". ") {
                let num = &trimmed[..digits_len];
                let text = &trimmed[digits_len + 2..];
                let spans = crate::markdown::parse_inline_spans(text);
                let inlines = self.render_markdown_inlines(spans, line_counter);
                line_counter += 1;
                rendered_markdown_lines.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .w_full()
                        .min_w_0()
                        .gap_1p5()
                        .my_0p5()
                        .child(div().flex_shrink_0().text_xs().text_color(theme.text_muted).child(format!("{}.", num)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .line_height(gpui::relative(1.4))
                                .whitespace_normal()
                                .child(inlines),
                        ),
                );
                continue;
            }

            // 9. Regular paragraph / text line
            let spans = crate::markdown::parse_inline_spans(line);
            let inlines = self.render_markdown_inlines(spans, line_counter);
            line_counter += 1;
            rendered_markdown_lines.push(
                div()
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .line_height(gpui::relative(1.4))
                    .whitespace_normal()
                    .child(inlines),
            );
        }

        rendered_markdown_lines
    }

    /// Renders the individual details and settings tab for a specific community plugin.
    fn render_plugin_settings_tab(&self, plugin_id: &str, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");

        // Back button to Community Plugins list
        let back_btn = div()
            .id("btn-back-to-community-plugins")
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .rounded_md()
            .bg(theme.bg_hover)
            .cursor_pointer()
            .hover(|h| h.bg(theme.border))
            .child(icons::icon(icons::ICON_ARROW_LEFT, 14.0, theme.text_secondary))
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(if is_ru { "Назад к списку плагинов" } else { "Back to plugins" }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_tab = SettingsTab::CommunityPlugins;
                cx.notify();
            }));

        // Retrieve plugin folder, manifest, and README.
        // The directory name on disk might not match manifest.id (e.g. "obsidian-new-note-np" vs "new-note-np").
        // Search through discovered plugins or fallback to direct id lookup.
        let (plugin_dir, manifest, manifest_json) = if let Some(app) = self.weak_app.upgrade() {
            if let Some(vault) = &app.read(cx).state.vault {
                let plugins_dir = vault.root.join(".vault").join("plugins");
                let discovered = crate::plugins::loader::discover_plugins(&plugins_dir);
                if let Some(found) = discovered.into_iter().find(|p| p.manifest.id == plugin_id || p.dir_path.file_name().and_then(|n| n.to_str()) == Some(plugin_id)) {
                    let m_path = found.dir_path.join("manifest.json");
                    let raw_val: Option<serde_json::Value> = std::fs::read_to_string(&m_path).ok().and_then(|c| serde_json::from_str(&c).ok());
                    (Some(found.dir_path), Some(found.manifest), raw_val)
                } else {
                    let direct_dir = plugins_dir.join(plugin_id);
                    let m_path = direct_dir.join("manifest.json");
                    let content = std::fs::read_to_string(&m_path).ok();
                    let m = content.as_deref().and_then(|c| serde_json::from_str(c).ok());
                    let raw_val: Option<serde_json::Value> = content.as_deref().and_then(|c| serde_json::from_str(c).ok());
                    (Some(direct_dir), m, raw_val)
                }
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        // Search for README.md or readme.md
        let readme_content = plugin_dir.as_ref().and_then(|d| {
            let p1 = d.join("README.md");
            if p1.is_file() {
                return std::fs::read_to_string(p1).ok();
            }
            let p2 = d.join("readme.md");
            if p2.is_file() {
                return std::fs::read_to_string(p2).ok();
            }
            // Check case-insensitive in directory if needed
            if let Ok(entries) = std::fs::read_dir(d) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            if file_name.eq_ignore_ascii_case("readme.md") {
                                return std::fs::read_to_string(path).ok();
                            }
                        }
                    }
                }
            }
            None
        });

        let is_enabled = self.weak_app.upgrade().map(|app| app.read(cx).state.vault.as_ref().map(|v| v.is_community_plugin_enabled(plugin_id)).unwrap_or(false)).unwrap_or(false);
        let toggle_id = plugin_id.to_string();
        let uninstall_id = plugin_id.to_string();
        let plugin_dir_for_reveal = plugin_dir.clone();

        let display_name = manifest_json
            .as_ref()
            .and_then(|j| j.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .or_else(|| manifest.as_ref().map(|m| m.name.clone()))
            .unwrap_or_else(|| plugin_id.to_string());

        let version_str = manifest_json
            .as_ref()
            .and_then(|j| j.get("version").and_then(|v| v.as_str()).map(|s| format!("v{}", s)))
            .or_else(|| manifest.as_ref().map(|m| format!("v{}", m.version)))
            .unwrap_or_else(|| "v?".to_string());

        let author_str = manifest_json
            .as_ref()
            .and_then(|j| j.get("author").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .or_else(|| manifest.as_ref().map(|m| m.author.clone()))
            .unwrap_or_default();

        let desc_str = manifest_json
            .as_ref()
            .and_then(|j| j.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .or_else(|| manifest.as_ref().map(|m| m.description.clone()))
            .unwrap_or_default();

        let min_app = manifest_json
            .as_ref()
            .and_then(|j| j.get("minAppVersion").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .or_else(|| manifest.as_ref().and_then(|m| m.min_app_version.clone()))
            .unwrap_or_else(|| "0.1.0".to_string());

        let author_url_str = manifest_json
            .as_ref()
            .and_then(|j| j.get("authorUrl").and_then(|v| v.as_str()).map(|s| s.to_string()));

        let is_desktop_only_opt = manifest_json
            .as_ref()
            .and_then(|j| j.get("isDesktopOnly").and_then(|v| v.as_bool()));

        let actual_manifest_id = manifest_json
            .as_ref()
            .and_then(|j| j.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .or_else(|| manifest.as_ref().map(|m| m.id.clone()))
            .unwrap_or_else(|| plugin_id.to_string());

        // Header with title, version, author, and action buttons
        let header_actions = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .flex_wrap()
            .gap_2()
            // Enable / Disable button
            .child(
                div()
                    .id("btn-detail-toggle-plugin")
                    .px_2p5()
                    .py_1p5()
                    .rounded_md()
                    .bg(if is_enabled { theme.bg_hover } else { theme.accent })
                    .cursor_pointer()
                    .hover(|h| h.bg(if is_enabled { theme.border } else { theme.accent_hover }))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if is_enabled { theme.text_primary } else { theme.text_on_accent })
                    .child(if is_enabled {
                        if is_ru { "Отключить" } else { "Disable" }
                    } else {
                        if is_ru { "Включить" } else { "Enable" }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_community_plugin(&toggle_id, cx);
                    })),
            )
            // Reload plugin button
            .when(is_enabled, |parent| {
                let r_id = plugin_id.to_string();
                parent.child(
                    div()
                        .id("btn-detail-reload-plugin")
                        .px_2p5()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.bg_hover)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.border))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_muted))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_primary)
                                .child(if is_ru { "Перезагрузить" } else { "Reload" }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_installed_plugins_for_vault(cx);
                            // Refresh settings tab
                            if let Some(app) = this.weak_app.upgrade() {
                                let r_id_clone = r_id.clone();
                                app.update(cx, |app, _cx| {
                                    let _ = app.state.plugin_host.send_event(
                                        &r_id_clone,
                                        crate::plugins::api::HostEvent::UiEvent {
                                            view_id: 0,
                                            node_id: "__refresh_setting_tab__".to_string(),
                                            event: "refresh".to_string(),
                                            value: None,
                                        },
                                    );
                                });
                            }
                            cx.notify();
                        })),
                )
            })
            // Open folder in Windows Explorer button
            .when_some(plugin_dir_for_reveal, |parent, p_dir| {
                parent.child(
                    div()
                        .id("btn-detail-open-folder")
                        .px_2p5()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.bg_hover)
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.border))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_muted))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_primary)
                                .child(if is_ru { "Открыть папку" } else { "Open folder" }),
                        )
                        .on_click(cx.listener(move |_this, _, _, _cx| {
                            let _ = std::process::Command::new("explorer").arg(&p_dir).spawn();
                        })),
                )
            })
            // Uninstall button
            .child(
                div()
                    .id("btn-detail-uninstall-plugin")
                    .px_2p5()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgba(0xea433522))
                    .border_1()
                    .border_color(rgba(0xea433555))
                    .cursor_pointer()
                    .hover(|h| h.bg(rgba(0xea433544)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child(icons::icon(icons::ICON_TRASH_2, 13.0, rgba(0xea4335ff)))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgba(0xea4335ff))
                            .child(if is_ru { "Удалить" } else { "Delete" }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.uninstall_plugin_from_catalog(&uninstall_id, cx);
                        this.settings_tab = SettingsTab::CommunityPlugins;
                        cx.notify();
                    })),
            );

        let top_action_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .flex_wrap()
            .child(back_btn)
            .child(header_actions);

        let details_header = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2p5()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(display_name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(theme.bg_hover)
                            .text_color(theme.text_secondary)
                            .child(version_str.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(if is_enabled { rgba(0x34a85322) } else { theme.bg_hover })
                            .text_color(if is_enabled { rgba(0x34a853ff) } else { theme.text_muted })
                            .child(if is_enabled {
                                if is_ru { "Активен" } else { "Enabled" }
                            } else {
                                if is_ru { "Отключен" } else { "Disabled" }
                            }),
                    ),
            )
            .when(!author_str.is_empty(), |parent| {
                parent.child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(format!("{} {}", if is_ru { "Автор:" } else { "By" }, author_str)),
                )
            })
            .when(!desc_str.is_empty(), |parent| {
                parent.child(
                    div()
                        .text_sm()
                        .text_color(theme.text_secondary)
                        .child(desc_str),
                )
            });

        // Technical metadata card: show all manifest fields
        let mut meta_rows = vec![
            self.render_setting_row(
                if is_ru { "Название плагина (name)" } else { "Plugin Name" },
                None,
                div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD).text_color(theme.text_primary).child(display_name),
            ),
            self.render_setting_row(
                if is_ru { "Идентификатор (id)" } else { "Plugin ID" },
                None,
                div().text_xs().font_family(self.editor_font_family()).text_color(theme.text_secondary).child(actual_manifest_id),
            ),
            self.render_setting_row(
                if is_ru { "Версия (version)" } else { "Version" },
                None,
                div().text_xs().text_color(theme.text_secondary).child(version_str),
            ),
            self.render_setting_row(
                if is_ru { "Совместимость (minAppVersion)" } else { "Minimum App Version" },
                None,
                div().text_xs().text_color(theme.text_secondary).child(format!(">= {}", min_app)),
            ),
            self.render_setting_row(
                if is_ru { "Автор (author)" } else { "Author" },
                None,
                div().text_xs().text_color(theme.text_secondary).child(if author_str.is_empty() { "—".to_string() } else { author_str }),
            ),
        ];

        if let Some(author_url) = author_url_str {
            let author_url_click = author_url.clone();
            meta_rows.push(self.render_setting_row(
                if is_ru { "Сайт автора (authorUrl)" } else { "Author URL" },
                None,
                div()
                    .id("author-url-btn")
                    .text_xs()
                    .text_color(theme.accent)
                    .cursor_pointer()
                    .hover(|h| h.underline())
                    .child(author_url)
                    .on_click(cx.listener(move |_this, _, _, cx| {
                        cx.open_url(&author_url_click);
                    })),
            ));
        }

        if let Some(desktop_only) = is_desktop_only_opt {
            meta_rows.push(self.render_setting_row(
                if is_ru { "Только для десктопа (isDesktopOnly)" } else { "Desktop Only" },
                None,
                div().text_xs().text_color(theme.text_secondary).child(if desktop_only {
                    if is_ru { "true (только для десктопа)" } else { "true" }
                } else {
                    if is_ru { "false (кроссплатформенный)" } else { "false" }
                }),
            ));
        }

        meta_rows.push(self.render_setting_row(
            if is_ru { "Каталог на диске" } else { "Installation path" },
            None,
            div()
                .text_xs()
                .font_family(self.editor_font_family())
                .text_color(theme.text_muted)
                .max_w(px(320.0))
                .truncate()
                .child(plugin_dir.as_ref().map(|d| d.to_string_lossy().to_string()).unwrap_or_default()),
        ));

        // Content section: README or fallback information
        let content_section = if let Some(readme_text) = readme_content {
            let rendered_markdown_lines = self.render_markdown_content(&readme_text, cx);

            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child("README.MD"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .p_5()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .children(rendered_markdown_lines),
                )
        } else {
            // Fallback view when no README.md is present
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child(if is_ru { "ИНФОРМАЦИЯ О ПЛАГИНЕ" } else { "PLUGIN INFORMATION" }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .p_8()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .gap_2()
                        .child(icons::icon(icons::ICON_FILE_TEXT, 24.0, theme.text_muted))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(if is_ru { "Файл README.md отсутствует" } else { "No README.md file found" }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(if is_ru {
                                    "В каталоге плагина не найден файл документации. Вы можете добавить README.md для отображения руководства."
                                } else {
                                    "No documentation file exists in plugin directory. Add a README.md file to display instructions here."
                                }),
                        ),
                )
        };

        // Declarative UI or data.json configuration inspect section
        let p_id_str = plugin_id.to_string();
        let custom_tree = self.weak_app.upgrade().and_then(|app| {
            app.read(cx).plugin_views.get(&(p_id_str.clone(), 0)).cloned()
        });

        let settings_section = if let Some(tree) = custom_tree {
            let rendered_node = crate::plugins::ui::render_ui_node(
                &tree,
                plugin_id,
                0,
                theme,
                self.active_plugin_input.as_ref(),
                &self.plugin_input_focus_handle,
                self.active_plugin_select.as_ref(),
                self.plugin_select_popup.as_ref(),
                self.plugin_input_blink.visible(),
                window,
                cx,
                None,
            );
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child(if is_ru { "ПАРАМЕТРЫ ПЛАГИНА" } else { "PLUGIN SETTINGS" }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .p_5()
                        .rounded_lg()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(rendered_node),
                )
        } else {
            let current_data = self
                .weak_app
                .upgrade()
                .and_then(|app| {
                    app.read(cx)
                        .state
                        .vault
                        .as_ref()
                        .and_then(|v| v.load_plugin_data(plugin_id))
                })
                .unwrap_or(serde_json::json!({}));

            let data_str = serde_json::to_string_pretty(&current_data).unwrap_or_else(|_| "{}".to_string());
            let info_hint = if is_ru {
                "Хранилище параметров (.vault/plugins/<id>/data.json)"
            } else {
                "Configuration store (.vault/plugins/<id>/data.json)"
            };

            let json_preview = div()
                .flex()
                .flex_col()
                .p_3()
                .rounded_md()
                .bg(theme.bg_editor)
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_xs()
                        .font_family(self.editor_font_family())
                        .text_color(theme.text_secondary)
                        .child(data_str),
                );

            let row = self.render_setting_row(
                if is_ru { "Параметры плагина (data.json)" } else { "Plugin settings (data.json)" },
                Some(info_hint),
                json_preview,
            );

            self.render_settings_card(
                if is_ru { "Параметры" } else { "Settings" },
                vec![row],
            )
        };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(top_action_bar)
            .child(details_header)
            .child(self.render_settings_card(if is_ru { "Метаданные" } else { "Metadata" }, meta_rows))
            .child(content_section)
            .child(settings_section)
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_search_focused = self.search_focus_handle.is_focused(window);
        self.search_filter.sync_focus(is_search_focused, cx);
        self.render_settings_modal(window, cx)
    }
}

impl UiSink for SettingsView {
    fn send_plugin_ui_event(
        &mut self,
        plugin_id: &str,
        view_id: u64,
        node_id: String,
        event: &str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(app) = self.weak_app.upgrade() {
            let pid = plugin_id.to_string();
            app.update(cx, |app, cx| {
                app.send_plugin_ui_event(&pid, view_id, node_id, event, value, cx);
            });
        }
    }

    fn plugin_input_focus_handle(&self) -> &FocusHandle {
        &self.plugin_input_focus_handle
    }

    fn plugin_input_blink_enable(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.enable_view(|s| &mut s.plugin_input_blink, cx);
    }

    fn plugin_input_blink_pause_and_show(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.pause_and_show_view(|s| &mut s.plugin_input_blink, cx);
    }

    fn active_plugin_input(&self) -> Option<&PluginInputFocusState> {
        self.active_plugin_input.as_ref()
    }

    fn set_active_plugin_input(&mut self, state: Option<PluginInputFocusState>) {
        self.active_plugin_input = state;
    }

    fn active_plugin_input_mut(&mut self) -> Option<&mut PluginInputFocusState> {
        self.active_plugin_input.as_mut()
    }

    fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut input_state) = self.active_plugin_input.clone() else {
            return;
        };

        self.plugin_input_blink.pause_and_show_view(|s| &mut s.plugin_input_blink, cx);

        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;

        if key == "escape" {
            self.active_plugin_input = None;
            self.plugin_input_blink.disable_view(cx);
            cx.notify();
            return;
        }

        let mut chars: Vec<char> = input_state.value.chars().collect();
        let total_chars = chars.len();
        let mut pos = input_state.cursor_pos.min(total_chars);
        let sel_range = if let Some(anchor) = input_state.selection_anchor {
            if anchor != pos {
                Some((anchor.min(pos), anchor.max(pos)))
            } else {
                None
            }
        } else {
            None
        };

        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input_state.selection_anchor = Some(0);
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            let text_to_copy = if let Some((start, end)) = sel_range {
                chars.iter().skip(start).take(end - start).collect::<String>()
            } else {
                input_state.value.clone()
            };
            if !text_to_copy.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text_to_copy));
            }
            return;
        }

        let mut text_changed = false;

        let delete_selection = |chars: &mut Vec<char>, start: usize, end: usize| {
            chars.drain(start..end);
        };

        if key == "backspace" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut chars, start, end);
                pos = start;
                input_state.selection_anchor = None;
                text_changed = true;
            } else if pos > 0 {
                chars.remove(pos - 1);
                pos -= 1;
                text_changed = true;
            }
        } else if key == "delete" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut chars, start, end);
                pos = start;
                input_state.selection_anchor = None;
                text_changed = true;
            } else if pos < total_chars {
                chars.remove(pos);
                text_changed = true;
            }
        } else if key == "arrowleft" || key == "left" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
                pos = pos.saturating_sub(1);
            } else if let Some((start, _)) = sel_range {
                input_state.selection_anchor = None;
                pos = start;
            } else {
                input_state.selection_anchor = None;
                pos = pos.saturating_sub(1);
            }
        } else if key == "arrowright" || key == "right" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
                pos = (pos + 1).min(total_chars);
            } else if let Some((_, end)) = sel_range {
                input_state.selection_anchor = None;
                pos = end;
            } else {
                input_state.selection_anchor = None;
                pos = (pos + 1).min(total_chars);
            }
        } else if key == "home" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            pos = 0;
        } else if key == "end" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            pos = total_chars;
        } else if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let paste = if input_state.is_multiline {
                        text
                    } else {
                        text.replace('\n', " ").replace('\r', "")
                    };
                    if let Some((start, end)) = sel_range {
                        delete_selection(&mut chars, start, end);
                        pos = start;
                        input_state.selection_anchor = None;
                    }
                    for c in paste.chars() {
                        chars.insert(pos, c);
                        pos += 1;
                    }
                    text_changed = true;
                }
            }
        } else if key == "enter" {
            if input_state.is_multiline {
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut chars, start, end);
                    pos = start;
                    input_state.selection_anchor = None;
                }
                chars.insert(pos, '\n');
                pos += 1;
                text_changed = true;
            }
        } else {
            let typed = if let Some(c) = &event.keystroke.key_char {
                c.as_str()
            } else if key.chars().count() == 1 {
                key.as_str()
            } else {
                ""
            };
            if !typed.is_empty() && !ctrl {
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut chars, start, end);
                    pos = start;
                    input_state.selection_anchor = None;
                }
                for c in typed.chars() {
                    chars.insert(pos, c);
                    pos += 1;
                }
                text_changed = true;
            }
        }

        input_state.value = chars.into_iter().collect();
        input_state.cursor_pos = pos;

        if text_changed {
            if let Some(app) = self.weak_app.upgrade() {
                let pid = input_state.plugin_id.clone();
                let vid = input_state.view_id;
                let nid = input_state.node_id.clone();
                let val = input_state.value.clone();
                app.update(cx, |app, cx| {
                    if let Some(tree) = app.plugin_views.get_mut(&(pid.clone(), vid)) {
                        tree.update_input_value(&nid, &val);
                    }
                    let _ = app.state.plugin_host.send_event(
                        &pid,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id: vid,
                            node_id: nid,
                            event: "change".to_string(),
                            value: Some(val),
                        },
                    );
                    cx.notify();
                });
            }
        }

        self.active_plugin_input = Some(input_state);
        cx.notify();
    }

    fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<SelectOption>,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_plugin_select(plugin_id, view_id, node_id, options, current_value, window, cx);
    }

    fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        self.close_plugin_select(cx);
    }

    fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.is_plugin_select_open(plugin_id, view_id, node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_tab_keys_and_titles() {
        let comm_tab = SettingsTab::CommunityPlugins;
        assert_eq!(comm_tab.key(), "settings.tabs.community_plugins");
        assert_eq!(comm_tab.title("ru"), "Сторонние плагины");
        assert_eq!(comm_tab.title("en"), "Community plugins");

        let custom_plugin_tab = SettingsTab::PluginSettings("obsidian-new-note-np".to_string());
        assert_eq!(custom_plugin_tab.key(), "settings.tabs.plugin_settings");
        assert_eq!(custom_plugin_tab.title("ru"), "obsidian-new-note-np");
    }

    #[test]
    fn test_plugin_readme_file_discovery_and_fallback() {
        let unique_name = format!("vibecodium_test_plugin_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let temp_dir = std::env::temp_dir().join(unique_name);
        let plugin_dir = temp_dir.join("my-test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // 1. Without README
        let find_readme = |dir: &std::path::Path| -> Option<String> {
            let p1 = dir.join("README.md");
            if p1.is_file() {
                return std::fs::read_to_string(p1).ok();
            }
            let p2 = dir.join("readme.md");
            if p2.is_file() {
                return std::fs::read_to_string(p2).ok();
            }
            None
        };

        assert!(find_readme(&plugin_dir).is_none());

        // 2. With lowercase readme.md
        std::fs::write(plugin_dir.join("readme.md"), "# Hello Plugin\n- Feature 1").unwrap();
        let read = find_readme(&plugin_dir);
        assert!(read.is_some());
        assert!(read.unwrap().contains("# Hello Plugin"));

        // 3. With uppercase README.md takes priority
        std::fs::write(plugin_dir.join("README.md"), "# Upper README").unwrap();
        let read_upper = find_readme(&plugin_dir);
        assert!(read_upper.is_some());
        assert_eq!(read_upper.unwrap(), "# Upper README");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_markdown_code_block_and_span_parsing() {
        let md = "# Title\n\n```text\nfolder/file.js\n└── styles.css\n```\n\n- [x] Done task\n- [ ] Todo task\n- Bullet item with `inline code` and [Link](https://example.com)\n\nRegular paragraph.";
        let mut lines = md.lines().peekable();
        let mut code_blocks = Vec::new();
        let mut tasks = Vec::new();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                let mut block_lines = Vec::new();
                while let Some(next_line) = lines.peek() {
                    if next_line.trim().starts_with("```") {
                        lines.next();
                        break;
                    }
                    block_lines.push(lines.next().unwrap());
                }
                code_blocks.push(block_lines.join("\n"));
            } else if trimmed.starts_with("- [x] ") {
                tasks.push((true, trimmed.strip_prefix("- [x] ").unwrap()));
            } else if trimmed.starts_with("- [ ] ") {
                tasks.push((false, trimmed.strip_prefix("- [ ] ").unwrap()));
            }
        }

        assert_eq!(code_blocks.len(), 1);
        assert_eq!(code_blocks[0], "folder/file.js\n└── styles.css");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], (true, "Done task"));
        assert_eq!(tasks[1], (false, "Todo task"));
    }

    #[test]
    fn test_settings_tab_navigation_and_variants() {
        let tabs = vec![
            SettingsTab::Ui,
            SettingsTab::Editor,
            SettingsTab::Shortcuts,
            SettingsTab::CorePlugins,
            SettingsTab::CommunityPlugins,
            SettingsTab::System,
            SettingsTab::PluginSettings("dataview".to_string()),
        ];
        assert_eq!(tabs.len(), 7);
        assert!(matches!(tabs[0], SettingsTab::Ui));
        assert!(matches!(tabs[6], SettingsTab::PluginSettings(ref id) if id == "dataview"));
    }
}


