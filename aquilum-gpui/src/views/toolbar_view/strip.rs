use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, MouseMoveEvent, SharedString, Styled,
};
use crate::formatting::ActiveFormatting;
use crate::icons;
use crate::theme::Theme;
use crate::toolbar::{
    ToolbarAlignment, ToolbarItem, ToolbarItemKind, ToolbarPlacement, ToolbarSpec,
};
use crate::views::app_view::AppView;

use super::helpers::is_command_disabled;
use super::ToolbarTooltipInfo;

impl AppView {
    /// Renders horizontal toolbar strips placed at the given location (e.g. Top).
    pub fn render_toolbar_strips(
        &self,
        placement: ToolbarPlacement,
        origin_x: f32,
        origin_y: f32,
        available_w: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let toolbars = self.state.toolbars.get_toolbars(placement);
        if toolbars.is_empty() {
            return None;
        }

        let fmt_state = self.state.active_formatting_state();

        let mut container = div()
            .id(match placement {
                ToolbarPlacement::Top => "editor-toolbar-top-container",
                ToolbarPlacement::Bottom => "editor-toolbar-bottom-container",
            })
            .flex()
            .flex_col()
            .w_full()
            .bg(theme.bg_card)
            .border_b_1()
            .border_color(theme.border_subtle);

        for spec in toolbars {
            container = container.child(self.render_single_toolbar_strip(
                &spec,
                origin_x,
                origin_y,
                available_w,
                &fmt_state,
                theme,
                cx,
            ));
        }

        Some(container)
    }

