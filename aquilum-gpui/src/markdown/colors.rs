#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingPresetColor {
    pub id: &'static str,
    pub label: &'static str,
    pub hex: &'static str,
    pub desc: &'static str,
    pub rgb: (u8, u8, u8),
}

pub const HEADING_COLOR_PRESETS: &[HeadingPresetColor] = &[
    HeadingPresetColor {
        id: "orange",
        label: "Оранжевый",
        hex: "#E69F00",
        desc: "Хорошо сочетается с синим",
        rgb: (230, 159, 0),
    },
    HeadingPresetColor {
        id: "sky_blue",
        label: "Голубой",
        hex: "#56B4E9",
        desc: "Высокая различимость",
        rgb: (86, 180, 233),
    },
    HeadingPresetColor {
        id: "bluish_green",
        label: "Сине-зелёный",
        hex: "#009E73",
        desc: "Четвёртый ряд; teal",
        rgb: (0, 158, 115),
    },
    HeadingPresetColor {
        id: "yellow",
        label: "Жёлтый",
        hex: "#F0E442",
        desc: "Выделение и заливка",
        rgb: (240, 228, 66),
    },
    HeadingPresetColor {
        id: "blue",
        label: "Синий",
        hex: "#0072B2",
        desc: "Сильный акцент",
        rgb: (0, 114, 178),
    },
    HeadingPresetColor {
        id: "vermilion",
        label: "Алый",
        hex: "#D55E00",
        desc: "Планки погрешностей, акцент",
        rgb: (213, 94, 0),
    },
    HeadingPresetColor {
        id: "reddish_purple",
        label: "Розово-фиолетовый",
        hex: "#CC79A7",
        desc: "Предел для категорий",
        rgb: (204, 121, 167),
    },
    HeadingPresetColor {
        id: "red",
        label: "Красный",
        hex: "#F44336",
        desc: "Красный акцент (цвет заголовка заметки)",
        rgb: (244, 67, 54),
    },
];

pub fn find_heading_preset(name: &str) -> &'static HeadingPresetColor {
    for preset in HEADING_COLOR_PRESETS {
        if preset.id.eq_ignore_ascii_case(name)
            || preset.label.eq_ignore_ascii_case(name)
            || preset.hex.eq_ignore_ascii_case(name)
        {
            return preset;
        }
    }
    match name.to_ascii_lowercase().as_str() {
        "orange" => &HEADING_COLOR_PRESETS[0],
        "sky_blue" | "slate" => &HEADING_COLOR_PRESETS[1],
        "teal" | "emerald" | "bluish_green" => &HEADING_COLOR_PRESETS[2],
        "yellow" => &HEADING_COLOR_PRESETS[3],
        "blue" | "indigo" => &HEADING_COLOR_PRESETS[4],
        "vermilion" => &HEADING_COLOR_PRESETS[5],
        "purple" | "violet" | "fuchsia" | "reddish_purple" => &HEADING_COLOR_PRESETS[6],
        "red" | "crimson" | "rose" => &HEADING_COLOR_PRESETS[7],
        _ => &HEADING_COLOR_PRESETS[4], // Default to blue
    }
}

pub fn parse_hex_color(hex: &str) -> Option<gpui::Rgba> {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        if let Ok(val) = u32::from_str_radix(clean, 16) {
            let r = ((val >> 16) & 0xff) as u8;
            let g = ((val >> 8) & 0xff) as u8;
            let b = (val & 0xff) as u8;
            return Some(gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)));
        }
    } else if clean.len() == 3 {
        if let Ok(val) = u32::from_str_radix(clean, 16) {
            let r = ((val >> 8) & 0xf) as u8;
            let g = ((val >> 4) & 0xf) as u8;
            let b = (val & 0xf) as u8;
            let r = (r << 4) | r;
            let g = (g << 4) | g;
            let b = (b << 4) | b;
            return Some(gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)));
        }
    }
    None
}

pub fn heading_color_to_rgb(name: &str) -> gpui::Rgba {
    if let Some(rgba) = parse_hex_color(name) {
        return rgba;
    }
    let preset = find_heading_preset(name);
    let (r, g, b) = preset.rgb;
    gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
}

pub fn heading_color_to_hex(name: &str) -> String {
    let clean = name.trim().trim_start_matches('#');
    if parse_hex_color(name).is_some() {
        if clean.len() == 6 {
            return format!("#{}", clean.to_ascii_uppercase());
        } else if clean.len() == 3 {
            let r = &clean[0..1];
            let g = &clean[1..2];
            let b = &clean[2..3];
            return format!("#{r}{r}{g}{g}{b}{b}").to_ascii_uppercase();
        }
    }
    find_heading_preset(name).hex.to_string()
}

