use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, MouseDownEvent,
    Styled,
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
            .when(is_resizer_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-1.0))
                        .w(px(3.0))
                        .bg(theme.accent),
                )
            })
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
    pub fn render_right_sidebar(&self, theme: &Theme, _cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("right-sidebar")
            .flex()
            .flex_col()
            .w(px(self.right_sidebar_width.clamp(160.0, 600.0)))
            .flex_shrink_0()
            .overflow_hidden()
            .h_full()
            .bg(theme.bg_sidebar)
            .border_l_1()
            .border_color(theme.border)
    }
}