    /// Renders a single horizontal toolbar strip with overflow support.
    fn render_single_toolbar_strip(
        &self,
        spec: &ToolbarSpec,
        origin_x: f32,
        origin_y: f32,
        available_w: f32,
        fmt: &ActiveFormatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let toolbar_id = spec.id.clone();

        // Calculate item widths and partition for overflow (larger 32px buttons)
        let item_widths: Vec<f32> = spec
            .items
            .iter()
            .map(|item| match &item.kind {
                ToolbarItemKind::Button { .. } => 32.0,
                ToolbarItemKind::Dropdown { command, .. } => {
                    if command.is_some() {
                        46.0 // split button: 28px main + 18px arrow
                    } else {
                        44.0 // icon (17px) + chevron (12px) + gap (3px) + padding (6px * 2) = 44px
                    }
                }
                ToolbarItemKind::Separator => 11.0, // 1px line + 10px margin
            })
            .collect();

        let max_w = (available_w - 24.0).max(0.0);
        let overflow_btn_w = 44.0;

        // A prefix of items stays on the strip; everything after it goes into the
        // overflow menu (see `toolbar_visible_count`).
        let visible_count = toolbar_visible_count(&item_widths, max_w, overflow_btn_w);
        let visible_items: Vec<ToolbarItem> = spec.items[..visible_count].to_vec();
        let visible_widths: Vec<f32> = item_widths[..visible_count].to_vec();
        let overflow_items: Vec<ToolbarItem> = spec.items[visible_count..].to_vec();

        // Center the row when the toolbar is configured that way. The same offset is used
        // for tooltips, dropdown anchors and drag slots so they stay aligned with the
        // actual flex layout.
        let content_width = visible_widths.iter().sum::<f32>()
            + 3.0 * (visible_widths.len().saturating_sub(1) as f32)
            + if overflow_items.is_empty() {
                0.0
            } else if visible_items.is_empty() {
                overflow_btn_w
            } else {
                3.0 + overflow_btn_w
            };
        let inner_width = (available_w - 16.0).max(0.0);
        // Centered rows carry a flexible spacer on each side, so one gap sits before
        // the first icon and one after the last. When the row does not fit, both
        // spacers collapse and the row starts at that 11px, never past the right edge.
        let content_offset = if spec.alignment == ToolbarAlignment::Center {
            11.0 + ((inner_width - 6.0 - content_width) / 2.0).max(0.0)
        } else {
            8.0
        };

        // Drag state for this strip: the source and the icon under the pointer. The
        // insertion line is drawn between icons and the order changes only on release.
        let drag = self
            .toolbar_drag
            .as_ref()
            .filter(|drag| drag.is_dragging && drag.toolbar_id == toolbar_id && drag.can_drag);
        let from_index = drag.and_then(|drag| {
            visible_items
                .iter()
                .position(|item| item.id == drag.source_id)
        });
        let to_index = drag.and_then(|drag| {
            drag.target_id
                .as_ref()
                .and_then(|target| visible_items.iter().position(|item| &item.id == target))
        });
        let drop_boundary = match (from_index, to_index) {
            (Some(from), Some(to)) if from != to => {
                Some(if from < to { to + 1 } else { to })
            }
            _ => None,
        };
        let indicator_x = drop_boundary.map(|boundary| {
            content_offset + visible_widths[..boundary].iter().sum::<f32>()
                + 3.0 * boundary as f32
                - 1.5
        });

        let mut strip = div()
            .id(SharedString::from(format!("toolbar-strip-{}", toolbar_id)))
            .relative()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .h(px(40.0))
            .px(px(8.0))
            .gap(px(3.0))
            .overflow_hidden();

        // Flexible spacers center the row and shrink to zero when it does not fit,
        // instead of `justify_center` clipping equal parts off both edges (which left
        // the overflow button past the right edge).
        let centered = spec.alignment == ToolbarAlignment::Center;
        if centered {
            strip = strip.child(div().flex_1());
        }

        let item_start_y = origin_y;
        let mut item_slots: Vec<(f32, f32, String, bool)> = Vec::new();

        // Render visible items with accurate accumulated X positions in window coordinates
        let mut current_item_x = origin_x + content_offset;
        for (i, item) in visible_items.iter().enumerate() {
            let item_w = visible_widths[i];
            item_slots.push((
                current_item_x,
                current_item_x + item_w,
                item.id.clone(),
                !item.is_separator(),
            ));
            strip = strip.child(self.render_toolbar_item(
                &toolbar_id,
                item,
                current_item_x,
                item_start_y,
                item_w,
                true,
                fmt,
                theme,
                cx,
            ));
            current_item_x += item_w + 3.0;
        }

        // Render overflow menu if items exceeded available width
        if !overflow_items.is_empty() {
            let overflow_id = format!("{}-overflow", toolbar_id);
            // Clickable (opens the overflow menu) but not part of the reorderable list.
            item_slots.push((
                current_item_x,
                current_item_x + overflow_btn_w,
                overflow_id.clone(),
                false,
            ));
            let overflow_dropdown_item = ToolbarItem {
                id: overflow_id,
                kind: ToolbarItemKind::Dropdown {
                    icon: "more-horizontal".to_string(),
                    tooltip: Some("More actions".to_string()),
                    command: None,
                    items: overflow_items,
                },
            };
            strip = strip.child(self.render_toolbar_item(
                &toolbar_id,
                &overflow_dropdown_item,
                current_item_x,
                item_start_y,
                overflow_btn_w,
                false,
                fmt,
                theme,
                cx,
            ));
        }

        if centered {
            strip = strip.child(div().flex_1());
        }

        // Vertical insertion line where the dragged icon will land.
        if let Some(x) = indicator_x {
            strip = strip.child(
                div()
                    .id(SharedString::from(format!(
                        "toolbar-drop-indicator-{}",
                        toolbar_id
                    )))
                    .absolute()
                    .left(px(x))
                    .top(px(10.0))
                    .w(px(2.0))
                    .h(px(20.0))
                    .rounded_full()
                    .bg(theme.accent),
            );
        }

        // The press that arms a drag is handled by the icon itself (see
        // `render_toolbar_item`), so a click never depends on matching the pointer X
        // against these slots.
        strip = strip.on_mouse_move(cx.listener({
            let toolbar_id = toolbar_id.clone();
            let slots = item_slots.clone();
            move |this, event: &MouseMoveEvent, _, cx| {
                let Some(drag) = this.toolbar_drag.as_mut() else {
                    return;
                };
                if drag.toolbar_id != toolbar_id {
                    return;
                }
                let x: f32 = event.position.x.into();
                drag.pointer_x = x;
                drag.pointer_y = event.position.y.into();
                if drag.can_drag && (x - drag.start_x).abs() > 4.0 {
                    drag.is_dragging = true;
                }
                if drag.is_dragging {
                    if let Some((_, _, id, can_drag)) =
                        slots.iter().find(|(left, right, _, _)| x >= *left && x < *right)
                    {
                        if *can_drag {
                            drag.target_id = if id.as_str() == drag.source_id {
                                None
                            } else {
                                Some(id.clone())
                            };
                        }
                    }
                    cx.notify();
                }
            }
        }));
        // Released over a gap, separator or empty strip area: commit to the last
        // highlighted icon. Releases over an icon are committed here too, because the
        // icon's handler leaves a drag alone.
        strip = strip.on_mouse_up(MouseButton::Left, cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, _, _, cx| {
                let Some(drag) = this.toolbar_drag.take() else {
                    return;
                };
                if drag.toolbar_id == toolbar_id && drag.is_dragging && drag.can_drag {
                    if let Some(target) = drag.target_id {
                        this.reorder_toolbar_item(
                            &toolbar_id,
                            &drag.source_id,
                            &target,
                            cx,
                        );
                    }
                }
                cx.notify();
            }
        }));
        strip = strip.on_mouse_up_out(MouseButton::Left, cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, _, _, cx| {
                if this
                    .toolbar_drag
                    .as_ref()
                    .is_some_and(|drag| drag.toolbar_id == toolbar_id)
                {
                    this.toolbar_drag = None;
                    cx.notify();
                }
            }
        }));
        // Leaving the strip clears the indicated slot so the line never points at a
        // place the release cannot reach.
        strip = strip.on_hover(cx.listener({
            let toolbar_id = toolbar_id.clone();
            move |this, hovered: &bool, _, cx| {
                if *hovered {
                    return;
                }
                if let Some(drag) = this.toolbar_drag.as_mut() {
                    if drag.toolbar_id == toolbar_id && drag.target_id.take().is_some() {
                        cx.notify();
                    }
                }
            }
        }));

        // Right-click on the empty part of the strip opens the toolbar menu. Clicks on an
        // icon keep their normal behaviour.
        strip = strip.on_mouse_down(MouseButton::Right, cx.listener({
            let slots = item_slots.clone();
            move |this, event: &gpui::MouseDownEvent, _, cx| {
                let x: f32 = event.position.x.into();
                if slots.iter().any(|(left, right, _, _)| x >= *left && x < *right) {
                    return;
                }
                this.open_toolbar_context_menu(
                    event.position.x.into(),
                    event.position.y.into(),
                    (origin_x, origin_y, available_w, 40.0),
                    cx,
                );
            }
        }));

        strip
    }

    /// Renders an individual item (button, split-dropdown, dropdown, or separator).
    fn render_toolbar_item(
        &self,
        toolbar_id: &str,
        item: &ToolbarItem,
        item_x: f32,
        item_y: f32,
        item_w: f32,
        draggable: bool,
        fmt: &ActiveFormatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_dragged = self.toolbar_drag.as_ref().is_some_and(|drag| {
            drag.is_dragging
                && drag.toolbar_id == toolbar_id
                && drag.source_id == item.id
        });
        // While reordering, the insertion line is the only affordance: no hover/active
        // highlight on the icon under the pointer.
        let drag_active = self
            .toolbar_drag
            .as_ref()
            .is_some_and(|drag| drag.is_dragging);

        match &item.kind {
            ToolbarItemKind::Button { command_id, icon, tooltip } => {
                let cmd_id = command_id.clone();
                let is_disabled = is_command_disabled(&cmd_id, fmt);

                let icon_asset = icons::resolve_toolbar_icon(icon);
                let hotkey = self.get_command_hotkey(&cmd_id);
                let tooltip_text = tooltip.clone().or_else(|| self.get_command_name(&cmd_id));

                let tb_id = toolbar_id.to_string();
                let it_id = item.id.clone();

                let mut btn = div()
                    .id(SharedString::from(format!("tb-btn-{}-{}", tb_id, it_id)))
                    .h(px(32.0))
                    .w(px(32.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .when(is_dragged, |d| {
                        d.border_1()
                            .border_color(theme.accent)
                            .bg(theme.bg_selected)
                    });

                if is_disabled {
                    btn = btn
                        .opacity(0.35)
                        .cursor_not_allowed();
                } else {
                    btn = btn
                        .cursor_pointer()
                        .when(!drag_active, |d| {
                            d.hover(|h| h.bg(theme.bg_hover))
                                .active(|a| a.bg(theme.bg_hover))
                        });

                    let cmd_to_exec = cmd_id.clone();
                    let source_id = it_id.clone();
                    btn = btn.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(Self::arm_toolbar_drag_listener(
                            tb_id.clone(),
                            it_id.clone(),
                            draggable,
                        )),
                    );
                    btn = btn.on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        let Some(drag) = this.toolbar_drag.as_ref() else {
                            return;
                        };
                        // A drag that ended on another icon is committed as a reorder
                        // by the strip; this icon's action must not also run. A small
                        // pointer jitter stays a click and still executes.
                        if drag.source_id != source_id {
                            return;
                        }
                        this.toolbar_drag = None;
                        this.close_toolbar_dropdown(cx);
                        this.execute_command(&cmd_to_exec, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));

                    if let Some(title) = tooltip_text {
                        let hk = hotkey;
                        btn = btn.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: hk.clone(),
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }
                }

                let icon_color = if is_disabled {
                    theme.text_muted
                } else {
                    theme.text_primary
                };

                btn.child(
                    gpui::svg()
                        .path(icon_asset)
                        .w(px(17.0))
                        .h(px(17.0))
                        .text_color(icon_color)
                        .flex_shrink_0()
                )
            }
            ToolbarItemKind::Dropdown { icon, tooltip, command, items } => {
                let tb_id = toolbar_id.to_string();
                let it_id = item.id.clone();
                let dropdown_items = items.clone();

                let icon_asset = icons::resolve_toolbar_icon(icon);

                let is_open = self
                    .toolbar_view
                    .open_dropdown
                    .as_ref()
                    .map(|d| d.toolbar_id == tb_id && d.item_id == it_id)
                    .unwrap_or(false);

                let icon_color = theme.text_primary;

                let tooltip_text = tooltip.clone();

                if let Some(cmd_id) = command {
                    // Split button: main button executes command, arrow part opens menu
                    let cmd_to_exec = cmd_id.clone();
                    let tb_id_arrow = tb_id.clone();
                    let it_id_arrow = it_id.clone();
                    let dd_items_arrow = dropdown_items.clone();

                    let mut split_container = div()
                        .id(SharedString::from(format!("tb-split-{}-{}", tb_id, it_id)))
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(32.0))
                        .w(px(46.0))
                        .flex_shrink_0()
                        .rounded_md()
                        .when(is_dragged, |d| {
                            d.border_1()
                                .border_color(theme.accent)
                                .bg(theme.bg_selected)
                        })
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)));

                    // Main button
                    let main_btn = div()
                        .h_full()
                        .w(px(28.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded_l_md()
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::arm_toolbar_drag_listener(
                                tb_id.clone(),
                                it_id.clone(),
                                draggable,
                            )),
                        )
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, window, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.close_toolbar_dropdown(cx);
                                this.execute_command(&cmd_to_exec, window, cx);
                                window.focus(&this.focus_handle);
                                cx.notify();
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icon_asset)
                                .w(px(17.0))
                                .h(px(17.0))
                                .text_color(icon_color)
                                .flex_shrink_0()
                        );

                    // Dropdown chevron arrow
                    let arrow_btn = div()
                        .h_full()
                        .w(px(18.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded_r_md()
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::arm_toolbar_drag_listener(
                                tb_id.clone(),
                                it_id.clone(),
                                draggable,
                            )),
                        )
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, _, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.toggle_toolbar_dropdown(
                                    &tb_id_arrow,
                                    &it_id_arrow,
                                    item_x,
                                    item_y + 42.0,
                                    dd_items_arrow.clone(),
                                    cx,
                                );
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icons::ICON_CHEVRON_DOWN)
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(theme.text_secondary)
                                .flex_shrink_0()
                        );

                    split_container = split_container.child(main_btn).child(arrow_btn);

                    if let Some(title) = tooltip_text {
                        split_container = split_container.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: None,
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }

                    split_container
                } else {
                    // Regular dropdown button: clicking opens dropdown
                    let tb_id_click = tb_id.clone();
                    let it_id_click = it_id.clone();
                    let dd_items_click = dropdown_items.clone();

                    let mut dropdown_btn = div()
                        .id(SharedString::from(format!("tb-dropdown-{}-{}", tb_id, it_id)))
                        .h(px(32.0))
                        .w(px(44.0))
                        .flex_shrink_0()
                        .px(px(6.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(px(3.0))
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_dragged, |d| {
                            d.border_1()
                                .border_color(theme.accent)
                                .bg(theme.bg_selected)
                        })
                        .when(is_open, |d| d.bg(theme.bg_hover))
                        .when(!drag_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::arm_toolbar_drag_listener(
                                tb_id.clone(),
                                it_id.clone(),
                                draggable,
                            )),
                        )
                        .on_mouse_up(MouseButton::Left, cx.listener({
                            let source_id = it_id.clone();
                            move |this, _, _, cx| {
                                let Some(drag) = this.toolbar_drag.as_ref() else {
                                    return;
                                };
                                if drag.source_id != source_id {
                                    return;
                                }
                                this.toolbar_drag = None;
                                this.toggle_toolbar_dropdown(
                                    &tb_id_click,
                                    &it_id_click,
                                    item_x,
                                    item_y + 42.0,
                                    dd_items_click.clone(),
                                    cx,
                                );
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(icon_asset)
                                .w(px(17.0))
                                .h(px(17.0))
                                .text_color(icon_color)
                                .flex_shrink_0()
                        )
                        .child(
                            gpui::svg()
                                .path(icons::ICON_CHEVRON_DOWN)
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(theme.text_secondary)
                                .flex_shrink_0()
                        );

                    if let Some(title) = tooltip_text {
                        dropdown_btn = dropdown_btn.on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                            if this
                                .toolbar_drag
                                .as_ref()
                                .is_some_and(|drag| drag.is_dragging)
                            {
                                this.toolbar_view.hovered_tooltip = None;
                                cx.notify();
                                return;
                            }
                            if *is_hovered {
                                this.toolbar_view.hovered_tooltip = Some(ToolbarTooltipInfo {
                                    title: title.clone(),
                                    hotkey: None,
                                    x: item_x,
                                    y: item_y + 42.0,
                                    item_w,
                                });
                            } else if let Some(ref current) = this.toolbar_view.hovered_tooltip {
                                if current.title == title {
                                    this.toolbar_view.hovered_tooltip = None;
                                }
                            }
                            cx.notify();
                        }));
                    }

                    dropdown_btn
                }
            }
            ToolbarItemKind::Separator => div()
                .id(SharedString::from(format!("tb-sep-{}-{}", toolbar_id, item.id)))
                .h(px(20.0))
                .w(px(1.0))
                .bg(theme.border_subtle)
                .mx(px(5.0))
                .flex_shrink_0(),
        }
    }
}

/// Number of leading items that stay on the strip before the overflow menu.
///
/// A kept prefix must fit together with the overflow button while any tail remains.
/// Once an item does not fit, every later item moves to the menu: re-testing each
/// item on its own let a short separator or the last icon slip back onto the strip
/// after the overflow button's room had already been spent, so the centered row grew
/// past the right edge and the "more" button hung outside the window.
pub(super) fn toolbar_visible_count(item_widths: &[f32], max_w: f32, overflow_btn_w: f32) -> usize {
    let mut current_w = 0.0;
    for (idx, &width) in item_widths.iter().enumerate() {
        let w = width + 3.0; // width + gap
        let has_tail = idx + 1 < item_widths.len();
        let reserve_overflow = if has_tail { overflow_btn_w } else { 0.0 };
        if current_w + w + reserve_overflow > max_w {
            return idx;
        }
        current_w += w;
    }
    item_widths.len()
}
