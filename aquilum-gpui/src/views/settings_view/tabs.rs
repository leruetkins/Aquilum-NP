use super::*;

impl SettingsView {

    // =========================================================================
    // 1. Interface (Ui) Tab
    // =========================================================================

    /// Renders the UI and Display settings tab.
    pub(super) fn render_ui_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        // The theme pickers reuse the smart list used by the language and
        // font selectors, so they stay searchable and keyboard-driven as the
        // preset list grows.
        let theme_picker = |slot: ThemeSlot, selected: &str, id_prefix: &'static str| {
            let selected_name = crate::theme::preset_name(selected).to_string();
            let open_popup = match self.theme_popup.as_ref() {
                Some((open_slot, popup)) if *open_slot == slot => Some(popup.clone()),
                _ => None,
            };

            div()
                .relative()
                .child(
                    div()
                        .id(gpui::ElementId::from(gpui::SharedString::from(format!(
                            "select-{id_prefix}-btn"
                        ))))
                        .cursor_pointer()
                        .child(self.render_select_pill(&selected_name))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.theme_popup_is_open(slot) {
                                this.close_theme_popup(cx);
                            } else {
                                this.open_theme_popup(slot, window, cx);
                            }
                        })),
                )
                .when_some(open_popup, |parent, popup| {
                    parent.child(
                        gpui::deferred(
                            div()
                                .id(gpui::ElementId::from(gpui::SharedString::from(format!(
                                    "{id_prefix}-dropdown-menu"
                                ))))
                                .absolute()
                                .top_full()
                                .right_0()
                                .mt_1()
                                .child(popup),
                        )
                        .with_priority(999),
                    )
                })
        };

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
                Some("ui.lightTheme"),
                &self.t("settings.ui.light_theme"),
                Some(&self.t("settings.ui.light_theme_desc")),
                theme_picker(ThemeSlot::Light, &config.ui.light_theme, "light-theme"),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.darkTheme"),
                &self.t("settings.ui.dark_theme"),
                Some(&self.t("settings.ui.dark_theme_desc")),
                theme_picker(ThemeSlot::Dark, &config.ui.dark_theme, "dark-theme"),
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
                        // Reset the cursor state so that when enabled
                        // the list does not jump from a stale mouse position.
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
            self.render_setting_row_with_id(
                Some("ui.swapTabBarButtons"),
                &self.t("settings.ui.swap_tab_bar_buttons"),
                Some(&self.t("settings.ui.swap_tab_bar_buttons_desc")),
                div()
                    .id("row-swap-tab-bar-buttons")
                    .child(self.render_toggle_switch(config.ui.swap_tab_bar_buttons))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.ui.swap_tab_bar_buttons = !this.config.ui.swap_tab_bar_buttons;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.noteWidth"),
                &self.t("settings.editor.note_width"),
                Some(&self.t("settings.editor.note_width_desc")),
                self.render_stepper(
                    "ui-note-width",
                    &self.t_fmt("settings.units.percent", &[&config.editor.editor_width_pct.to_string()]),
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.editor_width_pct > 0 {
                            this.config.editor.editor_width_pct = this.config.editor.editor_width_pct.saturating_sub(5);
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.editor_width_pct < 100 {
                            this.config.editor.editor_width_pct = (this.config.editor.editor_width_pct + 5).min(100);
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.noteAlign"),
                &self.t("settings.editor.note_align"),
                Some(&self.t("settings.editor.note_align_desc")),
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
                            .id("btn-align-left-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.note_align == crate::settings::NoteAlignment::Left, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.note_align != crate::settings::NoteAlignment::Left, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.note_align = crate::settings::NoteAlignment::Left;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-align-center-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.note_align == crate::settings::NoteAlignment::Center, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.note_align != crate::settings::NoteAlignment::Center, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.note_align = crate::settings::NoteAlignment::Center;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.titleAlign"),
                &self.t("settings.editor.title_align_window"),
                Some(&self.t("settings.editor.title_align_window_desc")),
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
                            .id("btn-title-align-left-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::WindowLeft, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::WindowLeft, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::WindowLeft;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-title-align-center-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::WindowCenter, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::WindowCenter, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::WindowCenter;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.titleAlignEditor"),
                &self.t("settings.editor.title_align_editor"),
                Some(&self.t("settings.editor.title_align_editor_desc")),
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
                            .id("btn-title-align-editor-left-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::EditorLeft, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::EditorLeft, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::EditorLeft;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-title-align-editor-center-ui")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::EditorCenter, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::EditorCenter, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::EditorCenter;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
        ];

        let font_rows = vec![
            self.render_setting_row_with_id(
                Some("ui.fontFamily"),
                &self.t("settings.ui.font_family"),
                Some(&self.t("settings.ui.font_family_desc")),
                self.render_select_pill(&config.ui.font.font_family),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.fontWeight"),
                &self.t("settings.ui.font_weight"),
                Some(&self.t("settings.ui.font_weight_desc")),
                self.render_select_pill(&self.t(if config.ui.font.font_weight >= 600 {
                    "settings.ui.font_weight_semibold"
                } else {
                    "settings.ui.font_weight_regular"
                })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("ui.fontSize"),
                &self.t("settings.ui.font_size"),
                Some(&self.t("settings.ui.font_size_desc")),
                self.render_stepper(
                    "ui-font-size",
                    &self.t_fmt("settings.units.px", &[&config.ui.font.font_size_base.to_string()]),
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
                cx,
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
    pub(super) fn render_editor_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let heading_rows = vec![
            self.render_setting_row_with_id(
                Some("editor.heading1"),
                &self.t("settings.editor.h1_title"),
                Some(&self.t("settings.editor.h1_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h1-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h1.font_size.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.heading2"),
                &self.t("settings.editor.h2_title"),
                Some(&self.t("settings.editor.h2_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h2-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h2.font_size.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.heading3"),
                &self.t("settings.editor.h3_title"),
                Some(&self.t("settings.editor.h3_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h3-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h3.font_size.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.heading4"),
                &self.t("settings.editor.h4_title"),
                Some(&self.t("settings.editor.h4_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h4-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h4.font_size.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.heading5"),
                &self.t("settings.editor.h5_title"),
                Some(&self.t("settings.editor.h5_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h5-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h5.font_size.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.heading6"),
                &self.t("settings.editor.h6_title"),
                Some(&self.t("settings.editor.h6_desc")),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(self.render_stepper(
                        "h6-size",
                        &self.t_fmt("settings.units.px", &[&config.editor.headings.h6.font_size.to_string()]),
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
                cx,
            ),
        ];

        let font_rows = vec![
            {
                let current_font = config.editor.font.font_family.clone();
                self.render_setting_row_with_id(
                    Some("editor.fontFamily"),
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
                    cx,
                )
            },
            self.render_setting_row_with_id(
                Some("editor.fontSizeBase"),
                &self.t("settings.editor.font_size"),
                Some(&self.t("settings.editor.font_size_desc")),
                self.render_stepper(
                    "editor-font-size",
                    &self.t_fmt("settings.units.px", &[&config.editor.font.font_size_base.to_string()]),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.lineHeight"),
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
                cx,
            ),
        ];

        let tabs_display = self.t_fmt("settings.editor.live_tabs_unit", &[&config.editor.live_tabs.to_string()]);
        let layout_rows = vec![
            self.render_setting_row_with_id(
                Some("editor.noteWidth"),
                &self.t("settings.editor.note_width"),
                Some(&self.t("settings.editor.note_width_desc")),
                self.render_stepper(
                    "editor-note-width",
                    &self.t_fmt("settings.units.percent", &[&config.editor.editor_width_pct.to_string()]),
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.editor_width_pct > 0 {
                            this.config.editor.editor_width_pct = this.config.editor.editor_width_pct.saturating_sub(5);
                            this.save_settings(cx);
                        }
                    },
                    |this: &mut SettingsView, cx| {
                        if this.config.editor.editor_width_pct < 100 {
                            this.config.editor.editor_width_pct = (this.config.editor.editor_width_pct + 5).min(100);
                            this.save_settings(cx);
                        }
                    },
                    cx,
                ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.noteAlign"),
                &self.t("settings.editor.note_align"),
                Some(&self.t("settings.editor.note_align_desc")),
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
                            .id("btn-align-left-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.note_align == crate::settings::NoteAlignment::Left, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.note_align != crate::settings::NoteAlignment::Left, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.note_align = crate::settings::NoteAlignment::Left;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-align-center-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.note_align == crate::settings::NoteAlignment::Center, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.note_align != crate::settings::NoteAlignment::Center, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.note_align = crate::settings::NoteAlignment::Center;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.titleAlign"),
                &self.t("settings.editor.title_align_window"),
                Some(&self.t("settings.editor.title_align_window_desc")),
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
                            .id("btn-title-align-left-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::WindowLeft, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::WindowLeft, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::WindowLeft;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-title-align-center-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::WindowCenter, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::WindowCenter, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::WindowCenter;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.titleAlignEditor"),
                &self.t("settings.editor.title_align_editor"),
                Some(&self.t("settings.editor.title_align_editor_desc")),
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
                            .id("btn-title-align-editor-left-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::EditorLeft, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::EditorLeft, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_left"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::EditorLeft;
                                this.save_settings(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-title-align-editor-center-editor")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(config.editor.title_align == crate::settings::TitleAlignment::EditorCenter, |s| s.bg(theme.accent).text_color(theme.text_on_accent))
                            .when(config.editor.title_align != crate::settings::TitleAlignment::EditorCenter, |s| s.text_color(theme.text_muted).hover(|h| h.bg(theme.bg_hover)))
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings.editor.align_center"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.config.editor.title_align = crate::settings::TitleAlignment::EditorCenter;
                                this.save_settings(cx);
                            })),
                    ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.maxWidth"),
                &self.t("settings.editor.max_width"),
                Some(&self.t("settings.editor.max_width_desc")),
                self.render_stepper(
                    "editor-max-width",
                    &self.t_fmt("settings.units.ch", &[&config.editor.max_width_ch.to_string()]),
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
                cx,
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
            self.render_setting_row_with_id(
                Some("editor.liveTabs"),
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
                cx,
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
            self.render_setting_row_with_id(
                Some("editor.exactRevealCursor"),
                &self.t("settings.editor.exact_reveal_cursor"),
                Some(&self.t("settings.editor.exact_reveal_cursor_desc")),
                div()
                    .id("row-exact-reveal-cursor")
                    .child(self.render_toggle_switch(config.editor.exact_reveal_cursor))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.exact_reveal_cursor = !this.config.editor.exact_reveal_cursor;
                        this.save_settings(cx);
                    })),
                cx,
            ),
        ];

        let debounce_display = self.t_fmt("settings.editor.debounce_unit", &[&config.editor.save_debounce_ms.to_string()]);
        let input_rows = vec![
            self.render_setting_row_with_id(
                Some("editor.smartDashes"),
                &self.t("settings.editor.smart_dashes"),
                Some(&self.t("settings.editor.smart_dashes_desc")),
                div()
                    .id("row-smart-dashes")
                    .child(self.render_toggle_switch(config.editor.smart_dashes))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.smart_dashes = !this.config.editor.smart_dashes;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.listCallouts"),
                &self.t("settings.editor.list_callouts"),
                Some(&self.t("settings.editor.list_callouts_desc")),
                div()
                    .id("row-list-callouts")
                    .child(self.render_toggle_switch(config.editor.list_callouts))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.list_callouts = !this.config.editor.list_callouts;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.linkSuggest"),
                &self.t("settings.editor.link_suggest"),
                Some(&self.t("settings.editor.link_suggest_desc")),
                div()
                    .id("row-link-suggest")
                    .child(self.render_toggle_switch(config.editor.link_suggest))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.link_suggest = !this.config.editor.link_suggest;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.hideCurrentLineOnSelection"),
                &self.t("settings.editor.hide_current_line_on_selection"),
                Some(&self.t("settings.editor.hide_current_line_on_selection_desc")),
                div()
                    .id("row-hide-current-line-on-selection")
                    .child(self.render_toggle_switch(config.editor.hide_current_line_on_selection))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.editor.hide_current_line_on_selection = !this.config.editor.hide_current_line_on_selection;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("editor.debounce"),
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
                cx,
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
    pub(super) fn render_reader_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let reader_rows = vec![
            self.render_setting_row_with_id(
                Some("reader.fontFamily"),
                &self.t("settings.reader.font_family"),
                Some(&self.t("settings.reader.font_family_desc")),
                self.render_select_pill(&config.reader.font.font_family),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("reader.lineHeight"),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("reader.justify"),
                &self.t("settings.reader.justify"),
                Some(&self.t("settings.reader.justify_desc")),
                div()
                    .id("row-justify")
                    .child(self.render_toggle_switch(config.reader.justify))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.reader.justify = !this.config.reader.justify;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("reader.hyphenate"),
                &self.t("settings.reader.hyphenate"),
                Some(&self.t("settings.reader.hyphenate_desc")),
                div()
                    .id("row-hyphenate")
                    .child(self.render_toggle_switch(config.reader.hyphenate))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.reader.hyphenate = !this.config.reader.hyphenate;
                        this.save_settings(cx);
                    })),
                cx,
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
    pub(super) fn render_search_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let config = &self.config;

        let search_rows = vec![
            self.render_setting_row_with_id(
                Some("search.candidatePool"),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("search.maxTerms"),
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
                cx,
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
    pub(super) fn render_templates_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let template_rows = vec![
            self.render_setting_row_with_id(
                Some("templates.folder"),
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
                cx,
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
    pub(super) fn render_files_tab(&self, window: &Window, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let attachment_rows = vec![
            self.render_setting_row_with_id(
                Some("files.attachmentsFolder"),
                &self.t("settings.files.attachments_folder"),
                Some(&self.t("settings.files.attachments_folder_desc")),
                self.render_files_folder_input(window, cx),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("files.showAttachmentsFolder"),
                &self.t("settings.files.show_attachments_folder"),
                Some(&self.t("settings.files.show_attachments_folder_desc")),
                div()
                    .id("row-show-attachments-folder")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.files.show_attachments_folder))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.files.show_attachments_folder = !this.config.files.show_attachments_folder;
                        this.save_settings(cx);
                        if let Some(app) = this.weak_app.upgrade() {
                            app.update(cx, |app, cx| {
                                app.state.reload_file_tree();
                                cx.notify();
                            });
                        }
                        cx.notify();
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("files.autoTrashAttachments"),
                &self.t("settings.files.auto_trash_attachments"),
                Some(&self.t("settings.files.auto_trash_attachments_desc")),
                div()
                    .id("row-auto-trash-attachments")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.files.auto_trash_attachments))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.files.auto_trash_attachments = !this.config.files.auto_trash_attachments;
                        this.save_settings(cx);
                    })),
                cx,
            ),
        ];

        let image_rows = vec![
            self.render_setting_row_with_id(
                Some("images.cornerRadius"),
                &self.t("editor.image.images_radius"),
                Some(&self.t("editor.image.images_radius_desc")),
                self.image_stepper(
                    "row-image-radius",
                    self.t_fmt("settings.units.percent", &[&config.images.corner_radius.to_string()]),
                    config.images.corner_radius > 0,
                    config.images.corner_radius < 50,
                    cx,
                    |this, cx| {
                        this.config.images.corner_radius =
                            this.config.images.corner_radius.saturating_sub(1);
                        this.save_settings(cx);
                    },
                    |this, cx| {
                        this.config.images.corner_radius =
                            (this.config.images.corner_radius + 1).min(50);
                        this.save_settings(cx);
                    },
                ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.captionTemplate"),
                &self.t("editor.image.caption_template"),
                Some(&self.t("editor.image.caption_template_desc")),
                {
                    let current = config.images.caption_template.clone();
                    div()
                        .id("row-caption-template")
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
                        .cursor_pointer()
                        .child(current.clone())
                        .child(crate::icons::icon(
                            crate::icons::ICON_CHEVRON_DOWN,
                            12.0,
                            theme.text_muted,
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // i18n-allow: caption template presets are document content, not UI chrome
                            const PRESETS: [&str; 3] = ["Рис. {n}. {text}", "Fig. {n}. {text}", "{text}"];
                            let current = this.config.images.caption_template.clone();
                            let index = PRESETS.iter().position(|preset| *preset == current).unwrap_or(0);
                            this.config.images.caption_template = PRESETS[(index + 1) % PRESETS.len()].to_string();
                            this.save_settings(cx);
                        }))
                },
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.captionPosition"),
                &self.t("editor.image.caption_position_default"),
                Some(&self.t("editor.image.caption_position_default_desc")),
                {
                    let position = config.images.caption_position.clone();
                    let button = |id: &'static str, label: String, selected: bool, value: &'static str| {
                        div()
                            .id(gpui::ElementId::Name(id.into()))
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(selected, |s| {
                                s.bg(theme.accent).text_color(theme.text_on_accent)
                            })
                            .when(!selected, |s| {
                                s.text_color(theme.text_muted)
                                    .hover(|h| h.bg(theme.bg_hover))
                            })
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.config.images.caption_position = value.to_string();
                                this.save_settings(cx);
                            }))
                            .child(label)
                    };
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .bg(theme.bg_sidebar)
                        .p_0p5()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .child(button(
                            "row-caption-above",
                            self.t("editor.image.above"),
                            position == "above",
                            "above",
                        ))
                        .child(button(
                            "row-caption-below",
                            self.t("editor.image.below"),
                            position != "above",
                            "below",
                        ))
                },
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.captionNumbered"),
                &self.t("editor.image.caption_numbered_default"),
                Some(&self.t("editor.image.caption_numbered_default_desc")),
                div()
                    .id("row-caption-numbered")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.images.caption_numbered))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.images.caption_numbered = !this.config.images.caption_numbered;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.openEditorAfterInsert"),
                &self.t("editor.image.open_editor_after_insert"),
                Some(&self.t("editor.image.open_editor_after_insert_desc")),
                div()
                    .id("row-open-image-editor")
                    .cursor_pointer()
                    .child(self.render_toggle_switch(config.images.open_editor_after_insert))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.config.images.open_editor_after_insert = !this.config.images.open_editor_after_insert;
                        this.save_settings(cx);
                    })),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.largeFileMb"),
                &self.t("editor.image.large_file_mb"),
                Some(&self.t("editor.image.large_file_mb_desc")),
                self.image_stepper(
                    "row-large-file",
                    self.t_fmt("settings.units.mb", &[&config.images.large_file_mb.to_string()]),
                    config.images.large_file_mb > 5,
                    config.images.large_file_mb < 200,
                    cx,
                    |this, cx| {
                        this.config.images.large_file_mb =
                            this.config.images.large_file_mb.saturating_sub(5).max(5);
                        this.save_settings(cx);
                    },
                    |this, cx| {
                        this.config.images.large_file_mb =
                            (this.config.images.large_file_mb + 5).min(200);
                        this.save_settings(cx);
                    },
                ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.trashDays"),
                &self.t("editor.image.trash_days"),
                Some(&self.t("editor.image.trash_days_desc")),
                self.image_stepper(
                    "row-trash-days",
                    format!("{}", config.images.trash_days),
                    config.images.trash_days > 1,
                    config.images.trash_days < 365,
                    cx,
                    |this, cx| {
                        this.config.images.trash_days =
                            this.config.images.trash_days.saturating_sub(1).max(1);
                        this.save_settings(cx);
                    },
                    |this, cx| {
                        this.config.images.trash_days =
                            (this.config.images.trash_days + 1).min(365);
                        this.save_settings(cx);
                    },
                ),
                cx,
            ),
            self.render_setting_row_with_id(
                Some("images.clearTrash"),
                &self.t("editor.image.clear_trash"),
                Some(&self.t("editor.image.clear_trash_desc")),
                div()
                    .id("row-clear-image-trash")
                    .px_3()
                    .py_1()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .child(self.t("editor.image.clear"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(app) = this.weak_app.upgrade() {
                            app.update(cx, |app, _| {
                                if let Some(root) = app.state.workspace_path.clone() {
                                    let folder = app.config.files.folder.clone();
                                    crate::images::clear_trash(&root, &folder);
                                    crate::images::invalidate_resolution();
                                }
                            });
                        }
                        cx.notify();
                    })),
                cx,
            ),
        ];

        let deletion_rows = vec![
            self.render_setting_row_with_id(
                Some("files.closeDeletedTabs"),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("files.keepModifiedOnDelete"),
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
                cx,
            ),
        ];

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(
                &self.t("settings.files.attachments_section"),
                attachment_rows,
            ))
            .child(self.render_settings_card(
                &self.t("settings.files.images_section"),
                image_rows,
            ))
            .child(self.render_settings_card(
                &self.t("settings.files.deletion_section"),
                deletion_rows,
            ))
    }

    // =========================================================================
    // 10. Trash Tab
    // =========================================================================

    /// Renders deleted notes management and trash retention tab.
    pub(super) fn render_trash_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let config = &self.config;

        let trash_rows = vec![
            self.render_setting_row_with_id(
                Some("trash.autoDelete"),
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
                cx,
            ),
            self.render_setting_row_with_id(
                Some("trash.folder"),
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
                cx,
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
    pub(super) fn render_system_tab(&self, _cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let system_rows = vec![
            self.render_setting_row(
                &self.t("settings.system.version"),
                None,
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
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
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
    pub(super) fn render_shortcuts_tab(&self, _cx: &Context<Self>) -> gpui::Div {
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
    pub(super) fn render_core_plugins_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let _theme = &self.theme;
        let header_title = self.t("settings.plugins.core.title");

        // Built-in modules list
        let plugins = vec![
            (
                "command-palette",
                self.t("settings.plugins.core.command_palette.name"),
                self.t("settings.plugins.core.command_palette.desc"),
                true, // Command palette is always on
            ),
            (
                "word-counter",
                self.t("settings.plugins.core.word_counter.name"),
                self.t("settings.plugins.core.word_counter.desc"),
                true,
            ),
            (
                "editor-toolbar",
                self.t("settings.plugins.core.editor_toolbar.name"),
                self.t("settings.plugins.core.editor_toolbar.desc"),
                true,
            ),
            (
                "file-recovery",
                self.t("settings.plugins.core.file_recovery.name"),
                self.t("settings.plugins.core.file_recovery.desc"),
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
                            let lang = this.config.ui.language.clone();
                            app.update(cx, |app, cx| {
                                if let Some(vault) = &app.state.vault {
                                    if is_enabled {
                                        let _ = vault.disable_core_plugin(&tid);
                                    } else {
                                        let _ = vault.enable_core_plugin(&tid);
                                    }
                                    if tid == "editor-toolbar" {
                                        crate::plugins::editor_toolbar::sync_editor_toolbar_with_vault(&app.state, Some(vault));
                                    }
                                    app.state.status_text = crate::i18n::t_args(
                                        "settings.plugins.core.status_updated",
                                        &lang,
                                        &[("plugin", tid.as_str())],
                                    );
                                    cx.notify();
                                }
                            });
                            cx.notify();
                        }
                    }));

                let action_container = if id == "editor-toolbar" {
                    let gear_btn = div()
                        .id("core-settings-editor-toolbar")
                        .p_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(self.theme.bg_hover))
                        .child(icons::icon(icons::ICON_SETTINGS, 16.0, self.theme.text_secondary))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings_tab = SettingsTab::PluginSettings("editor-toolbar".to_string());
                            cx.notify();
                        }));
                    div().flex().flex_row().items_center().gap_3().child(gear_btn).child(toggle)
                } else {
                    div().child(toggle)
                };

                let setting_id = match id {
                    "command-palette" => Some("corePlugins.commandPalette"),
                    "word-counter" => Some("corePlugins.wordCounter"),
                    "editor-toolbar" => Some("corePlugins.editorToolbar"),
                    "file-recovery" => Some("corePlugins.fileRecovery"),
                    _ => None,
                };
                self.render_setting_row_with_id(setting_id, &name, Some(&desc), action_container, cx)
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&header_title, rows))
    }

    /// Renders the Community Plugins settings tab (Obsidian-style third-party plugins management).
    pub(super) fn render_community_plugins_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;

        let header_title = self.t("settings.plugins.community.title");

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

        let safe_mode_row = self.render_setting_row_with_id(
            Some("communityPlugins.safeMode"),
            &self.t("settings.plugins.community.safe_mode"),
            Some(&self.t("settings.plugins.community.safe_mode_desc")),
            safe_mode_toggle,
            cx,
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
                    .child(self.t("settings.plugins.community.browse")),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_plugin_catalog(window, cx);
            }));

        let browse_row = self.render_setting_row(
            &self.t("settings.plugins.community.catalog"),
            Some(&self.t("settings.plugins.community.catalog_desc")),
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
                    self.t("settings.plugins.community.compat_full"),
                ),
                crate::plugins::compat::CompatibilityStatus::Partial(score) => (
                    rgba(0xdbab0922),
                    rgba(0xd29922ff),
                    self.t_fmt(
                        "settings.plugins.community.compat_score",
                        &[&format!("{:.0}", score * 100.0)],
                    ),
                ),
                crate::plugins::compat::CompatibilityStatus::Incompatible => (
                    rgba(0xf8514922),
                    rgba(0xf85149ff),
                    self.t("settings.plugins.community.incompatible"),
                ),
                crate::plugins::compat::CompatibilityStatus::Unknown => (
                    rgba(0x8b949e22),
                    rgba(0x8b949eff),
                    self.t("settings.plugins.community.compat_unknown"),
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

        let installed_card_title = self.t_fmt(
            "settings.plugins.community.installed",
            &[&installed_plugins.len().to_string()],
        );

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.render_settings_card(&header_title, vec![safe_mode_row, browse_row]))
            .when(!plugin_rows.is_empty(), |this| {
                this.child(self.render_settings_card(&installed_card_title, plugin_rows))
            })
    }

    /// Converts parsed inline spans into a StyledText or InteractiveText element that wraps across lines
    pub(super) fn render_markdown_inlines(
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
    pub(super) fn render_plugin_settings_tab(&self, plugin_id: &str, window: &Window, cx: &Context<Self>) -> gpui::Div {
        if plugin_id == "editor-toolbar" {
            return self.render_editor_toolbar_settings(window, cx);
        }

        let theme = &self.theme;

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
                    .child(self.t("settings.plugins.detail.back")),
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
                        self.t("settings.plugins.detail.disable")
                    } else {
                        self.t("settings.plugins.detail.enable")
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
                                .child(self.t("settings.plugins.detail.reload")),
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
                                .child(self.t("settings.plugins.detail.open_folder")),
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
                            .child(self.t("common.delete")),
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
                                self.t("settings.plugins.detail.enabled")
                            } else {
                                self.t("settings.plugins.detail.disabled")
                            }),
                    ),
            )
            .when(!author_str.is_empty(), |parent| {
                parent.child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(self.t_fmt("settings.plugins.detail.author", &[author_str.as_str()])),
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
                &self.t("settings.plugins.metadata.name"),
                None,
                div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD).text_color(theme.text_primary).child(display_name),
            ),
            self.render_setting_row(
                &self.t("settings.plugins.metadata.id"),
                None,
                div().text_xs().font_family(self.editor_font_family()).text_color(theme.text_secondary).child(actual_manifest_id),
            ),
            self.render_setting_row(
                &self.t("settings.plugins.metadata.version"),
                None,
                div().text_xs().text_color(theme.text_secondary).child(version_str),
            ),
            self.render_setting_row(
                &self.t("settings.plugins.metadata.min_app_version"),
                None,
                div().text_xs().text_color(theme.text_secondary).child(format!(">= {}", min_app)),
            ),
            self.render_setting_row(
                &self.t("settings.plugins.metadata.author"),
                None,
                div().text_xs().text_color(theme.text_secondary).child(if author_str.is_empty() { "—".to_string() } else { author_str }),
            ),
        ];

        if let Some(author_url) = author_url_str {
            let author_url_click = author_url.clone();
            meta_rows.push(self.render_setting_row(
                &self.t("settings.plugins.metadata.author_url"),
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
                &self.t("settings.plugins.metadata.desktop_only"),
                None,
                div().text_xs().text_color(theme.text_secondary).child(if desktop_only {
                    self.t("settings.plugins.metadata.desktop_only_true")
                } else {
                    self.t("settings.plugins.metadata.desktop_only_false")
                }),
            ));
        }

        meta_rows.push(self.render_setting_row(
            &self.t("settings.plugins.metadata.install_path"),
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
                        .child(self.t("settings.plugins.readme.title")),
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
                                .child(self.t("settings.plugins.readme.missing")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(self.t("settings.plugins.readme.hint")),
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
                        .child(self.t("settings.plugins.config.title")),
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
            let info_hint = self.t("settings.plugins.config.store");

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
                &self.t("settings.plugins.config.card"),
                Some(&info_hint),
                json_preview,
            );

            self.render_settings_card(
                &self.t("settings.plugins.config.card_short"),
                vec![row],
            )
        };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(top_action_bar)
            .child(details_header)
            .child(self.render_settings_card(&self.t("settings.plugins.metadata.title"), meta_rows))
            .child(content_section)
            .child(settings_section)
    }
}
