use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, MouseDownEvent,
    Styled, Window,
};
use crate::theme::Theme;
use crate::views::app_view::AppView;

impl AppView {
    /// Toggles the collapsed state of the right sidebar.
    pub fn toggle_right_sidebar(&mut self, cx: &mut Context<Self>) {
        self.right_sidebar_collapsed = !self.right_sidebar_collapsed;
        cx.notify();
    }

    /// Renders the 1px resizer divider between the editor and right sidebar with interactive hitbox.
    pub fn render_right_resizer(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_resizer_active = self.is_resizing_right_sidebar || self.is_hovering_right_resizer;
        let resizer_color = if is_resizer_active {
            theme.accent
        } else {
            theme.border
        };

        div()
            .id("right-sidebar-resizer")
            .relative()
            .flex_shrink_0()
            .w(px(1.0))
            .h_full()
            .bg(resizer_color)
            .child(
                div()
                    .id("right-sidebar-resizer-hitbox")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-3.0))
                    .w(px(7.0))
                    .cursor(gpui::CursorStyle::ResizeColumn)
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.is_hovering_right_resizer = *hovered;
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if event.click_count == 2 {
                            this.toggle_right_sidebar(cx);
                        } else {
                            this.is_resizing_right_sidebar = true;
                            cx.notify();
                        }
                    })),
            )
    }

    /// Renders the right sidebar container (Obsidian-style).
    pub fn render_right_sidebar(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let mut container = div()
            .id("right-sidebar")
            .flex()
            .flex_col()
            .w(px(self.right_sidebar_width.clamp(160.0, 600.0)))
            .flex_shrink_0()
            .overflow_hidden()
            .h_full()
            .bg(theme.bg_sidebar)
            // The adjacent `right-sidebar-resizer` already draws this border;
            // drawing another one here would double the separator (and stay
            // gray while the resizer turns accent).
            .p(px(8.0))
            .gap(px(6.0));

        let right_leaves: Vec<&crate::plugins::api::PluginLeafView> = self
            .plugin_leaf_views
            .values()
            .filter(|l| l.location == "right" || l.location.is_empty())
            .collect();

        if !right_leaves.is_empty() {
            let active_leaf_id = self
                .active_right_leaf
                .as_deref()
                .unwrap_or(&right_leaves[0].leaf_id);

            let active_leaf = right_leaves
                .iter()
                .find(|l| l.leaf_id == active_leaf_id)
                .copied()
                .unwrap_or(right_leaves[0]);

            // 1. Tab bar for right leaves
            let mut tab_bar = div()
                .id("right-sidebar-tabs")
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .pb(px(4.0))
                .border_b_1()
                .border_color(theme.border);

            for leaf in &right_leaves {
                let is_active = leaf.leaf_id == active_leaf.leaf_id;
                let leaf_id_select = leaf.leaf_id.clone();
                let leaf_id_close = leaf.leaf_id.clone();
                let icon_str = leaf.icon.as_deref().unwrap_or("layout");
                let icon_asset = crate::icons::map_obsidian_icon(icon_str);
                let title = leaf.title.clone();

                tab_bar = tab_bar.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .when(is_active, |d| d.bg(theme.bg_card))
                        .when(!is_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.select_right_leaf(&leaf_id_select, cx);
                            }),
                        )
                        .child(crate::icons::icon(
                            icon_asset,
                            13.0,
                            if is_active {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(
                            div()
                                .text_xs()
                                .max_w(px(120.0))
                                .overflow_hidden()
                                .text_color(if is_active {
                                    theme.text_primary
                                } else {
                                    theme.text_muted
                                })
                                .child(title),
                        )
                        .child(
                            div()
                                .px(px(2.0))
                                .rounded(px(2.0))
                                .text_xs()
                                .text_color(theme.text_muted)
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_plugin_leaf(&leaf_id_close, cx);
                                    }),
                                )
                                .child("×"),
                        ),
                );
            }

            container = container.child(tab_bar);

            // 2. Action buttons header if active leaf has actions
            if !active_leaf.actions.is_empty() {
                let mut actions_bar = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .py(px(2.0));

                for act in &active_leaf.actions {
                    let act_id = act.id.clone();
                    let act_plugin_id = active_leaf.plugin_id.clone();
                    let act_icon = crate::icons::map_obsidian_icon(&act.icon);
                    actions_bar = actions_bar.child(
                        div()
                            .p(px(3.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.trigger_plugin_action(&act_plugin_id, &act_id, cx);
                                }),
                            )
                            .child(crate::icons::icon(act_icon, 13.0, theme.text_muted)),
                    );
                }
                container = container.child(actions_bar);
            }

            // 3. Active leaf view content
            container = container.child(
                div()
                    .id("right-sidebar-leaf-content")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_scroll()
                    .p(px(4.0))
                    .rounded(px(4.0))
                    .bg(theme.bg_card)
                    .child(crate::plugins::ui::render_ui_node(
                        &active_leaf.tree,
                        &active_leaf.plugin_id,
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
                    )),
            );
        } else {
            container = container.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("No active plugin views"),
            );
        }

        container
    }
}
