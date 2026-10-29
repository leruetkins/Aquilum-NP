//! PDF export module powered by Typst.
//!
//! Provides native, 100% autonomous Markdown -> Typst -> PDF rendering without
//! external tools (no Chromium, Node.js, or LaTeX required).

use std::collections::HashMap;
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::Library;
use typst::LibraryExt;
use typst::World;
use typst_layout::PagedDocument;

/// Typst World implementation for in-memory document compilation.
pub struct MemoryWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
    main_id: FileId,
    files: HashMap<FileId, Bytes>,
    now: Datetime,
}

impl MemoryWorld {
    pub fn new(source_text: String) -> Self {
        let vpath = VirtualPath::new("main.typ").expect("valid main.typ path");
        let rooted_path = RootedPath::new(VirtualRoot::Project, vpath);
        let main_id = FileId::new(rooted_path);
        let source = Source::new(main_id, source_text);

        let mut book = FontBook::new();
        let mut fonts = Vec::new();

        // Load embedded fonts from typst-assets
        for font_bytes in typst_assets::fonts() {
            let buffer = Bytes::new(font_bytes);
            for font in Font::iter(buffer) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }

        // Also load embedded fonts from app assets if available
        let app_fonts: [&[u8]; 6] = [
            include_bytes!("../assets/fonts/roboto_mono_regular.ttf"),
            include_bytes!("../assets/fonts/roboto_mono_bold.ttf"),
            include_bytes!("../assets/fonts/roboto_mono_italic.ttf"),
            include_bytes!("../assets/fonts/roboto_mono_bold_italic.ttf"),
            include_bytes!("../assets/fonts/droid_sans_mono.ttf"),
            include_bytes!("../assets/fonts/droid_sans_mono_bold.ttf"),
        ];
        for font_bytes in app_fonts {
            let buffer = Bytes::new(font_bytes);
            for font in Font::iter(buffer) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }

        let library = LazyHash::new(Library::default());
        let book = LazyHash::new(book);

        Self {
            library,
            book,
            fonts,
            source,
            main_id,
            files: HashMap::new(),
            now: Datetime::from_ymd(2026, 1, 1).unwrap(),
        }
    }
}

impl World for MemoryWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(bytes) = self.files.get(&id) {
            Ok(bytes.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Some(self.now)
    }
}

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

