use std::collections::HashSet;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled,
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
    Checkbox {
        #[serde(default)]
        id: String,
        label: String,
        #[serde(default)]
        checked: bool,
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
            | UiNode::Checkbox { id, .. }
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
            _ => &[],
        }
    }

    /// Recursively counts the total number of nodes in this subtree.
    pub fn count_nodes(&self) -> usize {
        1 + self.children().iter().map(|c| c.count_nodes()).sum::<usize>()
    }
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
                .children(children.iter().map(|child| render_ui_node(child, plugin_id, view_id, theme, cx)))
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
                .children(children.iter().map(|child| render_ui_node(child, plugin_id, view_id, theme, cx)))
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
            let bg_color = match variant {
                ButtonVariant::Default => theme.bg_card,
                ButtonVariant::Primary => theme.accent,
                ButtonVariant::Danger => gpui::rgb(0xed8796), // Red error token
            };
            let text_color = match variant {
                ButtonVariant::Default => theme.text_primary,
                ButtonVariant::Primary => theme.text_on_accent,
                ButtonVariant::Danger => gpui::rgb(0x181825),
            };

            let p_id = plugin_id.to_string();
            let n_id = id.clone();
            let is_disabled = *disabled;

            div()
                .id(ElementId::from(SharedString::from(format!("ui-btn-{}-{}", view_id, id))))
                .flex()
                .items_center()
                .justify_center()
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(4.0))
                .bg(bg_color)
                .text_color(text_color)
                .text_sm()
                .cursor(if is_disabled {
                    gpui::CursorStyle::Arrow
                } else {
                    gpui::CursorStyle::PointingHand
                })
                .hover(|s| if !is_disabled { s.bg(theme.bg_hover) } else { s })
                .when(!is_disabled, |this| {
                    this.on_mouse_down(gpui::MouseButton::Left, cx.listener(move |app, _, _, _| {
                        let _ = app.state.plugin_host.send_event(
                            &p_id,
                            crate::plugins::api::HostEvent::UiEvent {
                                view_id,
                                node_id: n_id.clone(),
                                event: "click".to_string(),
                                value: None,
                            },
                        );
                    }))
                })
                .child(label.clone())
                .into_any_element()
        }
        UiNode::Input {
            id,
            placeholder,
            value,
        } => {
            let display_text = value
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(placeholder.as_deref())
                .unwrap_or("");

            div()
                .id(ElementId::from(SharedString::from(format!("ui-input-{}-{}", view_id, id))))
                .flex()
                .w_full()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme.bg_editor)
                .text_sm()
                .text_color(if value.is_some() {
                    theme.text_primary
                } else {
                    theme.text_muted
                })
                .child(display_text.to_string())
                .into_any_element()
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
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |app, _, _, _| {
                    let _ = app.state.plugin_host.send_event(
                        &p_id,
                        crate::plugins::api::HostEvent::UiEvent {
                            view_id,
                            node_id: n_id.clone(),
                            event: "change".to_string(),
                            value: Some((!is_checked).to_string()),
                        },
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
                        .flex()
                        .flex_col()
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .hover(|s| s.bg(theme.bg_hover))
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |app, _, _, _| {
                            let _ = app.state.plugin_host.send_event(
                                &pid,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id: view_i,
                                    node_id: item_id.clone(),
                                    event: "select".to_string(),
                                    value: None,
                                },
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
            .children(children.iter().map(|c| render_ui_node(c, plugin_id, view_id, theme, cx)))
            .into_any_element(),
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
}
