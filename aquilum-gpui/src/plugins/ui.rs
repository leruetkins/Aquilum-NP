use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, point, px, relative, AnyElement, Context, Div, ElementId, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TextAlign,
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
    #[serde(other)]
    Secondary,
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

// ──────────────────────────────────────────────────────── inline-стили плагинов
//
// Плагин пишет `el.style.width = "300px"` — CSS, которого хост не исполняет. Здесь объявлено ровно
// то подмножество, которое хост умеет применить к нативному узлу; остальное отбрасывается ещё в JS
// (`__style_to_uinode`). Поэтому «поле заполнено» = «будет применено», а не «может быть, повезёт».

/// Длина: пиксели или проценты от родителя.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Len {
    Px(f32),
    Pct(f32),
}

impl Len {
    fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return None;
        }
        if let Some(v) = s.strip_suffix('%') {
            return v.trim().parse::<f32>().ok().filter(|v| v.is_finite()).map(Len::Pct);
        }
        s.strip_suffix("px")
            .unwrap_or(s)
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(Len::Px)
    }
}

/// Четыре стороны: padding/margin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Sides {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Sides {
    fn uniform(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }
}

/// Смещения абсолютно позиционированного узла.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Inset {
    pub top: Option<Len>,
    pub right: Option<Len>,
    pub bottom: Option<Len>,
    pub left: Option<Len>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StylePosition {
    Relative,
    Absolute,
}

/// Подмножество CSS-свойств, применяемое к нативному узлу.
///
/// Строится из плоской карты строк (`{"width": "300px"}`) через `TryFrom`, поэтому парсинг
/// происходит один раз при разборе дерева, а не на каждом кадре.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(try_from = "BTreeMap<String, String>")]
pub struct NodeStyle {
    pub width: Option<Len>,
    pub height: Option<Len>,
    pub min_width: Option<Len>,
    pub max_width: Option<Len>,
    pub min_height: Option<Len>,
    pub max_height: Option<Len>,
    pub padding: Option<Sides>,
    pub margin: Option<Sides>,
    /// 0xRRGGBBAA: `Rgba` не сериализуется, а u32 спокойно переживает границу воркера.
    pub background: Option<u32>,
    pub color: Option<u32>,
    pub opacity: Option<f32>,
    pub border_radius: Option<f32>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub text_align: Option<TextAlign>,
    pub position: Option<StylePosition>,
    pub inset: Inset,
    pub overflow_hidden: bool,
    /// `display: none` — узел не рендерится, но остаётся в дереве: плагин может вернуть его обратно.
    pub hidden: bool,
}

