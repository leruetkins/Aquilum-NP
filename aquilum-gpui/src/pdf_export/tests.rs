    use super::*;

    #[test]
    fn test_list_and_enum_styling() {
        let code = r#"
#set list(marker: text(fill: rgb("4c78dd"))[•])
#set enum(numbering: n => text(fill: rgb("4c78dd"), weight: "bold")[#n.])

- Маркер 1
- Маркер 2

+ Номер 1
+ Номер 2
"#;
        let res = compile_typst_to_pdf(code);
        assert!(res.is_ok(), "Err: {:?}", res.err());
    }

    #[test]
    fn test_all_grammar_types_compilation() {
        let typst_markup = r##"
#set text(font: "Roboto Mono", size: 11pt, lang: "ru")

#let grammar-solid(l_col, t_col, body) = {
  let c_line = if l_col != none { l_col } else if t_col != none { t_col } else { black }
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  underline(stroke: (paint: c_line, thickness: 1.0pt), offset: 2.5pt)[#content]
}

#let grammar-double(l_col, t_col, body) = context {
  let size = measure(body)
  let w = size.width
  let c_line = if l_col != none { l_col } else if t_col != none { t_col } else { black }
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  box[#content#place(bottom + left, dy: 1.8pt)[#line(length: w, stroke: 0.85pt + c_line)]#place(bottom + left, dy: 3.6pt)[#line(length: w, stroke: 0.85pt + c_line)]]
}

#let grammar-dotted(l_col, t_col, body) = {
  let c_line = if l_col != none { l_col } else if t_col != none { t_col } else { black }
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  underline(stroke: (paint: c_line, thickness: 1.0pt, dash: (array: (3pt, 2.5pt))), offset: 2.5pt)[#content]
}

#let grammar-dashdot(l_col, t_col, body) = {
  let c_line = if l_col != none { l_col } else if t_col != none { t_col } else { black }
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  underline(stroke: (paint: c_line, thickness: 1.0pt, dash: (array: (4pt, 2pt, "dot", 2pt))), offset: 2.5pt)[#content]
}

#let grammar-wavy(l_col, t_col, body) = context {
  let size = measure(body)
  let w = size.width
  let count = calc.max(1, int(calc.round(w / 6.0pt)))
  let step = w / count
  let elements = ()
  for i in range(count) {
    let x0 = i * step
    let x_half = x0 + step * 0.5
    let x_end = x0 + step
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
  }
  let c_line = if l_col != none { l_col } else if t_col != none { t_col } else { rgb("#008000") }
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  box[#content#place(bottom + left, dy: 2.8pt)[#curve(stroke: 0.95pt + c_line, curve.move((0pt, 0pt)), ..elements)]]
}

#let grammar-none(l_col, t_col, body) = {
  let content = if t_col != none { text(fill: t_col)[#body] } else { body }
  content
}

#grammar-wavy(rgb(0, 128, 0), none)[Маленькая]
#grammar-solid(rgb(255, 0, 0), rgb(0, 0, 255))[кошка]
#grammar-dashdot(none, none)[тихо]
#grammar-double(rgb(34, 170, 119), none)[спала]
#grammar-dashdot(rgb(255, 165, 0), none)[на подоконнике].
"##;
        let res = compile_typst_to_pdf(typst_markup);
        assert!(res.is_ok(), "Typst compile failed: {:?}", res.err());
    }

    #[test]
    fn an_image_reference_becomes_a_typst_image_block() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 40, 60, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let images = vec![PdfImage {
            target: "a.png".to_string(),
            path: "attachments/a.png".to_string(),
            bytes: png,
            width_px: None,
            height_px: None,
            radius_pct: Some(10.0),
            caption: Some("Рис. 1. Схема".to_string()),
            position: "below".to_string(),
        }];
        let markup = markdown_to_typst_with_images("Note", "![[a.png]]", None, &images);
        assert!(markup.contains("block(clip: true, radius:"));
        assert!(markup.contains("attachments/a.png"));
        assert!(markup.contains("Рис. 1. Схема"));
        let pdf = export_markdown_to_pdf_with_images("Note", "![[a.png]]", None, images);
        assert!(pdf.is_ok(), "Err: {:?}", pdf.err());
    }

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
        let result = export_markdown_to_pdf("Тестовая Заметка", md, None);
        assert!(result.is_ok(), "Markdown to PDF export failed: {:?}", result.err());
        let bytes = result.unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 2000);
    }

    #[test]
    fn test_user_grammar_and_headings_export() {
        let md = r#"## Пример входа

```markdown
::d{green}[Маленькая] ::s{red/blue}[кошка] ::a[тихо] ::p{#2a7}[спала] ::a{orange}[на подоконнике].
```

::d{green}[Маленькая] ::s{red/blue}[кошка] ::a[тихо] ::p{#2a7}[спала] ::a{orange}[на подоконнике].
"#;
        let headings = crate::settings::HeadingsSettings::default();
        let typst = markdown_to_typst("Пример разбора", md, Some(&headings));

        // Verify heading 2 style contains sky_blue hex (#56B4E9)
        assert!(typst.contains("#56B4E9"), "Expected H2 color in typst show rule");

        // Verify raw code block preserved
        assert!(typst.contains("::d{green}[Маленькая]"), "Raw code block must preserve original syntax");

        // Verify paragraph converted to typst grammar calls
        assert!(typst.contains("#grammar-wavy(rgb(0, 128, 0), none)[Маленькая]"), "Expected grammar-wavy");
        assert!(typst.contains("#grammar-solid(rgb(255, 0, 0), rgb(0, 0, 255))[кошка]"), "Expected grammar-solid with red/blue");
        assert!(typst.contains("#grammar-dashdot(none, none)[тихо]"), "Expected grammar-dashdot");
        assert!(typst.contains("#grammar-double(rgb(34, 170, 119), none)[спала]"), "Expected grammar-double with #2a7 teal");
        assert!(typst.contains("#grammar-dashdot(rgb(255, 165, 0), none)[на подоконнике]"), "Expected grammar-dashdot with orange");

        // Verify compilation to PDF
        let result = export_markdown_to_pdf("Пример разбора", md, Some(&headings));
        assert!(result.is_ok(), "PDF compilation failed: {:?}", result.err());
        let bytes = result.unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }
    #[test]
    fn test_ordered_list_and_table_vertical_lines() {
        let md = r#"
## Правила цветов

1. Без блока цветов: ::s[Кошка] - стандартная линия.
2. Один цвет: ::s{#2a7}[Кошка] - цвет текста.
3. Два цвета: ::s{red/blue}[Кошка] - первый цвет для линии, второй для текста.

| Тег | Член предложения | Линия |
| :--- | :--- | :--- |
| `::s[...]` | подлежащее | одна сплошная |
| `::p[...]` | сказуемое | двойная сплошная |
| `::d[...]` | определение | волнистая |
"#;
        let typst = markdown_to_typst("Правила", md, None);

        // Verify numbered list uses `+ ` for ordered items
        assert!(typst.contains("+ Без блока цветов"), "Ordered list must use `+ ` for numbering in Typst");
        assert!(typst.contains("+ Один цвет"), "Ordered list item 2 must use `+ `");
        assert!(typst.contains("+ Два цвета"), "Ordered list item 3 must use `+ `");

        // Verify table has vertical borders
        assert!(typst.contains("left: 0.6pt + rgb(\"cbd5e1\")"), "Table must include vertical borders");
        assert!(typst.contains("right: 0.6pt + rgb(\"cbd5e1\")"), "Table must include vertical borders");

        // Verify double line uses place + line with measured width
        assert!(typst.contains("#line(length: w"), "Double underline must use equal line lengths");

        // Verify compilation succeeds
        let res = export_markdown_to_pdf("Правила", md, None);
        assert!(res.is_ok(), "Export failed: {:?}", res.err());
    }
