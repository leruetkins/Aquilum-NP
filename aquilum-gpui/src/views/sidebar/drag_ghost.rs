//! Floating preview ("ghost") shared by sidebar drag-and-drop gestures.
//!
//! Drawn at the pointer so the gesture stays visible even when the source row scrolls
//! out of view or is dimmed.

use gpui::{div, prelude::*, px, AnyElement, SharedString};

use crate::icons;
use crate::theme::Theme;

/// Builds the ghost for an entry dragged inside the sidebar.
pub(crate) fn sidebar_drag_ghost(
    display_name: impl Into<SharedString>,
    icon: &'static str,
    icon_size: f32,
    pointer_x: f32,
    pointer_y: f32,
    theme: &Theme,
) -> AnyElement {
    div()
        .absolute()
        .left(px(pointer_x + 14.0))
        .top(px(pointer_y + 10.0))
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.bg_card)
        .border_1()
        .border_color(theme.accent)
        .shadow_lg()
        .opacity(0.92)
        .child(icons::icon(icon, icon_size, theme.accent))
        .child(
            div()
                .text_xs()
                .text_color(theme.text_primary)
                .child(display_name.into()),
        )
        .into_any_element()
}
