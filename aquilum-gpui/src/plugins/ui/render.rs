use std::cell::Cell;
use std::rc::Rc;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, point, px, AnyElement, Context, ElementId, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use crate::theme::Theme;
use crate::views::app_view::AppView;
use super::style::*;
use super::tree::*;

/// Trait implemented by views that host declarative plugin UI elements (e.g. `AppView`, `SettingsView`).
pub trait UiSink: 'static + Sized {
    fn send_plugin_ui_event(
        &mut self,
        plugin_id: &str,
        view_id: u64,
        node_id: String,
        event: &str,
        value: Option<String>,
        cx: &mut Context<Self>,
    );
    fn plugin_input_focus_handle(&self) -> &FocusHandle;
    fn plugin_input_blink_enable(&mut self, cx: &mut Context<Self>);
    fn plugin_input_blink_pause_and_show(&mut self, cx: &mut Context<Self>);
    fn active_plugin_input(&self) -> Option<&PluginInputFocusState>;
    fn set_active_plugin_input(&mut self, state: Option<PluginInputFocusState>);
    fn active_plugin_input_mut(&mut self) -> Option<&mut PluginInputFocusState>;
    fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>);
    fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<SelectOption>,
        current_value: String,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    );
    fn close_plugin_select(&mut self, cx: &mut Context<Self>);
    fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool;

    fn plugin_modal_scrollbar_drag_active(&self) -> bool { false }
    fn start_plugin_modal_scrollbar_drag(&mut self, _pointer_y: f32, _scroll_y: f32, _cx: &mut Context<Self>) {}
    fn set_plugin_modal_scroll_y(&mut self, _scroll_y: f32, _cx: &mut Context<Self>) {}
    fn plugin_modal_middle_active(&self) -> bool { false }
    fn start_plugin_modal_middle_scroll(&mut self, _cx: &mut Context<Self>) {}
    fn stop_plugin_modal_middle_scroll(&mut self, _cx: &mut Context<Self>) {}
    fn on_plugin_modal_middle_mouse_move(&self, _y: f32, _cx: &mut Context<Self>) {}
    fn plugin_modal_smart_bounds(&self) -> Option<std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>> { None }
}

impl UiSink for AppView {
    fn send_plugin_ui_event(
        &mut self,
        plugin_id: &str,
        view_id: u64,
        node_id: String,
        event: &str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let _ = self.state.plugin_host.send_event(
            plugin_id,
            crate::plugins::api::HostEvent::UiEvent {
                view_id,
                node_id,
                event: event.to_string(),
                value,
            },
        );
        cx.notify();
    }

    fn plugin_input_focus_handle(&self) -> &FocusHandle {
        &self.plugin_input_focus_handle
    }

    fn plugin_input_blink_enable(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.enable(crate::blink_manager::BlinkTarget::PluginInput, cx);
    }

    fn plugin_input_blink_pause_and_show(&mut self, cx: &mut Context<Self>) {
        self.plugin_input_blink.pause_and_show(crate::blink_manager::BlinkTarget::PluginInput, cx);
    }

    fn active_plugin_input(&self) -> Option<&PluginInputFocusState> {
        self.active_plugin_input.as_ref()
    }

    fn set_active_plugin_input(&mut self, state: Option<PluginInputFocusState>) {
        self.active_plugin_input = state;
    }

    fn active_plugin_input_mut(&mut self) -> Option<&mut PluginInputFocusState> {
        self.active_plugin_input.as_mut()
    }

    fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        self.handle_plugin_input_key(event, cx);
    }

    fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<SelectOption>,
        current_value: String,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.open_plugin_select(plugin_id, view_id, node_id, options, current_value, window, cx);
    }

    fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        self.close_plugin_select(cx);
    }

    fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.is_plugin_select_open(plugin_id, view_id, node_id)
    }

    fn plugin_modal_scrollbar_drag_active(&self) -> bool {
        self.plugin_modal_scrollbar_drag.active
    }

    fn start_plugin_modal_scrollbar_drag(&mut self, pointer_y: f32, scroll_y: f32, cx: &mut Context<Self>) {
        self.plugin_modal_scrollbar_drag.begin(pointer_y, scroll_y);
        cx.notify();
    }

    fn set_plugin_modal_scroll_y(&mut self, scroll_y: f32, cx: &mut Context<Self>) {
        self.plugin_modal_scroll_handle.set_offset(point(px(0.0), px(-scroll_y)));
        cx.notify();
    }

    fn plugin_modal_middle_active(&self) -> bool {
        self.plugin_modal_middle_active
    }

    fn start_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        self.start_plugin_modal_middle_scroll(cx);
    }

    fn stop_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        self.stop_plugin_modal_middle_scroll(cx);
    }

    fn on_plugin_modal_middle_mouse_move(&self, y: f32, cx: &mut Context<Self>) {
        self.on_plugin_modal_middle_mouse_move(y, cx);
    }

    fn plugin_modal_smart_bounds(&self) -> Option<std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>> {
        Some(self.plugin_modal_smart_bounds.clone())
    }
}

/// Scroll context and state passed down to declarative UI rendering.
#[derive(Clone, Default)]
pub struct PluginScrollContext<'a> {
    pub scroll_handle: Option<&'a gpui::ScrollHandle>,
    pub scrollbar_drag_active: bool,
    pub middle_scroll_active: bool,
    pub smart_bounds: Option<std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>>,
}

