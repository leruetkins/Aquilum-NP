use super::*;

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
    /// School-grammar sentence member: `::type{line/text}[text]`. `text` is the raw
    /// content, escapes included, so its char count stays equal to the source count
    /// and every span/column offset keeps pointing at the right source char.
    /// `content` is that same text parsed into the inline markup a member may carry;
    /// see [`parse_grammar_content`] for the kinds that are allowed inside.
    Grammar {
        kind: GrammarKind,
        line_color: Option<GrammarColor>,
        text_color: Option<GrammarColor>,
        text: String,
        content: Vec<VisibleSpan>,
    },
}

/// Sentence members of Russian school parsing, selected by the tag letter.
///
/// An unknown letter is not a grammar tag: the whole construct is left as plain
/// text (see [`parse_inline_syntax_spans`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrammarKind {
    Subject,
    Predicate,
    Definition,
    Object,
    Adverbial,
    Address,
    Introductory,
}

impl GrammarKind {
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "s" => Some(Self::Subject),
            "p" => Some(Self::Predicate),
            "d" => Some(Self::Definition),
            "o" => Some(Self::Object),
            "a" => Some(Self::Adverbial),
            "v" => Some(Self::Address),
            "i" => Some(Self::Introductory),
            _ => None,
        }
    }

    /// The underline a member gets in school parsing.
    pub fn line(self) -> GrammarLine {
        match self {
            Self::Subject => GrammarLine::Solid,
            Self::Predicate => GrammarLine::Double,
            Self::Definition => GrammarLine::Wavy,
            Self::Object => GrammarLine::Dotted,
            Self::Adverbial => GrammarLine::DashDot,
            Self::Address | Self::Introductory => GrammarLine::None,
        }
    }
}

/// How the line under a grammar span is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrammarLine {
    None,
    Solid,
    Double,
    Wavy,
    Dotted,
    DashDot,
}

/// A colour written inside a grammar tag, as `(r, g, b)`.
pub type GrammarColor = (u8, u8, u8);

/// Painted attributes of one grammar span. `None` colours inherit the text colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GrammarSpanStyle {
    pub line: GrammarLine,
    pub line_color: Option<GrammarColor>,
    pub text_color: Option<GrammarColor>,
}

impl GrammarSpanStyle {
    /// Text colour resolved against the row's own painted colour.
    pub fn text_rgba(self, inherited: gpui::Rgba) -> gpui::Rgba {
        self.text_color.map(grammar_color_to_rgba).unwrap_or(inherited)
    }

    /// Line colour resolved against the resolved text colour.
    pub fn line_rgba(self, inherited: gpui::Rgba) -> gpui::Rgba {
        self.line_color
            .map(grammar_color_to_rgba)
            .unwrap_or_else(|| self.text_rgba(inherited))
    }
}

pub fn grammar_color_to_rgba(color: GrammarColor) -> gpui::Rgba {
    gpui::rgb(((color.0 as u32) << 16) | ((color.1 as u32) << 8) | (color.2 as u32))
}

fn rgba_to_grammar_color(color: gpui::Rgba) -> GrammarColor {
    let channel = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(color.r), channel(color.g), channel(color.b))
}

/// Parses one colour of a grammar tag: `#2a7`, `#22aa77`, `rgb(34, 170, 119)` or a
/// CSS colour name. An unrecognised colour is treated as unset, so it simply
/// inherits the text colour instead of breaking the tag.
pub fn parse_grammar_color(raw: &str) -> Option<GrammarColor> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('#') {
        return parse_hex_color(raw).map(rgba_to_grammar_color);
    }
    let lower = raw.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgb(")
        .or_else(|| lower.strip_prefix("rgba("))
    {
        let inner = inner.strip_suffix(')')?;
        let mut parts = inner.split(',').map(str::trim);
        let r = parts.next()?.parse::<u8>().ok()?;
        let g = parts.next()?.parse::<u8>().ok()?;
        let b = parts.next()?.parse::<u8>().ok()?;
        return Some((r, g, b));
    }
    css_color_name(&lower)
}

fn css_color_name(name: &str) -> Option<GrammarColor> {
    Some(match name {
        "black" => (0, 0, 0),
        "silver" => (192, 192, 192),
        "gray" | "grey" => (128, 128, 128),
        "white" => (255, 255, 255),
        "maroon" => (128, 0, 0),
        "red" => (255, 0, 0),
        "purple" => (128, 0, 128),
        "fuchsia" | "magenta" => (255, 0, 255),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "olive" => (128, 128, 0),
        "yellow" => (255, 255, 0),
        "navy" => (0, 0, 128),
        "blue" => (0, 0, 255),
        "teal" => (0, 128, 128),
        "aqua" | "cyan" => (0, 255, 255),
        "orange" => (255, 165, 0),
        "pink" => (255, 192, 203),
        "brown" => (165, 42, 42),
        "gold" => (255, 215, 0),
        "indigo" => (75, 0, 130),
        "violet" => (238, 130, 238),
        "crimson" => (220, 20, 60),
        "salmon" => (250, 128, 114),
        "tomato" => (255, 99, 71),
        "turquoise" => (64, 224, 208),
        "beige" => (245, 245, 220),
        "khaki" => (240, 230, 140),
        "lavender" => (230, 230, 250),
        "plum" => (221, 160, 221),
        "orchid" => (218, 112, 214),
        "coral" => (255, 127, 80),
        "chocolate" => (210, 105, 30),
        "tan" => (210, 180, 140),
        "wheat" => (245, 222, 179),
        "ivory" => (255, 255, 240),
        "azure" => (240, 255, 255),
        _ => return None,
    })
}