/// Сериализация в ту же плоскую карту строк, что присылает JS: иначе дерево нельзя
/// прогнать через `serde_json::to_string`/`from_str` — форма записи и чтения разъедутся.
impl Serialize for NodeStyle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let len = |v: Len| match v {
            Len::Px(p) => format!("{p}px"),
            Len::Pct(p) => format!("{p}%"),
        };
        let mut map = serializer.serialize_map(None)?;

        if let Some(v) = self.width {
            map.serialize_entry("width", &len(v))?;
        }
        if let Some(v) = self.height {
            map.serialize_entry("height", &len(v))?;
        }
        if let Some(v) = self.min_width {
            map.serialize_entry("minWidth", &len(v))?;
        }
        if let Some(v) = self.max_width {
            map.serialize_entry("maxWidth", &len(v))?;
        }
        if let Some(v) = self.min_height {
            map.serialize_entry("minHeight", &len(v))?;
        }
        if let Some(v) = self.max_height {
            map.serialize_entry("maxHeight", &len(v))?;
        }
        for (prefix, sides) in [("padding", self.padding), ("margin", self.margin)] {
            if let Some(s) = sides {
                map.serialize_entry(&format!("{prefix}Top"), &format!("{}px", s.top))?;
                map.serialize_entry(&format!("{prefix}Right"), &format!("{}px", s.right))?;
                map.serialize_entry(&format!("{prefix}Bottom"), &format!("{}px", s.bottom))?;
                map.serialize_entry(&format!("{prefix}Left"), &format!("{}px", s.left))?;
            }
        }
        if let Some(c) = self.background {
            map.serialize_entry("backgroundColor", &format!("#{c:08x}"))?;
        }
        if let Some(c) = self.color {
            map.serialize_entry("color", &format!("#{c:08x}"))?;
        }
        if let Some(v) = self.opacity {
            map.serialize_entry("opacity", &v.to_string())?;
        }
        if let Some(v) = self.border_radius {
            map.serialize_entry("borderRadius", &format!("{v}px"))?;
        }
        if let Some(v) = self.font_size {
            map.serialize_entry("fontSize", &format!("{v}px"))?;
        }
        if let Some(v) = self.font_weight {
            map.serialize_entry("fontWeight", &v.to_string())?;
        }
        if let Some(v) = self.text_align {
            map.serialize_entry(
                "textAlign",
                match v {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                },
            )?;
        }
        if let Some(v) = self.position {
            map.serialize_entry(
                "position",
                match v {
                    StylePosition::Relative => "relative",
                    StylePosition::Absolute => "absolute",
                },
            )?;
        }
        for (key, value) in [
            ("top", self.inset.top),
            ("right", self.inset.right),
            ("bottom", self.inset.bottom),
            ("left", self.inset.left),
        ] {
            if let Some(v) = value {
                map.serialize_entry(key, &len(v))?;
            }
        }
        if self.overflow_hidden {
            map.serialize_entry("overflow", "hidden")?;
        }
        if self.hidden {
            map.serialize_entry("display", "none")?;
        }

        map.end()
    }
}

impl TryFrom<BTreeMap<String, String>> for NodeStyle {
    type Error = std::convert::Infallible;

    fn try_from(raw: BTreeMap<String, String>) -> Result<Self, Self::Error> {
        Ok(Self::from_map(&raw))
    }
}

/// Ключ CSS в канонический вид: без регистра и разделителей (`background-color` → `backgroundcolor`).
fn norm_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl NodeStyle {
    fn from_map(raw: &BTreeMap<String, String>) -> Self {
        let map: BTreeMap<String, &str> = raw.iter().map(|(k, v)| (norm_key(k), v.as_str())).collect();
        let get = |name: &str| map.get(name).copied();

        Self {
            width: get("width").and_then(Len::parse),
            height: get("height").and_then(Len::parse),
            min_width: get("minwidth").and_then(Len::parse),
            max_width: get("maxwidth").and_then(Len::parse),
            min_height: get("minheight").and_then(Len::parse),
            max_height: get("maxheight").and_then(Len::parse),
            padding: sides(&map, "padding"),
            margin: sides(&map, "margin"),
            background: get("backgroundcolor").or_else(|| get("background")).and_then(parse_color),
            color: get("color").and_then(parse_color),
            // visibility: hidden сохраняет место в раскладке — ближайший эквивалент в GPUI —
            // нулевая непрозрачность, а не удаление узла.
            opacity: match (get("opacity").and_then(parse_f32), get("visibility")) {
                (Some(v), _) => Some(v.clamp(0.0, 1.0)),
                (None, Some(v)) if v.trim() == "hidden" => Some(0.0),
                _ => None,
            },
            border_radius: get("borderradius").and_then(parse_px),
            font_size: get("fontsize").and_then(parse_px),
            font_weight: get("fontweight").and_then(parse_f32).map(|v| v.round().clamp(100.0, 900.0) as u16),
            text_align: match get("textalign").map(|v| v.trim().to_ascii_lowercase()) {
                Some(v) if v == "center" => Some(TextAlign::Center),
                Some(v) if v == "right" || v == "end" => Some(TextAlign::Right),
                Some(v) if v == "left" || v == "start" => Some(TextAlign::Left),
                _ => None,
            },
            position: match get("position").map(|v| v.trim().to_ascii_lowercase()) {
                Some(v) if v == "absolute" || v == "fixed" => Some(StylePosition::Absolute),
                Some(v) if v == "relative" || v == "sticky" => Some(StylePosition::Relative),
                _ => None,
            },
            inset: Inset {
                top: get("top").and_then(Len::parse),
                right: get("right").and_then(Len::parse),
                bottom: get("bottom").and_then(Len::parse),
                left: get("left").and_then(Len::parse),
            },
            overflow_hidden: matches!(get("overflow").map(|v| v.trim()), Some("hidden") | Some("clip")),
            hidden: matches!(get("display").map(|v| v.trim()), Some("none")),
        }
    }

    /// Есть ли что применять: пустой стиль не должен превращаться в лишнюю обёртку.
    pub fn is_empty(&self) -> bool {
        self == &NodeStyle::default()
    }
}

