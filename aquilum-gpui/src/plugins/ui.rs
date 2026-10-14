use std::collections::HashSet;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, Context, ElementId, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use serde::{Deserialize, Serialize};

use crate::theme::Theme;
use crate::views::app_view::AppView;

/// Spacing gap size between layout items.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    None,
    Small,
    Medium,
    Large,
}

impl Gap {
    pub fn to_px(&self) -> f32 {
        match self {
            Gap::None => 0.0,
            Gap::Small => 4.0,
            Gap::Medium => 8.0,
            Gap::Large => 16.0,
        }
    }
}

impl Default for Gap {
    fn default() -> Self {
        Gap::Medium
    }
}

/// Text styling options for UiNode::Text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Normal,
    Muted,
    Heading,
    Code,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle::Normal
    }
}

/// Button visual style variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Default,
    Primary,
    Danger,
}

impl Default for ButtonVariant {
    fn default() -> Self {
        ButtonVariant::Default
    }
}

/// Single item within a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Option entry for UiNode::Select.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// Declarative UI component tree node transmitted from plugin workers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiNode {
    Column {
        #[serde(default)]
        gap: Gap,
        #[serde(default)]
        children: Vec<UiNode>,
    },
    Row {
        #[serde(default)]
        gap: Gap,
        #[serde(default)]
        children: Vec<UiNode>,
    },
    Text {
        value: String,
        #[serde(default)]
        style: TextStyle,
    },
    Markdown {
        value: String,
    },
    Button {
        #[serde(default)]
        id: String,
        label: String,
        #[serde(default)]
        variant: ButtonVariant,
        #[serde(default)]
        disabled: bool,
    },
    Input {
        #[serde(default)]
        id: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        value: Option<String>,
    },
    Textarea {
        #[serde(default)]
        id: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        value: Option<String>,
        #[serde(default)]
        rows: Option<usize>,
    },
    Checkbox {
        #[serde(default)]
        id: String,
        label: String,
        #[serde(default)]
        checked: bool,
    },
    /// Obsidian-style toggle switch
    Toggle {
        #[serde(default)]
        id: String,
        #[serde(default)]
        value: bool,
    },
    /// Numeric slider control
    Slider {
        #[serde(default)]
        id: String,
        #[serde(default)]
        min: f64,
        #[serde(default)]
        max: f64,
        #[serde(default)]
        step: f64,
        #[serde(default)]
        value: f64,
    },
    /// Dropdown selector
    Select {
        #[serde(default)]
        id: String,
        #[serde(default)]
        options: Vec<SelectOption>,
        #[serde(default)]
        value: String,
    },
    /// Obsidian-style setting row: name and description on left, control on right
    SettingItem {
        name: String,
        #[serde(default)]
        desc: Option<String>,
        control: Box<UiNode>,
    },
    List {
        #[serde(default)]
        id: String,
        #[serde(default)]
        items: Vec<ListItem>,
    },
    Toolbar {
        #[serde(default)]
        children: Vec<UiNode>,
    },
    Separator,
}