/// Renders a declarative UI node into GPUI elements using application theme tokens.
pub fn render_ui_node<V: UiSink>(
    node: &UiNode,
    plugin_id: &str,
    view_id: u64,
    theme: &Theme,
    active_input: Option<&PluginInputFocusState>,
    input_focus_handle: &FocusHandle,
    active_select: Option<&(String, u64, String)>,
    select_popup: Option<&gpui::Entity<crate::views::tab_list::TabListPopup>>,
    caret_visible: bool,
    window: &gpui::Window,
    cx: &Context<V>,
    scroll_ctx: Option<&PluginScrollContext>,
) -> AnyElement {
    render_ui_node_inner(
        node,
        None,
        plugin_id,
        view_id,
        theme,
        active_input,
        input_focus_handle,
        active_select,
        select_popup,
        caret_visible,
        window,
        cx,
        scroll_ctx,
    )
}

/// `sheet` — the style the plugin's `styles.css` already declared for this node. It comes only
/// from the `Styled` branch: no one else knows about the stylesheet, and the class heuristic must
/// be able to yield to it (see `BoxOwnership`).
fn render_ui_node_inner<V: UiSink>(
    node: &UiNode,
    sheet: Option<&NodeStyle>,
    plugin_id: &str,
    view_id: u64,
    theme: &Theme,
    active_input: Option<&PluginInputFocusState>,
    input_focus_handle: &FocusHandle,
    active_select: Option<&(String, u64, String)>,
    select_popup: Option<&gpui::Entity<crate::views::tab_list::TabListPopup>>,
    caret_visible: bool,
    window: &gpui::Window,
    cx: &Context<V>,
    scroll_ctx: Option<&PluginScrollContext>,
) -> AnyElement {
    match node {
        UiNode::Column { id, class, gap, children } => {
            if children.is_empty() {
                return div().into_any_element();
            }
            let gap_px = gap.to_px();
            let cls = class.as_deref().unwrap_or("");
            let is_modal_container = cls.contains("modal-container") || cls.contains("modal") || cls.contains("modal-content");
            let is_header = cls.contains("header");
            let is_item_info = cls.contains("item-info") || cls.contains("-info") || cls.contains("info");
            let is_list = cls.contains("list") || cls.contains("scroll");

            let col_id = id.clone().unwrap_or_else(|| if is_list { "list".into() } else { "col".into() });
            let mut col = div()
                .id(ElementId::from(SharedString::from(format!("ui-col-{}-{}", view_id, col_id))))
                .flex()
                .flex_col()
                .w_full();

            if is_modal_container {
                col = col
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden();
            } else if is_header {
                col = col
                    .flex_shrink_0()
                    .px(px(24.0))
                    .pt(px(24.0))
                    .pb(px(14.0))
                    .gap(px(4.0));
            } else if is_item_info {
                col = col
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .overflow_hidden();
            } else if is_list {
                col = col
                    .flex_1()
                    .min_h_0()
                    .max_h(px(340.0))
                    .overflow_scroll()
                    .py(px(4.0));

                if let Some(ctx) = scroll_ctx {
                    if let Some(handle) = ctx.scroll_handle {
                        col = col.track_scroll(handle);
                    }
                }
            } else {
                col = col.gap(px(gap_px));
            }

            if let Some(node_id) = id {
                let p_id = plugin_id.to_string();
                let n_id = node_id.clone();
                col = col
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.send_plugin_ui_event(&p_id, view_id, n_id.clone(), "click", None, cx);
                    }));
            }

            let is_modal = cls.contains("modal-container") || cls.contains("modal") || cls.contains("modal-content");

            let mut rendered_children: Vec<AnyElement> = Vec::new();
            let mut i = 0;
            while i < children.len() {
                if let UiNode::Button { class: btn_cls, .. } = &children[i] {
                    let is_full = btn_cls.as_deref().map_or(false, |c| c.contains("w-full") || c.contains("full-width") || c.contains("is-fullwidth"));
                    if !is_full {
                        let mut btn_elements = Vec::new();
                        while i < children.len() {
                            if let UiNode::Button { class: inner_cls, .. } = &children[i] {
                                let inner_full = inner_cls.as_deref().map_or(false, |c| c.contains("w-full") || c.contains("full-width") || c.contains("is-fullwidth"));
                                if !inner_full {
                                    btn_elements.push(render_ui_node_inner(
                                        &children[i],
                                        None,
                                        plugin_id,
                                        view_id,
                                        theme,
                                        active_input,
                                        input_focus_handle,
                                        active_select,
                                        select_popup,
                                        caret_visible,
                                        window,
                                        cx,
                                        scroll_ctx,
                                    ));
                                    i += 1;
                                    continue;
                                }
                            }
                            break;
                        }

                        if is_modal {
                            rendered_children.push(
                                div()
                                    .flex()
                                    .flex_row()
                                    .w_full()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(12.0))
                                    .pt(px(14.0))
                                    .pb(px(24.0))
                                    .px(px(24.0))
                                    .children(btn_elements)
                                    .into_any_element(),
                            );
                        } else {
                            rendered_children.push(
                                div()
                                    .flex()
                                    .flex_row()
                                    .w_full()
                                    .items_center()
                                    .justify_start()
                                    .gap(px(8.0))
                                    .py(px(4.0))
                                    .children(btn_elements)
                                    .into_any_element(),
                            );
                        }
                        continue;
                    }
                }

                rendered_children.push(render_ui_node_inner(
                    &children[i],
                    None,
                    plugin_id,
                    view_id,
                    theme,
                    active_input,
                    input_focus_handle,
                    active_select,
                    select_popup,
                    caret_visible,
                    window,
                    cx,
                    scroll_ctx,
                ));
                i += 1;
            }

            let col = apply_sheet(col, sheet);

            if is_list {
                if let Some(ctx) = scroll_ctx {
                    if let Some(handle) = ctx.scroll_handle {
                        let scroll_style = crate::views::scrollbar::ScrollbarStyle {
                            track_w: 16.0,
                            thumb_w: 6.0,
                            thumb_w_active: 8.0,
                            thumb_inset: 3.0,
                            min_thumb_h: 24.0,
                            track_right_offset: 2.0,
                        };
                        let is_dragging = ctx.scrollbar_drag_active;
                        let is_middle = ctx.middle_scroll_active;

                        let mut wrapper = div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .max_h(px(340.0))
                            .w_full()
                            // The scrollbar gutter is host chrome, not plugin padding.
                            // In the stylesheet `.ef-list { padding: 8px 0 }`, so keeping the
                            // gutter in the node padding lets the style legitimately override it —
                            // then the content slides under the scrollbar.
                            .pr(px(14.0))
                            .on_mouse_down(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.start_plugin_modal_middle_scroll(cx);
                            }))
                            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.stop_plugin_modal_middle_scroll(cx);
                            }))
                            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                                this.on_plugin_modal_middle_mouse_move(event.position.y.into(), cx);
                            }))
                            .child(col.children(rendered_children));

                        if let Some(bounds_cell) = ctx.smart_bounds.clone() {
                            wrapper = wrapper.child(
                                gpui::canvas(
                                    move |bounds, _window, _cx| {
                                        bounds_cell.set(bounds);
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            );
                        }

                        if is_middle {
                            wrapper = wrapper.child(
                                div()
                                    .absolute()
                                    .top(px(6.0))
                                    .left(px(8.0))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded_md()
                                    .bg(theme.bg_card)
                                    .border_1()
                                    .border_color(theme.border)
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child("⇅ СКМ-скролл"),
                            );
                        }

                        if let Some(geometry) = crate::views::scrollbar::ScrollbarGeometry::from_handle(handle, &scroll_style) {
                            let bar = crate::views::scrollbar::overlay_scrollbar(
                                "ui-list-scrollbar-track",
                                &geometry,
                                &scroll_style,
                                theme,
                                is_dragging,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let pointer_y: f32 = event.position.y.into();
                                    let target_y = geometry.offset_for_track_click(pointer_y);
                                    this.set_plugin_modal_scroll_y(target_y, cx);
                                }),
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.start_plugin_modal_scrollbar_drag(event.position.y.into(), geometry.scroll_y, cx);
                                }),
                            );
                            wrapper = wrapper.child(bar);
                        }

                        return wrapper.into_any_element();
                    }
                }
            }

            col.children(rendered_children).into_any_element()
        }
        UiNode::Row { id, class, gap, children } => {
            let gap_px = gap.to_px();
            let is_tree_row = class.as_deref().map_or(false, |c| {
                c.contains("tree-item-self")
                    || c.contains("nav-file-title")
                    || c.contains("nav-folder-title")
                    || c.contains("recent-files-title")
                    || c.contains("tree-row")
                    || (c.contains("nav-file") && !c.contains("nav-folder"))
            });
            let is_modal_item = !is_tree_row && class.as_deref().map_or(false, |c| {
                c.contains("ef-item") || c.contains("modal-item") || (view_id != 0 && (c.contains("item") || c.contains("entry")))
            });
            let is_stats = class.as_deref().map_or(false, |c| c.contains("stats") || c.contains("count") || c.contains("summary"));
            let is_controls = class.as_deref().map_or(false, |c| c.contains("controls") || c.contains("actions") || c.contains("toolbar"));
            let is_footer = class.as_deref().map_or(false, |c| c.contains("footer") || c.contains("buttons") || c.contains("button-container"));

            let row_id = id.clone().unwrap_or_else(|| "row".into());
            let mut row = div()
                .id(ElementId::from(SharedString::from(format!("ui-row-{}-{}", view_id, row_id))))
                .flex()
                .flex_row()
                .items_center()
                .w_full();

            if is_tree_row {
                let is_active = class.as_deref().map_or(false, |c| has_word(c, "active"));
                row = row
                    .px(px(8.0))
                    .py(px(3.0))
                    .min_h(px(24.0))
                    .gap(px(6.0))
                    .rounded_md()
                    .cursor_pointer()
                    .justify_between();

                if is_active {
                    row = row
                        .bg(theme.bg_selected)
                        .text_color(theme.accent);
                } else {
                    row = row
                        .hover(|h| h.bg(theme.bg_hover))
                        .text_color(theme.text_secondary);
                }
            } else if is_modal_item {
                row = row
                    // The row height is a multiple of 4: at 125–200% scale it lands on a whole
                    // number of pixels; otherwise adjacent row separators "jump" by 1px.
                    // 68 matches a two-line row (name + path) with 10+10 padding.
                    .min_h(px(68.0))
                    .px(px(24.0))
                    .py(px(8.0))
                    .gap(px(12.0))
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .hover(|h| h.bg(theme.bg_hover));
            } else if is_stats {
                row = row
                    .flex_shrink_0()
                    .px(px(24.0))
                    .py(px(10.0))
                    .bg(theme.bg_sidebar)
                    .border_b_1()
                    .border_color(theme.border_subtle);
            } else if is_controls {
                row = row
                    .flex_shrink_0()
                    .px(px(24.0))
                    .py(px(10.0))
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(theme.border_subtle);
            } else if is_footer {
                // No background of its own: the footer is flush with the dialog's bottom edge, and
                // `overflow_hidden` in this GPUI clips to a rectangle (`ContentMask` stores only `bounds`),
                // so a filled footer poked its square corners past the rounded corner. The dialog
                // background is the same anyway — there is nothing to remove.
                row = row
                    .flex_shrink_0()
                    .px(px(24.0))
                    .py(px(14.0))
                    .gap(px(10.0))
                    .justify_end()
                    .border_t_1()
                    .border_color(theme.border_subtle);
            } else {
                row = row.gap(px(gap_px));
            }

            if let Some(node_id) = id {
                let p_id = plugin_id.to_string();
                let n_id = node_id.clone();
                if !is_modal_item && !is_tree_row {
                    row = row
                        .justify_between()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover));
                }
                row = row.on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.send_plugin_ui_event(&p_id, view_id, n_id.clone(), "click", None, cx);
                }));
            }
            apply_sheet(row, sheet)
                .children(children.iter().map(|child| {
                    render_ui_node_inner(
                        child,
                        None,
                        plugin_id,
                        view_id,
                        theme,
                        active_input,
                        input_focus_handle,
                        active_select,
                        select_popup,
                        caret_visible,
                        window,
                        cx,
                        scroll_ctx,
                    )
                }))
                .into_any_element()
        }
        UiNode::Text { value, class, style } => {
            let cls = class.as_deref().unwrap_or("");
            if cls.contains("tree-item-inner") || cls.contains("nav-file-title-content") {
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(theme.text_primary)
                    .overflow_hidden()
                    .truncate()
                    .child(value.clone())
                    .into_any_element()
            } else if has_word(cls, "name") || has_word(cls, "title") {
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .overflow_hidden()
                    .child(value.clone())
                    .into_any_element()
            } else if has_word(cls, "path") || has_word(cls, "desc") || has_word(cls, "description") || has_word(cls, "faint") {
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .overflow_hidden()
                    .child(value.clone())
                    .into_any_element()
            } else if has_word(cls, "count") || has_word(cls, "badge") || has_word(cls, "stats") {
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(value.to_uppercase())
                    .into_any_element()
            } else if has_word(cls, "subtitle") || has_word(cls, "subtext") {
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(value.clone())
                    .into_any_element()
            } else if has_word(cls, "scanning") || has_word(cls, "loading") || cls.contains("empty-state") || has_word(cls, "empty") || has_word(cls, "status") {
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .py(px(32.0))
                    .px(px(24.0))
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(value.clone())
                    .into_any_element()
            } else {
                match style {
                    TextStyle::Normal => div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme.text_primary)
                        .child(value.clone())
                        .into_any_element(),
                    TextStyle::Muted => div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(value.clone())
                        .into_any_element(),
                    TextStyle::Heading => div()
                        .text_base()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .child(value.clone())
                        .into_any_element(),
                    TextStyle::Code => div()
                        .flex_shrink_0()
                        .text_xs()
                        .bg(theme.code_bg)
                        .text_color(theme.success)
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .child(value.clone())
                        .into_any_element(),
                }
            }
        }
        UiNode::Markdown { value } => div()
            .text_sm()
            .text_color(theme.text_primary)
            .child(value.clone())
            .into_any_element(),
        UiNode::Button {
            id,
            label,
            class,
            variant,
            disabled,
        } => {
            let is_disabled = *disabled;
            // A disabled button's colors are ours. The plugin does not know our theme: its fallback
            // values (`var(--text-error, #ff6b6b)` and the like) are tuned for a dark theme and on
            // a light one give "light red on pink" — the label is unreadable. Dimensions,
            // padding and radius from the stylesheet still stay with the plugin.
            let filtered;
            let sheet = if is_disabled {
                filtered = sheet.map(|s| NodeStyle { background: None, color: None, opacity: None, ..s.clone() });
                filtered.as_ref()
            } else {
                sheet
            };
            let cls = class.as_deref().unwrap_or("");
            let is_icon_btn = cls.contains("clickable-icon")
                || cls.contains("menu-item-icon")
                || cls.contains("file-delete")
                || cls.contains("delete")
                || cls.contains("icon")
                || label == "×"
                || label == "✕";
            let is_danger_btn = !is_icon_btn && (matches!(variant, ButtonVariant::Danger) || cls.contains("danger") || cls.contains("warning"));
            let is_danger_active = cls.contains("danger--active") || has_word(cls, "active") || (is_danger_btn && !is_disabled);
            let is_secondary_btn = !is_icon_btn && (matches!(variant, ButtonVariant::Secondary) || cls.contains("secondary") || cls.contains("ghost") || cls.contains("cancel"));
            let is_primary_btn = !is_icon_btn && (matches!(variant, ButtonVariant::Primary) || cls.contains("mod-cta") || cls.contains("mod-primary") || cls.contains("primary"));

            let p_id = plugin_id.to_string();
            let n_id = id.clone();

            let mut btn = div()
                .id(ElementId::from(SharedString::from(format!("ui-btn-{}-{}", view_id, id))))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .cursor(if is_disabled {
                    gpui::CursorStyle::Arrow
                } else {
                    gpui::CursorStyle::PointingHand
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                });

            if is_icon_btn {
                btn = btn
                    .w(px(20.0))
                    .h(px(20.0))
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .hover(|s| s.bg(theme.bg_hover).text_color(theme.text_primary));
            } else if is_danger_btn {
                if is_danger_active {
                    btn = btn
                        .h(px(30.0))
                        .px(px(14.0))
                        .min_w(px(80.0))
                        .rounded_md()
                        .bg(gpui::rgb(0xd23f4d))
                        .text_color(gpui::rgb(0xffffff))
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .shadow_xs();
                } else {
                    btn = btn
                        .h(px(30.0))
                        .px(px(14.0))
                        .min_w(px(80.0))
                        .rounded_md()
                        .bg(gpui::rgba(0xed879620))
                        .border_1()
                        .border_color(gpui::rgba(0xed879640))
                        .text_color(gpui::rgb(0xed8796))
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .opacity(0.6);
                }
            } else if is_secondary_btn {
                btn = btn
                    .h(px(28.0))
                    .px(px(12.0))
                    .min_w(px(70.0))
                    .rounded_md()
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .text_color(theme.text_primary)
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .shadow_xs();
            } else {
                let (bg_color, border_color, text_color) = match variant {
                    ButtonVariant::Default => (
                        theme.bg_card,
                        theme.border,
                        theme.text_primary,
                    ),
                    ButtonVariant::Primary => (
                        theme.accent,
                        theme.accent,
                        theme.text_on_accent,
                    ),
                    ButtonVariant::Danger => (
                        gpui::rgb(0xed8796),
                        gpui::rgb(0xed8796),
                        gpui::rgb(0x181825),
                    ),
                    ButtonVariant::Secondary => (
                        gpui::rgba(0x00000000),
                        gpui::rgba(0x00000000),
                        theme.text_muted,
                    ),
                };
                btn = btn
                    .h(if is_primary_btn { px(32.0) } else { px(28.0) })
                    .px(if is_primary_btn { px(18.0) } else { px(12.0) })
                    .min_w(if is_primary_btn { px(90.0) } else { px(64.0) })
                    .rounded_md()
                    .bg(bg_color)
                    .border_1()
                    .border_color(border_color)
                    .shadow_xs()
                    .text_color(text_color)
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM);
            }

            if !is_disabled {
                if is_icon_btn {
                    btn = btn.hover(|s| s.bg(theme.bg_hover).text_color(theme.text_primary)).active(|s| s.opacity(0.7));
                } else if is_danger_active {
                    btn = btn.hover(|s| s.opacity(0.88)).active(|s| s.opacity(0.75));
                } else if is_secondary_btn {
                    btn = btn.hover(|s| s.bg(theme.bg_hover)).active(|s| s.bg(theme.bg_card));
                } else {
                    match variant {
                        ButtonVariant::Default => btn = btn
                            .hover(|s| s.bg(theme.bg_hover).border_color(theme.accent))
                            .active(|s| s.bg(theme.bg_hover)),
                        ButtonVariant::Primary | ButtonVariant::Danger => btn = btn
                            .hover(|s| s.opacity(0.88))
                            .active(|s| s.opacity(0.75)),
                        ButtonVariant::Secondary => btn = btn
                            .hover(|s| s.bg(theme.bg_hover).text_color(theme.text_primary)),
                    }
                }
                btn = btn.on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.send_plugin_ui_event(
                        &p_id,
                        view_id,
                        n_id.clone(),
                        "click",
                        None,
                        cx,
                    );
                }));
            } else if !is_danger_btn {
                btn = btn.opacity(0.5);
            }

            apply_sheet(btn, sheet).child(label.clone()).into_any_element()
        }
        UiNode::Input {
            id,
            placeholder,
            value,
        } => {
            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let is_active = active_input.map_or(false, |st| {
                st.plugin_id == p_id && st.view_id == view_id && st.node_id == n_id
            });

            let current_val = if is_active {
                active_input.unwrap().value.clone()
            } else {
                value.clone().unwrap_or_default()
            };
            let cursor_pos = if is_active {
                active_input.unwrap().cursor_pos
            } else {
                0
            };
            let selection_range = if is_active {
                active_input.unwrap().selection_range()
            } else {
                None
            };
            let box_origin_x = if is_active {
                active_input.unwrap().box_origin_x.clone()
            } else {
                Rc::new(Cell::new(0.0))
            };

            let pixel_scale = window.scale_factor();
            let run = gpui::TextRun {
                len: current_val.len(),
                font: gpui::font("Segoe UI"),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                gpui::SharedString::from(current_val.clone()),
                gpui::px(14.0),
                &[run],
                None,
            );

            let val_chars: Vec<char> = current_val.chars().collect();
            let cursor_x = if current_val.is_empty() {
                0.0
            } else {
                let byte_index: usize = val_chars[..cursor_pos.min(val_chars.len())]
                    .iter()
                    .map(|c| c.len_utf8())
                    .sum();
                crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
            };

            let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
                if !current_val.is_empty() {
                    let b1: usize = val_chars[..s.min(val_chars.len())]
                        .iter()
                        .map(|c| c.len_utf8())
                        .sum();
                    let b2: usize = val_chars[..e.min(val_chars.len())]
                        .iter()
                        .map(|c| c.len_utf8())
                        .sum();
                    crate::snap::snapped_span(
                        f32::from(shaped.x_for_index(b1)),
                        f32::from(shaped.x_for_index(b2)),
                        pixel_scale,
                    )
                } else {
                    (0.0, 0.0)
                }
            } else {
                (0.0, 0.0)
            };

            let show_caret = is_active && caret_visible && selection_range.is_none();

            let p_id_click = p_id.clone();
            let n_id_click = n_id.clone();
            let p_id_move = p_id.clone();
            let n_id_move = n_id.clone();
            let p_id_up = p_id.clone();
            let n_id_up = n_id.clone();
            let p_id_up_out = p_id.clone();
            let n_id_up_out = n_id.clone();
            let val_for_click = current_val.clone();
            let box_cell_click = box_origin_x.clone();

            let mut outer = div()
                .id(ElementId::from(SharedString::from(format!("ui-input-{}-{}", view_id, id))))
                .flex()
                .flex_1()
                .min_w(px(120.0))
                .h(px(28.0))
                .px(px(8.0))
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(if is_active { theme.accent } else { theme.border })
                .bg(theme.bg_sidebar)
                .cursor(gpui::CursorStyle::IBeam)
                .overflow_hidden();

            if is_active {
                outer = outer.track_focus(input_focus_handle);
            }

            outer
                .on_mouse_down(MouseButton::Left, cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(app.plugin_input_focus_handle());
                    app.plugin_input_blink_enable(cx);
                    app.plugin_input_blink_pause_and_show(cx);

                    let click_x: f32 = event.position.x.into();
                    let box_x = box_cell_click.get();
                    let current_text = if let Some(st) = app.active_plugin_input() {
                        if st.plugin_id == p_id_click && st.view_id == view_id && st.node_id == n_id_click {
                            st.value.clone()
                        } else {
                            val_for_click.clone()
                        }
                    } else {
                        val_for_click.clone()
                    };

                    let pos = plugin_input_click_pos(&current_text, click_x, box_x, window);
                    let total_chars = current_text.chars().count();

                    let (anchor, new_pos, selecting) = if event.click_count >= 3 {
                        (Some(0), total_chars, false)
                    } else if event.click_count == 2 {
                        let (s, e) = crate::views::goto_search::word_range_at(&current_text, pos);
                        (Some(s), e, false)
                    } else if event.modifiers.shift {
                        let prev_pos = app.active_plugin_input()
                            .and_then(|st| st.selection_anchor)
                            .or_else(|| app.active_plugin_input().map(|st| st.cursor_pos))
                            .unwrap_or(pos);
                        (Some(prev_pos), pos, true)
                    } else {
                        (Some(pos), pos, true)
                    };

                    app.set_active_plugin_input(Some(PluginInputFocusState {
                        plugin_id: p_id_click.clone(),
                        view_id,
                        node_id: n_id_click.clone(),
                        value: current_text,
                        cursor_pos: new_pos,
                        selection_anchor: anchor,
                        is_mouse_selecting: selecting,
                        box_origin_x: box_cell_click.clone(),
                        is_multiline: false,
                    }));
                    cx.notify();
                }))
                .on_mouse_move(cx.listener(move |app, event: &gpui::MouseMoveEvent, window, cx| {
                    let mut changed = false;
                    if let Some(st) = app.active_plugin_input_mut() {
                        if st.is_mouse_selecting && st.plugin_id == p_id_move && st.view_id == view_id && st.node_id == n_id_move {
                            let click_x: f32 = event.position.x.into();
                            let box_x = st.box_origin_x.get();
                            let new_pos = plugin_input_click_pos(&st.value, click_x, box_x, window);
                            if st.cursor_pos != new_pos {
                                st.cursor_pos = new_pos;
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        app.plugin_input_blink_pause_and_show(cx);
                        cx.notify();
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(move |app, _, _, cx| {
                    if let Some(st) = app.active_plugin_input_mut() {
                        if st.plugin_id == p_id_up && st.view_id == view_id && st.node_id == n_id_up {
                            st.is_mouse_selecting = false;
                            if st.selection_anchor == Some(st.cursor_pos) {
                                st.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up_out(MouseButton::Left, cx.listener(move |app, _, _, cx| {
                    if let Some(st) = app.active_plugin_input_mut() {
                        if st.plugin_id == p_id_up_out && st.view_id == view_id && st.node_id == n_id_up_out {
                            st.is_mouse_selecting = false;
                            if st.selection_anchor == Some(st.cursor_pos) {
                                st.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }))
                .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                    cx.stop_propagation();
                    app.handle_plugin_input_key(event, cx);
                }))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h(px(20.0))
                        .child(
                            gpui::canvas(
                                {
                                    let cell = box_origin_x.clone();
                                    move |bounds, _, _| {
                                        cell.set(f32::from(bounds.origin.x));
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .when(selection_range.is_some() && sel_w > 0.0, |s| {
                            s.child(
                                div()
                                    .absolute()
                                    .left(px(sel_x1))
                                    .top(px(0.0))
                                    .w(px(sel_w))
                                    .h(px(20.0))
                                    .bg(theme.selection_bg)
                                    .rounded_xs(),
                            )
                        })
                        .child(
                            if current_val.is_empty() {
                                div()
                                    .font_family("Segoe UI")
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(theme.text_muted)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(placeholder.as_deref().unwrap_or("").to_string())
                            } else {
                                div()
                                    .font_family("Segoe UI")
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(theme.text_primary)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(current_val.clone())
                            }
                        )
                        .when(show_caret, |s| {
                            s.child(
                                div()
                                    .absolute()
                                    .left(px(cursor_x))
                                    .top(px(1.0))
                                    .w(px(1.5))
                                    .h(px(18.0))
                                    .bg(theme.accent)
                                    .rounded_xs(),
                            )
                        }),
                )
                .into_any_element()
        }
        UiNode::Textarea {
            id,
            placeholder,
            value,
            rows,
        } => {
            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let is_active = active_input.map_or(false, |st| {
                st.plugin_id == p_id && st.view_id == view_id && st.node_id == n_id
            });
            let row_count = rows.unwrap_or(6).clamp(3, 20);
            let box_h = (row_count as f32 * 20.0 + 16.0).clamp(90.0, 360.0);

            if is_active {
                let current_state = active_input.unwrap();
                let val = &current_state.value;
                let total_chars = val.chars().count();
                let pos = current_state.cursor_pos.min(total_chars);
                let before: String = val.chars().take(pos).collect();
                let after: String = val.chars().skip(pos).collect();
                let has_val = !val.is_empty();

                div()
                    .id(ElementId::from(SharedString::from(format!("ui-textarea-{}-{}", view_id, id))))
                    .track_focus(input_focus_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(|app, _, window, cx| {
                        cx.stop_propagation();
                        window.focus(app.plugin_input_focus_handle());
                        app.plugin_input_blink_pause_and_show(cx);
                    }))
                    .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                        cx.stop_propagation();
                        app.handle_plugin_input_key(event, cx);
                    }))
                    .flex()
                    .flex_col()
                    .w_full()
                    .h(px(box_h))
                    .p(px(8.0))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.accent)
                    .bg(theme.bg_sidebar)
                    .cursor(gpui::CursorStyle::IBeam)
                    .overflow_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_start()
                            .min_w_0()
                            .w_full()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family("Consolas")
                                    .text_color(theme.text_primary)
                                    .child(before),
                            )
                            .when(caret_visible, |s| {
                                s.child(
                                    div()
                                        .w(px(2.0))
                                        .h(px(14.0))
                                        .bg(theme.accent),
                                )
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_family("Consolas")
                                    .text_color(theme.text_primary)
                                    .child(after),
                            )
                            .when(!has_val, |parent| {
                                parent.when_some(placeholder.clone(), |p, ph| {
                                    p.child(
                                        div()
                                            .text_xs()
                                            .font_family("Consolas")
                                            .text_color(theme.text_muted)
                                            .child(ph),
                                    )
                                })
                            }),
                    )
                    .into_any_element()
            } else {
                let val_str = value.as_deref().unwrap_or("");
                let has_value = !val_str.is_empty();
                let display_text = if has_value {
                    val_str
                } else {
                    placeholder.as_deref().unwrap_or("")
                };

                let initial_val = value.clone().unwrap_or_default();
                let initial_chars = initial_val.chars().count();
                let p_id_focus = p_id.clone();
                let n_id_focus = n_id.clone();

                div()
                    .id(ElementId::from(SharedString::from(format!("ui-textarea-{}-{}", view_id, id))))
                    .flex()
                    .flex_col()
                    .w_full()
                    .h(px(box_h))
                    .p(px(8.0))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_sidebar)
                    .cursor(gpui::CursorStyle::IBeam)
                    .overflow_scroll()
                    .on_mouse_down(MouseButton::Left, cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.set_active_plugin_input(Some(PluginInputFocusState {
                            plugin_id: p_id_focus.clone(),
                            view_id,
                            node_id: n_id_focus.clone(),
                            value: initial_val.clone(),
                            cursor_pos: initial_chars,
                            selection_anchor: None,
                            is_mouse_selecting: false,
                            box_origin_x: Rc::new(Cell::new(0.0)),
                            is_multiline: true,
                        }));
                        app.plugin_input_blink_enable(cx);
                        app.plugin_input_blink_pause_and_show(cx);
                        window.focus(app.plugin_input_focus_handle());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_family("Consolas")
                            .text_color(if has_value {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            })
                            .child(display_text.to_string()),
                    )
                    .into_any_element()
            }
        }
        UiNode::Checkbox { id, label, checked } => {
            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let is_checked = *checked;

            div()
                .id(ElementId::from(SharedString::from(format!("ui-chk-{}-{}", view_id, id))))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .cursor(gpui::CursorStyle::PointingHand)
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.send_plugin_ui_event(
                        &p_id,
                        view_id,
                        n_id.clone(),
                        "change",
                        Some((!is_checked).to_string()),
                        cx,
                    );
                }))
                .child(
                    div()
                        .w(px(14.0))
                        .h(px(14.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(theme.border)
                        .bg(if is_checked { theme.accent } else { theme.bg_card })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(is_checked, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_on_accent)
                                    .child("✓"),
                            )
                        }),
                )
                .when(!label.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(theme.text_primary)
                            .child(label.clone()),
                    )
                })
                .into_any_element()
        }
        UiNode::List { id, items } => {
            let p_id = plugin_id.to_string();
            let view_i = view_id;

            div()
                .id(ElementId::from(SharedString::from(format!("ui-list-{}-{}", view_id, id))))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .w_full()
                .children(items.iter().map(|item| {
                    let item_id = item.id.clone();
                    let pid = p_id.clone();
                    div()
                        .id(ElementId::from(SharedString::from(format!("ui-list-it-{}-{}-{}", view_i, id, item_id))))
                        .flex()
                        .flex_col()
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .hover(|s| s.bg(theme.bg_hover))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.send_plugin_ui_event(
                                &pid,
                                view_i,
                                item_id.clone(),
                                "select",
                                None,
                                cx,
                            );
                        }))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.text_primary)
                                .child(item.title.clone()),
                        )
                        .when_some(item.description.clone(), |this, desc| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(desc),
                            )
                        })
                }))
                .into_any_element()
        }
        UiNode::Toolbar { children } => div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .p(px(4.0))
            .bg(theme.bg_card)
            .rounded(px(4.0))
            .children(children.iter().map(|c| {
                render_ui_node_inner(
                    c,
                    None,
                    plugin_id,
                    view_id,
                    theme,
                    active_input,
                    input_focus_handle,
                    active_select,
                    select_popup,
                    caret_visible,
                    window,
                    cx,
                    scroll_ctx,
                )
            }))
            .into_any_element(),
        UiNode::SettingItem { name, desc, control } => {
            let is_textarea = matches!(control.as_ref(), UiNode::Textarea { .. });
            let has_header = !name.trim().is_empty() || desc.is_some();
            let mut container = div()
                .flex()
                .flex_row()
                .when(is_textarea, |this| this.items_start())
                .when(!is_textarea, |this| this.items_center())
                .w_full()
                .py(px(8.0))
                .border_b_1()
                .border_color(theme.border_subtle);

            if has_header {
                container = container
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(name.clone()),
                            )
                            .when_some(desc.as_ref(), |this, d| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(d.clone()),
                                )
                            }),
                    );
            } else {
                container = container.justify_end();
            }

            container
                .child(
                    div()
                        .flex()
                        .when(is_textarea, |this| this.items_start())
                        .when(!is_textarea, |this| this.items_center())
                        .justify_end()
                        .flex_shrink_0()
                        .when(has_header, |this| match control.as_ref() {
                            UiNode::Input { .. } => this.min_w(px(220.0)).max_w(px(320.0)),
                            UiNode::Textarea { .. } => this.min_w(px(280.0)).max_w(px(460.0)),
                            _ => this.min_w(px(140.0)),
                        })
                        .child(render_ui_node_inner(
                            control,
                            None,
                            plugin_id,
                            view_id,
                            theme,
                            active_input,
                            input_focus_handle,
                            active_select,
                            select_popup,
                            caret_visible,
                            window,
                            cx,
                            scroll_ctx,
                        )),
                )
                .into_any_element()
        }
        UiNode::Toggle { id, value } => {
            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let is_on = *value;

            div()
                .id(ElementId::from(SharedString::from(format!("ui-tog-{}-{}", view_id, id))))
                .w(px(36.0))
                .h(px(20.0))
                .rounded(px(10.0))
                .p(px(2.0))
                .bg(if is_on { theme.accent } else { theme.bg_card })
                .border_1()
                .border_color(if is_on { theme.accent } else { theme.border })
                .cursor(gpui::CursorStyle::PointingHand)
                .hover(|s| s.opacity(0.85))
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.send_plugin_ui_event(
                        &p_id,
                        view_id,
                        n_id.clone(),
                        "change",
                        Some((!is_on).to_string()),
                        cx,
                    );
                }))
                .flex()
                .items_center()
                .when(is_on, |this| this.justify_end())
                .child(
                    div()
                        .w(px(14.0))
                        .h(px(14.0))
                        .rounded(px(7.0))
                        .bg(if is_on { theme.text_on_accent } else { theme.text_muted }),
                )
                .into_any_element()
        }
        UiNode::Slider {
            id,
            min,
            max,
            step: _,
            value,
        } => {
            let val = *value;
            let min_v = *min;
            let max_v = *max;
            let pct = if max_v > min_v {
                ((val - min_v) / (max_v - min_v)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let fill_w = (120.0 * pct as f32).max(2.0);

            div()
                .id(ElementId::from(SharedString::from(format!("ui-slider-{}-{}", view_id, id))))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .relative()
                        .w(px(120.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(theme.border)
                        .child(
                            div()
                                .h_full()
                                .w(px(fill_w))
                                .rounded(px(3.0))
                                .bg(theme.accent),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .w(px(32.0))
                        .child(format!("{:.0}", val)),
                )
                .into_any_element()
        }
        UiNode::Select { id, options, value } => {
            let display_label = options
                .iter()
                .find(|o| o.value == *value)
                .map(|o| o.label.as_str())
                .unwrap_or(value.as_str());

            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let current_val = value.clone();
            let opts = options.clone();
            let is_open = active_select == Some(&(p_id.clone(), view_id, n_id.clone()));

            let p_id_toggle = p_id.clone();
            let n_id_toggle = n_id.clone();

            div()
                .id(ElementId::from(SharedString::from(format!("ui-sel-wrap-{}-{}", view_id, id))))
                .relative()
                .flex_shrink_0()
                .child(
                    div()
                        .id(ElementId::from(SharedString::from(format!("ui-sel-{}-{}", view_id, id))))
                        .cursor_pointer()
                        .child(crate::views::settings_view::render_select_pill_styled(display_label, theme))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            cx.stop_propagation();
                            if app.is_plugin_select_open(&p_id_toggle, view_id, &n_id_toggle) {
                                app.close_plugin_select(cx);
                            } else {
                                app.open_plugin_select(
                                    p_id_toggle.clone(),
                                    view_id,
                                    n_id_toggle.clone(),
                                    opts.clone(),
                                    current_val.clone(),
                                    window,
                                    cx,
                                );
                            }
                        })),
                )
                .when(is_open, |parent| {
                    if let Some(popup) = select_popup {
                        parent.child(
                            gpui::deferred(
                                div()
                                    .id(ElementId::from(SharedString::from(format!("ui-sel-popup-{}-{}", view_id, id))))
                                    .absolute()
                                    .top_full()
                                    .right_0()
                                    .mt_1()
                                    .child(popup.clone()),
                            )
                            .with_priority(999),
                        )
                    } else {
                        parent
                    }
                })
                .into_any_element()
        }
        UiNode::Styled { style, child } => {
            if style.hidden {
                return div().into_any_element();
            }
            // Column, row and button carry the style themselves: their branches end with the same
            // `apply_style`, and the cascade is resolved on a single box. A wrapper here is not merely
            // redundant — it breaks the layout: an auto-height box makes the parent "indefinite",
            // and the `flex_1` child (basis 0) collapses to zero. That is exactly how the file
            // list vanished in Empty Files Cleaner.
            if takes_style_directly(child.as_ref()) {
                return render_ui_node_inner(
                    child,
                    Some(style),
                    plugin_id,
                    view_id,
                    theme,
                    active_input,
                    input_focus_handle,
                    active_select,
                    select_popup,
                    caret_visible,
                    window,
                    cx,
                    scroll_ctx,
                );
            }
            let inner = render_ui_node_inner(
                child,
                None,
                plugin_id,
                view_id,
                theme,
                active_input,
                input_focus_handle,
                active_select,
                select_popup,
                caret_visible,
                window,
                cx,
                scroll_ctx,
            );
            // The wrapper is the box of the plugin element itself: size, padding, background, position.
            // The internal layout (row/column, gap) stays with the node itself. A style without
            // geometry (color, font size, weight) gets an ordinary box: it must pass
            // text properties down and not impose a column.
            let base = if style.affects_layout() {
                div().flex().flex_col().flex_shrink_0()
            } else {
                div()
            };
            apply_style(base, style)
                .child(inner)
                .into_any_element()
        }
        UiNode::Separator => div()
            .w_full()
            .h(px(1.0))
            .bg(theme.border_subtle)
            .my(px(4.0))
            .into_any_element(),
    }
}