/// `padding`/`margin` по правилам CSS: 1–4 значения + отдельные стороны поверх.
fn sides(map: &BTreeMap<String, &str>, base: &str) -> Option<Sides> {
    let mut any = false;
    let mut s = Sides::default();

    if let Some(raw) = map.get(base) {
        let parts: Vec<f32> = raw.split_whitespace().filter_map(parse_px).collect();
        let parsed = match parts.len() {
            1 => Some(Sides::uniform(parts[0])),
            2 => Some(Sides { top: parts[0], right: parts[1], bottom: parts[0], left: parts[1] }),
            3 => Some(Sides { top: parts[0], right: parts[1], bottom: parts[2], left: parts[1] }),
            4 => Some(Sides { top: parts[0], right: parts[1], bottom: parts[2], left: parts[3] }),
            _ => None,
        };
        if let Some(p) = parsed {
            s = p;
            any = true;
        }
    }

    for (suffix, slot) in [("top", 0), ("right", 1), ("bottom", 2), ("left", 3)] {
        if let Some(v) = map.get(&format!("{base}{suffix}")).and_then(|v| parse_px(v)) {
            match slot {
                0 => s.top = v,
                1 => s.right = v,
                2 => s.bottom = v,
                _ => s.left = v,
            }
            any = true;
        }
    }

    if any { Some(s) } else { None }
}

fn parse_f32(raw: &str) -> Option<f32> {
    raw.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Пиксели без единицы тоже принимаются: плагины пишут и `"12"`, и `"12px"`.
fn parse_px(raw: &str) -> Option<f32> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        return None;
    }
    parse_f32(s.strip_suffix("px").unwrap_or(s))
}

/// CSS-цвет → 0xRRGGBBAA. Понимает #rgb/#rrggbb/#rrggbbaa, rgb()/rgba() и базовые имена.
fn parse_color(raw: &str) -> Option<u32> {
    let s = raw.trim().to_ascii_lowercase();
    let named = match s.as_str() {
        "transparent" => Some(0x0000_0000),
        "white" => Some(0xffff_ffff),
        "black" => Some(0x0000_00ff),
        "red" => Some(0xff00_00ff),
        "green" => Some(0x0080_00ff),
        "blue" => Some(0x0000_ffff),
        "gray" | "grey" => Some(0x8080_80ff),
        _ => None,
    };
    if named.is_some() {
        return named;
    }

    if let Some(hex) = s.strip_prefix('#') {
        let v = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            3 => {
                let r = (v >> 8) & 0xf;
                let g = (v >> 4) & 0xf;
                let b = v & 0xf;
                Some(((r * 17) << 24) | ((g * 17) << 16) | ((b * 17) << 8) | 0xff)
            }
            6 => Some((v << 8) | 0xff),
            8 => Some(v),
            _ => None,
        };
    }

    let inner = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|p| p.trim().trim_end_matches('%').parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    let (r, g, b) = (*parts.first()?, *parts.get(1)?, *parts.get(2)?);
    let a = parts.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0);
    Some(
        ((r.clamp(0.0, 255.0) as u32) << 24)
            | ((g.clamp(0.0, 255.0) as u32) << 16)
            | ((b.clamp(0.0, 255.0) as u32) << 8)
            | (a * 255.0) as u32,
    )
}

