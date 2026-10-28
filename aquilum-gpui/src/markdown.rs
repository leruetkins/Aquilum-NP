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

/// Opens the native OS Color Picker dialog (e.g. ChooseColorW on Windows) to visually pick any color.
#[cfg(target_os = "windows")]
pub fn open_system_color_picker(initial_hex: &str) -> Option<String> {
    #[repr(C)]
    struct CHOOSECOLORW {
        l_struct_size: u32,
        hwnd_owner: *mut std::ffi::c_void,
        h_instance: *mut std::ffi::c_void,
        rgb_result: u32,
        lp_cust_colors: *mut u32,
        flags: u32,
        l_cust_data: isize,
        lpfn_hook: Option<unsafe extern "system" fn(*mut std::ffi::c_void, u32, usize, isize) -> usize>,
        lp_template_name: *const u16,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn ChooseColorW(lpcc: *mut CHOOSECOLORW) -> i32;
    }

    static CUSTOM_COLORS: std::sync::Mutex<[u32; 16]> = std::sync::Mutex::new([0x00FFFFFF; 16]);

    let init_rgb = parse_hex_color(initial_hex)
        .or_else(|| {
            let p = find_heading_preset(initial_hex);
            let (r, g, b) = p.rgb;
            Some(gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)))
        })
        .unwrap_or(gpui::rgb(0x0072B2));

    let r = (init_rgb.r * 255.0).round() as u32;
    let g = (init_rgb.g * 255.0).round() as u32;
    let b = (init_rgb.b * 255.0).round() as u32;
    let win_rgb = (r & 0xFF) | ((g & 0xFF) << 8) | ((b & 0xFF) << 16);

    const CC_RGBINIT: u32 = 0x00000001;
    const CC_FULLOPEN: u32 = 0x00000002;
    const CC_ANYCOLOR: u32 = 0x00000100;

    let mut custom_colors = CUSTOM_COLORS.lock().unwrap();

    let mut cc = CHOOSECOLORW {
        l_struct_size: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwnd_owner: std::ptr::null_mut(),
        h_instance: std::ptr::null_mut(),
        rgb_result: win_rgb,
        lp_cust_colors: custom_colors.as_mut_ptr(),
        flags: CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR,
        l_cust_data: 0,
        lpfn_hook: None,
        lp_template_name: std::ptr::null(),
    };

    let result = unsafe { ChooseColorW(&mut cc) };
    if result != 0 {
        let res_r = cc.rgb_result & 0xFF;
        let res_g = (cc.rgb_result >> 8) & 0xFF;
        let res_b = (cc.rgb_result >> 16) & 0xFF;
        Some(format!("#{:02X}{:02X}{:02X}", res_r, res_g, res_b))
    } else {
        None
    }
}

/// Samples the pixel color at the current mouse cursor position on Windows.
#[cfg(target_os = "windows")]
pub fn sample_screen_color_at_cursor() -> Option<String> {
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(lp_point: *mut POINT) -> i32;
        fn GetDC(h_wnd: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn ReleaseDC(h_wnd: *mut std::ffi::c_void, h_dc: *mut std::ffi::c_void) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetPixel(h_dc: *mut std::ffi::c_void, x: i32, y: i32) -> u32;
    }

    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) } == 0 {
        return None;
    }
    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }
    let pixel = unsafe { GetPixel(hdc, pt.x, pt.y) };
    unsafe { ReleaseDC(std::ptr::null_mut(), hdc) };

    if pixel == 0xFFFFFFFF {
        return None;
    }
    let r = pixel & 0xFF;
    let g = (pixel >> 8) & 0xFF;
    let b = (pixel >> 16) & 0xFF;
    Some(format!("#{:02X}{:02X}{:02X}", r, g, b))
}

#[cfg(not(target_os = "windows"))]
pub fn sample_screen_color_at_cursor() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn open_system_color_picker(_initial_hex: &str) -> Option<String> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeBlockKind {
    OpeningFence,
    InsideCode,
    ClosingFence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlockState {
    pub kind: CodeBlockKind,
    pub lang: Option<String>,
    pub block_id: usize,
}

/// Fence state *before* a line: everything the scanner needs to resume.
///
/// The scanner is a two-variable machine, so this is the whole of its memory.
/// That is what makes a mid-document restart possible: a window can be told the
/// state at its first line instead of being scanned from the top of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeScanState {
    /// Whether the scanner is inside a fenced block.
    pub in_code_block: bool,
    /// Id the next fence pair will carry.
    pub block_id: usize,
}

/// Resumable fence scanner — the machine behind [`analyze_code_blocks`].
///
/// Identical logic to the old one-shot loop, but the state lives in a struct
/// that can be saved (see [`crate::code_scan::CodeCheckpoints`]) and restored,
/// so the document can be scanned in pieces instead of all at once.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodeScanner {
    state: CodeScanState,
}

impl CodeScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume from a saved state.
    pub fn from_state(state: CodeScanState) -> Self {
        Self { state }
    }

    /// The state that applies to the next line.
    pub fn state(&self) -> CodeScanState {
        self.state
    }

    /// Consume one line, returning the state that applies to it.
    pub fn step(&mut self, line: &str) -> Option<CodeBlockState> {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if self.state.in_code_block {
                let block_id = self.state.block_id;
                self.state.in_code_block = false;
                self.state.block_id += 1;
                Some(CodeBlockState {
                    kind: CodeBlockKind::ClosingFence,
                    lang: None,
                    block_id,
                })
            } else {
                let block_id = self.state.block_id;
                let lang = trimmed.trim_start_matches('`').trim();
                let lang_opt = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                self.state.in_code_block = true;
                Some(CodeBlockState {
                    kind: CodeBlockKind::OpeningFence,
                    lang: lang_opt,
                    block_id,
                })
            }
        } else if self.state.in_code_block {
            Some(CodeBlockState {
                kind: CodeBlockKind::InsideCode,
                lang: None,
                block_id: self.state.block_id,
            })
        } else {
            None
        }
    }
}