/// Comprehensive palette of 24 harmonious pastel colors from modern UI color catalogs (Tailwind / Material).
/// Each entry contains (background, text) hex RGBA colors formatted for light mode.
pub const TAG_PALETTE_LIGHT_HEX: &[(u32, u32)] = &[
    (0xfee2e2ff, 0x991b1bff), // 0: Coral / Red
    (0xffecd9ff, 0xb85d19ff), // 1: Peach / Apricot
    (0xffedd5ff, 0x9a3412ff), // 2: Orange
    (0xfef3c7ff, 0x92400eff), // 3: Amber / Honey
    (0xfef9c3ff, 0x854d0eff), // 4: Warm Yellow
    (0xecfccbff, 0x3f6212ff), // 5: Lime
    (0xe2f5d5ff, 0x2d6a1eff), // 6: Sage / Olive
    (0xddf6ddff, 0x1e8222ff), // 7: Green
    (0xdcfce7ff, 0x166534ff), // 8: Mint
    (0xd1fae5ff, 0x065f46ff), // 9: Emerald
    (0xdbf3f0ff, 0x0e7a75ff), // 10: Teal
    (0xccfbf1ff, 0x115e59ff), // 11: Aqua / Turquoise
    (0xcffafeff, 0x155e75ff), // 12: Cyan
    (0xe0f2feff, 0x0369a1ff), // 13: Sky Blue
    (0xdbeafeff, 0x1d4ed8ff), // 14: Cerulean / Ocean
    (0xe0e7ffff, 0x1e40afff), // 15: Blue
    (0xe8eaf6ff, 0x3730a3ff), // 16: Indigo
    (0xede9feff, 0x4338caff), // 17: Periwinkle / Slate Blue
    (0xf3e8ffff, 0x5b21b6ff), // 18: Violet
    (0xf5e6fbff, 0x6b21a8ff), // 19: Purple
    (0xfae8ffff, 0x86198fff), // 20: Fuchsia / Orchid
    (0xfce4ecff, 0x9c276aff), // 21: Magenta / Plum
    (0xfce7f3ff, 0x9d174dff), // 22: Pink / Blush
    (0xffe4e6ff, 0x9f1239ff), // 23: Rose / Ruby
];

/// Dark mode counterpart for the 24-color catalog, using matching luminous accents.
pub const TAG_PALETTE_DARK_HEX: &[(u32, u32)] = &[
    (0x7f1d1d4d, 0xfca5a5ff), // 0: Coral / Red
    (0x7c2d124d, 0xfdba74ff), // 1: Peach / Apricot
    (0x7c2d124d, 0xfb923cff), // 2: Orange
    (0x78350f4d, 0xfcd34dff), // 3: Amber / Honey
    (0x713f124d, 0xfde047ff), // 4: Warm Yellow
    (0x3653144d, 0xbef264ff), // 5: Lime
    (0x234e184d, 0xa3e635ff), // 6: Sage / Olive
    (0x14532d4d, 0x86efacff), // 7: Green
    (0x14532d4d, 0x6ee7b7ff), // 8: Mint
    (0x064e3b4d, 0x34d399ff), // 9: Emerald
    (0x134e4a4d, 0x5eead4ff), // 10: Teal
    (0x134e4a4d, 0x2dd4bfff), // 11: Aqua / Turquoise
    (0x164e634d, 0x67e8f9ff), // 12: Cyan
    (0x0c4a6e4d, 0x7dd3fcff), // 13: Sky Blue
    (0x1e3a8a4d, 0x60a5faff), // 14: Cerulean / Ocean
    (0x1e3a8a4d, 0x93c5fdff), // 15: Blue
    (0x312e814d, 0xa5b4fcff), // 16: Indigo
    (0x3730a34d, 0xc7d2feff), // 17: Periwinkle / Slate Blue
    (0x4c1d954d, 0xc4b5fdff), // 18: Violet
    (0x581c874d, 0xd8b4feff), // 19: Purple
    (0x701a754d, 0xf0abfcff), // 20: Fuchsia / Orchid
    (0x701a754d, 0xf472b6ff), // 21: Magenta / Plum
    (0x8318434d, 0xfb7185ff), // 22: Pink / Blush
    (0x8813374d, 0xfda4afff), // 23: Rose / Ruby
];

/// Returns the background and foreground colors (as `(bg, fg)`) for any markdown `#tag`.
/// Absolutely no hardcoding: any word or tag string is deterministically hashed across the
/// catalog of harmonious pastel colors, ensuring consistent and distinct badges.
pub fn tag_color_pair(tag: &str, is_dark: bool) -> (gpui::Rgba, gpui::Rgba) {
    let clean = tag.trim_start_matches('#').to_lowercase();
    let mut h: u64 = 0xcbf29ce484222325; // 64-bit FNV-1a hash
    for b in clean.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let palette = if is_dark { TAG_PALETTE_DARK_HEX } else { TAG_PALETTE_LIGHT_HEX };
    let idx = (h as usize) % palette.len();
    let (bg, fg) = palette[idx];
    (gpui::rgba(bg), gpui::rgba(fg))
}