fn units(len: Len) -> gpui::Length {
    match len {
        Len::Px(v) => px(v).into(),
        Len::Pct(v) => relative(v / 100.0).into(),
    }
}

/// Совпадение слова внутри имени класса: `setting-item-name` содержит `name`, а `username`,
/// `rename` и `subtitle` — нет. Дефис, подчёркивание и пробел считаются границами слова.
fn has_word(cls: &str, word: &str) -> bool {
    cls.split(|c: char| !c.is_alphanumeric()).any(|part| part == word)
}

/// Применяет стиль поверх того, что уже решили класс и семантика узла: явный стиль плагина
/// главнее эвристики по классу.
fn apply_style(mut el: Div, style: &NodeStyle) -> Div {
    if let Some(v) = style.width {
        el = el.w(units(v));
    }
    if let Some(v) = style.height {
        el = el.h(units(v));
    }
    if let Some(v) = style.min_width {
        el = el.min_w(units(v));
    }
    if let Some(v) = style.max_width {
        el = el.max_w(units(v));
    }
    if let Some(v) = style.min_height {
        el = el.min_h(units(v));
    }
    if let Some(v) = style.max_height {
        el = el.max_h(units(v));
    }
    if let Some(p) = style.padding {
        el = el.pt(px(p.top)).pr(px(p.right)).pb(px(p.bottom)).pl(px(p.left));
    }
    if let Some(m) = style.margin {
        el = el.mt(px(m.top)).mr(px(m.right)).mb(px(m.bottom)).ml(px(m.left));
    }
    if let Some(c) = style.background {
        el = el.bg(gpui::rgba(c));
    }
    if let Some(c) = style.color {
        el = el.text_color(gpui::rgba(c));
    }
    if let Some(o) = style.opacity {
        el = el.opacity(o);
    }
    if let Some(r) = style.border_radius {
        el = el.rounded(px(r));
    }
    if let Some(f) = style.font_size {
        el = el.text_size(px(f));
    }
    if let Some(w) = style.font_weight {
        el = el.font_weight(match w {
            0..=499 => gpui::FontWeight::NORMAL,
            500..=599 => gpui::FontWeight::MEDIUM,
            600..=699 => gpui::FontWeight::SEMIBOLD,
            _ => gpui::FontWeight::BOLD,
        });
    }
    if let Some(a) = style.text_align {
        el = el.text_align(a);
    }
    if style.overflow_hidden {
        el = el.overflow_hidden();
    }
    if style.position == Some(StylePosition::Absolute) {
        el = el.absolute();
    }
    if let Some(v) = style.inset.top {
        el = el.top(units(v));
    }
    if let Some(v) = style.inset.right {
        el = el.right(units(v));
    }
    if let Some(v) = style.inset.bottom {
        el = el.bottom(units(v));
    }
    if let Some(v) = style.inset.left {
        el = el.left(units(v));
    }
    el
}

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
            // Стили не отменяют идентичность: события ввода ищут узел по id и должны найти его
            // сквозь обёртку.
            UiNode::Styled { child, .. } => child.id(),
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
                    .pr(px(18.0))
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
                                    btn_elements.push(render_ui_node(
                                        &children[i],
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

                rendered_children.push(render_ui_node(
                    &children[i],
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
                row = row
                    .flex_shrink_0()
                    .px(px(24.0))
                    .py(px(14.0))
                    .gap(px(10.0))
                    .justify_end()
                    .bg(theme.bg_card)
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
            row.children(children.iter().map(|child| {
                render_ui_node(
                    child,
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

            btn.child(label.clone()).into_any_element()
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
                render_ui_node(
                    c,
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
                        .child(render_ui_node(
                            control,
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
            let inner = render_ui_node(
                child,
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
            // Обёртка — это бокс самого элемента плагина: размеры, отступы, фон, позиция.
            // Внутренняя раскладка (row/column, gap) остаётся за самим узлом.
            apply_style(div().flex().flex_col().flex_shrink_0(), style)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn style_parses_supported_css_subset() {
        let raw = BTreeMap::from([
            ("width".to_string(), "320px".to_string()),
            ("height".to_string(), "100%".to_string()),
            ("padding".to_string(), "4px 8px".to_string()),
            ("paddingLeft".to_string(), "2px".to_string()),
            ("backgroundColor".to_string(), "#ff0000".to_string()),
            ("opacity".to_string(), "0.5".to_string()),
            ("display".to_string(), "none".to_string()),
            ("position".to_string(), "absolute".to_string()),
            ("top".to_string(), "12px".to_string()),
            ("zIndex".to_string(), "10".to_string()),
        ]);
        let style = NodeStyle::from_map(&raw);

        assert_eq!(style.width, Some(Len::Px(320.0)));
        assert_eq!(style.height, Some(Len::Pct(100.0)));
        assert_eq!(style.padding, Some(Sides { top: 4.0, right: 8.0, bottom: 4.0, left: 2.0 }));
        assert_eq!(style.background, Some(0xff00_00ff));
        assert_eq!(style.opacity, Some(0.5));
        assert!(style.hidden);
        assert_eq!(style.position, Some(StylePosition::Absolute));
        assert_eq!(style.inset.top, Some(Len::Px(12.0)));
    }

    #[test]
    fn style_parses_colors() {
        assert_eq!(parse_color("#f00"), Some(0xff00_00ff));
        assert_eq!(parse_color("#ff0000"), Some(0xff00_00ff));
        assert_eq!(parse_color("#ff000080"), Some(0xff00_0080));
        assert_eq!(parse_color("rgba(0, 128, 255, 0.5)"), Some(0x0080_ff7f));
        assert_eq!(parse_color("transparent"), Some(0));
        assert_eq!(parse_color("не цвет"), None);
    }

    #[test]
    fn class_matching_respects_word_boundaries() {
        assert!(has_word("setting-item-name", "name"));
        assert!(!has_word("username", "name"));
        assert!(!has_word("rename", "name"));
        assert!(!has_word("subtitle", "title"));
        assert!(has_word("is-active", "active"));
        assert!(!has_word("inactive", "active"));
    }

    #[test]
    fn styled_node_keeps_id_and_children() {
        let node = UiNode::Styled {
            style: NodeStyle::default(),
            child: Box::new(UiNode::Input {
                id: "query".into(),
                placeholder: None,
                value: None,
            }),
        };
        assert_eq!(node.id(), Some("query"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn styled_node_round_trips_through_json() {
        let node = UiNode::Styled {
            style: NodeStyle::from_map(&BTreeMap::from([
                ("width".to_string(), "320px".to_string()),
                ("backgroundColor".to_string(), "#ff0000".to_string()),
                ("padding".to_string(), "4px 8px".to_string()),
                ("position".to_string(), "absolute".to_string()),
                ("top".to_string(), "12px".to_string()),
            ])),
            child: Box::new(UiNode::Text {
                value: "x".into(),
                class: None,
                style: TextStyle::Normal,
            }),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: UiNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, back, "json: {json}");
    }

    #[test]
    fn test_ui_node_serialization_and_count() {
        let tree = UiNode::Column {
            id: None,
            class: None,
            gap: Gap::Medium,
            children: vec![
                UiNode::Text {
                    value: "Header".into(),
                    class: None,
                    style: TextStyle::Heading,
                },
                UiNode::Button {
                    id: "submit-btn".into(),
                    label: "Submit".into(),
                    class: None,
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
            id: None,
            class: None,
            gap: Gap::None,
            children: vec![
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "One".into(),
                    class: None,
                    variant: ButtonVariant::Default,
                    disabled: false,
                },
                UiNode::Button {
                    id: "btn-1".into(),
                    label: "Two".into(),
                    class: None,
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
            id: None,
            class: None,
            gap: Gap::Small,
            children: vec![
                UiNode::Text {
                    value: "Description".into(),
                    class: None,
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
            id: None,
            class: None,
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
            id: None,
            class: None,
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
            id: None,
            class: None,
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

