use std::collections::HashMap;
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Escapes characters that have special syntactic meaning in Typst text/content mode.
pub fn escape_typst_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '#' => out.push_str("\\#"),
            '*' => out.push_str("\\*"),
            '_' => out.push_str("\\_"),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '<' => out.push_str("\\<"),
            '>' => out.push_str("\\>"),
            '`' => out.push_str("\\`"),
            other => out.push(other),
        }
    }
    out
}

/// Resolves a heading preset name (or hex color) to a valid `#RRGGBB` hex string.
pub fn resolve_heading_color_hex(color_spec: &str) -> String {
    let trimmed = color_spec.trim();
    if trimmed.starts_with('#') {
        if trimmed.len() == 4 {
            let mut full = String::from("#");
            for c in trimmed[1..].chars() {
                full.push(c);
                full.push(c);
            }
            return full;
        }
        return trimmed.to_string();
    }
    crate::markdown::find_heading_preset(trimmed).hex.to_string()
}

/// Formats an optional grammar RGB color as Typst syntax, e.g. `rgb(255, 0, 0)` or `none`.
fn format_typst_color(c: Option<crate::markdown::GrammarColor>) -> String {
    match c {
        Some((r, g, b)) => format!("rgb({}, {}, {})", r, g, b),
        None => "none".to_string(),
    }
}

/// Converts inline grammar content into escaped Typst formatting.
pub fn grammar_content_to_typst(content: &str) -> String {
    let mut out = String::new();
    let spans = crate::markdown::parse_grammar_content(content);
    for vis in spans {
        match vis.span {
            crate::markdown::SyntaxSpan::Bold(t) => {
                out.push('*');
                out.push_str(&escape_typst_text(&t));
                out.push('*');
            }
            crate::markdown::SyntaxSpan::Italic(t) => {
                out.push('_');
                out.push_str(&escape_typst_text(&t));
                out.push('_');
            }
            crate::markdown::SyntaxSpan::BoldItalic(t) => {
                out.push('*');
                out.push('_');
                out.push_str(&escape_typst_text(&t));
                out.push('_');
                out.push('*');
            }
            crate::markdown::SyntaxSpan::InlineCode(t) => {
                out.push('`');
                out.push_str(&t);
                out.push('`');
            }
            crate::markdown::SyntaxSpan::Strikethrough(t) => {
                out.push_str("#strike[");
                out.push_str(&escape_typst_text(&t));
                out.push(']');
            }
            crate::markdown::SyntaxSpan::Text(t) => {
                out.push_str(&escape_typst_text(&t));
            }
            _ => {
                out.push_str(&escape_typst_text(crate::markdown::span_text(&vis.span)));
            }
        }
    }
    out
}

/// Converts a scanned grammar tag into a call to its corresponding Typst helper function.
pub(crate) fn grammar_tag_to_typst(scan: &crate::markdown::GrammarTagScan<'_>) -> String {
    let fn_name = match scan.kind.line() {
        crate::markdown::GrammarLine::Solid => "grammar-solid",
        crate::markdown::GrammarLine::Double => "grammar-double",
        crate::markdown::GrammarLine::Wavy => "grammar-wavy",
        crate::markdown::GrammarLine::Dotted => "grammar-dotted",
        crate::markdown::GrammarLine::DashDot => "grammar-dashdot",
        crate::markdown::GrammarLine::None => "grammar-none",
    };
    let l_col = format_typst_color(scan.line_color);
    let t_col = format_typst_color(scan.text_color);
    let body = grammar_content_to_typst(scan.content);
    format!("#{fn_name}({l_col}, {t_col})[{body}]")
}

