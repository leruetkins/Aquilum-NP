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
        LineKind::Quote(_) => 2,
        _ => marker_prefix_len(line, classified),
    }
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
        LineKind::Task { .. } | LineKind::BulletList { .. } | LineKind::NumberedList { .. } => {
            let p_len = marker_prefix_len(line, classified);
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
/// constant width and the selection rectangle does not jump when the delimiters become
/// visible on the active line. Both the geometry (`row_painted_text`) and the renderer
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
    if text.is_empty() {
        return;
    }
    for i in 0..text.chars().count() {
        line_chars.push(src_start + i);
    }
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code });
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
    segments.push(PaintedSegment { text: text.to_string(), bold, italic, code: false });
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
        LineKind::Quote(_) => (false, true),
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
            if v.sub_idx == 0 {
                push_verbatim(&mut segments, &mut line_chars, "> ", 0, base_bold, base_italic, false);
            } else {
                push_padding(&mut segments, &mut line_chars, "  ", v.char_start, base_bold, base_italic);
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
            let bullet = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") { &trimmed[..2] } else { "" };
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
            let (row_spans, _) = row_spans_and_hidden_prefix(line, classified, v, is_active_line);
            // The span slice starts at `char_start - prefix_offset` chars into the text
            // that follows the marker, i.e. at this line char.
            let mut src = head_char_count(line, prefix_offset) + v.char_start.saturating_sub(prefix_offset);
            for span in &row_spans {
                let (text, bold, italic, is_marker, is_code) = match span {
                    SyntaxSpan::Marker(t) => (t.as_str(), false, false, true, is_inline_code_delim(t)),
                    SyntaxSpan::Text(t) => (t.as_str(), base_bold, base_italic, false, false),
                    SyntaxSpan::Bold(t) => (t.as_str(), true, base_italic, false, false),
                    SyntaxSpan::Italic(t) => (t.as_str(), base_bold, true, false, false),
                    SyntaxSpan::BoldItalic(t) => (t.as_str(), true, true, false, false),
                    SyntaxSpan::InlineCode(t) => (t.as_str(), base_bold, base_italic, false, true),
                    SyntaxSpan::Strikethrough(t)
                    | SyntaxSpan::LinkText(t)
                    | SyntaxSpan::Underline(t)
                    | SyntaxSpan::Highlight(t) => (t.as_str(), base_bold, base_italic, false, false),
                };
                let n = text.chars().count();
                if n == 0 {
                    continue;
                }
                // An inline-code delimiter keeps its cell on an idle line, so the code
                // chip has a constant width and the selection rectangle does not jump
                // when the delimiters appear on the active line. The renderer paints
                // that cell with no ink at all, which is what reads as the chip's
                // left/right padding. Every other hidden marker collapses to nothing
                // and leaves a gap in the line chars.
                if is_marker && !is_active_line && !is_inline_code_delim(text) {
                    src += n;
                    continue;
                }
                push_verbatim(&mut segments, &mut line_chars, text, src, bold, italic, is_code);
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

    let terminal = line_chars.last().map(|&c| c + 1).unwrap_or(v.char_end);
    line_chars.push(terminal);
    PaintedRow { segments, line_chars }
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
        row_painted_text(
            line,
            classified,
            &v,
            is_active_line,
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
    fn painted_bullet_keeps_its_marker_on_every_row() {
        let line = "- item";
        let kind = classify_line(line, None);

        let first = painted(line, &kind, 0, 6, 0, false);
        assert_eq!(text_of(&first), "- item");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4, 5, 6]);

        // A wrapped continuation paints an indent of the same width; its padding
        // resolves to the first char of the wrapped content.
        let wrapped = painted(line, &kind, 4, 6, 1, false);
        assert_eq!(text_of(&wrapped), "  em");
        assert_eq!(wrapped.line_chars, vec![4, 4, 4, 5, 6]);
    }

    #[test]
    fn painted_quote_keeps_its_marker() {
        let line = "> hi";
        let kind = classify_line(line, None);

        let first = painted(line, &kind, 0, 4, 0, false);
        assert_eq!(text_of(&first), "> hi");
        assert_eq!(first.line_chars, vec![0, 1, 2, 3, 4]);

        let wrapped = painted(line, &kind, 3, 4, 1, false);
        assert_eq!(text_of(&wrapped), "  i");
        assert_eq!(wrapped.line_chars, vec![3, 3, 3, 4]);
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
}


