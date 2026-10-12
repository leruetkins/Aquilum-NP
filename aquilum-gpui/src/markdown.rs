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

pub fn analyze_code_blocks(lines: &[&str]) -> Vec<Option<CodeBlockState>> {
    let mut in_code_block = false;
    let mut current_block_id = 0;
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_block {
                result.push(Some(CodeBlockState {
                    kind: CodeBlockKind::ClosingFence,
                    lang: None,
                    block_id: current_block_id,
                }));
                in_code_block = false;
                current_block_id += 1;
            } else {
                let lang = trimmed.trim_start_matches('`').trim();
                let lang_opt = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                result.push(Some(CodeBlockState {
                    kind: CodeBlockKind::OpeningFence,
                    lang: lang_opt,
                    block_id: current_block_id,
                }));
                in_code_block = true;
            }
        } else if in_code_block {
            result.push(Some(CodeBlockState {
                kind: CodeBlockKind::InsideCode,
                lang: None,
                block_id: current_block_id,
            }));
        } else {
            result.push(None);
        }
    }

    result
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
    } else if let Some(rest) = trimmed_start.strip_prefix("> ") {
        LineKind::Quote(rest)
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

pub fn parse_inline_spans(mut s: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Inline code: `...`
        if s.starts_with('`') {
            if let Some(end_idx) = s[1..].find('`') {
                if !plain_accum.is_empty() {
                    spans.push(InlineSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let code_content = &s[1..1 + end_idx];
                spans.push(InlineSpan::InlineCode(code_content.to_string()));
                s = &s[2 + end_idx..];
                continue;
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
    LinkText(String),
    Underline(String),
    Highlight(String),
}

pub fn parse_inline_syntax_spans(mut s: &str) -> Vec<SyntaxSpan> {
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
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
                        spans.push(SyntaxSpan::LinkText(label.to_string()));
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

        // Inline code: `...`
        if s.starts_with('`') {
            if let Some(end_idx) = s[1..].find('`') {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("`".to_string()));
                let code_content = &s[1..1 + end_idx];
                spans.push(SyntaxSpan::InlineCode(code_content.to_string()));
                spans.push(SyntaxSpan::Marker("`".to_string()));
                s = &s[2 + end_idx..];
                continue;
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

pub fn build_visual_rows(lines: &[&str], word_wrap: bool, wrap_cols: usize) -> Vec<VisualRow> {
    let mut visual_rows = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let ranges = if word_wrap && wrap_cols > 0 {
            wrap_line_ranges(line, wrap_cols)
        } else {
            vec![(0, line.chars().count())]
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualRowHeightIndex {
    heights: Vec<f32>,
    /// prefix[i] = cumulative sum of heights for rows 0..i, length = heights.len() + 1
    prefix: Vec<f32>,
}

impl VisualRowHeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut prefix = Vec::with_capacity(heights.len() + 1);
        prefix.push(0.0);
        let mut running = 0.0;
        for &h in &heights {
            running += h;
            prefix.push(running);
        }
        Self { heights, prefix }
    }

    pub fn from_rows(
        lines: &[impl AsRef<str>],
        visual_rows: &[VisualRow],
        headings: Option<&crate::settings::HeadingsSettings>,
    ) -> Self {
        let mut heights = Vec::with_capacity(visual_rows.len());
        let default_headings = crate::settings::HeadingsSettings::default();
        let h_settings = headings.unwrap_or(&default_headings);

        for v in visual_rows {
            let line_str = lines.get(v.line_idx).map(|s| s.as_ref()).unwrap_or("");
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
            let row_h = if matches!(
                classified,
                LineKind::Heading1(_)
                    | LineKind::Heading2(_)
                    | LineKind::Heading3(_)
                    | LineKind::Heading4(_)
                    | LineKind::Heading5(_)
                    | LineKind::Heading6(_)
            ) {
                (h_size * 1.35).round().max(24.0)
            } else {
                24.0
            };
            heights.push(row_h);
        }

        Self::new(heights)
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn total(&self) -> f32 {
        *self.prefix.last().unwrap_or(&0.0)
    }

    pub fn top_of(&self, v_idx: usize) -> f32 {
        if v_idx < self.prefix.len() {
            self.prefix[v_idx]
        } else {
            self.total()
        }
    }

    pub fn height_of(&self, v_idx: usize) -> f32 {
        self.heights.get(v_idx).copied().unwrap_or(0.0)
    }

    pub fn v_idx_at_y(&self, y: f32) -> usize {
        if self.heights.is_empty() || y <= 0.0 {
            return 0;
        }
        let i = self.prefix.partition_point(|&p| p <= y);
        i.saturating_sub(1).min(self.heights.len().saturating_sub(1))
    }

    pub fn visible_range(&self, scroll_top: f32, viewport_h: f32, overscan_px: f32) -> std::ops::Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let top = (scroll_top - overscan_px).max(0.0);
        let bottom = scroll_top + viewport_h + overscan_px;
        let first = self.v_idx_at_y(top);
        let last = (self.v_idx_at_y(bottom) + 1).min(self.heights.len());
        first..last.max(first)
    }
}

pub fn slice_syntax_spans(
    spans: &[SyntaxSpan],
    start_char: usize,
    end_char: usize,
) -> Vec<SyntaxSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut curr_char = 0;

    for span in spans {
        let (kind, text) = match span {
            SyntaxSpan::Marker(t) => (0, t),
            SyntaxSpan::Text(t) => (1, t),
            SyntaxSpan::Bold(t) => (2, t),
            SyntaxSpan::Italic(t) => (3, t),
            SyntaxSpan::BoldItalic(t) => (4, t),
            SyntaxSpan::InlineCode(t) => (5, t),
            SyntaxSpan::Strikethrough(t) => (6, t),
            SyntaxSpan::LinkText(t) => (7, t),
            SyntaxSpan::Underline(t) => (8, t),
            SyntaxSpan::Highlight(t) => (9, t),
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
                    7 => SyntaxSpan::LinkText(sliced_str),
                    8 => SyntaxSpan::Underline(sliced_str),
                    9 => SyntaxSpan::Highlight(sliced_str),
                    _ => SyntaxSpan::Text(sliced_str),
                };
                result.push(sliced_span);
            }
        }

        curr_char = span_end;
        if curr_char >= end_char {
            break;
        }
    }

    result
}

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
            | SyntaxSpan::LinkText(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t) => (false, t),
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
            | SyntaxSpan::LinkText(t)
            | SyntaxSpan::Underline(t)
            | SyntaxSpan::Highlight(t) => (false, t),
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

pub fn row_spans_and_hidden_prefix(
    line: &str,
    classified: &LineKind<'_>,
    v: &VisualRow,
    is_active_line: bool,
) -> (Vec<SyntaxSpan>, usize) {
    let (text_to_parse, prefix_offset, hidden_heading) = match classified {
        LineKind::Heading1(_) => (line.strip_prefix("# ").unwrap_or(line), 2, if !is_active_line && v.sub_idx == 0 { 2 } else { 0 }),
        LineKind::Heading2(_) => (line.strip_prefix("## ").unwrap_or(line), 3, if !is_active_line && v.sub_idx == 0 { 3 } else { 0 }),
        LineKind::Heading3(_) => (line.strip_prefix("### ").unwrap_or(line), 4, if !is_active_line && v.sub_idx == 0 { 4 } else { 0 }),
        LineKind::Heading4(_) => (line.strip_prefix("#### ").unwrap_or(line), 5, if !is_active_line && v.sub_idx == 0 { 5 } else { 0 }),
        LineKind::Heading5(_) => (line.strip_prefix("##### ").unwrap_or(line), 6, if !is_active_line && v.sub_idx == 0 { 6 } else { 0 }),
        LineKind::Heading6(_) => (line.strip_prefix("###### ").unwrap_or(line), 7, if !is_active_line && v.sub_idx == 0 { 7 } else { 0 }),
        LineKind::Quote(_) => (line.strip_prefix("> ").unwrap_or(line), 2, 0),
        LineKind::Task { .. } => {
            let p_len = if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            };
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let bullet_len = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { 2 } else { 0 };
            let p_len = indent_len + bullet_len;
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let num_p_len = num.len() + 2;
            let p_len = indent_len + if trimmed.len() >= num_p_len { num_p_len } else { 0 };
            (if line.len() >= p_len { &line[p_len..] } else { "" }, p_len, 0)
        }
        _ => (line, 0, 0),
    };
    let spans = parse_inline_syntax_spans(text_to_parse);
    let s = v.char_start.saturating_sub(prefix_offset);
    let e = v.char_end.saturating_sub(prefix_offset);
    let sliced = slice_syntax_spans(&spans, s, e);
    (sliced, hidden_heading)
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
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::LinkText(l) if l == "ссылкой")));
        assert!(spans.iter().any(|s| matches!(s, SyntaxSpan::Marker(m) if m == "](https://example.com)")));
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
}