/// Preprocesses Markdown text:
/// 1. Preserves fenced code blocks (``` and ~~~) and inline code without altering custom markup.
/// 2. Converts Obsidian wikilinks `[[Target|Alias]]` and `[[Target]]` into standard markdown links.
/// 3. Extracts school-grammar tags `::type{colors}[content]` and replaces them with safe unique tokens.
pub fn preprocess_markdown(input: &str) -> (String, Vec<String>) {
    let mut replacements = Vec::new();
    let mut out = String::with_capacity(input.len());
    let mut in_code_block = false;

    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            out.push_str(line);
            continue;
        }

        if in_code_block {
            out.push_str(line);
            continue;
        }

        // Process line outside code fences
        let mut cur = line;
        while !cur.is_empty() {
            // Inline code span: keep raw without replacing grammar tags
            if cur.starts_with('`') {
                let fence_len = cur.chars().take_while(|&c| c == '`').count();
                let fence = &cur[..fence_len];
                let rest = &cur[fence_len..];
                if let Some(end_idx) = rest.find(fence) {
                    let total_code_len = fence_len + end_idx + fence_len;
                    out.push_str(&cur[..total_code_len]);
                    cur = &cur[total_code_len..];
                    continue;
                } else {
                    out.push('`');
                    cur = &cur[1..];
                    continue;
                }
            }

            // Wikilink: [[Target|Alias]] or [[Target]]
            if cur.starts_with("[[") {
                if let Some(close_idx) = cur[2..].find("]]") {
                    let inner = &cur[2..2 + close_idx];
                    let total_consumed = 2 + close_idx + 2;
                    if let Some((target, alias)) = inner.split_once('|') {
                        out.push_str(&format!("[{}]({})", alias.trim(), target.trim()));
                    } else {
                        let note = inner.trim();
                        out.push_str(&format!("[{}]({})", note, note));
                    }
                    cur = &cur[total_consumed..];
                    continue;
                }
            }

            // School grammar tag: ::type{colors}[content]
            if cur.starts_with("::") {
                if let Some(scan) = crate::markdown::scan_grammar_tag(cur) {
                    let typst_repr = grammar_tag_to_typst(&scan);
                    let placeholder = format!("QQQGRAMMARTAG{}QQQ", replacements.len());
                    replacements.push(typst_repr);
                    out.push_str(&placeholder);
                    cur = &cur[scan.consumed..];
                    continue;
                }
            }

            // Ordinary char
            let mut chars = cur.chars();
            let c = chars.next().unwrap();
            out.push(c);
            cur = chars.as_str();
        }
    }

    (out, replacements)
}

/// Appends escaped Typst text, substituting any `QQQGRAMMARTAG{n}QQQ` placeholders with their Typst code.
fn push_typst_text(out: &mut String, text: &str, replacements: &[String]) {
    let escaped = escape_typst_text(text);
    if replacements.is_empty() || !escaped.contains("QQQGRAMMARTAG") {
        out.push_str(&escaped);
        return;
    }
    let mut cur = escaped.as_str();
    while let Some(pos) = cur.find("QQQGRAMMARTAG") {
        out.push_str(&cur[..pos]);
        let rest = &cur[pos + "QQQGRAMMARTAG".len()..];
        if let Some(end_pos) = rest.find("QQQ") {
            let idx_str = &rest[..end_pos];
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(repl) = replacements.get(idx) {
                    out.push_str(repl);
                    cur = &rest[end_pos + 3..];
                    continue;
                }
            }
        }
        out.push_str("QQQGRAMMARTAG");
        cur = rest;
    }
    out.push_str(cur);
}

/// Converts Markdown text into a fully styled Typst document markup.
#[derive(Debug, Clone)]
pub struct PdfImage {
    /// Reference target as written in the note (`name.png`).
    pub target: String,
    /// Path inside the virtual Typst filesystem (`attachments/name.png`).
    pub path: String,
    pub bytes: Vec<u8>,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    /// Percent of the shorter side, 0–50.
    pub radius_pct: Option<f32>,
    /// Fully formatted caption text (with its figure number) or `None`.
    pub caption: Option<String>,
    pub position: String,
}