/// One colour block: `a/b` means line `a`, text `b`. A single colour is the
/// line colour; a style with no line has nowhere to paint it, so there it stays
/// the text colour.
fn parse_grammar_colors(
    spec: &str,
    has_line: bool,
) -> (Option<GrammarColor>, Option<GrammarColor>) {
    match spec.split_once('/') {
        Some((line, text)) => (parse_grammar_color(line), parse_grammar_color(text)),
        None if has_line => (parse_grammar_color(spec), None),
        None => (None, parse_grammar_color(spec)),
    }
}

/// One scanned grammar construct, still borrowing the source.
pub(crate) struct GrammarTagScan<'a> {
    pub(crate) kind: GrammarKind,
    pub(crate) line_color: Option<GrammarColor>,
    pub(crate) text_color: Option<GrammarColor>,
    /// `::type{colors}[` — the opening marker, `[` included.
    pub(crate) marker: &'a str,
    /// Raw content with `\]` still escaped.
    pub(crate) content: &'a str,
    /// Bytes consumed through the closing `]`.
    pub(crate) consumed: usize,
}

/// Scans `::type{colors}[text]` at the start of `s`.
///
/// The fragment ends at the first unescaped `]`; there is no closing `::`. Anything
/// malformed or unknown is `None`, which leaves it to the caller as plain text.
pub(crate) fn scan_grammar_tag(s: &str) -> Option<GrammarTagScan<'_>> {
    let rest = s.strip_prefix("::")?;
    let type_end = rest.find(|c: char| c == '{' || c == '[')?;
    let kind = GrammarKind::from_tag(&rest[..type_end])?;
    let bytes = s.as_bytes();
    let mut cursor = 2 + type_end;
    let (line_color, text_color) = if bytes.get(cursor) == Some(&b'{') {
        let close = s[cursor + 1..].find('}')? + cursor + 1;
        let colors =
            parse_grammar_colors(&s[cursor + 1..close], kind.line() != GrammarLine::None);
        cursor = close + 1;
        colors
    } else {
        (None, None)
    };
    if bytes.get(cursor) != Some(&b'[') {
        return None;
    }
    let content_start = cursor + 1;
    let mut end = content_start;
    let mut backslashes = 0usize;
    while end < bytes.len() {
        match bytes[end] {
            b'\\' => {
                backslashes += 1;
                end += 1;
                continue;
            }
            b']' if backslashes % 2 == 0 => break,
            _ => {}
        }
        backslashes = 0;
        end += 1;
    }
    if end >= bytes.len() {
        return None;
    }
    Some(GrammarTagScan {
        kind,
        line_color,
        text_color,
        marker: &s[..content_start],
        content: &s[content_start..end],
        consumed: end + 1,
    })
}

/// Resolves the escapes of a grammar fragment for painting: `\]` becomes `]` and
/// `\\` becomes `\`. The raw text is what the parser keeps, so char offsets still
/// count the escapes.
pub fn grammar_paint_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ']' || next == '\\' {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Inline markup a grammar member may carry, scanned by the same parser as the rest
/// of the line.
///
/// Emphasis, strikethrough and inline code keep their formatting; every other
/// construct is flattened into its literal source text, so a link or a highlight
/// inside a member looks exactly as it did before nested markup existed. Adding a
/// kind later means widening the `formatted` match below and teaching the two
/// painters (`push_grammar_runs`, `render_syntax_spans`) the same kind.
pub(crate) fn parse_grammar_content(raw: &str) -> Vec<VisibleSpan> {
    let spans = parse_inline_syntax_spans(raw);
    let mut out: Vec<VisibleSpan> = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        // An escape stays a two-char pair: the backslash paints nothing and the
        // escaped char keeps its own source offset, which the grammar painters rely on.
        if matches!(&spans[i], SyntaxSpan::Marker(m) if m == "\\") {
            if let Some(SyntaxSpan::Text(t)) = spans.get(i + 1) {
                out.push(VisibleSpan {
                    span: SyntaxSpan::Marker("\\".to_string()),
                    marker_visible: true,
                });
                push_grammar_text(&mut out, t);
                i += 2;
                continue;
            }
        }
        let formatted = matches!(spans[i], SyntaxSpan::Marker(_))
            && matches!(
                spans.get(i + 1),
                Some(
                    SyntaxSpan::Bold(_)
                        | SyntaxSpan::Italic(_)
                        | SyntaxSpan::BoldItalic(_)
                        | SyntaxSpan::InlineCode(_)
                        | SyntaxSpan::Strikethrough(_)
                )
            )
            && matches!(spans.get(i + 2), Some(SyntaxSpan::Marker(_)));
        if formatted {
            for span in &spans[i..i + 3] {
                out.push(VisibleSpan {
                    span: span.clone(),
                    marker_visible: true,
                });
            }
            i += 3;
            continue;
        }
        push_grammar_text(&mut out, span_text(&spans[i]));
        i += 1;
    }
    out
}

fn push_grammar_text(out: &mut Vec<VisibleSpan>, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(VisibleSpan { span: SyntaxSpan::Text(t), .. }) => t.push_str(text),
        _ => out.push(VisibleSpan {
            span: SyntaxSpan::Text(text.to_string()),
            marker_visible: true,
        }),
    }
}