pub fn analyze_code_blocks(lines: &[&str]) -> Vec<Option<CodeBlockState>> {
    let mut scanner = CodeScanner::new();
    lines.iter().map(|line| scanner.step(line)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind<'a> {
    Heading1(&'a str),
    Heading2(&'a str),
    Heading3(&'a str),
    Heading4(&'a str),
    Heading5(&'a str),
    Heading6(&'a str),
    Quote(&'a str),
    Task { checked: bool, text: &'a str },
    BulletList { indent: usize, text: &'a str },
    NumberedList { num: &'a str, text: &'a str },
    HorizontalRule,
    CodeFence { is_opening: bool, lang: Option<&'a str> },
    CodeLine(&'a str),
    Paragraph(&'a str),
}

pub fn classify_line<'a>(
    line: &'a str,
    code_state: Option<(CodeBlockKind, Option<&'a str>)>,
) -> LineKind<'a> {
    if let Some((kind, lang)) = code_state {
        return match kind {
            CodeBlockKind::OpeningFence => LineKind::CodeFence {
                is_opening: true,
                lang,
            },
            CodeBlockKind::InsideCode => LineKind::CodeLine(line),
            CodeBlockKind::ClosingFence => LineKind::CodeFence {
                is_opening: false,
                lang: None,
            },
        };
    }

    let trimmed_start = line.trim_start();
    let indent = line.len() - trimmed_start.len();

    if let Some(rest) = trimmed_start.strip_prefix("# ") {
        LineKind::Heading1(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("## ") {
        LineKind::Heading2(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("### ") {
        LineKind::Heading3(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("#### ") {
        LineKind::Heading4(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("##### ") {
        LineKind::Heading5(rest)
    } else if let Some(rest) = trimmed_start.strip_prefix("###### ") {
        LineKind::Heading6(rest)
    } else if let Some(q) = quote_prefix(trimmed_start) {
        LineKind::Quote(&trimmed_start[q.bytes..])
    } else if let Some(rest) = trimmed_start.strip_prefix("- [ ] ") {
        LineKind::Task { checked: false, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [ ] ") {
        LineKind::Task { checked: false, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- [x] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- [X] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [x] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* [X] ") {
        LineKind::Task { checked: true, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("- ") {
        LineKind::BulletList { indent, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("* ") {
        LineKind::BulletList { indent, text: rest }
    } else if let Some(rest) = trimmed_start.strip_prefix("+ ") {
        LineKind::BulletList { indent, text: rest }
    } else if trimmed_start == "---" || trimmed_start == "***" || trimmed_start == "___" {
        LineKind::HorizontalRule
    } else {
        // Проверка нумерованного списка "1. ", "2. "
        let digits_len = trimmed_start.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len > 0 && trimmed_start[digits_len..].starts_with(". ") {
            let num = &trimmed_start[..digits_len];
            let text = &trimmed_start[digits_len + 2..];
            LineKind::NumberedList { num, text }
        } else {
            LineKind::Paragraph(line)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineSpan {
    Text(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    InlineCode(String),
    Strikethrough(String),
    Link { label: String, url: String },
}

/// Number of backticks at the head of `s`, or 0 when it does not start with one.
///
/// A backtick string is a run of one *or more* backticks, and CommonMark closes a code
/// span with a run of **exactly** the same length. That is what makes
/// `` ``код с `обратными` `` `` a single span whose content holds literal backticks,
/// instead of an empty span followed by a lone one.
fn backtick_run_len(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b'`').count()
}

/// Byte offset of the first run of exactly `n` backticks in `s`, or `None` when there is
/// none. Backticks are ASCII, so the offset is a char boundary and safe to slice on; a
/// longer run does not match (`` `` `` is not closed by ``` ``` ```).
fn find_closing_backtick_run(s: &str, n: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - start == n {
            return Some(start);
        }
    }
    None
}

/// ASCII punctuation — the characters a backslash may escape (CommonMark). Anything else
/// after a backslash stays literal, backslash included.
fn is_escapable_punctuation(c: char) -> bool {
    matches!(c, '!'..='/' | ':'..='@' | '['..='`' | '{'..='~')
}

pub fn parse_inline_spans(mut s: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Backslash escape: the escaped char is literal, so it must not open a span.
        if s.starts_with('\\') {
            if let Some(c) = s[1..].chars().next() {
                if is_escapable_punctuation(c) {
                    plain_accum.push(c);
                    s = &s[1 + c.len_utf8()..];
                    continue;
                }
            }
        }

        // Inline code: a run of N backticks, closed by the next run of exactly N.
        let run = backtick_run_len(s);
        if run > 0 {
            match find_closing_backtick_run(&s[run..], run) {
                Some(content_len) => {
                    if !plain_accum.is_empty() {
                        spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let code_content = &s[run..run + content_len];
                    spans.push(InlineSpan::InlineCode(code_content.to_string()));
                    s = &s[run + content_len + run..];
                    continue;
                }
                // No run of the same length: the backticks are literal text, and the
                // whole run is consumed so it cannot be re-read as a shorter delimiter.
                None => {
                    plain_accum.push_str(&s[..run]);
                    s = &s[run..];
                    continue;
                }
            }
        }

        // Bold italic: ***...***
        if s.starts_with("***") {
            if let Some(end_idx) = s[3..].find("***") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[3..3 + end_idx];
                spans.push(InlineSpan::BoldItalic(content.to_string()));
                s = &s[6 + end_idx..];
                continue;
            }
        }

        // Bold: **...**
        if s.starts_with("**") {
            if let Some(end_idx) = s[2..].find("**") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[2..2 + end_idx];
                spans.push(InlineSpan::Bold(content.to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Italic: *...* (исключая списки)
        if s.starts_with('*') {
            if let Some(end_idx) = s[1..].find('*') {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[1..1 + end_idx];
                spans.push(InlineSpan::Italic(content.to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if s.starts_with("~~") {
            if let Some(end_idx) = s[2..].find("~~") {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let content = &s[2..2 + end_idx];
                spans.push(InlineSpan::Strikethrough(content.to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // WikiLink: [[target]] or [[target|alias]]
        if s.starts_with("[[") {
            if let Some(close_idx) = s[2..].find("]]") {
                let inner = &s[2..2 + close_idx];
                if !inner.is_empty() && !inner.contains('\n') {
                    if !plain_accum.is_empty() {
                        spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let (target, label) = if let Some((t, a)) = inner.split_once('|') {
                        (t.trim(), a.trim())
                    } else {
                        (inner.trim(), inner.trim())
                    };
                    spans.push(InlineSpan::Link {
                        label: label.to_string(),
                        url: target.to_string(),
                    });
                    let total_consumed = 2 + close_idx + 2;
                    s = &s[total_consumed..];
                    continue;
                }
            }
        }

        // Link: [label](url)
        if s.starts_with('[') {
            if let Some(close_bracket) = s[1..].find(']') {
                let after_bracket = &s[1 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        if !plain_accum.is_empty() {
                            spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                        }
                        let label = &s[1..1 + close_bracket];
                        let url = &after_bracket[1..1 + close_paren];
                        spans.push(InlineSpan::Link {
                            label: label.to_string(),
                            url: url.to_string(),
                        });
                        let total_consumed = 1 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }

        // Обычный символ
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            plain_accum.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    if !plain_accum.is_empty() {
        spans.push(InlineSpan::Text(plain_accum));
    }

    spans
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxSpan {
    Marker(String),
    Text(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    InlineCode(String),
    Strikethrough(String),
    Link { text: String, url: String },
    Underline(String),
    Highlight(String),
    Tag(String),
}

/// Which syntax markers of a line are shown right now.
///
/// The line layout is computed from the raw source, so revealing markers never changes how
/// many visual rows a line occupies or how tall the document is. Hidden markers simply
/// collapse out of the painted text (the one exception is an inline-code delimiter, which
/// keeps its cell so the code chip does not jump). `Ranges` holds absolute char offsets of
/// the inline elements (markers included) revealed around the caret or the selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum InlineReveal {
    #[default]
    None,
    All,
    Ranges(Vec<(usize, usize)>),
}

impl InlineReveal {
    /// Whether a marker at `[start, end)` of the raw line is visible.
    pub fn shows(&self, start: usize, end: usize) -> bool {
        match self {
            InlineReveal::None => false,
            InlineReveal::All => true,
            InlineReveal::Ranges(ranges) => ranges.iter().any(|&(a, b)| start < b && a < end),
        }
    }
}

/// A syntax span together with the visibility of the marker it came from.
/// `marker_visible` is always true for non-marker spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleSpan {
    pub span: SyntaxSpan,
    pub marker_visible: bool,
}

fn span_text(span: &SyntaxSpan) -> &str {
    match span {
        SyntaxSpan::Marker(t)
        | SyntaxSpan::Text(t)
        | SyntaxSpan::Bold(t)
        | SyntaxSpan::Italic(t)
        | SyntaxSpan::BoldItalic(t)
        | SyntaxSpan::InlineCode(t)
        | SyntaxSpan::Strikethrough(t)
        | SyntaxSpan::Underline(t)
        | SyntaxSpan::Highlight(t)
        | SyntaxSpan::Tag(t) => t,
        SyntaxSpan::Link { text, .. } => text,
    }
}

fn span_char_len(span: &SyntaxSpan) -> usize {
    span_text(span).chars().count()
}

/// Character offset (within `line`) where the inline content starts, together with the
/// spans parsed from that content. The prefix markers (heading hashes, quote caret, list
/// bullets) are block syntax and stay outside the inline reveal model.
pub fn line_inline_spans(line: &str, classified: &LineKind<'_>) -> (Vec<SyntaxSpan>, usize) {
    let (rest, prefix_bytes) = match classified {
        LineKind::Heading1(_) => (line.strip_prefix("# ").unwrap_or(line), 2),
        LineKind::Heading2(_) => (line.strip_prefix("## ").unwrap_or(line), 3),
        LineKind::Heading3(_) => (line.strip_prefix("### ").unwrap_or(line), 4),
        LineKind::Heading4(_) => (line.strip_prefix("#### ").unwrap_or(line), 5),
        LineKind::Heading5(_) => (line.strip_prefix("##### ").unwrap_or(line), 6),
        LineKind::Heading6(_) => (line.strip_prefix("###### ").unwrap_or(line), 7),
        LineKind::Quote(_) => {
            let prefix_bytes = callout_prefix(line)
                .map(|c| c.bytes)
                .or_else(|| quote_prefix(line).map(|q| q.bytes))
                .unwrap_or(0);
            (
                &line[prefix_bytes.min(line.len())..],
                prefix_bytes,
            )
        }
        LineKind::Task { .. } | LineKind::BulletList { .. } | LineKind::NumberedList { .. } => {
            let p_len = marker_prefix_len(line, classified);
            let rest = if line.len() >= p_len {
                &line[p_len..]
            } else {
                ""
            };
            (rest, p_len)
        }
        _ => (line, 0),
    };
    (
        parse_inline_syntax_spans(rest),
        head_char_count(line, prefix_bytes),
    )
}

/// Ranges of the inline elements (opening marker + content + closing marker) as char
/// offsets inside the span sequence. Revealing an element makes exactly its own markers
/// visible, which is the Obsidian live-preview behaviour.
pub fn inline_element_ranges(spans: &[SyntaxSpan]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut starts = Vec::with_capacity(spans.len());
    let mut acc = 0usize;
    for span in spans {
        starts.push(acc);
        acc += span_char_len(span);
    }

    let mut i = 0usize;
    while i < spans.len() {
        if i + 2 < spans.len() {
            let paired = matches!(spans[i], SyntaxSpan::Marker(_))
                && matches!(spans[i + 2], SyntaxSpan::Marker(_))
                && is_revealable_content(&spans[i + 1]);
            if paired {
                let end = starts[i] + span_char_len(&spans[i])
                    + span_char_len(&spans[i + 1])
                    + span_char_len(&spans[i + 2]);
                ranges.push((starts[i], end));
                i += 3;
                continue;
            }
        }
        if i + 1 < spans.len() {
            if let (SyntaxSpan::Marker(m), SyntaxSpan::Text(_)) = (&spans[i], &spans[i + 1]) {
                if m == "\\" {
                    ranges.push((starts[i], starts[i] + span_char_len(&spans[i]) + span_char_len(&spans[i + 1])));
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    ranges
}

fn is_revealable_content(span: &SyntaxSpan) -> bool {
    matches!(
        span,
        SyntaxSpan::Link { .. }
            | SyntaxSpan::Bold(_)
            | SyntaxSpan::Italic(_)
            | SyntaxSpan::BoldItalic(_)
            | SyntaxSpan::InlineCode(_)
            | SyntaxSpan::Strikethrough(_)
            | SyntaxSpan::Underline(_)
            | SyntaxSpan::Highlight(_)
    )
}

/// Reveal decision for one source line.
///
/// `cursor_char` and `selection` are char offsets *within the line*. A selection reveals
/// every element it touches, and the caret reveals the element it sits in or ends right
/// after. `reveal_all` is the Alt+click escape hatch that shows the whole markup of the
/// line.
pub fn inline_reveal(
    line: &str,
    classified: &LineKind<'_>,
    cursor_char: Option<usize>,
    selection: Option<(usize, usize)>,
    reveal_all: bool,
) -> InlineReveal {
    if reveal_all {
        return InlineReveal::All;
    }
    // Nothing to reveal on this line: skip parsing it at all. This is the common case for
    // every visible row the caret and the selection do not touch.
    let has_selection = selection.map_or(false, |(s, e)| s < e);
    if cursor_char.is_none() && !has_selection {
        return InlineReveal::None;
    }
    // Code and rules have no inline markup worth revealing piecewise.
    if matches!(
        classified,
        LineKind::CodeLine(_) | LineKind::CodeFence { .. } | LineKind::HorizontalRule
    ) {
        return InlineReveal::None;
    }

    let (spans, base) = line_inline_spans(line, classified);
    let elements = inline_element_ranges(&spans);
    if elements.is_empty() {
        return InlineReveal::None;
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if let Some((s, e)) = selection {
        if s < e {
            for &(a, b) in &elements {
                let range = (base + a, base + b);
                if s < range.1 && range.0 < e {
                    ranges.push(range);
                }
            }
        }
    }
    if let Some(c) = cursor_char {
        for &(a, b) in &elements {
            let range = (base + a, base + b);
            // The end boundary counts as inside. A hidden closing marker has no
            // cell, so a click right after the visible text (`word|` in
            // `[word](url)`) lands on the raw char that follows the marker and
            // would otherwise collapse the element the caret is sitting at the
            // end of.
            if range.0 <= c && c <= range.1 && !ranges.contains(&range) {
                ranges.push(range);
            }
        }
    }

    if ranges.is_empty() {
        InlineReveal::None
    } else {
        InlineReveal::Ranges(ranges)
    }
}

/// The part of a byte-range selection that falls inside one line, in line-char
/// offsets. `None` when the selection does not touch the line or is empty.
///
/// Shared by the render loop and the wrap geometry so a selected line is
/// wrapped for exactly the markers the renderer reveals on it.
pub fn selection_chars_in_line(
    line: &str,
    line_start: usize,
    selection: Option<&std::ops::Range<usize>>,
) -> Option<(usize, usize)> {
    let selection = selection?;
    let (start, end) = (selection.start, selection.end);
    if start >= end {
        return None;
    }
    let line_end = line_start + line.len();
    if end <= line_start || start >= line_end {
        return None;
    }
    let to_line_char = |byte: usize| {
        let byte = byte.clamp(line_start, line_end);
        line[..byte - line_start].chars().count()
    };
    Some((to_line_char(start), to_line_char(end)))
}

pub fn parse_inline_syntax_spans(mut s: &str) -> Vec<SyntaxSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Backslash escape: `\` before ASCII punctuation makes that char literal. The
        // backslash is a marker (hidden off the active line, shown muted on it, like the
        // other syntax) and the escaped char is plain text — so `` \` `` renders as a
        // literal backtick and opens no code span, and `\*` keeps its asterisk instead of
        // starting an emphasis span. This branch must stay ahead of every span opener.
        if s.starts_with('\\') {
            if let Some(c) = s[1..].chars().next() {
                if is_escapable_punctuation(c) {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    spans.push(SyntaxSpan::Marker("\\".to_string()));
                    spans.push(SyntaxSpan::Text(c.to_string()));
                    s = &s[1 + c.len_utf8()..];
                    continue;
                }
            }
        }

        // WikiLink: [[target]] or [[target|alias]]
        if s.starts_with("[[") {
            if let Some(close_idx) = s[2..].find("]]") {
                let inner = &s[2..2 + close_idx];
                if !inner.is_empty() && !inner.contains('\n') {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    if let Some((target, alias)) = inner.split_once('|') {
                        let open_marker = format!("[[{}|", target);
                        spans.push(SyntaxSpan::Marker(open_marker));
                        spans.push(SyntaxSpan::Link {
                            text: alias.to_string(),
                            url: target.to_string(),
                        });
                        spans.push(SyntaxSpan::Marker("]]".to_string()));
                    } else {
                        spans.push(SyntaxSpan::Marker("[[".to_string()));
                        spans.push(SyntaxSpan::Link {
                            text: inner.to_string(),
                            url: inner.to_string(),
                        });
                        spans.push(SyntaxSpan::Marker("]]".to_string()));
                    }
                    let total_consumed = 2 + close_idx + 2;
                    s = &s[total_consumed..];
                    continue;
                }
            }
        }

        // Link: [label](url)
        if s.starts_with('[') {
            if let Some(close_bracket) = s[1..].find(']') {
                let after_bracket = &s[1 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        if !plain_accum.is_empty() {
                            spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                        }
                        let label = &s[1..1 + close_bracket];
                        let url = &after_bracket[1..1 + close_paren];
                        spans.push(SyntaxSpan::Marker("[".to_string()));
                        spans.push(SyntaxSpan::Link {
                            text: label.to_string(),
                            url: url.to_string(),
                        });
                        let close_marker = format!("]({})", url);
                        spans.push(SyntaxSpan::Marker(close_marker));
                        let total_consumed = 1 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }

        // HTML Underline tag: <u>...</u>
        if s.starts_with("<u>") {
            if let Some(end_idx) = s[3..].find("</u>") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("<u>".to_string()));
                let content = &s[3..3 + end_idx];
                spans.push(SyntaxSpan::Underline(content.to_string()));
                spans.push(SyntaxSpan::Marker("</u>".to_string()));
                s = &s[3 + end_idx + 4..];
                continue;
            }
        }

        // Highlight: ==...==
        if s.starts_with("==") {
            if let Some(end_idx) = s[2..].find("==") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("==".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Highlight(content.to_string()));
                spans.push(SyntaxSpan::Marker("==".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Inline code: a run of N backticks, closed by the next run of exactly N. The
        // run length is the whole point: `` ``код с `обратными` `` `` is one span holding
        // literal single backticks, and reading it as a lone delimiter produced an empty
        // span plus a wrongly-short one (the content must not be re-scanned either).
        let run = backtick_run_len(s);
        if run > 0 {
            match find_closing_backtick_run(&s[run..], run) {
                Some(content_len) => {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let delimiter = "`".repeat(run);
                    let code_content = &s[run..run + content_len];
                    spans.push(SyntaxSpan::Marker(delimiter.clone()));
                    spans.push(SyntaxSpan::InlineCode(code_content.to_string()));
                    spans.push(SyntaxSpan::Marker(delimiter));
                    s = &s[run + content_len + run..];
                    continue;
                }
                // No run of the same length: the backticks are literal text, and the
                // whole run is consumed so it cannot be re-read as a shorter delimiter.
                None => {
                    plain_accum.push_str(&s[..run]);
                    s = &s[run..];
                    continue;
                }
            }
        }

        // Bold italic: ***...***
        if s.starts_with("***") {
            if let Some(end_idx) = s[3..].find("***") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("***".to_string()));
                let content = &s[3..3 + end_idx];
                spans.push(SyntaxSpan::BoldItalic(content.to_string()));
                spans.push(SyntaxSpan::Marker("***".to_string()));
                s = &s[6 + end_idx..];
                continue;
            }
        }

        // Bold: **...**
        if s.starts_with("**") {
            if let Some(end_idx) = s[2..].find("**") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("**".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Bold(content.to_string()));
                spans.push(SyntaxSpan::Marker("**".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Italic: *...*
        if s.starts_with('*') {
            if let Some(end_idx) = s[1..].find('*') {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("*".to_string()));
                let content = &s[1..1 + end_idx];
                spans.push(SyntaxSpan::Italic(content.to_string()));
                spans.push(SyntaxSpan::Marker("*".to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if s.starts_with("~~") {
            if let Some(end_idx) = s[2..].find("~~") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Strikethrough(content.to_string()));
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Tag: #tag (e.g. #миша, #гончаров, #сергей, #валиев)
        if s.starts_with('#') {
            let prev_ok = plain_accum.chars().last().map_or_else(
                || {
                    spans.last().map_or(true, |span| {
                        let last_char = match span {
                            SyntaxSpan::Text(t)
                            | SyntaxSpan::Bold(t)
                            | SyntaxSpan::Italic(t)
                            | SyntaxSpan::BoldItalic(t)
                            | SyntaxSpan::InlineCode(t)
                            | SyntaxSpan::Strikethrough(t)
                            | SyntaxSpan::Underline(t)
                            | SyntaxSpan::Highlight(t)
                            | SyntaxSpan::Tag(t) => t.chars().last(),
                            SyntaxSpan::Marker(m) => m.chars().last(),
                            SyntaxSpan::Link { text, .. } => text.chars().last(),
                        };
                        last_char.map_or(true, |c| !c.is_alphanumeric() && c != '#' && c != '/' && c != '\\')
                    })
                },
                |c| !c.is_alphanumeric() && c != '#' && c != '/' && c != '\\',
            );
            if prev_ok {
                if let Some(first_char) = s[1..].chars().next() {
                    if first_char.is_alphabetic() || first_char == '_' {
                        let mut tag_len = 1;
                        for c in s[1..].chars() {
                            if c.is_alphanumeric() || c == '_' || c == '-' || c == '/' {
                                tag_len += c.len_utf8();
                            } else {
                                break;
                            }
                        }
                        while tag_len > 1 && (s.as_bytes()[tag_len - 1] == b'-' || s.as_bytes()[tag_len - 1] == b'/') {
                            tag_len -= 1;
                        }
                        if tag_len > 1 {
                            if !plain_accum.is_empty() {
                                spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                            }
                            let tag_str = &s[..tag_len];
                            spans.push(SyntaxSpan::Tag(tag_str.to_string()));
                            s = &s[tag_len..];
                            continue;
                        }
                    }
                }
            }
        }

        // Regular character
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            plain_accum.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    if !plain_accum.is_empty() {
        spans.push(SyntaxSpan::Text(plain_accum));
    }

    spans
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    pub line_idx: usize,
    pub sub_idx: usize,
    pub is_last_sub: bool,
    pub char_start: usize,
    pub char_end: usize,
}

pub fn wrap_line_ranges(line: &str, max_cols: usize) -> Vec<(usize, usize)> {
    let char_count = line.chars().count();
    if char_count == 0 {
        return vec![(0, 0)];
    }
    if max_cols == 0 || char_count <= max_cols {
        return vec![(0, char_count)];
    }

    let chars: Vec<char> = line.chars().collect();
    let mut ranges = Vec::new();
    let mut start = 0;

    while start < char_count {
        let remaining = char_count - start;
        if remaining <= max_cols {
            ranges.push((start, char_count));
            break;
        }

        let limit = start + max_cols;
        // Check if there is whitespace right at limit
        let next_start = if limit < char_count && chars[limit].is_whitespace() {
            limit + 1
        } else {
            // Look backwards for whitespace between start + 1 and limit
            let mut found = None;
            for i in (start + 1..limit).rev() {
                if chars[i].is_whitespace() {
                    found = Some(i + 1);
                    break;
                }
            }
            found.unwrap_or(limit)
        };

        let next_start = next_start.max(start + 1).min(char_count);
        ranges.push((start, next_start));
        start = next_start;
    }

    ranges
}

/// How one line is painted right now: whether its markup is in the active
/// (visible) state and which inline markers the caret/selection reveals.
///
/// The wrap is derived from this, so the two callers that need the same answer
/// — the render loop and the geometry side (`workspace::view_state`) — must
/// build it the same way. [`line_paint_spec`] is the shared constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PaintedLineSpec {
    pub active: bool,
    pub reveal: InlineReveal,
}

/// Painted width of every raw char of a line, plus the indent its continuation
/// rows spend on synthetic padding.
///
/// `char_widths[i]` is the number of painted cells raw char `i` occupies: 1 for
/// a visible char (including a hidden inline-code delimiter, which keeps its
/// cell), 0 for a collapsed marker. A continuation row adds
/// `continuation_cols` cells of padding before its content, and those cells do
/// not belong to any raw char, so they are a per-row constant rather than part
/// of the array.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaintedLineMetrics {
    pub char_widths: Vec<u8>,
    pub continuation_cols: usize,
    /// Raw char where the inline content starts (after the block prefix). A
    /// wrap may break inside the prefix only as a last resort, so this keeps a
    /// hidden or painted `"# "` from filling a row on its own.
    pub content_start: usize,
}

/// Painted state of a line under the caret, the selection or an Alt+click.
pub fn line_paint_spec(
    line: &str,
    classified: &LineKind<'_>,
    cursor_char: Option<usize>,
    selection: Option<(usize, usize)>,
    reveal_all: bool,
) -> PaintedLineSpec {
    let has_selection = selection.map_or(false, |(s, e)| s < e);
    PaintedLineSpec {
        // A selected line is painted with its block markers, exactly like the
        // caret line is.
        active: cursor_char.is_some() || has_selection,
        reveal: inline_reveal(line, classified, cursor_char, selection, reveal_all),
    }
}

/// Columns a wrapped continuation row spends on padding before its content.
///
/// Mirrors the continuation branch of [`row_painted_text`] branch for branch.
pub fn continuation_prefix_cols(line: &str, classified: &LineKind<'_>) -> usize {
    let indent_chars = line.chars().count() - line.trim_start().chars().count();
    match classified {
        LineKind::Quote(_) => {
            let base = quote_prefix(line).map_or(0, |q| q.chars);
            if callout_prefix(line).is_some() {
                base + CALLOUT_ICON_CELLS
            } else {
                base
            }
        }
        LineKind::Task { .. } => {
            let prefix = row_prefix_offset(line, classified);
            line.chars().take(prefix).count()
        }
        LineKind::BulletList { .. } => indent_chars + 2,
        LineKind::NumberedList { num, .. } => indent_chars + num.chars().count() + 2,
        _ => 0,
    }
}

/// Measure the painted width of every raw char of `line`.
///
/// The measurement comes from [`row_painted_text`] itself — the same function
/// the renderer's geometry mirrors — so a wrap derived from it can never
/// disagree with what is painted.
pub fn painted_line_metrics(
    line: &str,
    classified: &LineKind<'_>,
    is_active_line: bool,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
) -> PaintedLineMetrics {
    let char_count = line.chars().count();
    let mut char_widths = vec![0u8; char_count];
    if char_count > 0 {
        let row = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: char_count,
        };
        let painted = row_painted_text(line, classified, &row, is_active_line, reveal, headings);
        for &raw in painted.line_chars.iter().take(painted.columns()) {
            if let Some(w) = char_widths.get_mut(raw) {
                *w = w.saturating_add(1);
            }
        }
    }
    PaintedLineMetrics {
        char_widths,
        continuation_cols: continuation_prefix_cols(line, classified),
        content_start: head_char_count(line, row_prefix_offset(line, classified)),
    }
}

/// Lines the renderer paints as a whole, whatever sub-row it is given: a rule
/// draws one horizontal line and a fence prints its raw text on every row.
/// Wrapping them would paint the text twice, so they always stay one row.
pub fn line_never_wraps(classified: &LineKind<'_>) -> bool {
    matches!(
        classified,
        LineKind::HorizontalRule | LineKind::CodeFence { .. }
    )
}

/// Wrap a line by its **painted** width instead of its raw char count.
///
/// A hidden marker costs no column, so a long URL that is not revealed takes no
/// room and the line wraps where the reader sees the text end — not where the
/// source ends. Ranges are contiguous raw char ranges, which is what
/// [`VisualRow`] and the renderer's slicing expect.
pub fn wrap_line_ranges_painted(
    line: &str,
    metrics: &PaintedLineMetrics,
    max_cols: usize,
) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let char_count = chars.len();
    if char_count == 0 {
        return vec![(0, 0)];
    }
    if max_cols == 0 || metrics.char_widths.len() != char_count {
        return vec![(0, char_count)];
    }

    // Cumulative painted width, so a range's width is one subtraction.
    let mut cum = Vec::with_capacity(char_count + 1);
    cum.push(0usize);
    for &w in &metrics.char_widths {
        cum.push(cum[cum.len() - 1] + w as usize);
    }
    if cum[char_count] <= max_cols {
        return vec![(0, char_count)];
    }

    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut first = true;
    while start < char_count {
        // Continuation rows pay for their synthetic indent out of the same
        // budget; the old raw wrap gave them the full width and let them
        // overflow the right edge.
        let prefix = if first { 0 } else { metrics.continuation_cols };
        let budget = max_cols.saturating_sub(prefix).max(1);
        let base = cum[start];

        // Largest raw boundary whose painted width still fits. Zero-width
        // (hidden) chars extend for free, so a boundary never lands on a
        // marker that paints nothing.
        let mut end = start + 1;
        while end < char_count && cum[end + 1] - base <= budget {
            end += 1;
        }

        if end >= char_count {
            ranges.push((start, char_count));
            break;
        }

        // Prefer a break after whitespace, but only if the row keeps at least
        // one painted content cell: the block prefix ("# ", "> ") must not
        // fill a row by itself while the content is pushed to the next one.
        let content_base = base.max(cum[metrics.content_start.min(char_count)]);
        let mut cut = None;
        for b in (start + 1..=end).rev() {
            if chars[b - 1].is_whitespace() && cum[b] > content_base {
                cut = Some(b);
                break;
            }
        }
        let cut = cut.unwrap_or(end).max(start + 1);
        ranges.push((start, cut));
        start = cut;
        first = false;
    }
    ranges
}

/// Canonical visual rows: every line wrapped as it is painted when no caret,
/// selection or Alt+click reveals anything.
///
/// This is the stable layout preview, Ctrl-selection and idle editing share.
/// Lines under the caret or inside the selection are re-wrapped on top of it
/// (see `EditorWindow::rewrap_line`), so only the lines the reader is actually
/// touching reflow.
pub fn build_visual_rows_canonical(
    lines: &[&str],
    code_states: &[Option<CodeBlockState>],
    word_wrap: bool,
    wrap_cols: usize,
    headings: &crate::settings::HeadingsSettings,
    is_table_row: impl Fn(usize) -> bool,
) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let state = code_states.get(line_idx).and_then(|s| s.as_ref());
        let classified = classify_line(line, state.map(|s| (s.kind, s.lang.as_deref())));
        let ranges = if is_table_row(line_idx)
            || !(word_wrap && wrap_cols > 0)
            || line_never_wraps(&classified)
        {
            vec![(0, line.chars().count())]
        } else {
            let metrics = painted_line_metrics(
                line,
                &classified,
                false,
                &InlineReveal::None,
                headings,
            );
            wrap_line_ranges_painted(line, &metrics, wrap_cols)
        };
        let count = ranges.len();
        for (sub_idx, (char_start, char_end)) in ranges.into_iter().enumerate() {
            visual_rows.push(VisualRow {
                line_idx,
                sub_idx,
                is_last_sub: sub_idx + 1 == count,
                char_start,
                char_end,
            });
        }
    }
    visual_rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRowVisualKind {
    Header,
    Delimiter,
    Body,
    LastRow,
}

pub fn build_visual_rows_with_table_filter(
    lines: &[&str],
    word_wrap: bool,
    wrap_cols: usize,
    is_table_row: impl Fn(usize) -> bool,
) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let ranges = if is_table_row(line_idx) || !(word_wrap && wrap_cols > 0) {
            vec![(0, line.chars().count())]
        } else {
            wrap_line_ranges(line, wrap_cols)
        };
        let count = ranges.len();
        for (sub_idx, (char_start, char_end)) in ranges.into_iter().enumerate() {
            visual_rows.push(VisualRow {
                line_idx,
                sub_idx,
                is_last_sub: sub_idx + 1 == count,
                char_start,
                char_end,
            });
        }
    }
    visual_rows
}

pub fn build_visual_rows(lines: &[&str], word_wrap: bool, wrap_cols: usize) -> Vec<VisualRow> {
    build_visual_rows_with_table_filter(lines, word_wrap, wrap_cols, |_| false)
}

/// Painted boundary column of a raw char offset inside one row.
///
/// Mirrors `RowShapedLayout::column_for_line_char`: a hidden marker shares the
/// column of the cell that follows it, so vertical motion can keep the caret's
/// visual x instead of its raw char offset.
pub fn painted_column_of_line_char(painted: &PaintedRow, raw_char: usize) -> usize {
    painted
        .line_chars
        .partition_point(|&line_char| line_char < raw_char)
        .min(painted.columns())
}

/// Raw line char painted at a boundary column of the row, i.e. the inverse of
/// [`painted_column_of_line_char`].
pub fn line_char_at_painted_column(painted: &PaintedRow, column: usize) -> usize {
    painted.line_chars[column.min(painted.columns())]
}

/// Raw line char a click on `column` of a row must put the caret on.
///
/// `line_chars` maps painted cells to the raw chars they came from (with the
/// row's end as the last entry). A hidden marker run leaves a gap between two
/// painted cells, and taking the next cell's raw char would put the caret past
/// the markup: clicking the end of `[word](url)` would land after the URL and
/// collapse the element. The boundary therefore resolves to the char right
/// after the previous visible one; the row's end stays the row's end so typing
/// there still appends after the whole line.
pub fn line_char_for_painted_boundary(line_chars: &[usize], column: usize) -> usize {
    let Some(&first) = line_chars.first() else {
        return 0;
    };
    let columns = line_chars.len() - 1;
    let column = column.min(columns);
    if column == 0 {
        return first;
    }
    if column == columns {
        return line_chars[columns];
    }
    line_chars[column].min(line_chars[column - 1].saturating_add(1))
}

pub fn find_visual_row_for_cursor(
    visual_rows: &[VisualRow],
    cursor_line: usize,
    cursor_col: usize,
) -> Option<(usize, usize)> {
    if visual_rows.is_empty() {
        return None;
    }
    let start_idx = visual_rows.partition_point(|v| v.line_idx < cursor_line);
    if start_idx < visual_rows.len() && visual_rows[start_idx].line_idx == cursor_line {
        let mut last_sub = (start_idx, 0);
        for v_idx in start_idx..visual_rows.len() {
            let v = &visual_rows[v_idx];
            if v.line_idx != cursor_line {
                break;
            }
            if cursor_col >= v.char_start && (cursor_col < v.char_end || v.is_last_sub) {
                let col_in_row = cursor_col.saturating_sub(v.char_start);
                return Some((v_idx, col_in_row));
            }
            last_sub = (v_idx, cursor_col.saturating_sub(v.char_start));
        }
        return Some(last_sub);
    }
    if start_idx >= visual_rows.len() {
        return visual_rows.last().map(|v| (visual_rows.len() - 1, cursor_col.saturating_sub(v.char_start)));
    }
    visual_rows.first().map(|_| (0, 0))
}

/// Rows per entry in [`VisualRowHeightIndex::bases`].
///
/// The base of a block is the cumulative height of every row before it, so a
/// lookup only has to add up the rows of a single block.
const HEIGHT_BLOCK: usize = 256;

/// Row geometry of a virtualized list, addressed in `f64`.
///
/// A plain `f32` prefix sum cannot describe a huge document: rows are 24 px
/// tall, and past 2^27 logical px (≈5.6M rows, i.e. a ~450 MB note) the f32 grid
/// itself is 16 px wide, so `y + 24` rounds to `y + 32` and two rows end up
/// either 16 or 32 px apart — never the 24 px the rows are actually painted at.
/// That mismatch is what leaves gaps and overlaps between selection rectangles
/// near the end of a very large note. `f64` has the range to hold the same
/// numbers exactly (its grid is 16 *nanometres* at 2^27), and storing a base per
/// block keeps the memory *below* the old per-row prefix: one `f64` per 256 rows
/// against one `f32` per row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualRowHeightIndex {
    heights: Vec<f32>,
    /// bases[b] = exact cumulative height of rows `0..b * HEIGHT_BLOCK`.
    bases: Vec<f64>,
    /// Exact cumulative height of every row.
    total: f64,
}

/// Calculates row height for a heading with font size `h_size` and given `base_line_h`.
/// Headings are aligned to multiples of 4 logical pixels, ensuring that at any standard
/// display scale (100%, 125%, 150%, 175%, 200%, 225%, 250%) their height is an exact
/// integer number of physical device pixels, eliminating subpixel gaps and overlaps.
pub fn heading_row_height(h_size: f32, base_line_h: f32) -> f32 {
    let unrounded = (h_size * 1.35).max(base_line_h);
    ((unrounded / 4.0).ceil() * 4.0).max(base_line_h)
}

/// Height of the visual row(s) of one line: fixed heights for table rows,
/// otherwise the heading height or the plain 24 px line. Used both when
/// building a window from scratch and when updating a single line in place.
pub fn visual_row_height(
    line_str: &str,
    h_settings: &crate::settings::HeadingsSettings,
    table_kind: Option<TableRowVisualKind>,
) -> f32 {
    if let Some(kind) = table_kind {
        return match kind {
            TableRowVisualKind::Delimiter => 1.0,
            TableRowVisualKind::Header | TableRowVisualKind::LastRow => 42.0,
            TableRowVisualKind::Body => 28.0,
        };
    }
    let classified = classify_line(line_str, None);
    let h_size = match classified {
        LineKind::Heading1(_) => h_settings.h1.font_size as f32,
        LineKind::Heading2(_) => h_settings.h2.font_size as f32,
        LineKind::Heading3(_) => h_settings.h3.font_size as f32,
        LineKind::Heading4(_) => h_settings.h4.font_size as f32,
        LineKind::Heading5(_) => h_settings.h5.font_size as f32,
        LineKind::Heading6(_) => h_settings.h6.font_size as f32,
        _ => 14.0,
    };
    if matches!(
        classified,
        LineKind::Heading1(_)
            | LineKind::Heading2(_)
            | LineKind::Heading3(_)
            | LineKind::Heading4(_)
            | LineKind::Heading5(_)
            | LineKind::Heading6(_)
    ) {
        heading_row_height(h_size, 24.0)
    } else {
        24.0
    }
}

impl VisualRowHeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut bases = Vec::with_capacity(heights.len() / HEIGHT_BLOCK + 1);
        let mut running = 0.0f64;
        for (i, &h) in heights.iter().enumerate() {
            if i % HEIGHT_BLOCK == 0 {
                bases.push(running);
            }
            // `f32` heights are exact in `f64`, and the running sum stays far
            // below 2^53, so the bases carry no rounding error at all.
            running += h as f64;
        }
        Self {
            heights,
            bases,
            total: running,
        }
    }

    /// Splices the heights of `old_count` rows starting at `first` and rebuilds
    /// the prefix bases. Pure numeric work — callers pass heights they have
    /// already computed for the replaced rows only.
    pub fn replace_rows(&mut self, first: usize, old_count: usize, new_heights: &[f32]) {
        let end = (first + old_count).min(self.heights.len());
        if first > end {
            return;
        }
        self.heights.splice(first..end, new_heights.iter().copied());
        self.bases.clear();
        let mut running = 0.0f64;
        for (i, &h) in self.heights.iter().enumerate() {
            if i % HEIGHT_BLOCK == 0 {
                self.bases.push(running);
            }
            running += h as f64;
        }
        self.total = running;
    }

    pub fn from_rows_with_tables(
        lines: &[impl AsRef<str>],
        visual_rows: &[VisualRow],
        headings: Option<&crate::settings::HeadingsSettings>,
        table_row_info: impl Fn(usize) -> Option<TableRowVisualKind>,
    ) -> Self {
        let mut heights = Vec::with_capacity(visual_rows.len());
        let default_headings = crate::settings::HeadingsSettings::default();
        let h_settings = headings.unwrap_or(&default_headings);

        for v in visual_rows {
            let line_str = lines.get(v.line_idx).map(|s| s.as_ref()).unwrap_or("");
            heights.push(visual_row_height(line_str, h_settings, table_row_info(v.line_idx)));
        }

        Self::new(heights)
    }

    pub fn from_rows(
        lines: &[impl AsRef<str>],
        visual_rows: &[VisualRow],
        headings: Option<&crate::settings::HeadingsSettings>,
    ) -> Self {
        Self::from_rows_with_tables(lines, visual_rows, headings, |_| None)
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    /// Top edge of row `v_idx`, or the total height when past the last row.
    ///
    /// Exact for any document size: the block base is exact and the local sum is
    /// at most `HEIGHT_BLOCK` heights, i.e. a few thousand pixels.
    pub fn top_of(&self, v_idx: usize) -> f64 {
        if v_idx >= self.heights.len() {
            return self.total;
        }
        let start = (v_idx / HEIGHT_BLOCK) * HEIGHT_BLOCK;
        let mut y = self.bases[v_idx / HEIGHT_BLOCK];
        for &h in &self.heights[start..v_idx] {
            y += h as f64;
        }
        y
    }

    pub fn height_of(&self, v_idx: usize) -> f32 {
        self.heights.get(v_idx).copied().unwrap_or(0.0)
    }

    pub fn v_idx_at_y(&self, y: f64) -> usize {
        if self.heights.is_empty() || y <= 0.0 {
            return 0;
        }
        // The last block whose base is at or above `y`; the row containing `y`
        // cannot be past it, because the next base is the end of that block.
        let block = self
            .bases
            .partition_point(|&base| base <= y)
            .saturating_sub(1)
            .min(self.bases.len() - 1);
        let start = block * HEIGHT_BLOCK;
        let end = (start + HEIGHT_BLOCK).min(self.heights.len());
        let mut bottom = self.bases[block];
        for k in start..end {
            bottom += self.heights[k] as f64;
            if bottom > y {
                return k;
            }
        }
        end - 1
    }

    pub fn visible_range(&self, scroll_top: f64, viewport_h: f64, overscan_px: f64) -> std::ops::Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let top = (scroll_top - overscan_px).max(0.0);
        let bottom = scroll_top + viewport_h + overscan_px;
        let first = self.v_idx_at_y(top);
        let last = (self.v_idx_at_y(bottom) + 1).min(self.heights.len());
        first..last.max(first)
    }

    /// Bytes held by the index. Used by the open-path budget benchmark in `perf.rs`
    /// to show that row geometry costs 4 bytes per row plus a base per block.
    pub fn bytes(&self) -> usize {
        self.heights.capacity() * std::mem::size_of::<f32>()
            + self.bases.capacity() * std::mem::size_of::<f64>()
    }
}

pub fn slice_syntax_spans(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
) -> Vec<SyntaxSpan> {
    slice_syntax_spans_revealed(spans, start_char, end_char, 0, &InlineReveal::All)
        .into_iter()
        .map(|v| v.span)
        .collect()
}

/// Slice `[start_char, end_char)` out of the span sequence, recording for every marker
/// whether `reveal` shows it. `abs_start` is the char offset of the span sequence inside
/// the raw line, so reveal ranges (which are line-relative) line up with the spans.
pub fn slice_syntax_spans_revealed(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
    abs_start: usize,
    reveal: &InlineReveal,
) -> Vec<VisibleSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut curr_char = 0;

    for span in spans {
        let (kind, text, url) = match span {
            SyntaxSpan::Marker(t) => (0, t, None),
            SyntaxSpan::Text(t) => (1, t, None),
            SyntaxSpan::Bold(t) => (2, t, None),
            SyntaxSpan::Italic(t) => (3, t, None),
            SyntaxSpan::BoldItalic(t) => (4, t, None),
            SyntaxSpan::InlineCode(t) => (5, t, None),
            SyntaxSpan::Strikethrough(t) => (6, t, None),
            SyntaxSpan::Link { text, url } => (7, text, Some(url)),
            SyntaxSpan::Underline(t) => (8, t, None),
            SyntaxSpan::Highlight(t) => (9, t, None),
            SyntaxSpan::Tag(t) => (10, t, None),
        };

        let span_len = text.chars().count();
        let span_end = curr_char + span_len;

        if span_end > start_char && curr_char < end_char {
            let s = start_char.saturating_sub(curr_char);
            let e = (end_char - curr_char).min(span_len);
            if s < e {
                let sliced_str: String = text.chars().skip(s).take(e - s).collect();
                let sliced_span = match kind {
                    0 => SyntaxSpan::Marker(sliced_str),
                    1 => SyntaxSpan::Text(sliced_str),
                    2 => SyntaxSpan::Bold(sliced_str),
                    3 => SyntaxSpan::Italic(sliced_str),
                    4 => SyntaxSpan::BoldItalic(sliced_str),
                    5 => SyntaxSpan::InlineCode(sliced_str),
                    6 => SyntaxSpan::Strikethrough(sliced_str),
                    7 => SyntaxSpan::Link {
                        text: sliced_str,
                        url: url.cloned().unwrap_or_default(),
                    },
                    8 => SyntaxSpan::Underline(sliced_str),
                    9 => SyntaxSpan::Highlight(sliced_str),
                    10 => SyntaxSpan::Tag(sliced_str),
                    _ => SyntaxSpan::Text(sliced_str),
                };
                // Only markers ever reach a hidden state; everything else is content.
                // `abs_start + curr_char + s` is the marker's absolute char position.
                let marker_visible = kind != 0
                    || reveal.shows(abs_start + curr_char + s, abs_start + curr_char + e);
                result.push(VisibleSpan {
                    span: sliced_span,
                    marker_visible,
                });
            }
        }

        curr_char = span_end;
        if curr_char >= end_char {
            break;
        }
    }

    result
}

/// Legacy visual-column -> row-char mapping.
///
/// Superseded for geometry: it assumes every column is one monospace cell, so it is
/// wrong for wide glyphs (emoji, CJK) and for the synthetic continuation indent of a
/// wrapped list/quote row. Row geometry now goes through
/// `views::app_view::RowShapedLayout`, which is built from shaped glyph positions.
pub fn visual_col_to_row_char(
    row_spans: &[SyntaxSpan],
    vis_col: usize,
    is_active_line: bool,
    hidden_heading_prefix: usize,
) -> usize {
    if is_active_line {
        return vis_col;
    }
    let mut remaining_vis = vis_col;
    let mut raw_offset = hidden_heading_prefix;

    for span in row_spans {
        let (is_marker, text) = match span {
            SyntaxSpan::Marker(t) => (true, t),
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => (false, t),
            SyntaxSpan::Link { text, .. } => (false, text),
        };
        let span_len = text.chars().count();
        if is_marker {
            raw_offset += span_len;
        } else {
            if remaining_vis <= span_len {
                return raw_offset + remaining_vis;
            }
            remaining_vis -= span_len;
            raw_offset += span_len;
        }
    }
    raw_offset + remaining_vis
}

/// Legacy row-char -> visual-column mapping.
///
/// Superseded for geometry: see `visual_col_to_row_char`.
pub fn row_char_to_visual_col(
    row_spans: &[SyntaxSpan],
    raw_char: usize,
    is_active_line: bool,
    hidden_heading_prefix: usize,
) -> usize {
    if is_active_line {
        return raw_char;
    }
    let mut target_raw = raw_char.saturating_sub(hidden_heading_prefix);
    let mut vis_col = 0;

    for span in row_spans {
        let (is_marker, text) = match span {
            SyntaxSpan::Marker(t) => (true, t),
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => (false, t),
            SyntaxSpan::Link { text, .. } => (false, text),
        };
        let span_len = text.chars().count();
        if is_marker {
            target_raw = target_raw.saturating_sub(span_len);
        } else {
            if target_raw <= span_len {
                return vis_col + target_raw;
            }
            target_raw -= span_len;
            vis_col += span_len;
        }
    }
    vis_col + target_raw
}

/// Byte length of the marker that precedes the content of `line` for the list-like
/// kinds (task box, bullet, number). Zero for everything else.
fn marker_prefix_len(line: &str, classified: &LineKind<'_>) -> usize {
    match classified {
        LineKind::Task { .. } => {
            if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            }
        }
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let bullet_len = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { 2 } else { 0 };
            indent_len + bullet_len
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let num_p_len = num.len() + 2;
            indent_len + if trimmed.len() >= num_p_len { num_p_len } else { 0 }
        }
        _ => 0,
    }
}

/// The leading markers of a block-quote line.
///
/// A level is a `>` optionally followed by the space markdown allows after it,
/// so `> > text` nests two levels and a bare `>` closes a paragraph inside the
/// quote without leaving the quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotePrefix {
    /// Number of `>` levels, at least one.
    pub depth: usize,
    /// Length of the marker prefix in bytes.
    pub bytes: usize,
    /// Length of the marker prefix in chars. This is how many cells the row
    /// spends on the prefix: the markers are hidden but not collapsed, so the
    /// canonical and the edited layout stay on the same grid.
    pub chars: usize,
}

/// The block-quote markers at the head of `line`, or `None` for anything else.
///
/// The scan only matches at the start of the line: an indented `>` is left to
/// the paragraph, exactly like an indented heading marker is.
pub fn quote_prefix(line: &str) -> Option<QuotePrefix> {
    let mut rest = line;
    let mut depth = 0usize;
    while let Some(after) = rest.strip_prefix('>') {
        depth += 1;
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    if depth == 0 {
        return None;
    }
    let bytes = line.len() - rest.len();
    Some(QuotePrefix {
        depth,
        bytes,
        chars: line[..bytes].chars().count(),
    })
}

/// Columns, measured in cells from the start of the row, of the `>` that opens
/// each quote level — the cell each level's bar hangs in.
///
/// `prefix` is the marker prefix of the line (see [`quote_prefix`]). The bars
/// follow the raw markers rather than a fixed step, so `> > >` and `>>>` both
/// get a bar per level and none of them lands on the text.
pub fn quote_bar_columns(prefix: &str) -> Vec<f32> {
    prefix
        .chars()
        .enumerate()
        .filter(|&(_, c)| c == '>')
        .map(|(i, _)| i as f32)
        .collect()
}

/// The family a callout belongs to, which is what its label is coloured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Info,
    Tip,
    Warning,
    Danger,
    Quote,
}

/// The name a callout type is filed under, or `None` for a quote that only looks
/// like one.
///
/// The aliases are the ones Obsidian accepts, so a note written for it keeps its
/// colour here. An unknown type is not a callout: the marker then stays ordinary
/// quoted text, visible, instead of turning a typo into a styled badge.
fn callout_kind(tag: &str) -> Option<CalloutKind> {
    match tag.to_ascii_lowercase().as_str() {
        "note" | "abstract" | "summary" | "tldr" | "example" => Some(CalloutKind::Note),
        "info" | "todo" | "question" | "help" | "faq" => Some(CalloutKind::Info),
        "tip" | "hint" | "important" | "success" | "check" | "done" => Some(CalloutKind::Tip),
        "warning" | "caution" | "attention" => Some(CalloutKind::Warning),
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => Some(CalloutKind::Danger),
        "quote" | "cite" => Some(CalloutKind::Quote),
        _ => None,
    }
}

/// A callout header: `> [!type] title`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutPrefix<'a> {
    pub kind: CalloutKind,
    /// The type exactly as the source spells it, e.g. `note`.
    pub label: &'a str,
    /// Length of the marker region in bytes: the quote prefix, `[!type]` and the
    /// single space markdown allows after it, when present.
    pub bytes: usize,
    /// Length of the marker region in chars — the cells the source spends on the
    /// type and its padding. An untitled header uses them for its type label.
    pub chars: usize,
    /// Everything after the marker region, i.e. the title (empty when there is
    /// none, so an untitled header shows its type instead).
    pub title: &'a str,
}

/// The callout marker at the head of a block quote, or `None` for a plain quote.
///
/// Only a quote at the top level carries a callout. Several `>` are a nested
/// quote first of all: what follows them is quoted text, so a `[!type]` there
/// stays in the source's own form instead of becoming a styled block.
pub fn callout_prefix(line: &str) -> Option<CalloutPrefix<'_>> {
    let quote = quote_prefix(line)?;
    if quote.depth > 1 {
        return None;
    }
    let inner = line[quote.bytes..].strip_prefix("[!")?;
    let close = inner.find(']')?;
    let tag = &inner[..close];
    if tag.is_empty() || !tag.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let kind = callout_kind(tag)?;
    // Past `]`, and past the one space markdown allows between marker and title.
    let marker = quote.bytes + 2 + close + 1;
    let bytes = if line[marker..].starts_with(' ') { marker + 1 } else { marker };
    Some(CalloutPrefix {
        kind,
        label: tag,
        bytes,
        chars: line[..bytes].chars().count(),
        title: &line[bytes..],
    })
}

/// What an idle untitled callout header paints in place of `[!type] `: the type
/// in title case, padded to the cells the marker has left after `leading_cells` —
/// the quote prefix and the icon.
///
/// A titled header hides the type name, so this padding only has to keep the
/// placeholder row as wide as the source marker it replaces.
pub fn callout_label_padded(callout: &CalloutPrefix<'_>, leading_cells: usize) -> String {
    let region = callout.chars.saturating_sub(leading_cells);
    let mut rest = callout.label.chars();
    let label: String = match rest.next() {
        Some(first) => first.to_uppercase().chain(rest).collect(),
        None => String::new(),
    };
    let used = label.chars().count().min(region);
    label
        .chars()
        .take(used)
        .chain(std::iter::repeat(' ').take(region - used))
        .collect()
}

/// The name a callout type is translated under, with Obsidian's aliases folded
/// into the type they stand for.
fn callout_name(tag: &str) -> &'static str {
    match tag.to_ascii_lowercase().as_str() {
        "abstract" | "summary" | "tldr" => "abstract",
        "info" => "info",
        "todo" => "todo",
        "tip" | "hint" | "important" => "tip",
        "success" | "check" | "done" => "success",
        "question" | "help" | "faq" => "question",
        "warning" | "caution" | "attention" => "warning",
        "failure" | "fail" | "missing" => "failure",
        "danger" | "error" => "danger",
        "bug" => "bug",
        "example" => "example",
        "quote" | "cite" => "quote",
        _ => "note",
    }
}

/// The localized name an idle untitled callout header shows, e.g. `Совет` for
/// `[!tip]` in Russian.
///
/// An unknown translation falls back to the source type in title case, so a
/// missing locale key never hides the header's text.
pub fn callout_label(callout: &CalloutPrefix<'_>, lang: &str) -> String {
    let key = format!("editor.callout.{}", callout_name(callout.label));
    let translated = crate::i18n::t(&key, lang);
    if translated != key {
        return translated;
    }
    let mut rest = callout.label.chars();
    match rest.next() {
        Some(first) => first.to_uppercase().chain(rest).collect(),
        None => String::new(),
    }
}


/// Cells an idle callout header spends before its label: the type's icon and the
/// gap that separates it from the text. Measured from the start of the marker
/// region, so the label and the title keep their columns.
pub const CALLOUT_ICON_CELLS: usize = 3;

/// Width of a callout's left bar in logical px.
///
/// It matches the block's corner radius (`rounded_*_md`, 6px): at the block's end
/// the bar's rounded corner then follows the block's own arc across its whole
/// width, where a narrow bar would show a step between its rounded and square
/// halves. A plain quote keeps its thin hairline bar.
pub const CALLOUT_BAR_WIDTH: f32 = 6.0;

/// Colour of a callout's label and title.
///
/// The palette is the Catppuccin set `theme.rs` is built from; `Theme` carries no
/// warning or danger role, and a callout is markdown content rather than chrome,
/// so the colour is resolved here the way tag colours already are.
pub fn callout_color(kind: CalloutKind, is_dark: bool) -> gpui::Rgba {
    gpui::rgb(callout_hex(kind, is_dark))
}

/// Wash behind a callout block: the family colour at a fraction of its opacity,
/// so the block reads as one rectangle while its prose keeps the text colours it
/// already has.
pub fn callout_bg(kind: CalloutKind, is_dark: bool) -> gpui::Rgba {
    let alpha = if is_dark { 0x24 } else { 0x1a };
    gpui::rgba((callout_hex(kind, is_dark) << 8) | alpha)
}

fn callout_hex(kind: CalloutKind, is_dark: bool) -> u32 {
    match (kind, is_dark) {
        (CalloutKind::Note | CalloutKind::Info, true) => 0x89b4fa,
        (CalloutKind::Note | CalloutKind::Info, false) => 0x1e66f5,
        (CalloutKind::Tip, true) => 0xa6e3a1,
        (CalloutKind::Tip, false) => 0x40a02b,
        (CalloutKind::Warning, true) => 0xf9e2af,
        (CalloutKind::Warning, false) => 0xdf8e1d,
        (CalloutKind::Danger, true) => 0xf38ba8,
        (CalloutKind::Danger, false) => 0xd20f39,
        (CalloutKind::Quote, true) => 0x6c7086,
        (CalloutKind::Quote, false) => 0x8c8fa1,
    }
}

/// The callout block a line belongs to, so every row of it can be tinted and
/// only its two ends rounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutBlock {
    pub kind: CalloutKind,
    /// First row of the block: the header.
    pub first: bool,
    /// Last row of the block.
    pub last: bool,
}

/// One entry per line: the callout block covering it, if any.
///
/// A block starts at a header and runs over the block quote under it — the extent
/// Obsidian draws. A second header ends the first block rather than joining it,
/// and `code_states` keeps a `> [!note]` inside a fence from opening one.
pub fn callout_blocks(
    lines: &[String],
    code_states: &[Option<CodeBlockState>],
) -> Vec<Option<CalloutBlock>> {
    let in_code = |idx: usize| code_states.get(idx).map_or(false, |s| s.is_some());
    let mut blocks = vec![None; lines.len()];
    let mut idx = 0;
    while idx < lines.len() {
        let header = if in_code(idx) { None } else { callout_prefix(&lines[idx]) };
        let Some(header) = header else {
            idx += 1;
            continue;
        };
        let mut end = idx;
        while end + 1 < lines.len()
            && !in_code(end + 1)
            && quote_prefix(&lines[end + 1]).is_some()
            && callout_prefix(&lines[end + 1]).is_none()
        {
            end += 1;
        }
        for row in idx..=end {
            blocks[row] = Some(CalloutBlock {
                kind: header.kind,
                first: row == idx,
                last: row == end,
            });
        }
        idx = end + 1;
    }
    blocks
}

/// Byte offset of the first char of a row's content, i.e. the length of the leading
/// bytes of `line` that the renderer paints as a marker instead of as span content.
pub fn row_prefix_offset(line: &str, classified: &LineKind<'_>) -> usize {
    match classified {
        LineKind::Heading1(_) => 2,
        LineKind::Heading2(_) => 3,
        LineKind::Heading3(_) => 4,
        LineKind::Heading4(_) => 5,
        LineKind::Heading5(_) => 6,
        LineKind::Heading6(_) => 7,
        LineKind::Quote(_) => callout_prefix(line)
            .map(|c| c.bytes)
            .or_else(|| quote_prefix(line).map(|q| q.bytes))
            .unwrap_or(0),
        _ => marker_prefix_len(line, classified),
    }
}

/// The dot an idle unordered-list row paints in place of its raw `- `/`* `/`+ `.
///
/// It is the same glyph the app's markdown previews draw (`views::settings_view`,
/// `views::plugin_catalog`), and it advances one cell in every editor font the app
/// ships or offers, so the substitution costs no column and the wrap geometry is
/// unchanged. The line under the caret keeps its raw marker instead, exactly like a
/// heading does, so the source stays editable where the user is working.
pub const RENDERED_BULLET: &str = "• ";

/// Marker text a bullet row paints: the raw source marker while the line is being
/// edited, the rendered dot otherwise.
///
/// The two must keep the same char count — the dot maps onto the raw marker's chars
/// cell for cell, which is what makes swapping it free for the caret grid — so an
/// unexpected marker length falls back to the raw text instead.
pub fn bullet_paint_text<'a>(raw_marker: &'a str, is_active_line: bool) -> &'a str {
    if is_active_line || raw_marker.chars().count() != RENDERED_BULLET.chars().count() {
        raw_marker
    } else {
        RENDERED_BULLET
    }
}

/// Columns, measured in cells from the start of the row, where a nested list item
/// draws its vertical indentation guides.
///
/// Every guide lines up with the centre of the dot of the list level it hangs
/// from, so the line runs straight under the dots instead of standing in front of
/// the next level's dot. `indent_step` is the document's nesting step (see
/// [`list_indent_step`]); an indent that is not a whole number of steps gets one
/// guide per completed step.
pub fn bullet_guide_columns(indent_chars: usize, indent_step: usize) -> Vec<f32> {
    if indent_step == 0 || indent_chars < indent_step {
        return Vec::new();
    }
    (0..indent_chars / indent_step)
        .map(|level| (level * indent_step) as f32 + 0.5)
        .collect()
}

/// Indent of a bullet-list row, in chars. `None` for everything else.
pub fn list_item_indent(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        Some(line.chars().count() - trimmed.chars().count())
    } else {
        None
    }
}

/// Nesting step of the unordered lists in `lines`: the smallest positive indent a
/// bullet row starts with.
///
/// It cannot be read off a single line — four spaces are one level of a four-space
/// document and two levels of a two-space one — so the renderer measures it across
/// the window it is about to paint. Indents below two columns cannot nest a list
/// item (the marker itself is two columns wide) and are ignored. Zero means the
/// window holds no nested bullets at all.
pub fn list_indent_step<'a>(lines: impl IntoIterator<Item = &'a str>) -> usize {
    lines
        .into_iter()
        .filter_map(list_item_indent)
        .filter(|&indent| indent >= 2)
        .min()
        .unwrap_or(0)
}

/// One run of text painted inside a visual row, with the font traits the renderer
/// ends up using for it (the row's base traits merged with the span's own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintedSegment {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// True for inline code and for its delimiters, i.e. for everything the editor
    /// covers with the code chip. The editor paints the chip from these runs (the row
    /// backdrop rectangles in `views::app_view`) instead of from a text-run background, so
    /// that the selection rectangle — which is painted before the text — is not covered by
    /// it, and so that the chip and the selection are cut from the same columns.
    pub code: bool,
    /// Full `#tag` name if this segment is a markdown tag.
    pub tag: Option<String>,
}

/// The exact text a visual row paints, plus the source line char of every painted char.
#[derive(Debug, Clone)]
pub struct PaintedRow {
    pub segments: Vec<PaintedSegment>,
    /// `line_chars[k]` = line char index (chars from the start of the line) painted at
    /// column `k`. One extra trailing entry holds the line char just past the last
    /// painted char, so `line_chars.len() == columns() + 1`.
    ///
    /// Non-decreasing, but not strictly so: synthetic continuation padding repeats the
    /// first char of the wrapped content, and hidden inline markers leave gaps (the one
    /// exception being an inline-code delimiter, which keeps its cell — see
    /// `is_inline_code_delim`).
    pub line_chars: Vec<usize>,
}

/// An inline-code delimiter is a run of one or more backticks — the run length is what
/// closes the span, so `` `` `` is a delimiter in its own right. It is the one inline
/// marker that keeps its cell when it is not painted, so that the code chip has a
/// constant width and the selection rectangle does not jump when the delimiters are
/// revealed around the caret. Both the geometry (`row_painted_text`) and the renderer
/// (`views::app_view::render_syntax_spans`) must agree on this.
pub fn is_inline_code_delim(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b == b'`')
}

impl PaintedRow {
    /// Number of painted character cells in this row.
    pub fn columns(&self) -> usize {
        self.line_chars.len().saturating_sub(1)
    }

    /// Column ranges the row covers with the inline-code chip, merged and in order.
    ///
    /// One painted char owns one column, so the char offset of a segment inside the
    /// painted text is the column it starts at. Adjacent code segments merge, which is
    /// what turns a delimiter and the code it wraps into a single chip.
    pub fn code_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut col = 0usize;
        for segment in &self.segments {
            let n = segment.text.chars().count();
            if segment.code && n > 0 {
                match ranges.last_mut() {
                    Some(last) if last.1 == col => last.1 = col + n,
                    _ => ranges.push((col, col + n)),
                }
            }
            col += n;
        }
        ranges
    }

    /// Column ranges of any `#tag` badges in this row, with their tag names.
    pub fn tag_ranges(&self) -> Vec<(usize, usize, String)> {
        let mut ranges: Vec<(usize, usize, String)> = Vec::new();
        let mut col = 0usize;
        for segment in &self.segments {
            let n = segment.text.chars().count();
            if let Some(ref tag) = segment.tag {
                if n > 0 {
                    ranges.push((col, col + n, tag.clone()));
                }
            }
            col += n;
        }
        ranges
    }
}

fn push_verbatim(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src_start: usize,
    bold: bool,
    italic: bool,
    code: bool,
) {
    push_verbatim_tagged(segments, line_chars, text, src_start, bold, italic, code, None);
}

fn push_verbatim_tagged(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src_start: usize,
    bold: bool,
    italic: bool,
    code: bool,
    tag: Option<String>,
) {
    if text.is_empty() {
        return;
    }
    for i in 0..text.chars().count() {
        line_chars.push(src_start + i);
    }
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code, tag });
}

/// Padding the renderer synthesises for a wrapped row's continuation indent. Every
/// char resolves to `src`, so a click in the padding lands on a real character.
fn push_padding(
    segments: &mut Vec<PaintedSegment>,
    line_chars: &mut Vec<usize>,
    text: &str,
    src: usize,
    bold: bool,
    italic: bool,
) {
    if text.is_empty() {
        return;
    }
    for _ in 0..text.chars().count() {
        line_chars.push(src);
    }
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code: false, tag: None });
}

/// Number of chars in the first `byte_len` bytes of `line` (clamped to a char boundary).
fn head_char_count(line: &str, byte_len: usize) -> usize {
    let mut end = byte_len.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].chars().count()
}

/// The exact text a visual row paints, in paint order, together with the source line
/// char of every painted char.
///
/// This mirrors `render_live_styled_row` (and `render_syntax_spans`) branch for branch.
/// The editor's geometry is derived from it instead of from a `column * char_width`
/// cell model, so wide glyphs (emoji, CJK, VS16/ZWJ sequences) and the continuation
/// indents of wrapped rows are measured where they actually land.
///
/// NOTE: `render_live_styled_row` in `views/app_view.rs` must stay in sync with this.
pub fn row_painted_text(
    line: &str,
    classified: &LineKind<'_>,
    v: &VisualRow,
    is_active_line: bool,
    reveal: &InlineReveal,
    headings: &crate::settings::HeadingsSettings,
) -> PaintedRow {
    let mut segments: Vec<PaintedSegment> = Vec::new();
    let mut line_chars: Vec<usize> = Vec::new();

    // The renderer paints a row with no content as a single blank cell.
    if line.is_empty() || v.char_start >= v.char_end {
        push_padding(&mut segments, &mut line_chars, " ", v.char_end, false, false);
        line_chars.push(v.char_end);
        return PaintedRow { segments, line_chars };
    }

    let prefix_offset = row_prefix_offset(line, classified);

    // Base font traits of the row body: `render_syntax_spans` receives these as the
    // default weight/style for every span it flattens.
    let (base_bold, base_italic) = match classified {
        LineKind::Heading1(_) => (headings.h1.bold, headings.h1.italic),
        LineKind::Heading2(_) => (headings.h2.bold, headings.h2.italic),
        LineKind::Heading3(_) => (headings.h3.bold, headings.h3.italic),
        LineKind::Heading4(_) => (headings.h4.bold, headings.h4.italic),
        LineKind::Heading5(_) => (headings.h5.bold, headings.h5.italic),
        LineKind::Heading6(_) => (headings.h6.bold, headings.h6.italic),
        // A callout header is a label, not quoted prose: the renderer sets it bold and
        // upright, so its cells are measured upright too.
        LineKind::Quote(_) if callout_prefix(line).is_some() => (true, false),
        // Quoted prose is painted upright (`render_live_styled_row` gives its runs
        // the normal face). An italic base here would measure different advances
        // and shift the glyphs, the chip and the caret off their columns.
        LineKind::Quote(_) => (false, false),
        _ => (false, false),
    };

    // 1. Marker prefix, in paint order.
    match classified {
        // A heading marker is only painted on the active line, and only on its first
        // row. The renderer emits the literal marker string, so map it to the leading
        // line chars.
        LineKind::Heading1(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "# ", 0, base_bold, base_italic, false),
        LineKind::Heading2(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "## ", 0, base_bold, base_italic, false),
        LineKind::Heading3(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "### ", 0, base_bold, base_italic, false),
        LineKind::Heading4(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "#### ", 0, base_bold, base_italic, false),
        LineKind::Heading5(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "##### ", 0, base_bold, base_italic, false),
        LineKind::Heading6(_) if is_active_line && v.sub_idx == 0 => push_verbatim(&mut segments, &mut line_chars, "###### ", 0, base_bold, base_italic, false),
        LineKind::Quote(_) => {
            let prefix = quote_prefix(line);
            let callout = callout_prefix(line);
            if v.sub_idx == 0 {
                // The markers stay in the grid whether or not they are shown: the
                // renderer hides them behind one bar per level and reveals the raw
                // text only on the line being edited. Both spellings are exactly
                // `prefix.chars` cells wide, so the wrap does not move.
                match (callout, prefix) {
                    // A callout spends its marker cells on its icon and, when it has
                    // no title of its own, on its type. A titled header is its own
                    // heading: the type name is not repeated, and the title follows
                    // the icon, like any other hidden inline marker.
                    (Some(c), Some(q)) if !is_active_line => {
                        push_verbatim(&mut segments, &mut line_chars, &" ".repeat(q.chars), 0, false, false, false);
                        push_padding(&mut segments, &mut line_chars, &" ".repeat(CALLOUT_ICON_CELLS), q.chars, false, false);
                        if c.title.is_empty() {
                            let label = callout_label_padded(&c, q.chars + CALLOUT_ICON_CELLS);
                            push_verbatim(&mut segments, &mut line_chars, &label, q.chars + CALLOUT_ICON_CELLS, true, false, false);
                        }
                    }
                    // Editing it: the raw marker, muted and upright like a heading's.
                    (Some(c), _) => push_verbatim(&mut segments, &mut line_chars, &line[..c.bytes], 0, false, false, false),
                    // The marker region is painted by its own upright div in the
                    // renderer, outside the italic prose wrapper, so it is measured
                    // upright here too — italic advances differ, and the content after
                    // it would otherwise start a fraction of a cell off the glyphs.
                    (None, Some(q)) if !is_active_line => {
                        push_verbatim(&mut segments, &mut line_chars, &" ".repeat(q.chars), 0, base_bold, false, false)
                    }
                    (None, Some(q)) => push_verbatim(&mut segments, &mut line_chars, &line[..q.bytes], 0, base_bold, false, false),
                    (None, None) => {}
                }
            } else {
                // A wrapped callout header continues under its title, i.e. past the
                // icon column, not under the quote bar alone.
                let icon_cells = if callout.is_some() { CALLOUT_ICON_CELLS } else { 0 };
                let pad = " ".repeat(prefix.map_or(0, |q| q.chars) + icon_cells);
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, base_bold, false);
            }
        }
        LineKind::Task { .. } => {
            if v.sub_idx == 0 {
                let head: String = line.chars().take(prefix_offset).collect();
                push_verbatim(&mut segments, &mut line_chars, &head, 0, true, base_italic, false);
            } else {
                push_padding(&mut segments, &mut line_chars, "      ", v.char_start, true, base_italic);
            }
        }
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let raw_bullet = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { &trimmed[..2] } else { "" };
            let bullet = bullet_paint_text(raw_bullet, is_active_line);
            if v.sub_idx == 0 {
                push_verbatim(&mut segments, &mut line_chars, indent, 0, false, base_italic, false);
                push_verbatim(&mut segments, &mut line_chars, bullet, indent.chars().count(), true, base_italic, false);
            } else {
                let pad = format!("{}  ", indent);
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, false, base_italic);
            }
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            let num_prefix_len = num.len() + 2;
            let num_part = if trimmed.len() >= num_prefix_len { &trimmed[..num_prefix_len] } else { "" };
            if v.sub_idx == 0 {
                push_verbatim(&mut segments, &mut line_chars, indent, 0, false, base_italic, false);
                push_verbatim(&mut segments, &mut line_chars, num_part, indent.chars().count(), true, base_italic, false);
            } else {
                let pad = " ".repeat(indent.chars().count() + num_part.chars().count());
                push_padding(&mut segments, &mut line_chars, &pad, v.char_start, false, base_italic);
            }
        }
        // A collapsed rule or fence paints a rule / a language label instead of the
        // line, but the geometry still follows the raw line so that a click resolves to
        // the character under the cursor.
        LineKind::HorizontalRule | LineKind::CodeFence { .. } => {
            push_verbatim(&mut segments, &mut line_chars, line, 0, false, false, false);
        }
        _ => {}
    }

    // 2. Content: the syntax-span slice the renderer paints for this row.
    match classified {
        // Handled as the prefix above.
        LineKind::HorizontalRule | LineKind::CodeFence { .. } => {}
        // Code rows paint the raw slice, with no inline-marker filtering.
        LineKind::CodeLine(_) => {
            let text: String = line
                .chars()
                .skip(v.char_start)
                .take(v.char_end.saturating_sub(v.char_start))
                .collect();
            push_verbatim(&mut segments, &mut line_chars, &text, v.char_start, base_bold, base_italic, false);
        }
        _ => {
            let (spans, content_offset) = line_inline_spans(line, classified);
            let s = v.char_start.saturating_sub(content_offset);
            let e = v.char_end.saturating_sub(content_offset);
            // The span slice starts at `char_start` chars into the line; skip the block
            // prefix the renderer paints separately.
            let mut src = content_offset + s;
            for visible in slice_syntax_spans_revealed(&spans, s, e, content_offset, reveal) {
                let (text, bold, italic, is_code, tag_opt) = match &visible.span {
                    // A marker is painted by the row's own font (or the code font for a
                    // delimiter), so it has to carry the row's base traits — italic
                    // advances differ from upright ones, and a revealed marker would
                    // otherwise shift the glyphs after it off their measured columns.
                    SyntaxSpan::Marker(t) => (t.as_str(), base_bold, base_italic, is_inline_code_delim(t), None),
                    SyntaxSpan::Text(t) => (t.as_str(), base_bold, base_italic, false, None),
                    SyntaxSpan::Bold(t) => (t.as_str(), true, base_italic, false, None),
                    SyntaxSpan::Italic(t) => (t.as_str(), base_bold, true, false, None),
                    SyntaxSpan::BoldItalic(t) => (t.as_str(), true, true, false, None),
                    SyntaxSpan::InlineCode(t) => (t.as_str(), base_bold, base_italic, true, None),
                    SyntaxSpan::Strikethrough(t)
                    | SyntaxSpan::Underline(t)
                    | SyntaxSpan::Highlight(t) => (t.as_str(), base_bold, base_italic, false, None),
                    SyntaxSpan::Tag(t) => (t.as_str(), base_bold, base_italic, false, Some(t.clone())),
                    SyntaxSpan::Link { text, .. } => (text.as_str(), base_bold, base_italic, false, None),
                };
                let n = text.chars().count();
                if n == 0 {
                    continue;
                }
                // An inline-code delimiter keeps its cell even when hidden, so the code
                // chip has a constant width and the selection rectangle does not jump
                // when the delimiters are revealed. The renderer paints that cell with
                // no ink at all, which is what reads as the chip's left/right padding.
                // Every other hidden marker collapses to nothing and leaves a gap in the
                // line chars.
                if !visible.marker_visible && !is_inline_code_delim(text) {
                    src += n;
                    continue;
                }
                push_verbatim_tagged(&mut segments, &mut line_chars, text, src, bold, italic, is_code, tag_opt);
                src += n;
            }
        }
    }

    if segments.is_empty() {
        // Nothing paintable, e.g. a row made of inline markers only.
        line_chars.clear();
        push_padding(&mut segments, &mut line_chars, " ", v.char_end, false, false);
        line_chars.push(v.char_end);
        return PaintedRow { segments, line_chars };
    }

    let terminal = if v.is_last_sub {
        v.char_end
    } else {
        line_chars.last().map(|&c| c + 1).unwrap_or(v.char_end)
    };
    line_chars.push(terminal);
    PaintedRow { segments, line_chars }
}

/// Cleans raw markdown link URL (removes title, quotes, angle brackets, adds scheme if needed for web links).
pub fn clean_link_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let url_part = if trimmed.starts_with('<') {
        if let Some(end_angle) = trimmed.find('>') {
            &trimmed[1..end_angle]
        } else {
            trimmed.trim_start_matches('<')
        }
    } else if let Some(idx) = trimmed.find(|c: char| c.is_whitespace()) {
        &trimmed[..idx]
    } else {
        trimmed
    };
    let url_part = url_part.trim_matches('"').trim_matches('\'');
    if url_part.is_empty() {
        return String::new();
    }
    let is_local = url_part.starts_with('.')
        || url_part.starts_with('/')
        || url_part.starts_with('\\')
        || url_part.contains('\\')
        || url_part.ends_with(".md")
        || url_part.ends_with(".markdown")
        || (url_part.len() >= 2 && url_part.as_bytes()[1] == b':');

    if !is_local && !url_part.contains("://") && !url_part.starts_with("mailto:") && !url_part.starts_with('#') {
        if url_part.contains('.') {
            return format!("https://{}", url_part);
        }
    }
    url_part.to_string()
}

/// Decodes percent-encoded characters (e.g. `%20` -> ` `, `%D0%9C` -> `М`).
pub fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                if let (Some(v1), Some(v2)) = (hex_val(h1), hex_val(h2)) {
                    bytes.push((v1 << 4) | v2);
                    continue;
                }
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Strips inline markdown markers (bold, italic, strikethrough, inline code, links, highlights, underline),
/// returning the plain visible text.
pub fn strip_inline_markdown(mut s: &str) -> String {
    // 1. Handle image links ![alt](url) -> alt
    let mut intermediate = String::with_capacity(s.len());
    while !s.is_empty() {
        if s.starts_with("![") {
            if let Some(close_bracket) = s[2..].find(']') {
                let after_bracket = &s[2 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        let alt = &s[2..2 + close_bracket];
                        intermediate.push_str(alt);
                        let total_consumed = 2 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            intermediate.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    // 2. Parse inline syntax spans
    let spans = parse_inline_syntax_spans(&intermediate);
    let mut out = String::with_capacity(intermediate.len());
    for span in spans {
        match span {
            SyntaxSpan::Marker(_) => {}
            SyntaxSpan::Text(t)
            | SyntaxSpan::Bold(t)
            | SyntaxSpan::Italic(t)
            | SyntaxSpan::BoldItalic(t)
            | SyntaxSpan::InlineCode(t)
            | SyntaxSpan::Strikethrough(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t)
            | SyntaxSpan::Tag(t) => {
                out.push_str(&t);
            }
            SyntaxSpan::Link { text, .. } => {
                out.push_str(&text);
            }
        }
    }

    // 3. Clean up any unclosed boundary markers from partial selection (e.g. "**word", "word**", "[word", "word](url)")
    let mut cleaned = out;
    if cleaned.contains("[[") && !cleaned.contains("]]") {
        cleaned = cleaned.replace("[[", "");
    }
    if cleaned.contains('[') && !cleaned.contains(']') {
        cleaned = cleaned.replace('[', "");
    }
    if let Some(pos) = cleaned.find("](") {
        if !cleaned[..pos].contains('[') {
            if let Some(end_paren) = cleaned[pos..].find(')') {
                cleaned.replace_range(pos..pos + end_paren + 1, "");
            } else {
                cleaned.truncate(pos);
            }
        }
    }
    let trimmed = cleaned.trim_start_matches(|c| c == '*' || c == '_' || c == '~' || c == '=' || c == '`' || c == '[');
    let trimmed = trimmed.trim_end_matches(|c| c == '*' || c == '_' || c == '~' || c == '=' || c == '`' || c == ']');
    trimmed.to_string()
}

/// Strips markdown tags and formatting, returning plain rendered text as seen in preview mode.
pub fn strip_markdown_tags(input: &str) -> String {
    let mut result_lines = Vec::new();
    let mut in_code_block = false;

    for line in input.lines() {
        let trimmed = line.trim();
        // Check for code fence
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue; // Code fences are hidden in preview mode
        }

        if in_code_block {
            result_lines.push(line.to_string());
            continue;
        }

        // Table separator line like |---|---|
        if crate::table::is_separator(trimmed) {
            continue;
        }

        // Table row like | a | b |
        if (trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1) || crate::table::is_separator(trimmed) {
            let cells = crate::table::split_row(trimmed);
            let stripped_cells: Vec<String> = cells
                .into_iter()
                .map(|c| {
                    let cell_content = &trimmed[c.content.clone()];
                    strip_inline_markdown(cell_content)
                })
                .collect();
            result_lines.push(stripped_cells.join("\t"));
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            continue;
        }

        // Headings: # Heading
        let mut work_line = line;
        let trimmed_start = line.trim_start();
        let leading_spaces = &line[..line.len() - trimmed_start.len()];

        if let Some(rest) = trimmed_start.strip_prefix("###### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("##### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("#### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("### ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("## ") {
            work_line = rest;
        } else if let Some(rest) = trimmed_start.strip_prefix("# ") {
            work_line = rest;
        } else if let Some(c) = callout_prefix(trimmed_start) {
            // A callout header previews as its title, never as its marker.
            work_line = c.title;
        } else if let Some(q) = quote_prefix(trimmed_start) {
            work_line = &trimmed_start[q.bytes..];
        }

        let stripped = strip_inline_markdown(work_line);
        if leading_spaces.is_empty() || work_line != line {
            result_lines.push(stripped);
        } else {
            result_lines.push(format!("{}{}", leading_spaces, stripped));
        }
    }

    let mut res = result_lines.join("\n");
    if input.ends_with('\n') {
        res.push('\n');
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_all_headings() {
        assert_eq!(classify_line("# H1", None), LineKind::Heading1("H1"));
        assert_eq!(classify_line("## H2", None), LineKind::Heading2("H2"));
        assert_eq!(classify_line("### H3", None), LineKind::Heading3("H3"));
        assert_eq!(classify_line("#### H4", None), LineKind::Heading4("H4"));
        assert_eq!(classify_line("##### H5", None), LineKind::Heading5("H5"));
        assert_eq!(classify_line("###### H6", None), LineKind::Heading6("H6"));
        assert_eq!(classify_line("---", None), LineKind::HorizontalRule);
    }

    #[test]
    fn test_parse_link_and_underline_and_highlight() {
        let input = "Обычный абзац с **жирным**, <u>HTML-подчёркиванием</u> и [ссылкой](https://example.com).";
        let spans = parse_inline_syntax_spans(input);

        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Bold(b) if b == "жирным")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Underline(u) if u == "HTML-подчёркиванием")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "ссылкой" && url == "https://example.com")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Marker(m) if m == "](https://example.com)")));
    }

    #[test]
    fn test_strip_inline_markdown_links() {
        let input = "Ссылка с заголовком: [Example](https://example.com \"Example Site\")";
        assert_eq!(strip_inline_markdown(input), "Ссылка с заголовком: Example");

        let partial = "Ссылка с заголовком: [Example";
        assert_eq!(strip_inline_markdown(partial), "Ссылка с заголовком: Example");

        let only_link = "[Example";
        assert_eq!(strip_inline_markdown(only_link), "Example");

        let mid_slice = "Example](https://example.com)";
        assert_eq!(strip_inline_markdown(mid_slice), "Example");
    }

    #[test]
    fn test_clean_link_url() {
        assert_eq!(clean_link_url("https://example.com \"Example Site\""), "https://example.com");
        assert_eq!(clean_link_url("<https://example.com>"), "https://example.com");
        assert_eq!(clean_link_url("https://example.com"), "https://example.com");
        assert_eq!(clean_link_url("example.com"), "https://example.com");
        assert_eq!(clean_link_url("mailto:user@test.com"), "mailto:user@test.com");
        assert_eq!(clean_link_url("../Люди/Михаил%20Валиев.md"), "../Люди/Михаил%20Валиев.md");
        assert_eq!(clean_link_url("<../Люди/Михаил Валиев.md>"), "../Люди/Михаил Валиев.md");
        assert_eq!(clean_link_url("Михаил%20Валиев.md"), "Михаил%20Валиев.md");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("../Люди/Михаил%20Валиев.md"), "../Люди/Михаил Валиев.md");
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("simple_text.md"), "simple_text.md");
        assert_eq!(percent_decode("%D0%9C%D0%B8%D1%88%D0%B0"), "Миша");
    }

    #[test]
    fn test_obsidian_document_link_and_wikilinks() {
        let sentence = "Сегодня решил начать вести дневник в Обсидиан. [Миша](../Люди/Михаил%20Валиев.md) на меня так посмотрел, словно рублём одарил. И сказал: \"О, курва!\".";
        let spans = parse_inline_syntax_spans(sentence);
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Миша" && url == "../Люди/Михаил%20Валиев.md")));

        let stripped = strip_markdown_tags(sentence);
        assert_eq!(stripped, "Сегодня решил начать вести дневник в Обсидиан. Миша на меня так посмотрел, словно рублём одарил. И сказал: \"О, курва!\".");

        // Wikilink without alias
        let wiki1 = "Привет [[Михаил Валиев]]!";
        let spans1 = parse_inline_syntax_spans(wiki1);
        assert!(spans1.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Михаил Валиев" && url == "Михаил Валиев")));
        assert_eq!(strip_markdown_tags(wiki1), "Привет Михаил Валиев!");

        // Wikilink with alias
        let wiki2 = "Привет [[Михаил Валиев|Миша]]!";
        let spans2 = parse_inline_syntax_spans(wiki2);
        assert!(spans2.iter().any(|s| matches!(s, SyntaxSpan::Link { text, url } if text == "Миша" && url == "Михаил Валиев")));
        assert_eq!(strip_markdown_tags(wiki2), "Привет Миша!");
    }

    #[test]
    fn test_visual_col_roundtrip_with_links_and_tags() {
        let line = "A [link](https://example.com) B";
        let spans = parse_inline_syntax_spans(line);
        // В неактивной строке отображается "A link B" (длина: 1 + 1 + 4 + 1 + 1 = 8 символов)
        // Исходная строка: 31 символ
        let raw_char = 2; // Символ 'l' в "link" (в исходной строке после "A [")
        let vis_col = row_char_to_visual_col(&spans, raw_char, false, 0);
        assert_eq!(vis_col, 2); // 'l' в визуальной строке "A link B"

        let recovered_raw = visual_col_to_row_char(&spans, vis_col, false, 0);
        assert_eq!(recovered_raw, raw_char);
    }

    #[test]
    fn test_okabe_ito_colors() {
        assert_eq!(HEADING_COLOR_PRESETS.len(), 8);

        // Verify all Okabe-Ito colors and exact RGB values requested by user
        let orange = find_heading_preset("orange");
        assert_eq!(orange.label, "Оранжевый");
        assert_eq!(orange.rgb, (230, 159, 0));

        let sky_blue = find_heading_preset("sky_blue");
        assert_eq!(sky_blue.label, "Голубой");
        assert_eq!(sky_blue.rgb, (86, 180, 233));

        let bluish_green = find_heading_preset("bluish_green");
        assert_eq!(bluish_green.label, "Сине-зелёный");
        assert_eq!(bluish_green.rgb, (0, 158, 115));

        let yellow = find_heading_preset("yellow");
        assert_eq!(yellow.label, "Жёлтый");
        assert_eq!(yellow.rgb, (240, 228, 66));

        let blue = find_heading_preset("blue");
        assert_eq!(blue.label, "Синий");
        assert_eq!(blue.rgb, (0, 114, 178));

        let vermilion = find_heading_preset("vermilion");
        assert_eq!(vermilion.label, "Алый");
        assert_eq!(vermilion.rgb, (213, 94, 0));

        let reddish_purple = find_heading_preset("reddish_purple");
        assert_eq!(reddish_purple.label, "Розово-фиолетовый");
        assert_eq!(reddish_purple.rgb, (204, 121, 167));

        let red = find_heading_preset("red");
        assert_eq!(red.label, "Красный");
        assert_eq!(red.hex, "#F44336");
        assert_eq!(red.rgb, (244, 67, 54));

        // Backward compatibility mappings
        assert_eq!(find_heading_preset("indigo").id, "blue");
        assert_eq!(find_heading_preset("violet").id, "reddish_purple");
        assert_eq!(find_heading_preset("teal").id, "bluish_green");
        assert_eq!(find_heading_preset("crimson").id, "red");
        assert_eq!(find_heading_preset("#F44336").id, "red");
    }

    #[test]
    fn test_visual_row_height_index_basic() {
        let index = VisualRowHeightIndex::new(vec![24.0, 38.0, 24.0]);
        assert_eq!(index.len(), 3);
        assert_eq!(index.total(), 86.0);
        assert_eq!(index.top_of(0), 0.0);
        assert_eq!(index.top_of(1), 24.0);
        assert_eq!(index.top_of(2), 62.0);
        assert_eq!(index.top_of(3), 86.0);
        assert_eq!(index.height_of(0), 24.0);
        assert_eq!(index.height_of(1), 38.0);
        assert_eq!(index.height_of(2), 24.0);
    }

    #[test]
    fn test_visual_row_height_index_v_idx_at_y() {
        let index = VisualRowHeightIndex::new(vec![24.0, 38.0, 24.0]);
        assert_eq!(index.v_idx_at_y(0.0), 0);
        assert_eq!(index.v_idx_at_y(10.0), 0);
        assert_eq!(index.v_idx_at_y(23.9), 0);
        assert_eq!(index.v_idx_at_y(24.0), 1);
        assert_eq!(index.v_idx_at_y(50.0), 1);
        assert_eq!(index.v_idx_at_y(61.9), 1);
        assert_eq!(index.v_idx_at_y(62.0), 2);
        assert_eq!(index.v_idx_at_y(85.9), 2);
        assert_eq!(index.v_idx_at_y(100.0), 2); // clamped to last row
    }

    #[test]
    fn test_visual_row_height_index_visible_range() {
        let heights = vec![24.0; 100];
        let index = VisualRowHeightIndex::new(heights);
        assert_eq!(index.total(), 2400.0);

        // scroll_top = 240.0 (row 10), viewport_h = 240.0 (10 rows), overscan = 0
        let range = index.visible_range(240.0, 240.0, 0.0);
        assert_eq!(range, 10..21);

        // with overscan of 48.0 (2 rows above and below)
        let range_overscan = index.visible_range(240.0, 240.0, 48.0);
        assert_eq!(range_overscan, 8..23);
    }

    /// Past 2^27 logical px the f32 grid is 16 px wide — coarser than a 24 px
    /// row — so a prefix sum in f32 starts spacing rows 32 px apart there while
    /// the rows are painted 24 px tall. That mismatch is exactly the gaps and
    /// overlaps seen in a 600 MB note, so the index has to stay exact at any
    /// size. 1000 px rows cross the threshold in 134k rows instead of 5.6M,
    /// which keeps the test cheap while exercising the same arithmetic.
    #[test]
    fn test_height_index_stays_exact_past_f32_grid() {
        let rows = 200_000usize;
        let row_h = 1000.0f32;
        let index = VisualRowHeightIndex::new(vec![row_h; rows]);

        assert!(index.total() > 134_217_728.0, "the test must cross 2^27");
        assert_eq!(index.total(), row_h as f64 * rows as f64, "total");

        for k in [rows - 4, rows - 3, rows - 2, rows - 1] {
            assert_eq!(index.top_of(k), row_h as f64 * k as f64, "top of row {k}");
            assert_eq!(
                index.top_of(k + 1) - index.top_of(k),
                row_h as f64,
                "pitch of row {k}"
            );
            assert_eq!(index.v_idx_at_y(index.top_of(k)), k, "lookup of row {k}");
        }

        // The last viewport must still resolve to real rows, not to the tail.
        let last = index.visible_range(index.total(), 800.0, 300.0);
        assert_eq!(last.end, rows);
        assert!(last.start < rows);
    }

    #[test]
    fn test_find_visual_row_for_cursor_large_doc() {
        let visual_rows: Vec<VisualRow> = (0..20_000)
            .map(|i| VisualRow {
                line_idx: i,
                sub_idx: 0,
                is_last_sub: true,
                char_start: 0,
                char_end: 20,
            })
            .collect();

        let (v_idx, col) = find_visual_row_for_cursor(&visual_rows, 15_432, 5).unwrap();
        assert_eq!(v_idx, 15_432);
        assert_eq!(col, 5);
    }

    // --- row_painted_text -------------------------------------------------------
    // These lock down the geometry model: the painted text must be exactly what
    // `render_live_styled_row` draws, and `line_chars` must map every painted column
    // back to the line char it came from.

    fn painted(
        line: &str,
        classified: &LineKind<'_>,
        char_start: usize,
        char_end: usize,
        sub_idx: usize,
        is_active_line: bool,
    ) -> PaintedRow {
        let v = VisualRow {
            line_idx: 0,
            sub_idx,
            is_last_sub: true,
            char_start,
            char_end,
        };
        let reveal = if is_active_line {
            InlineReveal::All
        } else {
            InlineReveal::None
        };
        row_painted_text(
            line,
            classified,
            &v,
            is_active_line,
            &reveal,
            &crate::settings::HeadingsSettings::default(),
        )
    }

    fn text_of(row: &PaintedRow) -> String {
        row.segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn painted_paragraph_is_the_whole_row() {
        let line = "hello";
        let row = painted(line, &classify_line(line, None), 0, 5, 0, false);
        assert_eq!(text_of(&row), "hello");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(row.columns(), 5);
    }

    #[test]
    fn painted_paragraph_drops_hidden_markers() {
        let line = "a**b**c";
        let row = painted(line, &classify_line(line, None), 0, 7, 0, false);
        assert_eq!(text_of(&row), "abc");
        // The two hidden `**` runs still consume line chars, so the columns skip them.
        assert_eq!(row.line_chars, vec![0, 3, 6, 7]);
        assert_eq!(row.columns(), 3);
    }

    #[test]
    fn painted_inline_code_keeps_its_delimiters_when_idle() {
        let line = "a `x` b";
        let kind = classify_line(line, None);

        // The delimiters keep their cells off the active line too, so the code chip has a
        // constant width and the selection rectangle does not jump when the line becomes
        // active. Every other hidden marker still collapses (see the `**` test above).
        let idle = painted(line, &kind, 0, 7, 0, false);
        assert_eq!(text_of(&idle), "a `x` b");
        assert_eq!(idle.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);

        let active = painted(line, &kind, 0, 7, 0, true);
        assert_eq!(text_of(&active), "a `x` b");
        assert_eq!(active.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn inline_code_run_length_matches_the_closing_delimiter() {
        // A double-backtick span holds literal single backticks; a single one inside it
        // must not close it.
        let spans = parse_inline_syntax_spans("a ``b`c`` d");
        let codes: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::InlineCode(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(codes, vec!["b`c"]);
        let markers: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::Marker(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec!["``", "``"]);
    }

    #[test]
    fn an_unmatched_backtick_run_stays_literal() {
        // No run of the same length: the backticks are text, and the lone pair after it
        // still forms a span. The old single-backtick reader turned `` into an empty span.
        let spans = parse_inline_syntax_spans("a `` b `c`");
        let codes: Vec<&str> = spans
            .iter()
            .filter_map(|s| match s {
                SyntaxSpan::InlineCode(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(codes, vec!["c"]);
        let text: String = spans
            .iter()
            .map(|s| match s {
                SyntaxSpan::Text(t) | SyntaxSpan::InlineCode(t) | SyntaxSpan::Marker(t) => t.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(text, "a `` b `c`");
    }

    #[test]
    fn every_backtick_run_is_a_code_delimiter() {
        assert!(is_inline_code_delim("`"));
        assert!(is_inline_code_delim("``"));
        assert!(is_inline_code_delim("```"));
        assert!(!is_inline_code_delim(""));
        assert!(!is_inline_code_delim("~"));
        assert!(!is_inline_code_delim("\\`"));
        assert!(!is_inline_code_delim("``a"));
    }

    #[test]
    fn painted_double_backtick_code_is_one_chip_over_both_delimiters() {
        // The exact line from `markdown-test.md:83`: three spans, the middle one opened
        // and closed by two backticks and holding literal backticks inside.
        let line = "Обычный текст `Встроенный код`, ``код с `обратными` кавычками внутри``, ` пробелы по краям `. Обычный текст";
        let kind = classify_line(line, None);
        let len = line.chars().count();
        let row = painted(line, &kind, 0, len, 0, false);

        // Nothing is dropped, and one painted char owns one column.
        assert_eq!(text_of(&row), line);

        let chars: Vec<char> = text_of(&row).chars().collect();
        let chips: Vec<String> = row
            .code_ranges()
            .iter()
            .map(|&(s, e)| chars[s..e].iter().collect())
            .collect();
        assert_eq!(
            chips,
            vec![
                "`Встроенный код`".to_string(),
                "``код с `обратными` кавычками внутри``".to_string(),
                "` пробелы по краям `".to_string(),
            ]
        );
    }

    #[test]
    fn a_backslash_escape_keeps_the_next_char_literal() {
        // `\`` must not open a code span and `\*` must not open emphasis: the escaped char
        // is plain text, the backslash is a marker.
        let spans = parse_inline_syntax_spans("a \\`не код\\` b");
        assert_eq!(
            spans,
            vec![
                SyntaxSpan::Text("a ".to_string()),
                SyntaxSpan::Marker("\\".to_string()),
                SyntaxSpan::Text("`".to_string()),
                SyntaxSpan::Text("не код".to_string()),
                SyntaxSpan::Marker("\\".to_string()),
                SyntaxSpan::Text("`".to_string()),
                SyntaxSpan::Text(" b".to_string()),
            ]
        );
    }

    #[test]
    fn a_non_punctuation_after_a_backslash_stays_escaped_free() {
        // `\a` is not an escape: the backslash is plain text, not a marker.
        let spans = parse_inline_syntax_spans("a \\b");
        assert_eq!(spans, vec![SyntaxSpan::Text("a \\b".to_string())]);
    }

    #[test]
    fn a_backslash_escaped_backtick_draws_no_chip() {
        let line = "\\`не код\\`";
        let kind = classify_line(line, None);
        let len = line.chars().count();

        // Neither state may produce a chip: the backticks are escaped, so they are text.
        assert!(painted(line, &kind, 0, len, 0, false).code_ranges().is_empty());
        assert!(painted(line, &kind, 0, len, 0, true).code_ranges().is_empty());

        // Idle: the backslashes are hidden, the escaped backticks stay visible as text.
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, false)), "`не код`");
        // Active: the backslashes are painted muted, so the raw syntax stays visible.
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, true)), "\\`не код\\`");
    }

    #[test]
    fn the_escaping_demo_line_opens_no_spans() {
        // `markdown-test.md:91-92` (the two lines are byte-identical).
        let line = "Экранирование: \\*не курсив\\*, \\_не курсив\\_, \\# не заголовок, \\[не ссылка\\], \\`не код\\`, \\\\ обратный слэш, \\> не цитата, \\- не список, \\| не таблица.";
        let spans = parse_inline_syntax_spans(line);
        assert!(
            spans
                .iter()
                .all(|s| matches!(s, SyntaxSpan::Text(_) | SyntaxSpan::Marker(_))),
            "no span may open on escaped punctuation: {spans:?}"
        );

        let kind = classify_line(line, None);
        let len = line.chars().count();
        let idle = painted(line, &kind, 0, len, 0, false);
        assert!(idle.code_ranges().is_empty());
        // Every `\X` collapses to `X` — including `\\` to a single backslash.
        assert_eq!(
            text_of(&idle),
            "Экранирование: *не курсив*, _не курсив_, # не заголовок, [не ссылка], `не код`, \\ обратный слэш, > не цитата, - не список, | не таблица."
        );
    }

    #[test]
    fn painted_paragraph_keeps_markers_on_the_active_line() {
        let line = "a**b**c";
        let row = painted(line, &classify_line(line, None), 0, 7, 0, true);
        assert_eq!(text_of(&row), "a**b**c");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    /// The editor paints the code chip from these column ranges instead of from a
    /// text-run background, so they must cover the delimiters as well as the code —
    /// the delimiters are what reads as the chip's padding.
    #[test]
    fn code_ranges_cover_the_chip_including_its_delimiters() {
        let line = "a `x` b";
        let kind = classify_line(line, None);

        // `a ` + the chip (`` `x` ``) + ` b`.
        assert_eq!(painted(line, &kind, 0, 7, 0, false).code_ranges(), vec![(2, 5)]);
        assert_eq!(painted(line, &kind, 0, 7, 0, true).code_ranges(), vec![(2, 5)]);

        // Two chips stay two ranges, and hidden markers before them shift the columns.
        let two = "a**b** `c` `d`";
        let two_row = painted(two, &classify_line(two, None), 0, two.chars().count(), 0, false);
        assert_eq!(text_of(&two_row), "ab `c` `d`");
        assert_eq!(two_row.code_ranges(), vec![(3, 6), (7, 10)]);

        // Adjacent spans inside one chip merge into a single range.
        let bold_code = "`a`**b**`c`";
        let bold_row = painted(bold_code, &classify_line(bold_code, None), 0, bold_code.chars().count(), 0, false);
        assert_eq!(text_of(&bold_row), "`a`b`c`");
        assert_eq!(bold_row.code_ranges(), vec![(0, 3), (4, 7)]);

        // A heading paints its marker as separate columns, so the chip moves with it.
        let heading = "## Заголовок с `кодом`";
        let heading_kind = classify_line(heading, None);
        let heading_active = painted(heading, &heading_kind, 0, heading.chars().count(), 0, true);
        let prefix = 3; // "## "
        let code_start = "## Заголовок с ".chars().count();
        assert_eq!(heading_active.code_ranges(), vec![(code_start, code_start + 7)]);
        assert!(prefix < code_start);

        // Plain text has no chip at all.
        assert!(painted("hello", &classify_line("hello", None), 0, 5, 0, false).code_ranges().is_empty());
    }

    #[test]
    fn painted_heading_hides_the_marker_until_active() {
        let line = "# Title";
        let kind = classify_line(line, None);

        let idle = painted(line, &kind, 0, 7, 0, false);
        assert_eq!(text_of(&idle), "Title");
        assert_eq!(idle.line_chars, vec![2, 3, 4, 5, 6, 7]);

        let active = painted(line, &kind, 0, 7, 0, true);
        assert_eq!(text_of(&active), "# Title");
        assert_eq!(active.line_chars, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn painted_heading_continuation_has_no_marker() {
        // "# Title": line chars are '#',' ','T','i','t','l','e'.
        let line = "# Title";
        let row = painted(line, &classify_line(line, None), 4, 7, 1, true);
        assert_eq!(text_of(&row), "tle");
        assert_eq!(row.line_chars, vec![4, 5, 6, 7]);
    }

    #[test]
    fn painted_bullet_renders_a_dot_and_reveals_its_marker_when_active() {
        let line = "- item";
        let kind = classify_line(line, None);

        // Idle rows paint the rendered dot, the way the previews do.
        let first = painted(line, &kind, 0, 6, 0, false);
        assert_eq!(text_of(&first), "• item");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4, 5, 6]);

        // The line under the caret keeps its raw marker, so `- ` stays editable.
        let active = painted(line, &kind, 0, 6, 0, true);
        assert_eq!(text_of(&active), "- item");
        assert_eq!(
            active.line_chars, first.line_chars,
            "the dot and the marker must map to the same source chars"
        );

        // A wrapped continuation paints an indent of the same width; its padding
        // resolves to the first char of the wrapped content.
        let wrapped = painted(line, &kind, 4, 6, 1, false);
        assert_eq!(text_of(&wrapped), "  em");
        assert_eq!(wrapped.line_chars, vec![4, 4, 4, 5, 6]);
    }

    #[test]
    fn nested_bullets_paint_a_dot_at_their_own_indent() {
        // The user-facing case: 4-space nesting, mixed markers.
        let cases = [
            ("- Пункт 1", "• Пункт 1"),
            ("    - Вложенный пункт", "    • Вложенный пункт"),
            ("    * Ещё вложенный", "    • Ещё вложенный"),
        ];
        for (line, idle_text) in cases {
            let kind = classify_line(line, None);
            assert!(matches!(kind, LineKind::BulletList { .. }), "{line:?} must classify as a bullet");
            let len = line.chars().count();
            let idle = painted(line, &kind, 0, len, 0, false);
            assert_eq!(text_of(&idle), idle_text);
            let active = painted(line, &kind, 0, len, 0, true);
            assert_eq!(text_of(&active), line, "{line:?} keeps its raw marker when active");
            assert_eq!(
                idle.line_chars, active.line_chars,
                "{line:?}: the dot must not shift the caret grid"
            );
        }
    }

    #[test]
    fn nested_bullet_continuation_lines_align_with_the_item_text() {
        let line = "    - Вложенный пункт списка, который надо перенести подальше";
        let kind = classify_line(line, None);
        let rows = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 12);
        assert!(rows.len() > 1, "the sample must wrap at 12 columns");
        assert!(rows[0].0.starts_with("    • "), "first row: {:?}", rows[0].0);
        for (text, columns) in rows.iter().skip(1) {
            assert!(text.starts_with("      "), "continuation row is offset under the item text: {text:?}");
            assert!(*columns <= 12, "row {text:?} paints {columns} columns");
        }
    }

    #[test]
    fn nested_bullets_get_an_indent_guide_per_level() {
        assert!(bullet_guide_columns(0, 4).is_empty(), "a top-level item has no guide");
        assert!(bullet_guide_columns(4, 0).is_empty(), "no step detected, no guides");
        assert!(bullet_guide_columns(2, 4).is_empty(), "an indent shorter than the step nests nothing");

        // Each guide sits in the centre of the ancestor dot's cell, so the line
        // hangs straight under the dot it belongs to.
        assert_eq!(bullet_guide_columns(4, 4), vec![0.5]);
        assert_eq!(bullet_guide_columns(8, 4), vec![0.5, 4.5]);
        assert_eq!(bullet_guide_columns(12, 4), vec![0.5, 4.5, 8.5]);
        // The same document written with a two-space step gets a guide per level.
        assert_eq!(bullet_guide_columns(2, 2), vec![0.5]);
        assert_eq!(bullet_guide_columns(4, 2), vec![0.5, 2.5]);
        assert_eq!(bullet_guide_columns(8, 2), vec![0.5, 2.5, 4.5, 6.5]);
        // An indent that is not a whole number of steps keeps the completed ones.
        assert_eq!(bullet_guide_columns(7, 4), vec![0.5]);

        for step in 2..=6usize {
            for level in 0..8usize {
                let indent = level * step;
                let cols = bullet_guide_columns(indent, step);
                assert_eq!(cols.len(), level, "indent {indent}, step {step}: {cols:?}");
                assert!(cols.iter().all(|&c| c < indent as f32), "indent {indent}: {cols:?}");
                assert!(cols.windows(2).all(|w| w[0] < w[1]), "indent {indent}: {cols:?}");
            }
        }
    }

    #[test]
    fn list_indent_step_follows_the_smallest_nesting_of_the_window() {
        assert_eq!(list_indent_step([]), 0);
        assert_eq!(list_indent_step(["- a", "# heading", "text"]), 0);
        assert_eq!(list_indent_step(["- a", "    - b", "        - c"]), 4);
        assert_eq!(list_indent_step(["- a", "  - b", "    - c"]), 2);
        assert_eq!(list_indent_step(["- a", "    - b", "  - sloppy"]), 2);
        assert_eq!(list_indent_step(["* a", "+ b", "   - c"]), 3);
        // A one-column indent cannot nest a list item, so it cannot set the step.
        assert_eq!(list_indent_step(["- a", " - b"]), 0);
    }

    #[test]
    fn painted_quote_hides_its_marker_until_the_line_is_edited() {
        let line = "> hi";
        let kind = classify_line(line, None);

        // Idle rows hide the marker but keep its cells, so the text sits where
        // the renderer hangs the quote bar.
        let first = painted(line, &kind, 0, 4, 0, false);
        assert_eq!(text_of(&first), "  hi");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4]);

        // The caret line shows the marker again, in exactly the same cells.
        let active = painted(line, &kind, 0, 4, 0, true);
        assert_eq!(text_of(&active), "> hi");
        assert_eq!(active.line_chars, first.line_chars);

        let wrapped = painted(line, &kind, 3, 4, 1, false);
        assert_eq!(text_of(&wrapped), "  i");
        assert_eq!(wrapped.line_chars, vec![3, 3, 3, 4]);
    }

    #[test]
    fn quote_prefix_counts_every_nesting_level() {
        assert_eq!(quote_prefix("> a").map(|q| (q.depth, q.chars)), Some((1, 2)));
        assert_eq!(quote_prefix("> > a").map(|q| (q.depth, q.chars)), Some((2, 4)));
        assert_eq!(quote_prefix("> > > a").map(|q| (q.depth, q.chars)), Some((3, 6)));
        // Bare `>` closes a paragraph inside the quote; it is still a level.
        assert_eq!(quote_prefix(">").map(|q| (q.depth, q.chars)), Some((1, 1)));
        assert_eq!(quote_prefix("> >").map(|q| (q.depth, q.chars)), Some((2, 3)));
        // `>` opens a level with or without the space markdown allows after it.
        assert_eq!(quote_prefix(">a").map(|q| (q.depth, q.chars)), Some((1, 1)));
        assert_eq!(quote_prefix(""), None);
        assert_eq!(quote_prefix("plain"), None);
        // An indented marker is not a quote at line start.
        assert_eq!(quote_prefix("  > a"), None);
    }

    #[test]
    fn nested_quote_paints_one_marker_pair_per_level() {
        let line = "> > > deep";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("deep"));

        let idle = painted(line, &kind, 0, 10, 0, false);
        assert_eq!(text_of(&idle), "      deep");
        assert_eq!(idle.line_chars, (0..=10).collect::<Vec<_>>());

        let active = painted(line, &kind, 0, 10, 0, true);
        assert_eq!(text_of(&active), "> > > deep");
        assert_eq!(active.line_chars, idle.line_chars);
    }

    #[test]
    fn a_bare_quote_marker_keeps_its_own_row() {
        assert_eq!(classify_line(">", None), LineKind::Quote(""));
        assert_eq!(classify_line("> >", None), LineKind::Quote(""));

        let line = ">";
        let kind = classify_line(line, None);
        let row = painted(line, &kind, 0, 1, 0, false);
        assert_eq!(text_of(&row), " ");
        assert_eq!(row.line_chars, vec![0, 1]);
    }

    #[test]
    fn quote_bars_sit_on_the_level_markers() {
        assert_eq!(quote_bar_columns("> "), vec![0.0]);
        assert_eq!(quote_bar_columns("> > "), vec![0.0, 2.0]);
        assert_eq!(quote_bar_columns("> > > "), vec![0.0, 2.0, 4.0]);
        // Markers written without their spaces stay off the text.
        assert_eq!(quote_bar_columns(">>> "), vec![0.0, 1.0, 2.0]);
        assert_eq!(quote_bar_columns(""), Vec::<f32>::new());
    }

    #[test]
    fn quote_prefix_costs_the_same_whether_hidden_or_revealed() {
        for line in ["> hi", "> > hi", "> > > hi", ">", "> >", "> > >"] {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let idle = painted(line, &kind, 0, len, 0, false);
            let active = painted(line, &kind, 0, len, 0, true);
            assert_eq!(idle.columns(), active.columns(), "{line:?}");
            assert_eq!(idle.line_chars, active.line_chars, "{line:?}");
            assert_eq!(idle.columns(), len, "{line:?}");
        }
    }

    #[test]
    fn quote_continuation_rows_keep_the_nesting_indent() {
        let line = "> > hi";
        let kind = classify_line(line, None);
        let wrapped = painted(line, &kind, 5, 6, 1, false);
        assert_eq!(text_of(&wrapped), "    i");
        assert_eq!(wrapped.line_chars, vec![5, 5, 5, 5, 5, 6]);
    }

    #[test]
    fn the_quotes_section_nests_level_by_level() {
        // One bar per level at the marker it replaces, and the text one marker
        // past the last bar — the shape the sample in the task asks for.
        let cases: &[(&str, &str, &[f32])] = &[
            ("> Обычная цитата.", "  Обычная цитата.", &[0.0]),
            (">", " ", &[0.0]),
            ("> > Вложенная цитата.", "    Вложенная цитата.", &[0.0, 2.0]),
            ("> >", "   ", &[0.0, 2.0]),
            (
                "> > > Ещё более вложенная цитата.",
                "      Ещё более вложенная цитата.",
                &[0.0, 2.0, 4.0],
            ),
        ];
        for &(line, painted_text, bars) in cases {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let row = painted(line, &kind, 0, len, 0, false);
            assert_eq!(text_of(&row), painted_text, "{line:?}");
            let prefix = quote_prefix(line).unwrap();
            assert_eq!(quote_bar_columns(&line[..prefix.bytes]), bars.to_vec(), "{line:?}");
        }
    }

    #[test]
    fn a_callout_marker_inside_a_nested_quote_stays_quote_text() {
        // Several `>` make a nested quote first of all: what follows the markers
        // is quoted text, even when it looks like a callout header, and it is
        // painted exactly as the source spells it.
        let line = "> >[!tip] Познавательная заметка";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("[!tip] Познавательная заметка"));
        assert_eq!(callout_prefix(line), None);
        let quote = quote_prefix(line).expect("a nested quote");
        assert_eq!(quote.depth, 2);
        assert_eq!(quote.chars, 3);
        assert_eq!(quote_bar_columns(&line[..quote.bytes]), vec![0.0, 2.0]);

        let len = line.chars().count();
        let idle = painted(line, &kind, 0, len, 0, false);
        assert_eq!(text_of(&idle), "   [!tip] Познавательная заметка");
        assert_eq!(idle.columns(), len);
        assert_eq!(continuation_prefix_cols(line, &kind), quote.chars);

        // A body line is an ordinary nested quote as well.
        let body = "> > Тело выноски";
        let body_kind = classify_line(body, None);
        assert_eq!(callout_prefix(body), None);
        let body_idle = painted(body, &body_kind, 0, body.chars().count(), 0, false);
        assert_eq!(text_of(&body_idle), "    Тело выноски");
        assert_eq!(quote_prefix(body).map(|q| q.chars), Some(4));
    }

    #[test]
    fn callout_prefix_reads_the_type_and_the_title() {
        let untitled = callout_prefix("> [!note]").expect("a note callout");
        assert_eq!(untitled.kind, CalloutKind::Note);
        assert_eq!(untitled.label, "note");
        assert_eq!(untitled.chars, 9);
        assert_eq!(untitled.title, "");

        let titled = callout_prefix("> [!info] Заголовок выноски").expect("an info callout");
        assert_eq!(titled.kind, CalloutKind::Info);
        assert_eq!(titled.title, "Заголовок выноски");
        // The marker region takes the one space markdown allows after it, so the
        // title starts in the column the source gives it.
        assert_eq!(&"> [!info] Заголовок выноски"[..titled.bytes], "> [!info] ");
        assert_eq!(titled.chars, 10);

        // Obsidian's aliases keep their family.
        assert_eq!(callout_prefix("> [!warning]").unwrap().kind, CalloutKind::Warning);
        assert_eq!(callout_prefix("> [!NOTE]").unwrap().kind, CalloutKind::Note);
        assert_eq!(callout_prefix("> [!danger]").unwrap().kind, CalloutKind::Danger);
        assert_eq!(callout_prefix("> [!tip]").unwrap().kind, CalloutKind::Tip);
        assert_eq!(callout_prefix("> [!quote]").unwrap().kind, CalloutKind::Quote);

        // Not callouts: a plain quote, an unknown type, an empty one, a marker that
        // is not in a quote.
        assert_eq!(callout_prefix("> plain"), None);
        assert_eq!(callout_prefix("> [!nope] title"), None);
        assert_eq!(callout_prefix("> []"), None);
        assert_eq!(callout_prefix("[!note]"), None);
    }

    #[test]
    fn painted_callout_header_shows_its_type_where_the_marker_was() {
        // An untitled callout takes its label as its title: the marker cells become
        // the type's icon, `Note` and padding, exactly as wide as the source marker.
        let line = "> [!note]";
        let kind = classify_line(line, None);
        assert_eq!(kind, LineKind::Quote("[!note]"));

        let idle = painted(line, &kind, 0, 9, 0, false);
        assert_eq!(text_of(&idle), "     Note");
        assert_eq!(idle.line_chars, vec![0, 1, 2, 2, 2, 5, 6, 7, 8, 9]);
        assert_eq!(idle.columns(), line.chars().count());

        let active = painted(line, &kind, 0, 9, 0, true);
        assert_eq!(text_of(&active), "> [!note]");
        // The icon spends two cells on the source's `[!`, so the cell-to-char map
        // differs; only the column count has to survive the reveal.
        assert_eq!(active.columns(), idle.columns());
    }

    #[test]
    fn a_titled_callout_hides_the_type_and_starts_the_title_after_the_icon() {
        let line = "> [!info] Заголовок выноски";
        let kind = classify_line(line, None);
        let len = line.chars().count();

        // The marker word is gone: only the quote indent and the icon column
        // stand between the bar and the title.
        let idle = painted(line, &kind, 0, len, 0, false);
        let title_chars = callout_prefix(line).expect("an info callout").title.chars().count();
        assert_eq!(text_of(&idle), "     Заголовок выноски");
        assert_eq!(idle.columns(), 2 + CALLOUT_ICON_CELLS + title_chars);

        // Editing the line reveals the source marker, as it does around any other
        // hidden inline marker.
        let active = painted(line, &kind, 0, len, 0, true);
        assert_eq!(text_of(&active), line);
        assert_eq!(active.columns(), len);

        // A wrapped header continues under the title, past the icon column.
        assert_eq!(continuation_prefix_cols(line, &kind), 2 + CALLOUT_ICON_CELLS);
        let wrapped = painted(line, &kind, 10, len, 1, false);
        assert_eq!(text_of(&wrapped), "     Заголовок выноски");
        assert_eq!(wrapped.columns(), 2 + CALLOUT_ICON_CELLS + len - 10);
    }

    #[test]
    fn an_untitled_callout_shows_its_localized_type() {
        let line = "> [!tip]";
        let kind = classify_line(line, None);
        let callout = callout_prefix(line).expect("a tip callout");

        // The painted placeholder is language-independent; the renderer's label
        // comes from the locale.
        let len = line.chars().count();
        assert_eq!(text_of(&painted(line, &kind, 0, len, 0, false)), "     Tip");

        assert_eq!(callout_label(&callout, "ru"), "Совет");
        assert_eq!(callout_label(&callout, "en"), "Tip");
        // Obsidian's aliases are translated under the type they stand for.
        let abstract_callout = callout_prefix("> [!summary]").expect("an abstract alias");
        assert_eq!(callout_label(&abstract_callout, "ru"), "Аннотация");
        // An unrecognised code falls back to the app's default locale.
        assert_eq!(callout_label(&callout, "xx"), "Совет");
    }

    #[test]
    fn a_callout_header_hands_the_title_to_the_inline_spans() {
        // The label is block syntax; the title is content, so inline formatting in
        // it parses as usual.
        let line = "> [!info] **Заголовок**";
        let (spans, offset) = line_inline_spans(line, &classify_line(line, None));
        assert_eq!(offset, 10);
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Bold(_))));

        let (_, offset) = line_inline_spans("> hi", &classify_line("> hi", None));
        assert_eq!(offset, 2);
    }

    #[test]
    fn callout_label_is_the_type_in_title_case() {
        let leading = 2 + CALLOUT_ICON_CELLS;
        let untitled = callout_prefix("> [!note]").unwrap();
        // What the marker has left after the quote and the icon column: `[!note]`
        // leaves four cells, and `Note` takes exactly those.
        assert_eq!(callout_label_padded(&untitled, leading), "Note");

        let titled = callout_prefix("> [!info] Заголовок выноски").unwrap();
        // The region includes the space after the marker, so five cells remain.
        assert_eq!(callout_label_padded(&titled, leading), "Info ");
    }

    #[test]
    fn callout_colours_separate_the_families_in_both_themes() {
        let kinds = [
            CalloutKind::Note,
            CalloutKind::Tip,
            CalloutKind::Warning,
            CalloutKind::Danger,
        ];
        for &is_dark in &[true, false] {
            for (i, a) in kinds.iter().enumerate() {
                for b in &kinds[i + 1..] {
                    assert_ne!(
                        callout_color(*a, is_dark),
                        callout_color(*b, is_dark),
                        "{a:?} and {b:?} share a colour (dark: {is_dark})"
                    );
                }
            }
        }
        assert_ne!(
            callout_color(CalloutKind::Note, true),
            callout_color(CalloutKind::Note, false)
        );
    }

    #[test]
    fn a_callout_block_covers_its_quote_and_marks_both_ends() {
        let lines: Vec<String> = [
            "> [!note] Title",
            "> first body line",
            ">",
            "> last body line",
            "",
            "after",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let states = vec![None; lines.len()];
        let blocks = callout_blocks(&lines, &states);

        let first = blocks[0].expect("the header opens a block");
        assert_eq!(first.kind, CalloutKind::Note);
        assert!(first.first);
        assert!(!first.last);
        assert_eq!(blocks[1], Some(CalloutBlock { kind: CalloutKind::Note, first: false, last: false }));
        assert_eq!(blocks[2], Some(CalloutBlock { kind: CalloutKind::Note, first: false, last: false }));
        let last = blocks[3].expect("the last quoted row closes the block");
        assert_eq!(last.kind, CalloutKind::Note);
        assert!(!last.first);
        assert!(last.last);
        assert_eq!(blocks[4], None, "a blank line ends the quote");
        assert_eq!(blocks[5], None);
    }

    #[test]
    fn a_single_row_callout_is_both_ends() {
        let lines = vec!["> [!tip]".to_string()];
        let blocks = callout_blocks(&lines, &[None]);
        assert_eq!(blocks[0], Some(CalloutBlock { kind: CalloutKind::Tip, first: true, last: true }));
    }

    #[test]
    fn a_second_header_opens_its_own_block() {
        let lines: Vec<String> = [
            "> [!note] one",
            "> body",
            "> [!warning] two",
            "> body",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&lines, &vec![None; lines.len()]);
        assert_eq!(blocks[0].unwrap().kind, CalloutKind::Note);
        assert!(blocks[1].unwrap().last, "the next header ends the first block");
        assert_eq!(blocks[2].unwrap().kind, CalloutKind::Warning);
        assert!(blocks[2].unwrap().first);
        assert!(blocks[3].unwrap().last);
    }

    #[test]
    fn a_nested_quote_opens_no_callout_block() {
        let lines: Vec<String> = [
            "> >[!tip] Познавательная заметка",
            "> > тело цитаты",
            "> [!note] верхняя выноска",
            "> тело выноски",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&lines, &vec![None; lines.len()]);

        assert_eq!(blocks[0], None, "several `>` are a nested quote, not a callout");
        assert_eq!(blocks[1], None);
        assert_eq!(blocks[2].unwrap().kind, CalloutKind::Note);
        assert!(blocks[2].unwrap().first);
        assert!(blocks[3].unwrap().last);

        // Deeper rows stay in an enclosing top-level callout's block.
        let nested_inside: Vec<String> = [
            "> [!tip]",
            "> > вложенная цитата внутри выноски",
            "> обычное тело",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let blocks = callout_blocks(&nested_inside, &vec![None; nested_inside.len()]);
        assert!(blocks.iter().all(Option::is_some));
    }

    #[test]
    fn a_fenced_callout_marker_opens_no_block() {
        let lines: Vec<String> = [
            "```",
            "> [!note]",
            "```",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let states = analyze_code_blocks(&lines.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        assert!(states.iter().all(|s| s.is_some()));
        let blocks = callout_blocks(&lines, &states);
        assert!(blocks.iter().all(Option::is_none));
    }

    #[test]
    fn painted_code_line_keeps_raw_markers() {
        // Code rows paint the raw slice, so `**` stays visible even on an idle line.
        let line = "**x**";
        let row = painted(line, &LineKind::CodeLine(line), 0, 5, 0, false);
        assert_eq!(text_of(&row), "**x**");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn painted_empty_line_is_one_blank_cell() {
        let row = painted("", &classify_line("", None), 0, 0, 0, false);
        assert_eq!(text_of(&row), " ");
        assert_eq!(row.line_chars, vec![0, 0]);
        assert_eq!(row.columns(), 1);
    }

    #[test]
    fn painted_rule_follows_the_raw_line() {
        // A collapsed rule paints no text, but the geometry must still follow the line
        // so that a click resolves to a character.
        let line = "---";
        let row = painted(line, &LineKind::HorizontalRule, 0, 3, 0, false);
        assert_eq!(text_of(&row), "---");
        assert_eq!(row.line_chars, vec![0, 1, 2, 3]);
    }

    #[test]
    fn painted_row_columns_always_match_line_chars() {
        let cases: &[(&str, bool)] = &[
            ("plain text", false),
            ("plain text", true),
            ("**bold** and _italic_", false),
            ("**bold** and _italic_", true),
            ("# Heading with emoji 😀 tail", false),
            ("# Heading with emoji 😀 tail", true),
            ("- bullet with 🚀 emoji", false),
            ("> quote with ⚠️ emoji", false),
            ("- [ ] task with 🏳️‍🌈 flag", false),
            ("1. numbered", false),
        ];
        for &(line, active) in cases {
            let kind = classify_line(line, None);
            let len = line.chars().count();
            let row = painted(line, &kind, 0, len, 0, active);
            assert_eq!(
                row.line_chars.len(),
                row.columns() + 1,
                "line_chars/columns mismatch for {line:?} (active={active})"
            );
            assert_eq!(
                text_of(&row).chars().count(),
                row.columns(),
                "painted text length mismatch for {line:?} (active={active})"
            );
            // Monotone non-decreasing, so the reverse lookup can binary search.
            assert!(
                row.line_chars.windows(2).all(|w| w[0] <= w[1]),
                "line_chars is not monotone for {line:?} (active={active})"
            );
        }
    }

    #[test]
    fn test_heading_row_height_multiples_of_four_and_integer_pixels() {
        let scales = [1.0f32, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];
        // Standard heading sizes from default settings: h1=22, h2=19, h3=17, h4=15, h5=14, h6=13
        for h_size in [22.0f32, 19.0, 17.0, 15.0, 14.0, 13.0] {
            let row_h = heading_row_height(h_size, 24.0);
            assert_eq!(
                row_h % 4.0,
                0.0,
                "heading row_h {row_h} for h_size {h_size} must be a multiple of 4"
            );
            assert!(
                row_h >= 24.0,
                "heading row_h {row_h} must be at least base_line_h"
            );
            for scale in scales {
                let physical = row_h * scale;
                assert_eq!(
                    physical,
                    physical.round(),
                    "physical pixels {physical} must be an exact integer at scale {scale}"
                );
            }
        }
    }

    #[test]
    fn test_strip_markdown_tags() {
        assert_eq!(strip_markdown_tags("# Header 1"), "Header 1");
        assert_eq!(strip_markdown_tags("### Subheader"), "Subheader");
        assert_eq!(strip_markdown_tags("Text with **bold** and *italic* and `code`"), "Text with bold and italic and code");
        assert_eq!(strip_markdown_tags("Link: [Click](https://example.com) and ==highlight=="), "Link: Click and highlight");
        assert_eq!(strip_markdown_tags("<u>Underlined</u> text"), "Underlined text");
        assert_eq!(strip_markdown_tags("> Quote line"), "Quote line");
        assert_eq!(strip_markdown_tags("> > Nested quote line"), "Nested quote line");
        assert_eq!(strip_markdown_tags("> > > Deep quote line"), "Deep quote line");
        // A callout header previews as its title, not as its marker.
        assert_eq!(strip_markdown_tags("> [!info] **Title**"), "Title");

        let code = "```rust\nlet x = 10;\nprintln!(\"{x}\");\n```";
        assert_eq!(strip_markdown_tags(code), "let x = 10;\nprintln!(\"{x}\");");

        let table = "| Col 1 | Col 2 |\n|---|---|\n| **Val 1** | *Val 2* |";
        assert_eq!(strip_markdown_tags(table), "Col 1\tCol 2\nVal 1\tVal 2");

        // Partial selection cleanup
        assert_eq!(strip_markdown_tags("**Partial bold"), "Partial bold");
        assert_eq!(strip_markdown_tags("Partial bold**"), "Partial bold");
        assert_eq!(strip_markdown_tags("`partial code"), "partial code");
    }

    #[test]
    fn test_parse_tags() {
        let text = "Сегодня я встретил нескольких старых знакомых: #гончаров, #сергей, #валиев и #миша.";
        let spans = parse_inline_syntax_spans(text);
        let tags: Vec<String> = spans
            .into_iter()
            .filter_map(|s| match s {
                SyntaxSpan::Tag(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tags, vec!["#гончаров", "#сергей", "#валиев", "#миша"]);

        // Not tags: # followed by space (headings), or mid-word foo#bar
        let non_tags = parse_inline_syntax_spans("foo#bar # and #123");
        let non_tags_found: Vec<String> = non_tags
            .into_iter()
            .filter_map(|s| match s {
                SyntaxSpan::Tag(t) => Some(t),
                _ => None,
            })
            .collect();
        assert!(non_tags_found.is_empty(), "Expected no tags, found: {:?}", non_tags_found);
    }

    #[test]
    fn test_tag_colors() {
        // Determinism: leading '#' and case do not alter color
        let (bg1, fg1) = tag_color_pair("#Rust", false);
        let (bg2, fg2) = tag_color_pair("rust", false);
        assert_eq!(bg1, bg2);
        assert_eq!(fg1, fg2);

        // Different tags map to the palette deterministically
        let (bg_dark, fg_dark) = tag_color_pair("#project/work", true);
        assert!(TAG_PALETTE_DARK_HEX.iter().any(|&(b, f)| gpui::rgba(b) == bg_dark && gpui::rgba(f) == fg_dark));

        let words = ["миша", "гончаров", "сергей", "валиев", "todo", "obsidian", "dev", "notes"];
        let mut distinct = std::collections::HashSet::new();
        for w in words {
            let c = tag_color_pair(w, false).0;
            distinct.insert((c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()));
        }
        assert!(distinct.len() >= 5, "Expected good distribution across catalog, got {}", distinct.len());
    }

    #[test]
    fn test_painted_row_tag_ranges() {
        let line = "Привет #миша и #гончаров!";
        let char_count = line.chars().count();
        let painted_row = painted(line, &classify_line(line, None), 0, char_count, 0, false);
        let ranges = painted_row.tag_ranges();
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].2, "#миша");
        assert_eq!(ranges[1].2, "#гончаров");

        let full_text: String = painted_row.segments.iter().map(|s| s.text.as_str()).collect();
        let chars: Vec<char> = full_text.chars().collect();
        let s0 = ranges[0].0;
        let e0 = ranges[0].1;
        let s1 = ranges[1].0;
        let e1 = ranges[1].1;
        let tag0_str: String = chars[s0..e0].iter().collect();
        let tag1_str: String = chars[s1..e1].iter().collect();
        assert_eq!(tag0_str, "#миша");
        assert_eq!(tag1_str, "#гончаров");
    }

    #[test]
    fn inline_element_ranges_cover_markers_and_content() {
        let spans = parse_inline_syntax_spans("[a](u) **b** `c`");
        assert_eq!(inline_element_ranges(&spans), vec![(0, 6), (7, 12), (13, 16)]);
    }

    #[test]
    fn reveal_only_the_element_under_the_caret() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);

        let caret_in_link = inline_reveal(line, &kind, Some(5), None, false);
        assert!(caret_in_link.shows(2, 3), "link opener must be visible");
        assert!(!caret_in_link.shows(11, 13), "bold markers must stay hidden");

        assert_eq!(
            inline_reveal(line, &kind, Some(1), None, false),
            InlineReveal::None,
            "a caret in plain text reveals nothing"
        );
        assert_eq!(
            inline_reveal(line, &kind, Some(5), None, true),
            InlineReveal::All,
            "Alt+click reveals the whole line"
        );
    }

    #[test]
    fn caret_at_the_visible_end_of_an_element_reveals_it() {
        // The closing "](url)" is hidden, so a click right after "abc" maps to
        // the raw char after it: the element's end boundary. It has to count as
        // inside, or the element collapses under the caret exactly where the
        // text ends.
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let element_end = "x [abc](https://example.com)".chars().count();

        assert_eq!(
            inline_reveal(line, &kind, Some(element_end), None, false),
            InlineReveal::Ranges(vec![(2, element_end)])
        );

        // One char further is plain text and hides the markup again.
        assert_eq!(
            inline_reveal(line, &kind, Some(element_end + 1), None, false),
            InlineReveal::None
        );
    }

    #[test]
    fn selection_reveals_every_touched_element() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);
        let reveal = inline_reveal(line, &kind, None, Some((7, 12)), false);
        assert!(reveal.shows(2, 3), "the selection starts inside the link");
        assert!(reveal.shows(11, 13), "the selection reaches the bold markers");
    }

    #[test]
    fn reveal_offsets_include_the_block_prefix() {
        let line = "# x [a](u)";
        let kind = classify_line(line, None);
        let reveal = inline_reveal(line, &kind, Some(5), None, false);
        // The link's `[` sits at line char 4 (after the "# " block prefix), not 2.
        assert!(reveal.shows(4, 5));

        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        let row = row_painted_text(line, &kind, &v, true, &reveal, &headings);
        assert_eq!(text_of(&row), "# x [a](u)");
    }

    #[test]
    fn reveal_controls_painted_markers() {
        let line = "x [a](u) y **b**";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };

        let hidden = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        assert_eq!(text_of(&hidden), "x a y b");

        let link = row_painted_text(
            line,
            &kind,
            &v,
            false,
            &inline_reveal(line, &kind, Some(5), None, false),
            &headings,
        );
        assert_eq!(text_of(&link), "x [a](u) y b");

        let all = row_painted_text(line, &kind, &v, false, &InlineReveal::All, &headings);
        assert_eq!(text_of(&all), "x [a](u) y **b**");
    }

    /// Wrap `line` by painted width and return what each row actually paints.
    /// Going through `row_painted_text` is the point: the test would fail if the
    /// wrap and the renderer's geometry ever disagreed.
    fn painted_wrap_rows(
        line: &str,
        kind: &LineKind<'_>,
        active: bool,
        reveal: &InlineReveal,
        cols: usize,
    ) -> Vec<(String, usize)> {
        let headings = crate::settings::HeadingsSettings::default();
        let metrics = painted_line_metrics(line, kind, active, reveal, &headings);
        let ranges = wrap_line_ranges_painted(line, &metrics, cols);
        let count = ranges.len();
        ranges
            .iter()
            .enumerate()
            .map(|(sub_idx, &(char_start, char_end))| {
                let v = VisualRow {
                    line_idx: 0,
                    sub_idx,
                    is_last_sub: sub_idx + 1 == count,
                    char_start,
                    char_end,
                };
                let row = row_painted_text(line, kind, &v, active, reveal, &headings);
                let text: String = row.segments.iter().map(|s| s.text.as_str()).collect();
                (text, row.columns())
            })
            .collect()
    }

    #[test]
    fn hidden_url_takes_no_columns_when_wrapping() {
        let line = "слово [ссылка](https://example.com/очень/длинный/путь) конец";
        let kind = classify_line(line, None);

        let canonical = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 30);
        assert_eq!(canonical.len(), 1, "the hidden URL must not force a second row");
        assert_eq!(canonical[0].0, "слово ссылка конец");

        // The same line with the link revealed pays for the URL and wraps.
        let revealed = painted_wrap_rows(line, &kind, false, &InlineReveal::All, 30);
        assert!(revealed.len() > 1);
    }

    #[test]
    fn painted_columns_skip_hidden_markers() {
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let char_count = line.chars().count();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: char_count,
        };
        let painted = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);

        // "x abc y": the "[" and the whole "(url)" take no cell.
        assert_eq!(painted.columns(), 7);

        // Raw chars: 0 'x', 1 ' ', 2 '[', 3 'a'.
        assert_eq!(painted_column_of_line_char(&painted, 3), 2);
        // A hidden marker shares the painted column of the text after it, so
        // vertical motion does not drift on the caret line.
        assert_eq!(painted_column_of_line_char(&painted, 2), 2);
        // Round trip to the raw char the caret must land on.
        assert_eq!(line_char_at_painted_column(&painted, 2), 3);
        assert_eq!(
            line_char_at_painted_column(&painted, painted.columns()),
            char_count
        );
    }

    #[test]
    fn click_at_the_end_of_a_word_lands_inside_the_element() {
        let line = "x [abc](https://example.com) y";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };
        let painted = row_painted_text(line, &kind, &v, false, &InlineReveal::None, &headings);
        // Painted cells: "x abc y".
        assert_eq!(painted.columns(), 7);

        // The boundary after "abc" must not jump over the hidden "](url)":
        // the caret has to stay inside the link source, or the element
        // collapses exactly where its visible text ends.
        let after_word = line_char_for_painted_boundary(&painted.line_chars, 5);
        assert_eq!(after_word, 6, "the caret must stay inside the link source");
        assert!(matches!(
            inline_reveal(line, &kind, Some(after_word), None, false),
            InlineReveal::Ranges(_)
        ));

        // The start of the word is inside the element as well.
        let before_word = line_char_for_painted_boundary(&painted.line_chars, 2);
        assert_eq!(before_word, 2);
        assert!(matches!(
            inline_reveal(line, &kind, Some(before_word), None, false),
            InlineReveal::Ranges(_)
        ));

        // The row's end stays the row's end, so typing there appends after the
        // whole line instead of inside the markup.
        assert_eq!(
            line_char_for_painted_boundary(&painted.line_chars, 7),
            line.chars().count()
        );
    }

    #[test]
    fn heading_marker_is_paid_for_only_when_active() {
        let line = "# abcdefghij";
        let kind = classify_line(line, None);

        let canonical = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 10);
        assert_eq!(canonical.len(), 1, "the hidden \"# \" costs no column");
        assert_eq!(canonical[0].0, "abcdefghij");

        let active = painted_wrap_rows(line, &kind, true, &InlineReveal::None, 10);
        assert_eq!(active.len(), 2, "the visible \"# \" takes two columns");
        assert_eq!(active[0].0, "# abcdefgh");
    }

    #[test]
    fn inline_code_keeps_its_cells_and_emphasis_does_not() {
        let line = "**a** `b` c";
        let kind = classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let metrics = painted_line_metrics(line, &kind, false, &InlineReveal::None, &headings);

        let widths = &metrics.char_widths;
        // `**a** `b` c`: the bold markers collapse, the backticks keep a cell.
        assert_eq!(&widths[0..2], &[0, 0], "bold opener is collapsed");
        assert_eq!(widths[2], 1, "'a' is painted");
        assert_eq!(&widths[3..5], &[0, 0], "bold closer is collapsed");
        assert_eq!(widths[6], 1, "the inline-code opener keeps its cell");
        assert_eq!(widths[7], 1, "'b' is painted");
        assert_eq!(widths[8], 1, "the inline-code closer keeps its cell");
        assert_eq!(
            widths.iter().sum::<u8>(),
            7,
            "only the four emphasis markers cost nothing"
        );
    }

    #[test]
    fn continuation_rows_pay_for_their_indent() {
        let line = "- aaaa bbbb cccc dddd eeee";
        let kind = classify_line(line, None);
        let rows = painted_wrap_rows(line, &kind, false, &InlineReveal::None, 10);
        assert!(rows.len() > 1);
        for (i, (text, columns)) in rows.iter().enumerate() {
            assert!(
                *columns <= 10,
                "row {i} paints {columns} columns ({text:?}), over the 10-column budget"
            );
        }
        assert!(rows[0].0.starts_with("• "), "the first row carries the bullet");
        assert!(rows[1].0.starts_with("  "), "continuation rows are indented");
    }

    #[test]
    fn painted_ranges_cover_the_line_contiguously() {
        let lines = [
            "обычный абзац без разметки, который надо перенести",
            "[ссылка](https://example.com/путь) и **жирный** текст",
            "# Заголовок с [ссылкой](u)",
            "- пункт списка с `кодом` и *курсивом*",
            "> цитата с очень длинной строкой, которая точно не поместится",
            "```rust",
            "    отступ и ещё немного текста для переноса",
        ];
        let headings = crate::settings::HeadingsSettings::default();
        for line in lines {
            let kind = classify_line(line, None);
            let char_count = line.chars().count();
            for cols in [1usize, 5, 10, 40] {
                for (active, reveal) in [
                    (false, InlineReveal::None),
                    (true, InlineReveal::All),
                ] {
                    let metrics =
                        painted_line_metrics(line, &kind, active, &reveal, &headings);
                    let ranges = wrap_line_ranges_painted(line, &metrics, cols);
                    assert_eq!(ranges[0].0, 0, "{line:?} cols={cols}");
                    assert_eq!(
                        ranges.last().unwrap().1,
                        char_count,
                        "{line:?} cols={cols}"
                    );
                    for pair in ranges.windows(2) {
                        assert_eq!(
                            pair[0].1, pair[1].0,
                            "ranges must be contiguous for {line:?} cols={cols}"
                        );
                    }
                    for &(s, e) in &ranges {
                        assert!(e > s, "empty range in {line:?} cols={cols}");
                    }
                }
            }
        }
    }

    #[test]
    fn paragraph_rows_never_exceed_the_budget() {
        let line = "раз два три четыре пять шесть семь восемь девять десять";
        let kind = classify_line(line, None);
        for cols in [5usize, 9, 14, 27] {
            for (text, columns) in painted_wrap_rows(line, &kind, false, &InlineReveal::None, cols) {
                assert!(
                    columns <= cols,
                    "row {text:?} paints {columns} columns over budget {cols}"
                );
            }
        }
    }
}



