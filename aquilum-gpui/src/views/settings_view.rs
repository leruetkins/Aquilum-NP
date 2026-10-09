use crate::icons;
use super::app_view::AppView;
use gpui::{
    div, prelude::*, px, rgba, Context, IntoElement, MouseButton,
};

/// Represents the navigation tabs available in the Settings modal window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl SettingsTab {
    /// Returns the user-facing localized display title for the tab.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Ui => "Интерфейс",
            Self::Editor => "Редактор",
            Self::Reader => "Читалка",
            Self::Search => "Поиск",
            Self::Templates => "Шаблоны",
            Self::Files => "Файлы",
            Self::Analysis => "Анализ",
            Self::Mcp => "AI-агенты",
            Self::History => "История",
            Self::Trash => "Корзина",
            Self::System => "Система",
            Self::Shortcuts => "Горячие клавиши",
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
        ]
    }
}

impl AppView {
    /// Toggles visibility of the Settings modal window.
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = !self.is_settings_open;
        if self.is_settings_open {
            self.config = self.state.core.settings.get_config();
        }
        cx.notify();
    }

    /// Opens the Settings modal, optionally navigating directly to the given tab.
    pub fn open_settings(&mut self, tab: Option<SettingsTab>, cx: &mut Context<Self>) {
        self.is_settings_open = true;
        self.config = self.state.core.settings.get_config();
        if let Some(t) = tab {
            self.settings_tab = t;
        }
        cx.notify();
    }

    /// Closes the Settings modal window and restores editor focus.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = false;
        cx.notify();
    }

    /// Persists the modified AppConfig to disk via Core SettingsManager.
    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let _ = self.state.core.settings.update_config(self.config.clone());
        cx.notify();
    }

    /// Renders the full Settings modal dialog including backdrop and content.
    pub fn render_settings_modal(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        div()
            .id("settings-modal-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000aa))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_settings(cx);
            }))
            .child(
                div()
                    .id("settings-modal-dialog")
                    .flex()
                    .flex_col()
                    .w(px(880.0))
                    .h(px(620.0))
                    .max_w(px(960.0))
                    .max_h(px(720.0))
                    .rounded_xl()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    // Dialog Header
                    .child(self.render_dialog_header(cx))
                    // Dialog Body: Sidebar + Main Content Area
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            // Left navigation panel
                            .child(self.render_settings_nav(cx))
                            // Right scrollable content panel
                            .child(self.render_settings_content(cx)),
                    ),
            )
    }

    /// Renders the title bar and close button of the Settings dialog.
    fn render_dialog_header(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(icons::icon(icons::ICON_SETTINGS, 16.0, theme.accent))
                    .child(
                        div()
                            .text_base()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child("Настройки"),
                    ),
            )
            .child(
                div()
                    .id("btn-close-settings")
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(icons::ICON_CLOSE, 16.0, theme.text_muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_settings(cx);
                    })),
            )
    }

    /// Renders the vertical tab navigation sidebar inside the Settings dialog.
    fn render_settings_nav(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let active_tab = self.settings_tab;

        div()
            .flex()
            .flex_col()
            .w(px(210.0))
            .min_w(px(210.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .p_2()
            .gap_1()
            .children(SettingsTab::all().iter().map(|&tab| {
                let is_active = tab == active_tab;
                let text_color = if is_active { theme.text_primary } else { theme.text_secondary };
                let icon_color = if is_active { theme.accent } else { theme.text_muted };
                let bg_color = if is_active { theme.bg_hover } else { rgba(0x00000000) };

                div()
                    .id(gpui::ElementId::from(("nav-tab", tab as usize)))
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
                            .child(tab.title()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_tab = tab;
                        cx.notify();
                    }))
            }))
    }

    /// Renders the active tab's settings content container.
    fn render_settings_content(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        div()
            .id("settings-content-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_scroll()
            .bg(theme.bg_editor)
            .p_6()
            .gap_5()
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
            })
    }

    // =========================================================================
    // Reusable UI Building Blocks
    // =========================================================================

    /// Wraps a collection of rows inside a titled card container.
    fn render_settings_card(&self, title: &str, rows: Vec<gpui::Div>) -> gpui::Div {
        let theme = &self.theme;
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
                    .children(rows),
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
                    .gap_0p5()
                    .max_w(px(380.0))
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
            .child(control)
    }

    /// Renders an interactive toggle switch widget.
    fn render_toggle_switch(&self, checked: bool) -> gpui::Div {
        let theme = &self.theme;
        let track_bg = if checked { theme.accent } else { theme.border };
        let knob_left = if checked { px(18.0) } else { px(2.0) };

        div()
            .relative()
            .w(px(38.0))
            .h(px(22.0))
            .rounded_full()
            .bg(track_bg)
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(knob_left)
                    .w(px(18.0))
                    .h(px(18.0))
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
        F1: Fn(&mut AppView, &mut Context<AppView>) + 'static,
        F2: Fn(&mut AppView, &mut Context<AppView>) + 'static,
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
                    .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
                        on_dec(this, cx);
                    })),
            )
            .child(
                div()
                    .px_2()
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
                    .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
                        on_inc(this, cx);
                    })),
            )
    }

    /// Renders a clickable styled pill dropdown or selector button.
    fn render_select_pill(&self, label: &str) -> gpui::Div {
        let theme = &self.theme;
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

    // =========================================================================
    // 1. Interface (Ui) Tab
    // =========================================================================

    /// Renders the UI and Display settings tab.
    fn render_ui_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let display_rows = vec![
            self.render_setting_row(
                "Тема оформления",
                Some("Цветовая палитра интерфейса (системная, светлая, тёмная)"),
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
                            .when(config.ui.theme == "system", |s| s.bg(theme.accent).text_color(theme.bg_sidebar))
                            .when(config.ui.theme != "system", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Системная")
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
                            .when(config.ui.theme == "light", |s| s.bg(theme.accent).text_color(theme.bg_sidebar))
                            .when(config.ui.theme != "light", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Светлая")
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
                            .when(config.ui.theme == "dark", |s| s.bg(theme.accent).text_color(theme.bg_sidebar))
                            .when(config.ui.theme != "dark", |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Тёмная")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.ui.theme = "dark".to_string();
                                this.save_settings(cx);
                            })),
                    ),
            ),
            self.render_setting_row(
                "Язык интерфейса",
                Some("Язык меню и системных сообщений"),
                div()
                    .id("select-language")
                    .cursor_pointer()
                    .child(self.render_select_pill(if config.ui.language == "en" { "English" } else { "Русский" }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.ui.language = if this.config.ui.language == "en" { "ru".to_string() } else { "en".to_string() };
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Акцентный цвет",
                Some("Основной цвет кнопок, ссылок и курсора"),
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
            ),
        ];

        let font_rows = vec![
            self.render_setting_row(
                "Шрифт интерфейса",
                Some("Семейство шрифтов для панелей и навигации"),
                self.render_select_pill(&config.ui.font.font_family),
            ),
            self.render_setting_row(
                "Начертание шрифта",
                Some("Базовая толщина текста интерфейса"),
                self.render_select_pill(if config.ui.font.font_weight >= 600 { "Semibold" } else { "Regular" }),
            ),
            self.render_setting_row(
                "Базовый размер шрифта",
                Some("Размер текста элементов интерфейса в пикселях"),
                self.render_stepper(
                    "ui-font-size",
                    &format!("{} px", config.ui.font.font_size_base),
                    |this: &mut AppView, cx| {
                        if this.config.ui.font.font_size_base > 11 {
                            this.config.ui.font.font_size_base -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
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
            .child(self.render_settings_card("Отображение", display_rows))
            .child(self.render_settings_card("Шрифты интерфейса", font_rows))
    }

    // =========================================================================
    // 2. Editor Tab
    // =========================================================================

    /// Renders the Editor layout, typography and behaviors settings tab.
    fn render_editor_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let font_rows = vec![
            self.render_setting_row(
                "Шрифт редактора",
                Some("Моноширинный шрифт для редактирования заметок"),
                self.render_select_pill(&config.editor.font.font_family),
            ),
            self.render_setting_row(
                "Размер шрифта",
                Some("Размер символов редактора в пикселях"),
                self.render_stepper(
                    "editor-font-size",
                    &format!("{} px", config.editor.font.font_size_base),
                    |this: &mut AppView, cx| {
                        if this.config.editor.font.font_size_base > 12 {
                            this.config.editor.font.font_size_base -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
                        if this.config.editor.font.font_size_base < 32 {
                            this.config.editor.font.font_size_base += 1;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                "Высота строки",
                Some("Межстрочный интервал текста заметки"),
                self.render_stepper(
                    "editor-line-height",
                    &format!("{:.2}", config.editor.line_height),
                    |this: &mut AppView, cx| {
                        if this.config.editor.line_height > 1.2 {
                            this.config.editor.line_height = (this.config.editor.line_height - 0.05).max(1.2);
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
                        if this.config.editor.line_height < 2.5 {
                            this.config.editor.line_height = (this.config.editor.line_height + 0.05).min(2.5);
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        let layout_rows = vec![
            self.render_setting_row(
                "Максимальная ширина строки",
                Some("Ограничение ширины текста редактора в символах (ch)"),
                self.render_stepper(
                    "editor-max-width",
                    &format!("{} ch", config.editor.max_width_ch),
                    |this: &mut AppView, cx| {
                        if this.config.editor.max_width_ch > 40 {
                            this.config.editor.max_width_ch -= 5;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
                        if this.config.editor.max_width_ch < 140 {
                            this.config.editor.max_width_ch += 5;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                "Перенос длинных строк",
                Some("Мягкий перенос текста по границе окна (Word wrap)"),
                div()
                    .id("row-word-wrap")
                    .child(self.render_toggle_switch(self.word_wrap))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_word_wrap(cx);
                    })),
            ),
            self.render_setting_row(
                "Ширина табуляции",
                Some("Количество пробелов при нажатии Tab"),
                self.render_stepper(
                    "editor-tabs",
                    &format!("{} пробела", config.editor.live_tabs),
                    |this: &mut AppView, cx| {
                        if this.config.editor.live_tabs > 1 {
                            this.config.editor.live_tabs -= 1;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
                        if this.config.editor.live_tabs < 8 {
                            this.config.editor.live_tabs += 1;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
        ];

        let input_rows = vec![
            self.render_setting_row(
                "Умные тире",
                Some("Автоматическая замена двойного дефиса '--' на длинное тире '—'"),
                div()
                    .id("row-smart-dashes")
                    .child(self.render_toggle_switch(config.editor.smart_dashes))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.smart_dashes = !this.config.editor.smart_dashes;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Выделение списков",
                Some("Стилизованное отображение маркеров списков и чекбоксов"),
                div()
                    .id("row-list-callouts")
                    .child(self.render_toggle_switch(config.editor.list_callouts))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.list_callouts = !this.config.editor.list_callouts;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Автоподстановка ссылок",
                Some("Всплывающее меню вариантов при вводе двойных скобок '[['"),
                div()
                    .id("row-link-suggest")
                    .child(self.render_toggle_switch(config.editor.link_suggest))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.link_suggest = !this.config.editor.link_suggest;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Задержка автосохранения",
                Some("Таймаут после ввода текста перед записью на диск"),
                self.render_stepper(
                    "editor-debounce",
                    &format!("{} мс", config.editor.save_debounce_ms),
                    |this: &mut AppView, cx| {
                        if this.config.editor.save_debounce_ms > 250 {
                            this.config.editor.save_debounce_ms -= 250;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
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
            .child(self.render_settings_card("Шрифты редактора", font_rows))
            .child(self.render_settings_card("Разметка и строки", layout_rows))
            .child(self.render_settings_card("Ввод и поведение", input_rows))
    }

    // =========================================================================
    // 3. Reader Tab
    // =========================================================================

    /// Renders the Reader and Presentation mode settings tab.
    fn render_reader_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let reader_rows = vec![
            self.render_setting_row(
                "Шрифт читалки",
                Some("Типографический шрифт для комфортного чтения больших заметок"),
                self.render_select_pill(&config.reader.font.font_family),
            ),
            self.render_setting_row(
                "Высота строки",
                Some("Интервал строк в режиме чтения"),
                self.render_stepper(
                    "reader-line-height",
                    &format!("{:.2}", config.reader.line_height),
                    |this: &mut AppView, cx| {
                        this.config.reader.line_height = (this.config.reader.line_height - 0.05).max(1.2);
                        this.save_settings(cx);
                    },
                    |this: &mut AppView, cx| {
                        this.config.reader.line_height = (this.config.reader.line_height + 0.05).min(2.5);
                        this.save_settings(cx);
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                "Выравнивание по ширине",
                Some("Форматирование абзацев по обоим краям страницы"),
                div()
                    .id("row-justify")
                    .child(self.render_toggle_switch(config.reader.justify))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.reader.justify = !this.config.reader.justify;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Переносы слов (Hyphenation)",
                Some("Автоматический перенос длинных слов по слогам"),
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
            .child(self.render_settings_card("Режим чтения", reader_rows))
    }

    // =========================================================================
    // 4. Search Tab
    // =========================================================================

    /// Renders Tantivy full-text search index parameters tab.
    fn render_search_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let search_rows = vec![
            self.render_setting_row(
                "Размер пула кандидатов",
                Some("Количество результатов первичного отбора индекса Tantivy"),
                self.render_stepper(
                    "search-pool",
                    &format!("{}", config.search.candidate_pool_size),
                    |this: &mut AppView, cx| {
                        if this.config.search.candidate_pool_size > 64 {
                            this.config.search.candidate_pool_size -= 32;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
                        if this.config.search.candidate_pool_size < 1024 {
                            this.config.search.candidate_pool_size += 32;
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
            ),
            self.render_setting_row(
                "Максимум терминов в запросе",
                Some("Ограничение числа слов для нечёткого поиска"),
                self.render_stepper(
                    "search-terms",
                    &format!("{}", config.search.max_query_terms),
                    |this: &mut AppView, cx| {
                        if this.config.search.max_query_terms > 8 {
                            this.config.search.max_query_terms -= 8;
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut AppView, cx| {
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
            .child(self.render_settings_card("Параметры поиска", search_rows))
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
                "Папка шаблонов",
                Some("Относительный путь к папке с Markdown-шаблонами в базе"),
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
            .child(self.render_settings_card("Шаблоны заметок", template_rows))
    }

    // =========================================================================
    // 6. Files & Attachments Tab
    // =========================================================================

    /// Renders file attachment paths and storage configuration tab.
    fn render_files_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let file_rows = vec![
            self.render_setting_row(
                "Папка для вложений",
                Some("Куда сохраняются вставленные изображения, видео и документы"),
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
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card("Файлы и медиа", file_rows))
    }

    // =========================================================================
    // 7. Graph Analysis Tab
    // =========================================================================

    /// Renders graph analysis and BM25F ranking algorithms settings tab.
    fn render_analysis_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let analysis_rows = vec![
            self.render_setting_row(
                "Алгоритм BM25F",
                Some("Ранжирование связанных заметок с учётом веса заголовка"),
                div()
                    .id("row-bm25f")
                    .child(self.render_toggle_switch(config.analysis.enable_bm25f))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.analysis.enable_bm25f = !this.config.analysis.enable_bm25f;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Алгоритм Adamic-Adar",
                Some("Анализ топологии графа на основе общих соседей заметок"),
                div()
                    .id("row-adamic")
                    .child(self.render_toggle_switch(config.analysis.enable_adamic_adar))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.analysis.enable_adamic_adar = !this.config.analysis.enable_adamic_adar;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "WikiXiv рекомендации",
                Some("Поиск контекстных ассоциаций и смысловых связей"),
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
            .child(self.render_settings_card("Методы анализа связей", analysis_rows))
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
                "Локальный сервер MCP",
                Some("Разрешить AI-ассистентам читать базу знаний через протокол Model Context Protocol"),
                div()
                    .id("row-mcp")
                    .child(self.render_toggle_switch(config.mcp.enabled))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.mcp.enabled = !this.config.mcp.enabled;
                        this.save_settings(cx);
                    })),
            ),
            self.render_setting_row(
                "Порт сервера",
                Some("TCP порт локального HTTP моста MCP"),
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
                "Разрешить запись заметок",
                Some("Разрешить внешним AI-агентам создавать и обновлять заметки"),
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
            .child(self.render_settings_card("Model Context Protocol (AI)", mcp_rows))
    }

    // =========================================================================
    // 9. History Tab
    // =========================================================================

    /// Renders note versioning and revision history retention settings tab.
    fn render_history_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let history_rows = vec![
            self.render_setting_row(
                "Срок хранения версий",
                Some("Период хранения снимков заметок в скрытой папке .aquilum/history"),
                div()
                    .id("row-history-retention")
                    .cursor_pointer()
                    .child(self.render_select_pill(match config.history.retention_days {
                        0 => "Всегда",
                        30 => "30 дней",
                        90 => "90 дней",
                        365 => "1 год",
                        _ => "Всегда",
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
            .child(self.render_settings_card("История изменений", history_rows))
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
                "Удалять безвозвратно",
                Some("Автоматическая очистка заметок из папки .trash"),
                div()
                    .id("row-trash-retention")
                    .cursor_pointer()
                    .child(self.render_select_pill(match config.trash.retention_days {
                        30 => "Через 30 дней",
                        90 => "Через 90 дней",
                        365 => "Через год",
                        _ => "Никогда",
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
                "Папка корзины",
                Some("Локальная корзина внутри текущей базы заметок"),
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
            .child(self.render_settings_card("Корзина заметок", trash_rows))
    }

    // =========================================================================
    // 11. System Tab
    // =========================================================================

    /// Renders desktop app version info and system integration tab.
    fn render_system_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let system_rows = vec![
            self.render_setting_row(
                "Версия приложения",
                Some("Архитектура скомпилированного нативного клиента"),
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
                "Движок хранения",
                Some("Локальная база данных метаданных и индекс связей"),
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
            .child(self.render_settings_card("О программе и системе", system_rows))
    }

    // =========================================================================
    // 12. Shortcuts Tab
    // =========================================================================

    /// Renders the keyboard shortcuts quick reference catalog tab.
    fn render_shortcuts_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let render_shortcut_item = |name: &str, kbd: &str| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .py_2()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(div().text_sm().text_color(theme.text_primary).child(name.to_string()))
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
            render_shortcut_item("Открыть папку базы знаний", "Ctrl + O"),
            render_shortcut_item("Создать новую заметку", "Ctrl + N"),
            render_shortcut_item("Сохранить заметку", "Ctrl + S"),
            render_shortcut_item("Быстрый поиск по базе", "Ctrl + F"),
            render_shortcut_item("Окно настроек", "Ctrl + ,"),
            render_shortcut_item("Свернуть / развернуть боковую панель", "Ctrl + \\"),
            render_shortcut_item("Переключить перенос длинных строк", "Alt + Z"),
        ];

        let editor_rows = vec![
            render_shortcut_item("Жирный текст (**bold**)", "Ctrl + B"),
            render_shortcut_item("Курсивный текст (*italic*)", "Ctrl + I"),
            render_shortcut_item("Выделить весь документ", "Ctrl + A"),
            render_shortcut_item("Копировать выделенное", "Ctrl + C"),
            render_shortcut_item("Вырезать выделенное", "Ctrl + X"),
            render_shortcut_item("Вставить из буфера", "Ctrl + V"),
            render_shortcut_item("Отменить действие (Undo)", "Ctrl + Z"),
            render_shortcut_item("Повторить действие (Redo)", "Ctrl + Y"),
            render_shortcut_item("Переход в начало / конец строки", "Home / End"),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card("Глобальные сочетания", global_rows))
            .child(self.render_settings_card("Редактирование текста", editor_rows))
    }
}
