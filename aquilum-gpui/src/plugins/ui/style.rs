use std::collections::BTreeMap;
use gpui::{px, relative, Styled, TextAlign};
use serde::{Deserialize, Serialize};
use super::tree::UiNode;

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

impl NodeStyle {
    /// Трогает ли стиль геометрию. Только такие стили требуют отдельного бокса вокруг узла:
    /// цвет и кегль достаточно повесить на обёртку-контейнер, а жёсткая колонка внутри строки
    /// из нескольких `span` сломала бы раскладку.
    pub fn affects_layout(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.max_width.is_some()
            || self.min_height.is_some()
            || self.max_height.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.position.is_some()
            || self.overflow_hidden
            || self.inset != Inset::default()
    }
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
pub(crate) fn norm_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl NodeStyle {
    pub(crate) fn from_map(raw: &BTreeMap<String, String>) -> Self {
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
pub(crate) fn sides(map: &BTreeMap<String, &str>, base: &str) -> Option<Sides> {
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

pub(crate) fn parse_f32(raw: &str) -> Option<f32> {
    raw.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Пиксели без единицы тоже принимаются: плагины пишут и `"12"`, и `"12px"`.
pub(crate) fn parse_px(raw: &str) -> Option<f32> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        return None;
    }
    parse_f32(s.strip_suffix("px").unwrap_or(s))
}

/// CSS-цвет → 0xRRGGBBAA. Понимает #rgb/#rrggbb/#rrggbbaa, rgb()/rgba() и базовые имена.
pub(crate) fn parse_color(raw: &str) -> Option<u32> {
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

pub(crate) fn units(len: Len) -> gpui::Length {
    match len {
        Len::Px(v) => px(v).into(),
        Len::Pct(v) => relative(v / 100.0).into(),
    }
}

/// Совпадение слова внутри имени класса: `setting-item-name` содержит `name`, а `username`,
/// `rename` и `subtitle` — нет. Дефис, подчёркивание и пробел считаются границами слова.
pub(crate) fn has_word(cls: &str, word: &str) -> bool {
    cls.split(|c: char| !c.is_alphanumeric()).any(|part| part == word)
}

/// Применяет стиль поверх того, что уже решили класс и семантика узла: явный стиль плагина
/// главнее эвристики по классу.
pub(crate) fn apply_style<T: Styled>(mut el: T, style: &NodeStyle) -> T {
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

/// Стиль плагина ложится на тот же бокс, что и эвристика по классу: последнее слово остаётся
/// за таблицей стилей — ровно как в каскаде CSS. Отдельная обёртка для этого не годится:
/// бокс с авто-высотой делает родителя «неопределённым», и `flex_1`-ребёнок (basis 0)
/// схлопывается в ноль — так пропадал список файлов в Empty Files Cleaner.
///
/// Обобщён по `Styled`, потому что узел с `id` — уже `Stateful<Div>`, а не `Div`.
pub(crate) fn apply_sheet<T: Styled>(el: T, sheet: Option<&NodeStyle>) -> T {
    match sheet {
        Some(style) => apply_style(el, style),
        None => el,
    }
}

/// Узел, чья ветка рендера сама заканчивается `apply_sheet`: обёртка ему не нужна и вредна.
///
/// Только эти три вида несут эвристику по классу, которая спорит с таблицей стилей за одну и ту
/// же коробку. Всё остальное (текст, поле ввода, чекбокс) получает обёртку — там она безвредна,
/// потому что геометрию никто не делит.
pub(crate) fn takes_style_directly(node: &UiNode) -> bool {
    matches!(
        node,
        UiNode::Column { .. } | UiNode::Row { .. } | UiNode::Button { .. }
    )
}

/// Пустой контрол для setting-строки: подставляется, когда плагин прислал `control: null`
/// (например, строка с одной иконкой вместо поля ввода).
pub(crate) fn empty_text_control() -> Box<UiNode> {
    Box::new(UiNode::Text { value: String::new(), class: None, style: TextStyle::Normal })
}

/// `control: null` в проводном формате — «контрола нет», а не ошибка разбора.
pub(crate) fn control_or_empty<'de, D>(deserializer: D) -> Result<Box<UiNode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<UiNode> = Option::deserialize(deserializer)?;
    Ok(v.map(Box::new).unwrap_or_else(empty_text_control))
}
