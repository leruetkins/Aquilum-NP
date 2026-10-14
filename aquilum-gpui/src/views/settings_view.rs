use crate::icons;
use super::app_view::AppView;
use gpui::{
    canvas, div, point, prelude::*, px, rgba, Context, DispatchPhase, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, ScrollDelta, ScrollWheelEvent,
};

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

    /// Toggles visibility of the Settings modal window.
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = !self.is_settings_open;
        if self.is_settings_open {
            self.config = self.state.settings.get_config();
        }
        cx.notify();
    }

    /// Opens the Settings modal, optionally navigating directly to the given tab.
    pub fn open_settings(&mut self, tab: Option<SettingsTab>, cx: &mut Context<Self>) {
        self.is_settings_open = true;
        self.config = self.state.settings.get_config();
        if let Some(t) = tab {
            self.settings_tab = t;
        }
        cx.notify();
    }

    /// Closes the Settings modal window and restores editor focus.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = false;
        self.active_color_picker = None;
        self.active_plugin_input = None;
        self.active_plugin_select = None;
        cx.notify();
    }

    /// Persists the modified AppConfig to disk via SettingsManager.
    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(ref mut note) = self.state.active_note {
            note.visual_cache_dirty = true;
        }
        let _ = self.state.settings.update_config(self.config.clone());
        cx.notify();
    }

    /// Renders the full Settings modal dialog including backdrop and content.
    pub fn render_settings_modal(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;

        let plugin_modal_overlay = if let SettingsTab::PluginSettings(ref pid) = self.settings_tab {
            self.plugin_views.get(&(pid.clone(), 1)).and_then(|modal_tree| {
                if let crate::plugins::ui::UiNode::Text { value, .. } = modal_tree {
                    if value.is_empty() {
                        return None;
                    }
                }
                let rendered_modal = crate::plugins::ui::render_ui_node(
                    modal_tree,
                    pid,
                    1,
                    theme,
                    self.active_plugin_input.as_ref(),
                    &self.plugin_input_focus_handle,
                    self.active_plugin_select.as_ref(),
                    cx,
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
                                                        .child("Настройки плагина"),
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
            .id("settings-modal-root")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .cursor(gpui::CursorStyle::Arrow)
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            // 1. Semi-transparent backdrop overlay: clicking here closes the modal
            .child(
                div()
                    .id("settings-modal-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000aa))
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if this.active_plugin_select.is_some() {
                            this.active_plugin_select = None;
                            cx.notify();
                        } else if this.active_color_picker.is_some() {
                            this.active_color_picker = None;
                            cx.notify();
                        } else {
                            this.close_settings(cx);
                        }
                    }))
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            // 2. Centered dialog layout container (non-blocking sibling)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .id("settings-modal-dialog")
                            .relative()
                            .flex()
                            .flex_col()
                            .w(px(900.0))
                            .h(px(660.0))
                            .max_w(px(980.0))
                            .max_h(px(760.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .cursor(gpui::CursorStyle::Arrow)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
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
                                    .rounded_b_xl()
                                    .cursor(gpui::CursorStyle::Arrow)
                                    // Left navigation panel
                                    .child(self.render_settings_nav(cx))
                                    // Right scrollable content panel
                                    .child(self.render_settings_content(cx)),
                            ),
                    ),
            )
            .when_some(plugin_modal_overlay, |this, overlay| this.child(overlay))
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
            .rounded_t_xl()
            .cursor(gpui::CursorStyle::Arrow)
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
                            .child(self.t("settings.title")),
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
    fn render_settings_nav(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let active_tab = &self.settings_tab;

        div()
            .id("settings-nav-scroll")
            .flex()
            .flex_col()
            .w(px(210.0))
            .min_w(px(210.0))
            .h_full()
            .overflow_scroll()
            .border_r_1()
            .border_color(theme.border_subtle)
            .rounded_bl_xl()
            .cursor(gpui::CursorStyle::Arrow)
            .p_2()
            .gap_1()
            .children(SettingsTab::all().iter().enumerate().map(|(idx, tab)| {
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
    }

    /// Renders the active tab's settings content container.
    fn render_settings_content(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let scroll_handle = self.settings_scroll_handle.clone();
        let is_catalog_open = self.plugin_catalog.is_open;

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
                SettingsTab::PluginSettings(ref id) => self.render_plugin_settings_tab(id, cx),
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
                    .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
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
        F1: Fn(&mut AppView, &mut Context<AppView>) + 'static,
        F2: Fn(&mut AppView, &mut Context<AppView>) + 'static,
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
                    .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
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
                    .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
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
                    .on_click(cx.listener(move |this: &mut AppView, _, window, cx| {
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
                            .on_click(cx.listener(move |this: &mut AppView, _, _, cx| {
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
            ),
            self.render_setting_row(
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
            ),
            self.render_setting_row(
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
            ),
            self.render_setting_row(
                &self.t("settings.ui.sidebar_smart_scroll"),
                Some(&self.t("settings.ui.sidebar_smart_scroll_desc")),
                div()
                    .id("row-sidebar-smart-scroll")
                    .child(self.render_toggle_switch(config.ui.sidebar_smart_scroll))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.ui.sidebar_smart_scroll = !this.config.ui.sidebar_smart_scroll;
                        // Сбрасываем курсорное состояние, чтобы при включении
                        // список не прыгал от устаревшей позиции мыши.
                        this.sidebar_smart_last_mouse_y.set(None);
                        this.sidebar_smart_target_y.set(None);
                        this.save_settings(cx);
                    })),
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
                &self.t("settings.editor.line_height"),
                Some(&self.t("settings.editor.line_height_desc")),
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

        let tabs_display = self.t_fmt("settings.editor.live_tabs_unit", &[&config.editor.live_tabs.to_string()]);
        let layout_rows = vec![
            self.render_setting_row(
                &self.t("settings.editor.max_width"),
                Some(&self.t("settings.editor.max_width_desc")),
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
                &self.t("settings.editor.word_wrap"),
                Some(&self.t("settings.editor.word_wrap_desc")),
                div()
                    .id("row-word-wrap")
                    .child(self.render_toggle_switch(self.word_wrap))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_word_wrap(cx);
                    })),
            ),
            self.render_setting_row(
                &self.t("settings.editor.live_tabs"),
                Some(&self.t("settings.editor.live_tabs_desc")),
                self.render_stepper(
                    "editor-tabs",
                    &tabs_display,
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
                &self.t("settings.search.max_terms"),
                Some(&self.t("settings.search.max_terms_desc")),
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
    fn render_files_tab(&self, _cx: &Context<Self>) -> gpui::Div {
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
                    .state
                    .vault
                    .as_ref()
                    .map(|v| v.is_core_plugin_enabled(id))
                    .unwrap_or(default_val);

                let toggle_id = p_id.clone();
                let toggle = div()
                    .id(gpui::ElementId::from(gpui::SharedString::from(format!("core-toggle-{}", id))))
                    .cursor_pointer()
                    .child(self.render_toggle_switch(is_enabled))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(vault) = &this.state.vault {
                            if is_enabled {
                                let _ = vault.disable_core_plugin(&toggle_id);
                            } else {
                                let _ = vault.enable_core_plugin(&toggle_id);
                            }
                            this.state.status_text = format!("Core plugin '{}' updated", toggle_id);
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
                    this.unload_all_plugins();
                    this.state.status_text = "Safe mode enabled: community plugins unloaded".to_string();
                } else {
                    // Safe mode turned OFF -> reload enabled plugins
                    this.load_installed_plugins_for_vault();
                    this.state.status_text = "Safe mode disabled: community plugins loaded".to_string();
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
        let installed_plugins = if let Some(vault) = &self.state.vault {
            let plugins_dir = vault.root.join(".vault").join("plugins");
            crate::plugins::loader::discover_plugins(&plugins_dir)
        } else {
            Vec::new()
        };

        let mut plugin_rows = Vec::new();
        for plugin in &installed_plugins {
            let p_id = plugin.manifest.id.clone();
            let is_enabled = self.state.vault.as_ref().map(|v| v.is_community_plugin_enabled(&p_id)).unwrap_or(false);

            let toggle_id = p_id.clone();
            let toggle = div()
                .id(gpui::ElementId::from(gpui::SharedString::from(format!("comm-toggle-{}", plugin.manifest.id))))
                .cursor_pointer()
                .child(self.render_toggle_switch(is_enabled))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(vault) = &this.state.vault {
                        if is_enabled {
                            let _ = vault.disable_community_plugin(&toggle_id);
                            // Unload runtime if running
                            if let Some(pos) = this.plugin_runtimes.iter().position(|r| r.manifest.id == toggle_id) {
                                let handle = this.plugin_runtimes.remove(pos);
                                let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                                this.state.plugin_host.unregister_plugin(&toggle_id);
                                this.state.commands.unregister_all_of(&toggle_id);
                                this.state.ribbon.unregister_all_of(&toggle_id);
                                this.state.status_bar.unregister_all_of(&toggle_id);
                                this.state.code_block_processors.unregister_all_of(&toggle_id);
                                this.code_block_views.retain(|_, (pid, _)| pid != &toggle_id);
                            }
                        } else {
                            let _ = vault.enable_community_plugin(&toggle_id);
                            // Reload plugins for vault
                            this.load_installed_plugins_for_vault();
                        }
                        cx.notify();
                    }
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
                                let _ = this.state.plugin_host.send_event(
                                    &settings_btn_id,
                                    crate::plugins::api::HostEvent::UiEvent {
                                        view_id: 0,
                                        node_id: "__refresh_setting_tab__".to_string(),
                                        event: "refresh".to_string(),
                                        value: None,
                                    },
                                );
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
                    let _ = this.state.plugin_host.send_event(
                        &details_btn_id,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id: 0,
                            node_id: "__refresh_setting_tab__".to_string(),
                            event: "refresh".to_string(),
                            value: None,
                        },
                    );
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
    fn render_plugin_settings_tab(&self, plugin_id: &str, cx: &Context<Self>) -> gpui::Div {
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
        let (plugin_dir, manifest, manifest_json) = if let Some(vault) = &self.state.vault {
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

        let is_enabled = self.state.vault.as_ref().map(|v| v.is_community_plugin_enabled(plugin_id)).unwrap_or(false);
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
                        if let Some(vault) = &this.state.vault {
                            if is_enabled {
                                let _ = vault.disable_community_plugin(&toggle_id);
                                if let Some(pos) = this.plugin_runtimes.iter().position(|r| r.manifest.id == toggle_id) {
                                    let handle = this.plugin_runtimes.remove(pos);
                                    let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                                    this.state.plugin_host.unregister_plugin(&toggle_id);
                                    this.state.commands.unregister_all_of(&toggle_id);
                                    this.state.ribbon.unregister_all_of(&toggle_id);
                                    this.state.status_bar.unregister_all_of(&toggle_id);
                                    this.state.code_block_processors.unregister_all_of(&toggle_id);
                                    this.code_block_views.retain(|_, (pid, _)| pid != &toggle_id);
                                }
                            } else {
                                let _ = vault.enable_community_plugin(&toggle_id);
                                this.load_installed_plugins_for_vault();
                            }
                            cx.notify();
                        }
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
                            this.load_installed_plugins_for_vault();
                            // Refresh settings tab
                            let _ = this.state.plugin_host.send_event(
                                &r_id,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id: 0,
                                    node_id: "__refresh_setting_tab__".to_string(),
                                    event: "refresh".to_string(),
                                    value: None,
                                },
                            );
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
        let custom_tree = self.plugin_views.get(&(p_id_str.clone(), 0));

        let settings_section = if let Some(tree) = custom_tree {
            let rendered_node = crate::plugins::ui::render_ui_node(
                tree,
                plugin_id,
                0,
                theme,
                self.active_plugin_input.as_ref(),
                &self.plugin_input_focus_handle,
                self.active_plugin_select.as_ref(),
                cx,
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
                .state
                .vault
                .as_ref()
                .and_then(|v| v.load_plugin_data(plugin_id))
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
}


