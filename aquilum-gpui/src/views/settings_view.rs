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
        }
    }

    /// Returns the user-facing localized display title for the tab.
    pub fn title(&self, lang: &str) -> String {
        crate::i18n::t(self.key(), lang)
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
                        if this.active_color_picker.is_some() {
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
                            .flex()
                            .flex_col()
                            .w(px(880.0))
                            .h(px(620.0))
                            .max_w(px(960.0))
                            .max_h(px(720.0))
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
            .rounded_bl_xl()
            .cursor(gpui::CursorStyle::Arrow)
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
                            .child(tab.title(&self.config.ui.language)),
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
        let scroll_handle = self.settings_scroll_handle.clone();

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
                                if phase == DispatchPhase::Capture
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
            render_shortcut_item(self.t("settings.shortcuts.open_folder"), "Ctrl + O"),
            render_shortcut_item(self.t("settings.shortcuts.new_note"), "Ctrl + N"),
            render_shortcut_item(self.t("settings.shortcuts.quick_open"), "Ctrl + P"),
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
}
