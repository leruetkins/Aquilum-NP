use super::*;

/// Movement in px after which a press on the titlebar sidebar button becomes
/// a window drag instead of a click.
const DRAG_THRESHOLD_PX: f64 = 3.0;

/// True when a press that began at `origin` has moved far enough to count as
/// a window drag rather than a click.
fn is_window_drag(origin: gpui::Point<Pixels>, current: gpui::Point<Pixels>) -> bool {
    (current - origin).magnitude() >= DRAG_THRESHOLD_PX
}

impl AppView {
    /// Rail button in the titlebar: left click toggles the sidebar, right
    /// click pins the window, dragging moves the window.
    pub(super) fn render_sidebar_toggle_button(
        &self,
        icon_color: gpui::Rgba,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let pinned = self.always_on_top;
        div()
            .id("btn-toggle-sidebar")
            .flex()
            .items_center()
            .justify_center()
            .p_1()
            .rounded_md()
            .cursor_pointer()
            .when(pinned, |b| {
                b.bg(theme.accent).hover(|h| h.bg(theme.accent_hover))
            })
            .when(!pinned, |b| b.hover(|h| h.bg(theme.bg_hover)))
            .child(icons::icon(
                icons::ICON_PANEL_LEFT,
                15.0,
                if pinned { theme.bg_sidebar } else { icon_color },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| {
                    this.begin_sidebar_button_press(event.position);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                this.update_sidebar_button_drag(event, window);
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.finish_sidebar_button_click(cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.sidebar_press_origin = None;
                    this.sidebar_drag_started = false;
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_always_on_top(window, cx);
                }),
            )
    }

    fn begin_sidebar_button_press(&mut self, position: gpui::Point<Pixels>) {
        self.sidebar_press_origin = Some(position);
    }

    fn update_sidebar_button_drag(&mut self, event: &MouseMoveEvent, window: &mut Window) {
        if event.pressed_button != Some(MouseButton::Left) {
            // The release may happen outside the button (or be consumed by the
            // native move loop); the first button-free move ends the gesture.
            self.sidebar_press_origin = None;
            self.sidebar_drag_started = false;
            return;
        }
        // Moves keep arriving while the OS moves the window; they must not
        // start a second drag.
        if self.sidebar_drag_started {
            return;
        }
        let Some(origin) = self.sidebar_press_origin else {
            return;
        };
        if !is_window_drag(origin, event.position) {
            return;
        }
        self.sidebar_press_origin = None;
        self.sidebar_drag_started = true;
        crate::window_ops::start_window_drag(window);
    }

    fn finish_sidebar_button_click(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_drag_started {
            self.sidebar_drag_started = false;
            return;
        }
        self.toggle_sidebar(cx);
    }
}

#[cfg(test)]
mod tests;
