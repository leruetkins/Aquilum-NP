use super::*;

impl SettingsView {
    pub fn new(
        model: Entity<SettingsModel>,
        weak_app: WeakEntity<AppView>,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> Self {
        let config = model.read(cx).config.clone();
        let theme = Theme::from_ui(&config.ui, None);
        let word_wrap = config.editor.word_wrap;

        let model_sub = cx.subscribe(&model, |this, model, event: &SettingsEvent, cx| {
            match event {
                SettingsEvent::ConfigChanged => {
                    this.config = model.read(cx).config.clone();
                    cx.notify();
                }
                SettingsEvent::ThemeChanged => {
                    let ui = model.read(cx).config.ui.clone();
                    this.theme = Theme::from_ui(&ui, None);
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
            theme_popup: None,
            theme_popup_subscription: None,
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
            toolbar_add_button_open: false,
            toolbar_add_dropdown_open: false,
            toolbar_target_dropdown_id: None,
            toolbar_command_filter: String::new(),
            toolbar_filter_focus_handle: cx.focus_handle(),
            toolbar_command_dropdown_open: false,
            toolbar_icon_dropdown_open: false,
            toolbar_icon_filter: String::new(),
            toolbar_icon_filter_focus_handle: cx.focus_handle(),
            toolbar_selected_command: None,
            toolbar_selected_icon: None,
            toolbar_custom_tooltip: String::new(),
            toolbar_status_msg: None,
            toolbar_cmd_scroll_handle: ScrollHandle::new(),
            toolbar_cmd_scroll_y: 0.0,
            toolbar_cmd_scroll_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            toolbar_icon_scroll_handle: ScrollHandle::new(),
            toolbar_icon_scroll_y: 0.0,
            toolbar_icon_scroll_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            toolbar_command_popup: None,
            toolbar_command_popup_subscription: None,
            toolbar_icon_popup: None,
            toolbar_icon_popup_subscription: None,
            toolbar_drag: None,
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
        self.theme = crate::theme::Theme::from_ui(&self.config.ui, None);
        let config = self.config.clone();
        let old_ui = self.model.read(cx).config.ui.clone();
        let theme_changed = old_ui.theme != config.ui.theme
            || old_ui.light_theme != config.ui.light_theme
            || old_ui.dark_theme != config.ui.dark_theme;
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
        if self.theme_popup.is_some() {
            self.close_theme_popup(cx);
            return true;
        }
        if self.plugin_select_popup.is_some() {
            self.close_plugin_select(cx);
            return true;
        }
        if self.toolbar_command_popup.is_some() {
            self.close_toolbar_command_popup(cx);
            return true;
        }
        if self.toolbar_icon_popup.is_some() {
            self.close_toolbar_icon_popup(cx);
            return true;
        }
        if self.active_color_picker.is_some() {
            self.active_color_picker = None;
            self.hex_is_focused = false;
            cx.notify();
            return true;
        }
        if self.toolbar_command_dropdown_open {
            self.toolbar_command_dropdown_open = false;
            cx.notify();
            return true;
        }
        if self.toolbar_icon_dropdown_open {
            self.toolbar_icon_dropdown_open = false;
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
                trailing_icon: None,
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
                Some(active_ix),
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
                trailing_icon: None,
            })
            .collect();
        let active_ix = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .position(|name| *name == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| TabListPopup::new(tabs, Some(active_ix), theme, None, None, None, cx));
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

    pub fn open_theme_popup(&mut self, slot: ThemeSlot, window: &mut Window, cx: &mut Context<Self>) {
        let is_dark = slot == ThemeSlot::Dark;
        let selected = if is_dark {
            self.config.ui.dark_theme.clone()
        } else {
            self.config.ui.light_theme.clone()
        };
        let presets = crate::theme::presets(is_dark);
        let tabs: Vec<TabEntry> = presets
            .iter()
            .enumerate()
            .map(|(id, preset)| TabEntry {
                id,
                title: SharedString::from(preset.name),
                subtitle: None,
                icon: icons::ICON_PIPETTE,
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            })
            .collect();
        let active_ix = presets.iter().position(|p| p.id == selected).unwrap_or(0);
        let theme = self.theme.clone();
        let placeholder = self.t("settings.ui.theme_search");
        let max_rows = presets.len().clamp(1, 10) as f32;

        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                Some(px(ROW_HEIGHT * max_rows)),
                Some(50.0),
                cx,
            )
            .with_search(placeholder)
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(preset) = crate::theme::presets(is_dark).get(*id) {
                            if is_dark {
                                this.config.ui.dark_theme = preset.id.to_string();
                            } else {
                                this.config.ui.light_theme = preset.id.to_string();
                            }
                            this.save_settings(cx);
                        }
                        this.close_theme_popup(cx);
                    }
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_theme_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.theme_popup = Some((slot, popup));
        self.theme_popup_subscription = Some(subscription);
        cx.notify();
    }

    pub fn close_theme_popup(&mut self, cx: &mut Context<Self>) {
        if self.theme_popup.take().is_none() {
            return;
        }
        self.theme_popup_subscription = None;
        cx.notify();
    }

    pub fn theme_popup_is_open(&self, slot: ThemeSlot) -> bool {
        self.theme_popup
            .as_ref()
            .is_some_and(|(open_slot, _)| *open_slot == slot)
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
                trailing_icon: None,
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = options.len().clamp(1, 10) as f32;
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
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
                }.unwrap_or_else(|| self.t("settings.plugins.edit_title"));

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
            .when_some(self.render_toolbar_cfg_drag_ghost(cx), |this, ghost| {
                this.child(ghost)
            })
    }

    /// Renders the standalone title bar for the Settings window.
    pub(super) fn render_titlebar(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_maximized = window.is_maximized();
        let title = format!("{} — Vibecodium", self.t("settings.title"));

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
            } else if self.toolbar_command_dropdown_open {
                self.toolbar_command_dropdown_open = false;
                cx.notify();
            } else if self.toolbar_icon_dropdown_open {
                self.toolbar_icon_dropdown_open = false;
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
            && !self.toolbar_filter_focus_handle.is_focused(window)
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
            let results = search_settings(query, &self.config.ui.language);
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

    pub(super) fn render_search_bar(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let is_search_focused = self.search_focus_handle.is_focused(window);
        let pixel_scale = window.scale_factor();
        let placeholder = self.t("settings.nav.search_placeholder");

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
    pub(super) fn render_settings_nav(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active_tab = &self.settings_tab;
        let query = self.search_query.trim();
        let search_results = if !query.is_empty() {
            search_settings(query, &self.config.ui.language)
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
                                    .child(self.t("settings.nav.search_empty"))
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
                                            .child(setting.title(&self.config.ui.language))
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
    pub(super) fn render_settings_content(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let scroll_handle = self.settings_scroll_handle.clone();
        let is_catalog_open = false;
        let is_toolbar_dropdown_open = self.toolbar_command_popup_is_open()
            || self.toolbar_icon_popup_is_open()
            || self.toolbar_command_dropdown_open
            || self.toolbar_icon_dropdown_open;

        div()
            .id("settings-content-scroll")
            .relative()
            .track_scroll(&self.settings_scroll_handle)
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .when(
                !self.toolbar_command_dropdown_open && !self.toolbar_icon_dropdown_open,
                |d| d.overflow_scroll(),
            )
            .when(
                self.toolbar_command_dropdown_open || self.toolbar_icon_dropdown_open,
                |d| d.overflow_hidden(),
            )
            .bg(theme.bg_editor)
            .rounded_br_xl()
            .cursor(gpui::CursorStyle::Arrow)
            .p_6()
            .gap_5()
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                if this.toolbar_command_dropdown_open || this.toolbar_icon_dropdown_open {
                    cx.stop_propagation();
                    return;
                }
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
                        // Capture-intercept the wheel before the div's default scroll:
                        // divide Lines ticks (mouse) by 3 — one tick ≈ one line,
                        // pixel deltas from the touchpad pass through as is.
                        let current_view = window.current_view();
                        window.on_mouse_event(
                            move |event: &ScrollWheelEvent, phase, window, cx| {
                                if !is_catalog_open
                                    && !is_toolbar_dropdown_open
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
    pub(super) fn render_settings_card(&self, title: &str, rows: Vec<gpui::Div>) -> gpui::Div {
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
    pub(super) fn render_setting_row_with_id(
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
    pub(super) fn render_setting_row(&self, label: &str, hint: Option<&str>, control: impl IntoElement) -> gpui::Div {
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
    pub(super) fn image_stepper(
        &self,
        id: &'static str,
        value: String,
        minus_enabled: bool,
        plus_enabled: bool,
        cx: &Context<Self>,
        minus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        plus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> gpui::Div {
        let theme = &self.theme;
        let button = |suffix: &'static str, label: &'static str, enabled: bool| {
            let full_id = format!("{}-{}", id, suffix);
            div()
                .id(gpui::ElementId::Name(full_id.into()))
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
                .cursor_pointer()
                .child(label)
        };
        let minus_button = button("minus", "-", minus_enabled).when(minus_enabled, |d| {
            d.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    minus(this, cx);
                }),
            )
        });
        let plus_button = button("plus", "+", plus_enabled).when(plus_enabled, |d| {
            d.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    plus(this, cx);
                }),
            )
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(minus_button)
            .child(
                div()
                    .min_w(px(44.0))
                    .text_xs()
                    .text_center()
                    .text_color(theme.text_primary)
                    .child(value),
            )
            .child(plus_button)
    }

    pub(super) fn render_toggle_switch(&self, checked: bool) -> gpui::Div {
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
    pub(super) fn render_stepper<F1, F2>(
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
    pub(super) fn render_heading_format_toggles<F1, F2>(
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
    pub(super) fn render_color_picker_button(
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
    pub(super) fn render_color_context_menu(
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
                    // Eyedropper / Color Picker button
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
