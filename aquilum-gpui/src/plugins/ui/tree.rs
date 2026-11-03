use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use serde::{Deserialize, Serialize};
use super::style::{control_or_empty, empty_text_control, ButtonVariant, Gap, ListItem, NodeStyle, SelectOption, TextStyle};

/// Declarative UI component tree node transmitted from plugin workers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiNode {
    Column {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        class: Option<String>,
        #[serde(default)]
        gap: Gap,
        #[serde(default)]
        children: Vec<UiNode>,
    },
    Row {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        class: Option<String>,
        #[serde(default)]
        gap: Gap,
        #[serde(default)]
        children: Vec<UiNode>,
    },
    Text {
        value: String,
        #[serde(default)]
        class: Option<String>,
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
        class: Option<String>,
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
        /// Missing or explicit `null` in the wire format degrades to an empty text node:
        /// a live plugin shipped `control: null` (icon-only control div), and one bad row
        /// must not invalidate the whole view.
        #[serde(default = "empty_text_control", deserialize_with = "control_or_empty")]
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
    /// Элемент плагина с явными inline-стилями: `style` — бокс элемента, `child` — его содержимое.
    /// Обёртка появляется только тогда, когда стиль непустой: `display: none` и размеры должны
    /// принадлежать самому элементу, а не его внутренней раскладке.
    Styled {
        style: NodeStyle,
        child: Box<UiNode>,
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
            // Обёртка стилей — декорация, а не узел: собственной идентичности у неё нет.
            // Ребёнок попадает в `children()` и там же проходит проверку на дубликат;
            // делегировать id сюда значило бы показать валидатору один и тот же узел дважды.
            UiNode::Styled { .. } => None,
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
            UiNode::Styled { child, .. } => std::slice::from_ref(child),
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
            UiNode::Styled { child, .. } => child.update_input_value(target_id, new_val),
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
            UiNode::Styled { child, .. } => child.get_input_value(target_id),
            _ => None,
        }
    }
}

/// Active interactive text input focus state for plugin UI components.
#[derive(Clone, Debug)]
pub struct PluginInputFocusState {
    pub plugin_id: String,
    pub view_id: u64,
    pub node_id: String,
    pub value: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub box_origin_x: Rc<Cell<f32>>,
    pub is_multiline: bool,
}

impl PluginInputFocusState {
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor_pos {
            None
        } else {
            Some((anchor.min(self.cursor_pos), anchor.max(self.cursor_pos)))
        }
    }
}

impl PartialEq for PluginInputFocusState {
    fn eq(&self, other: &Self) -> bool {
        self.plugin_id == other.plugin_id
            && self.view_id == other.view_id
            && self.node_id == other.node_id
            && self.value == other.value
            && self.cursor_pos == other.cursor_pos
            && self.selection_anchor == other.selection_anchor
            && self.is_mouse_selecting == other.is_mouse_selecting
            && self.is_multiline == other.is_multiline
    }
}

/// Maps a mouse click x (window coords) to a character index in the input text.
pub fn plugin_input_click_pos(text: &str, click_x: f32, box_origin_x: f32, window: &gpui::Window) -> usize {
    if text.is_empty() {
        return 0;
    }
    let rel_x = (click_x - box_origin_x).max(0.0);
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::font("Segoe UI"),
        color: Default::default(),
        underline: Default::default(),
        strikethrough: None,
        background_color: None,
    };
    let shaped = window.text_system().shape_line(
        gpui::SharedString::from(text.to_string()),
        gpui::px(14.0),
        &[run],
        None,
    );
    let byte_idx = shaped.closest_index_for_x(gpui::px(rel_x));
    text[..byte_idx.min(text.len())].chars().count()
}

/// Validates the tree to prevent excessive depth, memory exhaustion, or duplicated IDs.
pub fn validate_tree(root: &UiNode) -> Result<(), String> {
    const MAX_DEPTH: usize = 32;
    const MAX_NODES: usize = 50_000;
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
            return Err(format!("Tree total node count exceeds limit ({MAX_NODES})"));
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

/// Сводка трансляции одного дерева в GPUI: что реально отрисуется, что разберётся,
/// что будет отброшено. Наполняется golden-тестами на живых деревьях плагинов —
/// численно фиксирует покрытие на каждый коммит.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TreeStats {
    pub nodes: usize,
    /// Узлы, для которых рендер что-то рисует (всё, кроме `hidden`).
    pub rendered: usize,
    /// Узлы с inline-стилем, разобранным в `NodeStyle`.
    pub styled: usize,
    /// Стили, меняющие геометрию (требуют бокса-обёртки).
    pub layout_styles: usize,
    /// Текст/лейблы/описания суммарно, в байтах — прокси стоимости рендера текста.
    pub text_bytes: usize,
    pub by_kind: BTreeMap<&'static str, usize>,
    /// Разобранные значения по ключам стиля (`width` → сколько узлов с шириной).
    pub style_props: BTreeMap<&'static str, usize>,
}