/// Preprocesses Obsidian wikilinks `[[Target|Alias]]` and `[[Target]]` into standard markdown links.
pub fn preprocess_markdown(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '[' && chars.peek() == Some(&'[') {
            chars.next(); // consume second '['
            let mut inner = String::new();
            let mut closed = false;
            while let Some(c) = chars.next() {
                if c == ']' && chars.peek() == Some(&']') {
                    chars.next(); // consume second ']'
                    closed = true;
                    break;
                }
                inner.push(c);
            }
            if closed {
                if let Some((target, alias)) = inner.split_once('|') {
                    result.push_str(&format!("[{}]({})", alias.trim(), target.trim()));
                } else {
                    let note = inner.trim();
                    result.push_str(&format!("[{}]({})", note, note));
                }
            } else {
                result.push_str("[[");
                result.push_str(&inner);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Converts Markdown text into a fully styled Typst document markup.
pub fn markdown_to_typst(title: &str, markdown: &str) -> String {
    let preprocessed = preprocess_markdown(markdown);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(&preprocessed, options);

    let safe_title = escape_typst_text(title);

    let mut out = String::new();

    // Typst document configuration & styling
    out.push_str(&format!(
        r#"
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

// Headings
#show heading.where(level: 1): it => block(
  above: 1.6em, below: 0.8em,
  text(size: 18pt, weight: "bold", fill: rgb("1a202c"))[#it.body]
)
#show heading.where(level: 2): it => block(
  above: 1.4em, below: 0.7em,
  text(size: 14pt, weight: "bold", fill: rgb("2d3748"))[#it.body]
)
#show heading.where(level: 3): it => block(
  above: 1.2em, below: 0.6em,
  text(size: 12pt, weight: "bold", fill: rgb("4a5568"))[#it.body]
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

// Tables
#set table(
  stroke: (x, y) => if y == 0 {{ (bottom: 1.2pt + rgb("a0aec0")) }} else {{ (bottom: 0.5pt + rgb("e2e8f0")) }},
  fill: (col, row) => if row == 0 {{ rgb("f7fafc") }} else if calc.even(row) {{ rgb("f8fafc") }} else {{ none }},
  inset: (x: 8pt, y: 6pt),
)

"#
    ));

    // If title is non-empty, render main title banner
    if !title.is_empty() {
        out.push_str(&format!(
            "= {}\n\n#v(0.5em)\n\n",
            safe_title
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
                // Lists in typst format naturally with `- ` or `+ `
                let _ = first_num;
                out.push('\n');
            }
            Event::End(TagEnd::List(_)) => {
                out.push('\n');
            }
            Event::Start(Tag::Item) => {
                out.push_str("- ");
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
                    
                    // Alignments
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
                if in_code_block {
                    code_block_content.push_str(&txt);
                } else if let Some(ref mut ts) = table_state {
                    ts.current_cell.push_str(&escape_typst_text(&txt));
                } else {
                    out.push_str(&escape_typst_text(&txt));
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
            _ => {}
        }
    }

    out
}

/// Compile a raw Typst document string into PDF bytes.
pub fn compile_typst_to_pdf(typst_source: &str) -> Result<Vec<u8>, String> {
    let world = MemoryWorld::new(typst_source.to_string());
    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned.output.map_err(|diagnostics| {
        let mut msg = String::new();
        for diag in diagnostics {
            msg.push_str(&format!("{:?}: {}\n", diag.severity, diag.message));
        }
        msg
    })?;

    let pdf_options = typst_pdf::PdfOptions::default();
    typst_pdf::pdf(&document, &pdf_options).map_err(|errs| {
        let mut msg = String::new();
        for diag in errs {
            msg.push_str(&format!("{:?}: {}\n", diag.severity, diag.message));
        }
        msg
    })
}

/// High-level function: converts Markdown to PDF bytes.
pub fn export_markdown_to_pdf(title: &str, markdown: &str) -> Result<Vec<u8>, String> {
    let typst_markup = markdown_to_typst(title, markdown);
    match compile_typst_to_pdf(&typst_markup) {
        Ok(bytes) => {
            println!("[Typst Engine] Successfully compiled PDF ({} bytes)", bytes.len());
            Ok(bytes)
        }
        Err(err) => {
            eprintln!("[Typst Engine] Compilation ERROR:\n{}", err);
            eprintln!("[Typst Engine] Generated Typst markup was:\n{}", typst_markup);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_typst_to_pdf_smoke() {
        let typst_markup = r#"
#set page(paper: "a4", margin: 2cm)
#set text(font: "Roboto Mono", size: 12pt)
= Тестовый заголовок

Это тестовый документ на русском языке для проверки PDF-экспорта через Typst.
- Пункт 1
- Пункт 2

```rust
fn main() {
    println!("Hello from Aquilum Typst PDF!");
}
```
"#;
        let result = compile_typst_to_pdf(typst_markup);
        assert!(result.is_ok(), "Typst compile failed: {:?}", result.err());
        let bytes = result.unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "Output is not a valid PDF header");
        assert!(bytes.len() > 1000, "PDF size is unexpectedly small: {} bytes", bytes.len());
    }

    #[test]
    fn test_export_markdown_to_pdf_full() {
        let md = r#"
## Введение

Это заметка в формате **Markdown** с поддержкой *курсива*, ~~зачёркивания~~ и `кода`.

- [x] Реализовать PDF-экспорт на Typst
- [ ] Добавить поддержку стилей темы
- Обычный пункт списка

### Таблица данных

| Функция | Статус | Скорость |
| :--- | :---: | ---: |
| Typst Engine | Готово | 40ms |
| Чистый Rust | 100% | Нативно |

### Блок кода

```python
def hello_aquilum():
    print("Привет из нативного PDF-экспортера!")
```

> Typst — это современная, быстрая и расширяемая система вёрстки документов.
"#;
        let result = export_markdown_to_pdf("Тестовая Заметка", md);
        assert!(result.is_ok(), "Markdown to PDF export failed: {:?}", result.err());
        let bytes = result.unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 2000);
    }
}
