use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, Styled, Window};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

use super::ToolbarContextMenuState;

impl AppView {
    /// Opens the toolbar context menu at window coordinates.
    pub fn open_toolbar_context_menu(
        &mut self,
        x: f32,
        y: f32,
        area: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.close_toolbar_dropdown(cx);
        self.toolbar_context_menu = ToolbarContextMenuState {
            is_open: true,
            position_x: x,
            position_y: y,
            area_x: area.0,
            area_y: area.1,
            area_w: area.2,
            area_h: area.3,
        };
        cx.notify();
    }

    /// Closes the toolbar context menu.
    pub fn close_toolbar_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.toolbar_context_menu.is_open {
            self.toolbar_context_menu.is_open = false;
            cx.notify();
        }
    }

    /// Renders the context menu shown by right-clicking the empty toolbar area.
    pub fn render_toolbar_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let menu_w = 240.0;
        let menu_h = 44.0;
        let bounds = window.bounds();
        let win_w = f32::from(bounds.size.width);
        let win_h = f32::from(bounds.size.height);
        let menu_x = self
            .toolbar_context_menu
            .position_x
            .clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
        let menu_y = self
            .toolbar_context_menu
            .position_y
            .clamp(8.0, (win_h - menu_h - 8.0).max(8.0));
        let label = self.t("settings.toolbar.open_settings");

        div()
            .id("toolbar-context-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_toolbar_context_menu(cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();
                let menu = &this.toolbar_context_menu;
                let in_strip = x >= menu.area_x
                    && x <= menu.area_x + menu.area_w
                    && y >= menu.area_y
                    && y <= menu.area_y + menu.area_h;
                let area = (menu.area_x, menu.area_y, menu.area_w, menu.area_h);
                this.close_toolbar_context_menu(cx);
                // A second right-click on the same toolbar reopens the menu there.
                if in_strip {
                    this.open_toolbar_context_menu(x, y, area, cx);
                }
            }))
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("toolbar-context-panel")
                    .absolute()
                    .left(px(menu_x))
                    .top(px(menu_y))
                    .w(px(menu_w))
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_move(|_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("toolbar-context-settings")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_2p5()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_SETTINGS,
                                14.0,
                                theme.text_secondary,
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.text_primary)
                                    .child(label),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_toolbar_context_menu(cx);
                                this.open_settings(
                                    Some(crate::views::settings_view::SettingsTab::PluginSettings(
                                        crate::plugins::editor_toolbar::TOOLBAR_PLUGIN_ID
                                            .to_string(),
                                    )),
                                    cx,
                                );
                            })),
                    ),
            )
    }
}