fn preprocess_pdf_images(markdown: &str, images: &[PdfImage]) -> (String, HashMap<String, usize>) {
    let mut by_target = HashMap::new();
    for (index, image) in images.iter().enumerate() {
        by_target.insert(image.target.as_str(), index);
    }
    let mut dest_map = HashMap::new();
    let mut out = String::with_capacity(markdown.len() + 64);
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(reference) = crate::images::parse_image_reference(trimmed) {
            if let Some(index) = by_target.get(reference.target.as_str()).copied() {
                let image = &images[index];
                let alt = image
                    .caption
                    .as_deref()
                    .unwrap_or("")
                    .replace(']', "")
                    .replace('[', "");
                let title = image
                    .caption
                    .as_deref()
                    .unwrap_or("")
                    .replace('"', "'");
                out.push_str(&format!("![{}]({} \"{}\")", alt, image.path, title));
                dest_map.insert(image.path.clone(), index);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, dest_map)
}

fn typst_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn image_to_typst(image: &PdfImage) -> String {
    let width = match image.width_px {
        Some(width) => format!("{:.1}pt", width as f32 * 0.75),
        None => "80%".to_string(),
    };
    let inner = format!(
        "image(\"{}\", width: {})",
        typst_string(&image.path),
        width
    );
    let inner = match image.radius_pct.filter(|radius| *radius > 0.5) {
        Some(percent) => {
            let display_width = image
                .width_px
                .map(|width| width as f32 * 0.75)
                .unwrap_or(374.0);
            let ratio = match (image.width_px, image.height_px) {
                (Some(width), Some(height)) if width > 0 => height as f32 / width as f32,
                _ => 0.75,
            };
            let display_height = display_width * ratio;
            let radius = percent / 100.0 * display_width.min(display_height);
            format!("block(clip: true, radius: {radius:.1}pt, {inner})")
        }
        None => inner,
    };
    let caption = image.caption.as_deref().unwrap_or("");
    if !caption.is_empty() {
        let caption = escape_typst_text(caption);
        if image.position == "above" {
            format!(
                "#align(center)[#text(size: 9pt, style: \"italic\")[{caption}]]\n#block(width: 100%, align(center, {inner}))\n\n"
            )
        } else {
            format!(
                "#block(width: 100%, align(center, {inner}))\n#align(center)[#text(size: 9pt, style: \"italic\")[{caption}]]\n\n"
            )
        }
    } else {
        format!("#block(width: 100%, align(center, {inner}))\n\n")
    }
}

pub fn markdown_to_typst_with_images(
    title: &str,
    markdown: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
    images: &[PdfImage],
) -> String {
    let (markdown, by_dest) = preprocess_pdf_images(markdown, images);
    markdown_to_typst_inner(title, &markdown, headings, images, &by_dest)
}

pub fn markdown_to_typst(
    title: &str,
    markdown: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
) -> String {
    markdown_to_typst_inner(title, markdown, headings, &[], &HashMap::new())
}

fn markdown_to_typst_inner(
    title: &str,
    markdown: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
    images: &[PdfImage],
    by_dest: &HashMap<String, usize>,
) -> String {
    let (preprocessed, replacements) = preprocess_markdown(markdown);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(&preprocessed, options);

    let safe_title = escape_typst_text(title);

    let default_headings = crate::settings::HeadingsSettings::default();
    let h = headings.unwrap_or(&default_headings);

    let h1_color = resolve_heading_color_hex(&h.h1.color_name);
    let h2_color = resolve_heading_color_hex(&h.h2.color_name);
    let h3_color = resolve_heading_color_hex(&h.h3.color_name);
    let h4_color = resolve_heading_color_hex(&h.h4.color_name);
    let h5_color = resolve_heading_color_hex(&h.h5.color_name);
    let h6_color = resolve_heading_color_hex(&h.h6.color_name);

    let h1_weight = if h.h1.bold { "bold" } else { "regular" };
    let h2_weight = if h.h2.bold { "bold" } else { "regular" };
    let h3_weight = if h.h3.bold { "bold" } else { "regular" };
    let h4_weight = if h.h4.bold { "bold" } else { "regular" };
    let h5_weight = if h.h5.bold { "bold" } else { "regular" };
    let h6_weight = if h.h6.bold { "bold" } else { "regular" };

    let h1_style = if h.h1.italic { "italic" } else { "normal" };
    let h2_style = if h.h2.italic { "italic" } else { "normal" };
    let h3_style = if h.h3.italic { "italic" } else { "normal" };
    let h4_style = if h.h4.italic { "italic" } else { "normal" };
    let h5_style = if h.h5.italic { "italic" } else { "normal" };
    let h6_style = if h.h6.italic { "italic" } else { "normal" };

    let h1_size = ((h.h1.font_size as f32) * 0.9).clamp(14.0, 32.0);
    let h2_size = ((h.h2.font_size as f32) * 0.88).clamp(12.0, 26.0);
    let h3_size = ((h.h3.font_size as f32) * 0.85).clamp(11.0, 22.0);
    let h4_size = ((h.h4.font_size as f32) * 0.83).clamp(10.5, 18.0);
    let h5_size = ((h.h5.font_size as f32) * 0.82).clamp(10.0, 16.0);
    let h6_size = ((h.h6.font_size as f32) * 0.80).clamp(9.5, 14.0);

    let mut out = String::new();

    // Typst document configuration & styling
    if !images.is_empty() {
        out.push_str(
            r##"
#set figure(supplement: [Рис.])
#set figure.caption(separator: [. ])
#show figure.caption: set text(size: 9pt, style: "italic")
"##,
        );
    }
    out.push_str(&format!(
        r##"
#set page(
  paper: "a4",
  margin: (top: 2.5cm, bottom: 2.5cm, left: 2.2cm, right: 2.2cm),
  header: context if counter(page).get().first() > 1 [
    #align(right)[
      #text(size: 8.5pt, fill: luma(120), font: "Roboto Mono")[{safe_title}]
    ]
    #v(0.3em)
    #line(length: 100%, stroke: 0.4pt + luma(210))
  ],
  numbering: "1 / 1",
)

#set text(
  font: ("Liberation Serif", "Linux Libertine", "DejaVu Sans", "Roboto Mono"),
  size: 11pt,
  lang: "ru",
)

#set par(justify: false, leading: 0.7em)

// Lists with accent color markers
#set list(marker: (
  text(fill: rgb("{h1_color}"))[•],
  text(fill: rgb("{h1_color}"))[‣],
  text(fill: rgb("{h1_color}"))[–],
))
#set enum(numbering: n => text(fill: rgb("{h1_color}"), weight: "bold")[#n.])