impl TreeStats {
    pub fn add(&mut self, other: &TreeStats) {
        self.nodes += other.nodes;
        self.rendered += other.rendered;
        self.styled += other.styled;
        self.layout_styles += other.layout_styles;
        self.text_bytes += other.text_bytes;
        for (k, v) in &other.by_kind {
            *self.by_kind.entry(k).or_default() += v;
        }
        for (k, v) in &other.style_props {
            *self.style_props.entry(k).or_default() += v;
        }
    }
}

/// Чистый обход дерева: те же правила, что применяет рендер, но без GPUI-контекста.
/// Нужен golden-тестам, чтобы мерить реальные деревья плагинов и ловить дрейф
/// покрытия при правках рендера или бутстрапа.
pub fn tree_translation_stats(node: &UiNode) -> TreeStats {
    let mut acc = TreeStats::default();
    fn walk(node: &UiNode, acc: &mut TreeStats) {
        acc.nodes += 1;
        let (kind, style): (&'static str, Option<NodeStyle>) = match node {
            UiNode::Column { .. } => ("column", None),
            UiNode::Row { .. } => ("row", None),
            UiNode::Text { value, .. } => {
                acc.text_bytes += value.len();
                ("text", None)
            }
            UiNode::Markdown { value } => {
                acc.text_bytes += value.len();
                ("markdown", None)
            }
            UiNode::Button { label, .. } => {
                acc.text_bytes += label.len();
                ("button", None)
            }
            UiNode::Input { .. } => ("input", None),
            UiNode::Textarea { .. } => ("textarea", None),
            UiNode::Checkbox { label, .. } => {
                acc.text_bytes += label.len();
                ("checkbox", None)
            }
            UiNode::Toggle { .. } => ("toggle", None),
            UiNode::Slider { .. } => ("slider", None),
            UiNode::Select { .. } => ("select", None),
            UiNode::SettingItem { name, desc, .. } => {
                acc.text_bytes += name.len() + desc.as_deref().map_or(0, str::len);
                ("setting_item", None)
            }
            UiNode::List { items, .. } => {
                for it in items {
                    acc.text_bytes += it.title.len();
                }
                ("list", None)
            }
            UiNode::Toolbar { .. } => ("toolbar", None),
            UiNode::Styled { style, .. } => ("styled", Some(style.clone())),
            UiNode::Separator => ("separator", None),
        };
        *acc.by_kind.entry(kind).or_default() += 1;

        // display:none узлы остаются в дереве, но рендер их пропускает.
        if style.as_ref().map_or(true, |s| !s.hidden) {
            acc.rendered += 1;
        }
        if let Some(style) = style {
            if !style.is_empty() {
                acc.styled += 1;
            }
            if style.affects_layout() {
                acc.layout_styles += 1;
            }
            let bump = |acc: &mut TreeStats, key: &'static str| {
                *acc.style_props.entry(key).or_default() += 1;
            };
            if style.width.is_some() { bump(acc, "width"); }
            if style.height.is_some() { bump(acc, "height"); }
            if style.min_width.is_some() { bump(acc, "minWidth"); }
            if style.max_width.is_some() { bump(acc, "maxWidth"); }
            if style.min_height.is_some() { bump(acc, "minHeight"); }
            if style.max_height.is_some() { bump(acc, "maxHeight"); }
            if style.padding.is_some() { bump(acc, "padding"); }
            if style.margin.is_some() { bump(acc, "margin"); }
            if style.background.is_some() { bump(acc, "background"); }
            if style.color.is_some() { bump(acc, "color"); }
            if style.opacity.is_some() { bump(acc, "opacity"); }
            if style.border_radius.is_some() { bump(acc, "borderRadius"); }
            if style.font_size.is_some() { bump(acc, "fontSize"); }
            if style.font_weight.is_some() { bump(acc, "fontWeight"); }
            if style.text_align.is_some() { bump(acc, "textAlign"); }
            if style.position.is_some() { bump(acc, "position"); }
            for (key, v) in [
                ("top", style.inset.top), ("right", style.inset.right),
                ("bottom", style.inset.bottom), ("left", style.inset.left),
            ] {
                if v.is_some() { bump(acc, key); }
            }
            if style.overflow_hidden { bump(acc, "overflow"); }
        }

        for child in node.children() {
            walk(child, acc);
        }
    }
    walk(node, &mut acc);
    acc
}