impl UiNode {
    /// Returns the optional identifier of this component node.
    pub fn id(&self) -> Option<&str> {
        match self {
            UiNode::Button { id, .. }
            | UiNode::Input { id, .. }
            | UiNode::Textarea { id, .. }
            | UiNode::Checkbox { id, .. }
            | UiNode::Toggle { id, .. }
            | UiNode::Slider { id, .. }
            | UiNode::Select { id, .. }
            | UiNode::List { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Returns direct child nodes.
    pub fn children(&self) -> &[UiNode] {
        match self {
            UiNode::Column { children, .. }
            | UiNode::Row { children, .. }
            | UiNode::Toolbar { children, .. } => children,
            UiNode::SettingItem { control, .. } => std::slice::from_ref(control),
            _ => &[],
        }
    }

    /// Recursively counts the total number of nodes in this subtree.
    pub fn count_nodes(&self) -> usize {
        1 + self.children().iter().map(|c| c.count_nodes()).sum::<usize>()
    }

    /// Recursively updates the value of an `Input` or `Textarea` node with id matching `target_id`.
    pub fn update_input_value(&mut self, target_id: &str, new_val: &str) -> bool {
        match self {
            UiNode::Input { id, value, .. } | UiNode::Textarea { id, value, .. } if id == target_id => {
                *value = Some(new_val.to_string());
                true
            }
            UiNode::Column { children, .. }
            | UiNode::Row { children, .. }
            | UiNode::Toolbar { children, .. } => {
                for child in children {
                    if child.update_input_value(target_id, new_val) {
                        return true;
                    }
                }
                false
            }
            UiNode::SettingItem { control, .. } => {
                control.update_input_value(target_id, new_val)
            }
            _ => false,
        }
    }

    /// Recursively retrieves the current value of an `Input` or `Textarea` node with id matching `target_id`.
    pub fn get_input_value(&self, target_id: &str) -> Option<String> {
        match self {
            UiNode::Input { id, value, .. } | UiNode::Textarea { id, value, .. } if id == target_id => value.clone(),
            UiNode::Column { children, .. }
            | UiNode::Row { children, .. }
            | UiNode::Toolbar { children, .. } => {
                for child in children {
                    if let Some(v) = child.get_input_value(target_id) {
                        return Some(v);
                    }
                }
                None
            }
            UiNode::SettingItem { control, .. } => control.get_input_value(target_id),
            _ => None,
        }
    }
}

/// Active interactive text input focus state for plugin UI components.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginInputFocusState {
    pub plugin_id: String,
    pub view_id: u64,
    pub node_id: String,
    pub value: String,
    pub cursor_pos: usize,
    pub is_multiline: bool,
}

/// Validates the tree to prevent excessive depth, memory exhaustion, or duplicated IDs.
pub fn validate_tree(root: &UiNode) -> Result<(), String> {
    const MAX_DEPTH: usize = 32;
    const MAX_NODES: usize = 2000;
    const MAX_STR_LEN: usize = 64 * 1024;

    let mut total_nodes = 0;
    let mut ids = HashSet::new();

    fn walk(
        node: &UiNode,
        depth: usize,
        total_nodes: &mut usize,
        ids: &mut HashSet<String>,
    ) -> Result<(), String> {
        *total_nodes += 1;
        if depth > MAX_DEPTH {
            return Err("Tree hierarchy depth exceeds limit (32)".into());
        }
        if *total_nodes > MAX_NODES {
            return Err("Tree total node count exceeds limit (2000)".into());
        }

        if let Some(id) = node.id() {
            if !ids.insert(id.to_string()) {
                return Err(format!("Duplicate UI node id detected: '{}'", id));
            }
        }

        match node {
            UiNode::Text { value, .. } | UiNode::Markdown { value } => {
                if value.len() > MAX_STR_LEN {
                    return Err("Text content length exceeds limit (64KB)".into());
                }
            }
            UiNode::Button { label, .. } | UiNode::Checkbox { label, .. } => {
                if label.len() > MAX_STR_LEN {
                    return Err("Label length exceeds limit (64KB)".into());
                }
            }
            UiNode::Input { placeholder, value, .. } => {
                if let Some(p) = placeholder {
                    if p.len() > MAX_STR_LEN {
                        return Err("Placeholder length exceeds limit (64KB)".into());
                    }
                }
                if let Some(v) = value {
                    if v.len() > MAX_STR_LEN {
                        return Err("Input value length exceeds limit (64KB)".into());
                    }
                }
            }
            UiNode::SettingItem { name, desc, .. } => {
                if name.len() > MAX_STR_LEN {
                    return Err("Setting name length exceeds limit (64KB)".into());
                }
                if let Some(d) = desc {
                    if d.len() > MAX_STR_LEN {
                        return Err("Setting description length exceeds limit (64KB)".into());
                    }
                }
            }
            _ => {}
        }

        for child in node.children() {
            walk(child, depth + 1, total_nodes, ids)?;
        }

        Ok(())
    }

    walk(root, 0, &mut total_nodes, &mut ids)
}

/// Renders a declarative UI node into GPUI elements using application theme tokens.
pub fn render_ui_node(
    node: &UiNode,
    plugin_id: &str,
    view_id: u64,
    theme: &Theme,
    active_input: Option<&PluginInputFocusState>,
    input_focus_handle: &FocusHandle,
    active_select: Option<&(String, u64, String)>,
    cx: &Context<AppView>,
) -> AnyElement {
    match node {
        UiNode::Column { gap, children } => {
            let gap_px = gap.to_px();
            div()
                .flex()
                .flex_col()
                .gap(px(gap_px))
                .w_full()
                .children(children.iter().map(|child| {
                    render_ui_node(
                        child,
                        plugin_id,
                        view_id,
                        theme,
                        active_input,
                        input_focus_handle,
                        active_select,
                        cx,
                    )
                }))
                .into_any_element()
        }
        UiNode::Row { gap, children } => {
            let gap_px = gap.to_px();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(gap_px))
                .w_full()
                .children(children.iter().map(|child| {
                    render_ui_node(
                        child,
                        plugin_id,
                        view_id,
                        theme,
                        active_input,
                        input_focus_handle,
                        active_select,
                        cx,
                    )
                }))
                .into_any_element()
        }
        UiNode::Text { value, style } => match style {
            TextStyle::Normal => div()
                .text_sm()
                .text_color(theme.text_primary)
                .child(value.clone())
                .into_any_element(),
            TextStyle::Muted => div()
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
                .text_xs()
                .bg(theme.code_bg)
                .text_color(theme.success)
                .px(px(4.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .child(value.clone())
                .into_any_element(),
        },
        UiNode::Markdown { value } => div()
            .text_sm()
            .text_color(theme.text_primary)
            .child(value.clone())
            .into_any_element(),
        UiNode::Button {
            id,
            label,
            variant,
            disabled,
        } => {
            let is_disabled = *disabled;
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
            };

            let p_id = plugin_id.to_string();
            let n_id = id.clone();

            div()
                .id(ElementId::from(SharedString::from(format!("ui-btn-{}-{}", view_id, id))))
                .flex()
                .items_center()
                .justify_center()
                .px(px(12.0))
                .py(px(5.0))
                .rounded(px(5.0))
                .bg(bg_color)
                .border_1()
                .border_color(border_color)
                .shadow_xs()
                .text_color(text_color)
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .cursor(if is_disabled {
                    gpui::CursorStyle::Arrow
                } else {
                    gpui::CursorStyle::PointingHand
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .when(!is_disabled, |this| {
                    match variant {
                        ButtonVariant::Default => this
                            .hover(|s| s.bg(theme.bg_hover).border_color(theme.accent))
                            .active(|s| s.bg(theme.bg_hover)),
                        ButtonVariant::Primary | ButtonVariant::Danger => this
                            .hover(|s| s.opacity(0.88))
                            .active(|s| s.opacity(0.75)),
                    }
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        let _ = app.state.plugin_host.send_event(
                            &p_id,
                            crate::plugins::api::HostEvent::UiEvent {
                                view_id,
                                node_id: n_id.clone(),
                                event: "click".to_string(),
                                value: None,
                            },
                        );
                        cx.notify();
                    }))
                })
                .when(is_disabled, |this| this.opacity(0.5))
                .child(label.clone())
                .into_any_element()
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

            if is_active {
                let current_state = active_input.unwrap();
                let val = &current_state.value;
                let total_chars = val.chars().count();
                let pos = current_state.cursor_pos.min(total_chars);
                let before: String = val.chars().take(pos).collect();
                let after: String = val.chars().skip(pos).collect();
                let has_val = !val.is_empty();

                div()
                    .id(ElementId::from(SharedString::from(format!("ui-input-{}-{}", view_id, id))))
                    .track_focus(input_focus_handle)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                        cx.stop_propagation();
                        app.handle_plugin_input_key(event, cx);
                    }))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .min_w(px(160.0))
                    .max_w(px(320.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.accent)
                    .bg(theme.bg_editor)
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.text_primary)
                                    .child(before),
                            )
                            .child(
                                div()
                                    .w(px(1.5))
                                    .h(px(14.0))
                                    .bg(theme.accent),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.text_primary)
                                    .child(after),
                            )
                            .when(!has_val, |parent| {
                                parent.when_some(placeholder.clone(), |p, ph| {
                                    p.child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.text_muted)
                                            .child(ph),
                                    )
                                })
                            }),
                    )
                    .when(has_val, |parent| {
                        let p_id_clear = p_id.clone();
                        let n_id_clear = n_id.clone();
                        parent.child(
                            div()
                                .id(ElementId::from(SharedString::from(format!("ui-input-clear-{}-{}", view_id, id))))
                                .cursor(gpui::CursorStyle::PointingHand)
                                .px(px(4.0))
                                .py(px(1.0))
                                .rounded(px(3.0))
                                .hover(|h| h.bg(theme.bg_hover))
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child("✕")
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(ref mut st) = app.active_plugin_input {
                                        if st.plugin_id == p_id_clear && st.view_id == view_id && st.node_id == n_id_clear {
                                            st.value.clear();
                                            st.cursor_pos = 0;
                                        }
                                    }
                                    if let Some(tree) = app.plugin_views.get_mut(&(p_id_clear.clone(), view_id)) {
                                        tree.update_input_value(&n_id_clear, "");
                                    }
                                    let _ = app.state.plugin_host.send_event(
                                        &p_id_clear,
                                        crate::plugins::api::HostEvent::UiEvent {
                                            view_id,
                                            node_id: n_id_clear.clone(),
                                            event: "change".to_string(),
                                            value: Some("".to_string()),
                                        },
                                    );
                                    cx.notify();
                                })),
                        )
                    })
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
                    .id(ElementId::from(SharedString::from(format!("ui-input-{}-{}", view_id, id))))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .min_w(px(160.0))
                    .max_w(px(320.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_editor)
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.active_plugin_input = Some(PluginInputFocusState {
                            plugin_id: p_id_focus.clone(),
                            view_id,
                            node_id: n_id_focus.clone(),
                            value: initial_val.clone(),
                            cursor_pos: initial_chars,
                            is_multiline: false,
                        });
                        window.focus(&app.plugin_input_focus_handle);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_sm()
                            .text_color(if has_value {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            })
                            .child(display_text.to_string()),
                    )
                    .when(has_value, |parent| {
                        let p_id_clear = p_id.clone();
                        let n_id_clear = n_id.clone();
                        parent.child(
                            div()
                                .id(ElementId::from(SharedString::from(format!("ui-input-clear-{}-{}", view_id, id))))
                                .cursor(gpui::CursorStyle::PointingHand)
                                .px(px(4.0))
                                .py(px(1.0))
                                .rounded(px(3.0))
                                .hover(|h| h.bg(theme.bg_hover))
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child("✕")
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(tree) = app.plugin_views.get_mut(&(p_id_clear.clone(), view_id)) {
                                        tree.update_input_value(&n_id_clear, "");
                                    }
                                    let _ = app.state.plugin_host.send_event(
                                        &p_id_clear,
                                        crate::plugins::api::HostEvent::UiEvent {
                                            view_id,
                                            node_id: n_id_clear.clone(),
                                            event: "change".to_string(),
                                            value: Some("".to_string()),
                                        },
                                    );
                                    cx.notify();
                                })),
                        )
                    })
                    .into_any_element()
            }
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
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                        cx.stop_propagation();
                        app.handle_plugin_input_key(event, cx);
                    }))
                    .flex()
                    .flex_col()
                    .w_full()
                    .h(px(box_h))
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.accent)
                    .bg(theme.bg_editor)
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
                            .child(
                                div()
                                    .w(px(2.0))
                                    .h(px(14.0))
                                    .bg(theme.accent),
                            )
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
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_editor)
                    .cursor(gpui::CursorStyle::IBeam)
                    .overflow_scroll()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.active_plugin_input = Some(PluginInputFocusState {
                            plugin_id: p_id_focus.clone(),
                            view_id,
                            node_id: n_id_focus.clone(),
                            value: initial_val.clone(),
                            cursor_pos: initial_chars,
                            is_multiline: true,
                        });
                        window.focus(&app.plugin_input_focus_handle);
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
                    let _ = app.state.plugin_host.send_event(
                        &p_id,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id,
                            node_id: n_id.clone(),
                            event: "change".to_string(),
                            value: Some((!is_checked).to_string()),
                        },
                    );
                    cx.notify();
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
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.text_primary)
                        .child(label.clone()),
                )
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
                            let _ = app.state.plugin_host.send_event(
                                &pid,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id: view_i,
                                    node_id: item_id.clone(),
                                    event: "select".to_string(),
                                    value: None,
                                },
                            );
                            cx.notify();
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
                render_ui_node(
                    c,
                    plugin_id,
                    view_id,
                    theme,
                    active_input,
                    input_focus_handle,
                    active_select,
                    cx,
                )
            }))
            .into_any_element(),
        UiNode::SettingItem { name, desc, control } => {
            let has_header = !name.trim().is_empty() || desc.is_some();
            let mut container = div()
                .flex()
                .flex_row()
                .items_center()
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
                        .items_center()
                        .justify_end()
                        .flex_shrink_0()
                        .when(has_header, |this| this.min_w(px(140.0)))
                        .child(render_ui_node(
                            control,
                            plugin_id,
                            view_id,
                            theme,
                            active_input,
                            input_focus_handle,
                            active_select,
                            cx,
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
                    let _ = app.state.plugin_host.send_event(
                        &p_id,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id,
                            node_id: n_id.clone(),
                            event: "change".to_string(),
                            value: Some((!is_on).to_string()),
                        },
                    );
                    cx.notify();
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
                .id(ElementId::from(SharedString::from(format!("ui-sel-{}-{}", view_id, id))))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(if is_open { theme.accent } else { theme.border })
                .bg(theme.bg_card)
                .cursor(gpui::CursorStyle::PointingHand)
                .hover(|s| s.bg(theme.bg_hover))
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    let sel_key = (p_id_toggle.clone(), view_id, n_id_toggle.clone());
                    if app.active_plugin_select.as_ref() == Some(&sel_key) {
                        app.active_plugin_select = None;
                    } else {
                        app.active_plugin_select = Some(sel_key);
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_primary)
                        .child(display_label.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child(if is_open { "▲" } else { "▼" }),
                )
                .when(is_open, |parent| {
                    parent.child(
                        gpui::deferred(
                            div()
                                .id(ElementId::from(SharedString::from(format!("ui-sel-popup-{}-{}", view_id, id))))
                                .absolute()
                                .top(px(32.0))
                                .right_0()
                                .min_w(px(180.0))
                                .max_h(px(240.0))
                                .overflow_scroll()
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border)
                                .rounded(px(6.0))
                                .shadow_lg()
                                .p(px(4.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .cursor(gpui::CursorStyle::Arrow)
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .children(opts.iter().map(|opt| {
                                    let opt_val = opt.value.clone();
                                    let is_selected = opt_val == current_val;
                                    let p_id_item = p_id.clone();
                                    let n_id_item = n_id.clone();
                                    div()
                                        .id(ElementId::from(SharedString::from(format!("ui-sel-opt-{}-{}-{}", view_id, id, opt.value))))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .px(px(8.0))
                                        .py(px(5.0))
                                        .rounded(px(4.0))
                                        .when(is_selected, |s| s.bg(theme.bg_hover))
                                        .hover(|s| s.bg(theme.bg_hover))
                                        .cursor(gpui::CursorStyle::PointingHand)
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            cx.stop_propagation();
                                            app.active_plugin_select = None;
                                            let _ = app.state.plugin_host.send_event(
                                                &p_id_item,
                                                crate::plugins::api::HostEvent::UiEvent {
                                                    view_id,
                                                    node_id: n_id_item.clone(),
                                                    event: "change".to_string(),
                                                    value: Some(opt_val.clone()),
                                                },
                                            );
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(if is_selected { theme.accent } else { theme.text_primary })
                                                .font_weight(if is_selected { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                                                .child(opt.label.clone()),
                                        )
                                        .when(is_selected, |opt_row| {
                                            opt_row.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.accent)
                                                    .child("✓"),
                                            )
                                        })
                                }))
                        ).with_priority(999)
                    )
                })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_node_serialization_and_count() {
        let tree = UiNode::Column {
            gap: Gap::Medium,
            children: vec![
                UiNode::Text {
                    value: "Header".into(),
                    style: TextStyle::Heading,
                },
                UiNode::Button {
                    id: "submit-btn".into(),
                    label: "Submit".into(),
                    variant: ButtonVariant::Primary,
                    disabled: false,
                },
                UiNode::Separator,
            ],
        };

        assert_eq!(tree.count_nodes(), 4);

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"column\""));
        assert!(json.contains("\"type\":\"button\""));
        assert!(json.contains("\"variant\":\"primary\""));

        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }

    #[test]
    fn test_validate_tree_duplicate_ids() {
        let tree = UiNode::Column {
            gap: Gap::None,
            children: vec![
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "One".into(),
                    variant: ButtonVariant::Default,
                    disabled: false,
                },
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "Two".into(),
                    variant: ButtonVariant::Default,
                    disabled: false,
                },
            ],
        };

        let result = validate_tree(&tree);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate UI node id"));
    }

    #[test]
    fn test_validate_tree_ok() {
        let tree = UiNode::Column {
            gap: Gap::Small,
            children: vec![
                UiNode::Text {
                    value: "Description".into(),
                    style: TextStyle::Normal,
                },
                UiNode::Checkbox {
                    id: "agree".into(),
                    label: "I agree".into(),
                    checked: true,
                },
            ],
        };

        assert!(validate_tree(&tree).is_ok());
    }

    #[test]
    fn test_setting_item_and_controls_roundtrip() {
        let tree = UiNode::Column {
            gap: Gap::Medium,
            children: vec![
                UiNode::SettingItem {
                    name: "Enable Live Preview".into(),
                    desc: Some("Toggle real-time Markdown rendering".into()),
                    control: Box::new(UiNode::Toggle {
                        id: "toggle-live-preview".into(),
                        value: true,
                    }),
                },
                UiNode::SettingItem {
                    name: "Editor Font Size".into(),
                    desc: Some("Adjust base font size in pixels".into()),
                    control: Box::new(UiNode::Slider {
                        id: "slider-font-size".into(),
                        min: 10.0,
                        max: 32.0,
                        step: 1.0,
                        value: 16.0,
                    }),
                },
                UiNode::SettingItem {
                    name: "Default View".into(),
                    desc: None,
                    control: Box::new(UiNode::Select {
                        id: "select-default-view".into(),
                        options: vec![
                            SelectOption { value: "source".into(), label: "Source Mode".into() },
                            SelectOption { value: "preview".into(), label: "Live Preview".into() },
                        ],
                        value: "preview".into(),
                    }),
                },
            ],
        };

        assert!(validate_tree(&tree).is_ok());
        // 1 Column + 3 SettingItems + 3 controls (Toggle, Slider, Select) = 7 nodes
        assert_eq!(tree.count_nodes(), 7);

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"setting_item\""));
        assert!(json.contains("\"type\":\"toggle\""));
        assert!(json.contains("\"type\":\"slider\""));
        assert!(json.contains("\"type\":\"select\""));

        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }

    #[test]
    fn test_ui_node_input_value_update() {
        let mut tree = UiNode::Column {
            gap: Gap::Medium,
            children: vec![
                UiNode::SettingItem {
                    name: "Custom Format".into(),
                    desc: Some("Format template".into()),
                    control: Box::new(UiNode::Input {
                        id: "input-format".into(),
                        placeholder: Some("{words}".into()),
                        value: Some("initial".into()),
                    }),
                },
            ],
        };

        assert_eq!(tree.get_input_value("input-format"), Some("initial".to_string()));
        assert!(tree.update_input_value("input-format", "{words} руб."));
        assert_eq!(tree.get_input_value("input-format"), Some("{words} руб.".to_string()));
        assert!(!tree.update_input_value("non-existent", "test"));
    }

    #[test]
    fn test_ui_node_textarea() {
        let mut tree = UiNode::Column {
            gap: Gap::Medium,
            children: vec![
                UiNode::Textarea {
                    id: "words-json".into(),
                    placeholder: Some("Enter JSON".into()),
                    value: Some("{\"RUB\": []}".into()),
                    rows: Some(10),
                },
            ],
        };

        assert_eq!(tree.count_nodes(), 2);
        assert_eq!(tree.get_input_value("words-json"), Some("{\"RUB\": []}".to_string()));
        assert!(tree.update_input_value("words-json", "{\"RUB\": [\"руб\"]}"));
        assert_eq!(tree.get_input_value("words-json"), Some("{\"RUB\": [\"руб\"]}".to_string()));

        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"type\":\"textarea\""));
        assert!(json.contains("\"rows\":10"));
        let deserialized: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, tree);
    }
}