// Headings styled according to current theme/settings
#show heading.where(level: 1): it => block(
  above: 1.6em, below: 0.8em,
  text(size: {h1_size:.1}pt, weight: "{h1_weight}", style: "{h1_style}", fill: rgb("{h1_color}"))[#it.body]
)
#show heading.where(level: 2): it => block(
  above: 1.4em, below: 0.7em,
  text(size: {h2_size:.1}pt, weight: "{h2_weight}", style: "{h2_style}", fill: rgb("{h2_color}"))[#it.body]
)
#show heading.where(level: 3): it => block(
  above: 1.2em, below: 0.6em,
  text(size: {h3_size:.1}pt, weight: "{h3_weight}", style: "{h3_style}", fill: rgb("{h3_color}"))[#it.body]
)
#show heading.where(level: 4): it => block(
  above: 1.1em, below: 0.5em,
  text(size: {h4_size:.1}pt, weight: "{h4_weight}", style: "{h4_style}", fill: rgb("{h4_color}"))[#it.body]
)
#show heading.where(level: 5): it => block(
  above: 1.0em, below: 0.5em,
  text(size: {h5_size:.1}pt, weight: "{h5_weight}", style: "{h5_style}", fill: rgb("{h5_color}"))[#it.body]
)
#show heading.where(level: 6): it => block(
  above: 1.0em, below: 0.5em,
  text(size: {h6_size:.1}pt, weight: "{h6_weight}", style: "{h6_style}", fill: rgb("{h6_color}"))[#it.body]
)

// Code blocks & inline code
#show raw.where(block: true): it => block(
  fill: rgb("f8f9fa"),
  stroke: 0.5pt + rgb("e2e8f0"),
  inset: 10pt,
  radius: 4pt,
  width: 100%,
  text(font: "Roboto Mono", size: 9.5pt)[#it]
)
#show raw.where(block: false): it => box(
  fill: rgb("edf2f7"),
  inset: (x: 3.5pt, y: 1pt),
  outset: (y: 2pt),
  radius: 2pt,
  text(font: "Roboto Mono", size: 9.5pt)[#it]
)

// Blockquotes
#show quote: it => block(
  stroke: (left: 3pt + rgb("4c78dd")),
  inset: (left: 12pt, y: 4pt),
  fill: rgb("f8fafc"),
  pad(x: 4pt, text(style: "italic", fill: rgb("475569"))[#it.body])
)

// Tables with full vertical and horizontal grid lines
#set table(
  stroke: (x, y) => (
    left: 0.6pt + rgb("cbd5e1"),
    right: 0.6pt + rgb("cbd5e1"),
    top: 0.6pt + rgb("cbd5e1"),
    bottom: if y == 0 {{ 1.4pt + rgb("94a3b8") }} else {{ 0.6pt + rgb("cbd5e1") }},
  ),
  fill: (col, row) => if row == 0 {{ rgb("f1f5f9") }} else if calc.even(row) {{ rgb("f8fafc") }} else {{ none }},
  inset: (x: 9pt, y: 7pt),
)

// School grammar helper functions
#let grammar-solid(l_col, t_col, body) = {{
  let c_line = if l_col != none {{ l_col }} else if t_col != none {{ t_col }} else {{ black }}
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  underline(stroke: (paint: c_line, thickness: 1.0pt), offset: 2.5pt)[#content]
}}

#let grammar-double(l_col, t_col, body) = context {{
  let size = measure(body)
  let w = size.width
  let c_line = if l_col != none {{ l_col }} else if t_col != none {{ t_col }} else {{ black }}
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  box[#content#place(bottom + left, dy: 1.8pt)[#line(length: w, stroke: 0.85pt + c_line)]#place(bottom + left, dy: 3.6pt)[#line(length: w, stroke: 0.85pt + c_line)]]
}}

#let grammar-dotted(l_col, t_col, body) = {{
  let c_line = if l_col != none {{ l_col }} else if t_col != none {{ t_col }} else {{ black }}
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  underline(stroke: (paint: c_line, thickness: 1.0pt, dash: (array: (3pt, 2.5pt))), offset: 2.5pt)[#content]
}}

#let grammar-dashdot(l_col, t_col, body) = {{
  let c_line = if l_col != none {{ l_col }} else if t_col != none {{ t_col }} else {{ black }}
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  underline(stroke: (paint: c_line, thickness: 1.0pt, dash: (array: (4pt, 2pt, "dot", 2pt))), offset: 2.5pt)[#content]
}}

#let grammar-wavy(l_col, t_col, body) = context {{
  let size = measure(body)
  let w = size.width
  let count = calc.max(1, int(calc.round(w / 6.0pt)))
  let step = w / count
  let elements = ()
  for i in range(count) {{
    let x0 = i * step
    let x_half = x0 + step * 0.5
    let x_end = x0 + step
    // Typst curve.cubic argument order: (control-start, control-end, end)
    elements.push(curve.cubic(
      (x0 + step * 0.15, -1.25pt),
      (x0 + step * 0.35, -1.25pt),
      (x_half, 0pt),
    ))
    elements.push(curve.cubic(
      (x0 + step * 0.65, 1.25pt),
      (x0 + step * 0.85, 1.25pt),
      (x_end, 0pt),
    ))
  }}
  let c_line = if l_col != none {{ l_col }} else if t_col != none {{ t_col }} else {{ rgb("#008000") }}
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  box[#content#place(bottom + left, dy: 2.8pt)[#curve(stroke: 0.95pt + c_line, curve.move((0pt, 0pt)), ..elements)]]
}}

#let grammar-none(l_col, t_col, body) = {{
  let content = if t_col != none {{ text(fill: t_col)[#body] }} else {{ body }}
  content
}}

"##
    ));

    // If title is non-empty, render main title banner
    if !title.is_empty() {
        out.push_str(&format!(
            "#align(left)[#text(size: 22pt, weight: \"bold\", fill: rgb(\"{h1_color}\"))[{safe_title}]]\n\n#v(0.6em)\n\n"
        ));
    }

    struct TableState {
        alignments: Vec<Alignment>,
        in_head: bool,
        cells: Vec<(bool, String)>, // (is_header, content)
        current_cell: String,
    }

    let mut table_state: Option<TableState> = None;
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();
    let mut list_stack: Vec<bool> = Vec::new(); // true = ordered (numbered), false = unordered (bullet)

    let mut in_image = false;
    for event in parser {
        match event {
            Event::Start(Tag::Paragraph) => {
                if table_state.is_none() {
                    out.push('\n');
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if table_state.is_none() {
                    out.push_str("\n\n");
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                out.push('\n');
                let count = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                for _ in 0..count {
                    out.push('=');
                }
                out.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => {
                out.push_str("\n\n");
            }
            Event::Start(Tag::BlockQuote(_)) => {
                out.push_str("#quote(block: true)[\n");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                out.push_str("]\n\n");
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_block_content.clear();
                code_block_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                out.push_str(&format!(
                    "```{}\n{}```\n\n",
                    code_block_lang,
                    code_block_content
                ));
            }
            Event::Start(Tag::List(first_num)) => {
                let is_ordered = first_num.is_some();
                list_stack.push(is_ordered);
                out.push('\n');
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                out.push('\n');
            }
            Event::Start(Tag::Item) => {
                let indent_levels = list_stack.len().saturating_sub(1);
                for _ in 0..indent_levels {
                    out.push_str("  ");
                }
                let is_ordered = list_stack.last().copied().unwrap_or(false);
                if is_ordered {
                    out.push_str("+ ");
                } else {
                    out.push_str("- ");
                }
            }
            Event::End(TagEnd::Item) => {
                out.push('\n');
            }
            Event::Start(Tag::Emphasis) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push('_');
                } else {
                    out.push('_');
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push('_');
                } else {
                    out.push('_');
                }
            }
            Event::Start(Tag::Strong) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push('*');
                } else {
                    out.push('*');
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push('*');
                } else {
                    out.push('*');
                }
            }
            Event::Start(Tag::Strikethrough) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str("#strike[");
                } else {
                    out.push_str("#strike[");
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push(']');
                } else {
                    out.push(']');
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str(&format!("#link(\"{}\")[", dest_url));
                } else {
                    out.push_str(&format!("#link(\"{}\")[", dest_url));
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push(']');
                } else {
                    out.push(']');
                }
            }
            Event::Start(Tag::Table(alignments)) => {
                table_state = Some(TableState {
                    alignments,
                    in_head: false,
                    cells: Vec::new(),
                    current_cell: String::new(),
                });
            }
            Event::End(TagEnd::Table) => {
                if let Some(ts) = table_state.take() {
                    let col_count = ts.alignments.len().max(1);
                    out.push_str(&format!("#table(\n  columns: {col_count},\n"));
                    
                    let align_strs: Vec<&str> = ts.alignments.iter().map(|a| match a {
                        Alignment::Left => "left",
                        Alignment::Center => "center",
                        Alignment::Right => "right",
                        Alignment::None => "left",
                    }).collect();
                    out.push_str(&format!("  align: ({}),\n", align_strs.join(", ")));

                    for (is_head, cell_text) in ts.cells {
                        if is_head {
                            out.push_str(&format!("  [*{}*],\n", cell_text.trim()));
                        } else {
                            out.push_str(&format!("  [{}],\n", cell_text.trim()));
                        }
                    }
                    out.push_str(")\n\n");
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut ts) = table_state {
                    ts.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut ts) = table_state {
                    ts.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut ts) = table_state {
                    let cell_content = std::mem::take(&mut ts.current_cell);
                    let is_head = ts.in_head;
                    ts.cells.push((is_head, cell_content));
                }
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked {
                    "#box(baseline: 15%)[#rect(width: 9pt, height: 9pt, radius: 2pt, fill: rgb(\"4c78dd\"), stroke: 0.8pt + rgb(\"4c78dd\"))[#align(center + horizon)[#text(fill: white, size: 7pt, weight: \"bold\")[✓]]]] "
                } else {
                    "#box(baseline: 15%)[#rect(width: 9pt, height: 9pt, radius: 2pt, stroke: 0.8pt + luma(120))] "
                };
                out.push_str(marker);
            }
            Event::Text(txt) => {
                if in_image {
                    // Alt text belongs to the image's caption, not the body.
                } else if in_code_block {
                    code_block_content.push_str(&txt);
                } else if let Some(ref mut ts) = table_state {
                    push_typst_text(&mut ts.current_cell, &txt, &replacements);
                } else {
                    push_typst_text(&mut out, &txt, &replacements);
                }
            }
            Event::Code(txt) => {
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str(&format!("`{}`", txt));
                } else {
                    out.push_str(&format!("`{}`", txt));
                }
            }
            Event::InlineMath(txt) => {
                let math = format!("${}$", txt.trim());
                if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str(&math);
                } else {
                    out.push_str(&math);
                }
            }
            Event::DisplayMath(txt) => {
                out.push_str(&format!("\n$ {} $\n\n", txt.trim()));
            }
            Event::Rule => {
                out.push_str("\n#line(length: 100%, stroke: 0.5pt + luma(200))\n\n");
            }
            Event::SoftBreak => {
                if in_code_block {
                    code_block_content.push('\n');
                } else if let Some(ref mut ts) = table_state {
                    ts.current_cell.push(' ');
                } else {
                    out.push(' ');
                }
            }
            Event::HardBreak => {
                if in_code_block {
                    code_block_content.push('\n');
                } else if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str(" #linebreak() ");
                } else {
                    out.push_str(" #linebreak() \n");
                }
            }
            Event::FootnoteReference(name) => {
                out.push_str(&format!("#super[{}]", escape_typst_text(&name)));
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                in_image = true;
                if let Some(index) = by_dest.get(dest_url.as_ref()) {
                    out.push_str(&image_to_typst(&images[*index]));
                } else {
                    out.push_str(&format!("\"{}\"", escape_typst_text(&dest_url)));
                }
            }
            Event::End(TagEnd::Image) => {
                in_image = false;
            }
            _ => {}
        }
    }

    out
}
