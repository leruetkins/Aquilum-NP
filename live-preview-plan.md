# План: Obsidian-style Live Preview в редакторе на GPUI (v3, подробный)

Документ описывает проблему, архитектуру и пошаговую реализацию. Для каждого этапа: какая проблема решается, алгоритм, код, как проверить. Код в основном рабочий по логике, но API GPUI меняется между версиями, поэтому места, зависящие от GPUI, помечены «сверьтесь с вашей версией».

---

## 0. Оглавление

1. Проблема
2. Цели и бюджет времени
3. Общая архитектура
4. Модель данных
5. Этап 0: измерения (делаем первым)
6. Этап 1: разбор Markdown в блоки и spans
7. Этап 2: высоты блоков и виртуализация
8. Этап 3: shaping и кэш layout
9. Этап 4: курсор и раскрытие синтаксиса
10. Этап 5: соответствие source <-> visual и навигация
11. Этап 6: заголовки и другие layout-стили
12. Этап 7: инкрементальная обработка правок
13. Этап 8: оптимизации после профилирования
14. Тесты
15. Критерии готовности
16. Риски и открытые вопросы

---

## 1. Проблема

### 1.1 Что нужно

Режим Live Preview как в Obsidian: документ выглядит как отрендеренный Markdown, но остаётся редактируемым в одном окне.

| Состояние | Что видит пользователь |
|---|---|
| Курсор вне узла | `Заголовок`, **жирный**, `документация` (ссылка), без `#`, `**`, `[](url)` |
| Курсор внутри или на границе узла | `# Заголовок`, `**жирный**`, `[документация](https://...)` |

Исходный текст не меняется, меняется только его визуальное представление.

### 1.2 Что сейчас происходит

Симптом: редактор сильно тормозит при движении курсора стрелками.

Типичная цепочка, которая даёт такую картину:

```text
нажатие стрелки
 -> selection changed
 -> обработчик заново парсит весь Markdown            (O(N) по размеру файла)
 -> пересоздаются spans/decorations для всего файла   (O(N) аллокаций)
 -> cx.notify() на корневом view
 -> render: shape_line() для каждой строки документа  (O(N) shaping)
 -> layout всех строк, пересчёт высот
 -> paint
```

Иллюстрация порядка величин (цифры условные, подставьте свои после замеров): если на shaping одной строки уходит ~20 мкс, то для 5000 строк это 100 мс на кадр. Для плавного отклика нужно уложиться в ~8-16 мс на весь кадр, а обработчик стрелки должен занимать доли миллисекунды.

### 1.3 Первопричина

Работа не разделена по частоте событий:

| Событие | Как часто | Что должно пересчитываться |
|---|---|---|
| Нажатие стрелки | очень часто | активный узел, 1-2 блока |
| Правка текста | часто | затронутые блоки |
| Смена ширины окна | редко | layout (высоты), но не разбор |
| Открытие файла | редко | всё, один раз |
| Каждый кадр | всегда | только paint видимых блоков |

Если при каждой стрелке выполняется то, что нужно только при открытии файла, редактор тормозит.

### 1.4 Дополнительная сложность: геометрия

Крупные заголовки меняют высоту строк. Значит, чтобы скроллить и знать, где находится блок №5000, нужны высоты всех блоков, а не только видимых. Поэтому `uniform_list` (одна высота для всех) не подходит.

---

## 2. Цели и бюджет времени

| Операция | Цель |
|---|---|
| Обработчик стрелки (без кадра) | < 0,2 мс |
| Кадр после стрелки | < 4 мс на документе 5000 строк |
| Набор символа (обновление блока) | < 2 мс |
| Открытие файла 5000 строк | < 100 мс до первого кадра (остальное в фоне) |
| Зависимость от размера документа | время кадра и стрелки не должно расти с N |

Главный критерий: **время обработки стрелки не зависит от размера файла**.

---

## 3. Общая архитектура

```text
            ┌──────────────────────────────┐
            │ Rope (исходный текст)        │  единственный источник правды
            └──────────────┬───────────────┘
                           │ только при правке текста
                           ▼
            ┌──────────────────────────────┐
            │ pulldown-cmark + offset_iter │
            └──────────────┬───────────────┘
                           ▼
            ┌──────────────────────────────┐
            │ Blocks: range, kind, spans   │  кэш разбора
            └──────────────┬───────────────┘
                           ▼
            ┌──────────────────────────────┐
            │ Heights index (prefix sums)  │  геометрия всего документа
            └──────────────┬───────────────┘
                           ▼
            ┌──────────────────────────────┐
            │ Virtual list (видимые блоки) │
            └──────────────┬───────────────┘
                           ▼
       ┌───────────────────────────────────────────┐
       │ Для каждого видимого блока:               │
       │  spans + CursorState -> runs -> shaping   │
       │  (кэш layout по ключу)                    │
       └───────────────────────────────────────────┘

CursorState (активный блок и узлы) влияет только на видимость маркеров.
```

Слои и когда они обновляются:

| Слой | Обновляется при |
|---|---|
| Rope | правке текста |
| Blocks + spans | правке текста (только затронутые блоки) |
| Heights index | правке блока, смене ширины окна |
| CursorState | движении курсора |
| Layout cache | смене текста блока, стиля, ширины, активности блока |

---

## 4. Модель данных

```rust
use std::ops::Range;

pub type BlockId = u64;

pub struct MarkdownDocument {
    pub source: Rope,                    // исходный текст
    pub blocks: Vec<MarkdownBlock>,      // отсортированы по source_range
    pub heights: HeightIndex,            // геометрия
    next_block_id: BlockId,              // для стабильных id
}

pub struct MarkdownBlock {
    pub id: BlockId,
    pub source_range: Range<usize>,      // байтовые смещения в source
    pub kind: BlockKind,
    pub spans: Vec<Span>,                // отсортированы по source_range.start
    pub layout: Option<BlockLayout>,     // кэш высоты для текущей ширины
}

#[derive(Clone, Copy, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),          // уровень 1..=6
    CodeFence,
    BlockQuote,
    ListItem,
    Rule,
    Other,
}

pub struct Span {
    pub range: Range<usize>,             // в координатах source (абсолютных)
    pub kind: SpanKind,
    pub node: NodeId,                    // узел, которому принадлежит span
}

#[derive(Clone, Copy, PartialEq)]
pub enum SpanKind {
    Text,                                // обычный текст (можно не хранить)
    Marker,                              // скрываемый синтаксис: **, #, [, ](url)
    Style(StyleKind),                    // стиль содержимого
    Atomic,                              // виджет/embed: курсор перепрыгивает
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleKind {
    Bold, Italic, Strike, Code, Link, Heading(u8),
}

/// Узел Markdown (strong, emphasis, link, heading...). Нужен, чтобы раскрывать
/// маркеры всего узла разом.
pub type NodeId = u32;

pub struct Node {
    pub id: NodeId,
    pub range: Range<usize>,             // весь узел, включая маркеры
}

pub struct CursorState {
    pub pos: usize,                      // байтовое смещение каретки
    pub active_block: Option<BlockId>,
    pub active_nodes: SmallVec<[NodeId; 4]>,  // узлы, содержащие каретку
}

pub struct BlockLayout {
    pub width: f32,                      // для какой ширины измерено
    pub height: f32,
}
```

Важные решения:

- `Marker` и `Style` разделены: маркеры скрываются или показываются, стили влияют на внешний вид текста. Один и тот же `**жирный**` даёт два маркера и один `Style(Bold)`.
- `Span` ссылается на `NodeId`. Раскрытие узла: «показать все `Marker` с `node ∈ active_nodes`». Это даёт поведение Obsidian (при входе в ссылку раскрывается вся конструкция) без обхода документа.
- Все смещения байтовые и абсолютные в `source`. Для блоков, которые не менялись, они остаются валидными, пока перед ними не было правок (см. этап 7).

---

## 5. Этап 0: измерения (делаем первым)

### Проблема

Без замеров можно оптимизировать не то. Нужно сразу увидеть, на каком этапе уходит время, и убедиться, что исправление помогло.

### Что решает

Даёт числа «до» и «после» для каждого этапа и показывает, как время зависит от размера файла.

### Счётчики

```rust
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub struct PerfCounters {
    pub parse_calls: AtomicU64,
    pub span_rebuilds: AtomicU64,
    pub blocks_invalidated: AtomicU64,
    pub shape_calls: AtomicU64,
    pub layout_cache_hit: AtomicU64,
    pub layout_cache_miss: AtomicU64,
    pub notify_calls: AtomicU64,
}

pub static PERF: PerfCounters = PerfCounters {
    parse_calls: AtomicU64::new(0),
    span_rebuilds: AtomicU64::new(0),
    blocks_invalidated: AtomicU64::new(0),
    shape_calls: AtomicU64::new(0),
    layout_cache_hit: AtomicU64::new(0),
    layout_cache_miss: AtomicU64::new(0),
    notify_calls: AtomicU64::new(0),
};

impl PerfCounters {
    pub fn snapshot_and_reset(&self) -> [u64; 7] {
        [
            self.parse_calls.swap(0, Relaxed),
            self.span_rebuilds.swap(0, Relaxed),
            self.blocks_invalidated.swap(0, Relaxed),
            self.shape_calls.swap(0, Relaxed),
            self.layout_cache_hit.swap(0, Relaxed),
            self.layout_cache_miss.swap(0, Relaxed),
            self.notify_calls.swap(0, Relaxed),
        ]
    }
}
```

### Замер обработчика

```rust
use std::time::Instant;

fn on_selection_changed(&mut self, cx: &mut Context<Self>) {
    let t = Instant::now();

    self.update_cursor(/* ... */);

    let handler = t.elapsed();
    let c = PERF.snapshot_and_reset();
    log::debug!(
        "cursor: handler={:?} parse={} spans={} inval={} shape={} hit={} miss={} notify={}",
        handler, c[0], c[1], c[2], c[3], c[4], c[5], c[6]
    );
}
```

### Нормальный результат для стрелки

```text
cursor: handler=40µs parse=0 spans=0 inval=2 shape=2 hit=0 miss=2 notify=1
```

Если `parse > 0` или `spans` в сотнях, причина тормозов найдена.

### Замер кадра

Замерьте время `render` + `paint` вашего editor-элемента (в `Render::render` и в `paint` элемента) и отдельно время «prepaint/layout». Если у вас версия GPUI, где доступны встроенные измерения кадров, используйте их; иначе достаточно `Instant` вокруг ваших функций.

### Тест масштабирования

Сгенерируйте файлы разного размера и запишите время кадра после стрелки:

```rust
fn generate_doc(lines: usize) -> String {
    let mut s = String::new();
    for i in 0..lines {
        match i % 5 {
            0 => s.push_str(&format!("# Заголовок {i}\n\n")),
            1 => s.push_str("Текст с **жирным**, *курсивом* и [ссылкой](https://example.com).\n\n"),
            2 => s.push_str("- пункт списка\n- ещё пункт\n\n"),
            3 => s.push_str("```rust\nfn main() {}\n```\n\n"),
            _ => s.push_str("Обычный абзац без разметки.\n\n"),
        }
    }
    s
}
```

| Размер | Время кадра после стрелки (до) | (после) |
|---|---|---|
| 50 строк | | |
| 500 строк | | |
| 5000 строк | | |

Интерпретация:

| Что видно | Вероятная причина |
|---|---|
| Растёт линейно с размером | полный парсинг, shaping или обход всего документа |
| Не зависит от размера, но медленно | лишние аллокации, цикл событий, лишний `notify` |
| Растёт с числом Markdown-элементов | пересоздание spans или decorations |

---

## 6. Этап 1: разбор Markdown в блоки и spans

### Проблема

Сейчас разбор и построение декораций, вероятно, выполняются на весь документ и на каждое событие. Нужна структура, где разбор выполняется один раз на текст и хранится по блокам.

### Что решает

- Разбор не зависит от курсора.
- Блок является единицей инвалидации, высоты и кэша layout.
- Spans содержат source-диапазоны, поэтому курсор и скрытие работают без повторного разбора.

### Алгоритм: источник блоков

`pulldown-cmark` с `into_offset_iter()` возвращает пары `(Event, Range<usize>)`. Для события `Start(tag)` диапазон охватывает весь элемент целиком (включая маркеры).

Блоки верхнего уровня: события, у которых глубина вложенности равна 0.

```text
depth = 0
для каждого (event, range):
    если Start: если depth == 0 -> новый блок с range; depth += 1
    если End:   depth -= 1
    если не Start/End и depth == 0 (Rule, Html и т.п.) -> одиночный блок
```

### Код: сборка блоков

```rust
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

fn markdown_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
}

pub fn parse_blocks(source: &str, base_offset: usize, ids: &mut IdGen) -> Vec<MarkdownBlock> {
    let parser = Parser::new_ext(source, markdown_options()).into_offset_iter();

    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<(Range<usize>, BlockKind, Vec<(Event<'_>, Range<usize>)>)> = None;

    for (event, range) in parser {
        match &event {
            Event::Start(tag) => {
                if depth == 0 {
                    current = Some((range.clone(), block_kind(tag), Vec::new()));
                }
                depth += 1;
                if let Some((_, _, evs)) = current.as_mut() {
                    evs.push((event, range));
                }
            }
            Event::End(_) => {
                depth -= 1;
                if let Some((_, _, evs)) = current.as_mut() {
                    evs.push((event, range));
                }
                if depth == 0 {
                    let (r, kind, evs) = current.take().unwrap();
                    blocks.push(build_block(source, base_offset, r, kind, &evs, ids));
                }
            }
            _ => {
                if depth == 0 {
                    // одиночные блоки: Rule, Html, InlineHtml на верхнем уровне
                    let kind = if matches!(event, Event::Rule) { BlockKind::Rule } else { BlockKind::Other };
                    blocks.push(build_block(source, base_offset, range.clone(), kind,
                                            &[(event, range)], ids));
                } else if let Some((_, _, evs)) = current.as_mut() {
                    evs.push((event, range));
                }
            }
        }
    }
    blocks
}

fn block_kind(tag: &Tag<'_>) -> BlockKind {
    match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { level, .. } => BlockKind::Heading(*level as u8),
        Tag::CodeBlock(_) => BlockKind::CodeFence,
        Tag::BlockQuote(_) => BlockKind::BlockQuote,
        Tag::List(_) | Tag::Item => BlockKind::ListItem,
        _ => BlockKind::Other,
    }
}
```

`base_offset` нужен, чтобы при перепарсинге куска документа (этап 7) получать абсолютные смещения: `абсолютное = base_offset + локальное`.

### Алгоритм: spans из inline-событий

Идея: при `Start(Strong)` запоминаем диапазон узла; при `End(Strong)` генерируем два маркера и стиль содержимого.

Размер маркеров по типам узлов (для исходного текста, который соответствует CommonMark):

| Узел | Открывающий маркер | Закрывающий маркер |
|---|---|---|
| Strong | `**` или `__` (2 байта) | 2 байта |
| Emphasis | `*` или `_` (1 байт) | 1 байт |
| Strikethrough | `~~` (2 байта) | 2 байта |
| Code (inline) | N обратных кавычек | N обратных кавычек |
| Link | `[` (1 байт) | от конца последнего потомка до конца узла: `](url)` |
| Heading ATX | `#`... и пробел | нет |

```rust
struct NodeFrame {
    id: NodeId,
    range: Range<usize>,       // абсолютный
    tag: FrameTag,             // Strong/Emph/Strike/Link/Heading(level)
    last_child_end: usize,     // для Link: где заканчивается текст ссылки
}

pub fn build_spans(
    source: &str,
    base_offset: usize,
    events: &[(Event<'_>, Range<usize>)],
    node_ids: &mut u32,
) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut stack: Vec<NodeFrame> = Vec::new();

    for (event, local) in events {
        let range = (base_offset + local.start)..(base_offset + local.end);
        match event {
            Event::Start(Tag::Strong) => push_frame(&mut stack, node_ids, range, FrameTag::Strong),
            Event::Start(Tag::Emphasis) => push_frame(&mut stack, node_ids, range, FrameTag::Emph),
            Event::Start(Tag::Strikethrough) => push_frame(&mut stack, node_ids, range, FrameTag::Strike),
            Event::Start(Tag::Link { .. }) => push_frame(&mut stack, node_ids, range, FrameTag::Link),
            Event::Start(Tag::Heading { level, .. }) =>
                push_frame(&mut stack, node_ids, range, FrameTag::Heading(*level as u8)),

            Event::Text(_) => {
                // запоминаем конец последнего потомка в родителе (нужно для Link)
                if let Some(top) = stack.last_mut() { top.last_child_end = range.end; }
            }

            Event::Code(_) => {
                // inline code: range включает обратные кавычки
                let id = next_id(node_ids);
                let ticks = count_leading(source, range.start - base_offset, b'`');
                spans.push(marker(id, range.start..range.start + ticks));
                spans.push(style(id, range.start + ticks..range.end - ticks, StyleKind::Code));
                spans.push(marker(id, range.end - ticks..range.end));
                if let Some(top) = stack.last_mut() { top.last_child_end = range.end; }
            }

            Event::End(TagEnd::Strong) => close_pair(&mut spans, stack.pop().unwrap(), 2, StyleKind::Bold),
            Event::End(TagEnd::Emphasis) => close_pair(&mut spans, stack.pop().unwrap(), 1, StyleKind::Italic),
            Event::End(TagEnd::Strikethrough) => close_pair(&mut spans, stack.pop().unwrap(), 2, StyleKind::Strike),

            Event::End(TagEnd::Link) => {
                let f = stack.pop().unwrap();
                // [ ... ](url)
                spans.push(marker(f.id, f.range.start..f.range.start + 1));
                spans.push(style(f.id, f.range.start + 1..f.last_child_end, StyleKind::Link));
                spans.push(marker(f.id, f.last_child_end..f.range.end)); // "](url)"
            }

            Event::End(TagEnd::Heading(_)) => {
                let f = stack.pop().unwrap();
                let FrameTag::Heading(level) = f.tag else { unreachable!() };
                // ATX: "## Текст" -> маркер = "## " (решётки + пробелы)
                let text_start = skip_hashes_and_spaces(source, f.range.start - base_offset) + base_offset;
                spans.push(marker(f.id, f.range.start..text_start));
                spans.push(style(f.id, text_start..f.range.end, StyleKind::Heading(level)));
            }
            _ => {}
        }
    }
    spans.sort_by_key(|s| s.range.start);
    spans
}

fn close_pair(spans: &mut Vec<Span>, f: NodeFrame, mlen: usize, st: StyleKind) {
    spans.push(marker(f.id, f.range.start..f.range.start + mlen));
    spans.push(style(f.id, f.range.start + mlen..f.range.end - mlen, st));
    spans.push(marker(f.id, f.range.end - mlen..f.range.end));
}
```

Ограничения, которые нужно проверить при реализации:

- Setext-заголовки (`Заголовок` + `====`) имеют другую форму маркеров; для первой версии можно не скрывать маркеры у них.
- Для заголовка ATX с закрывающими `#` в конце (`# Текст #`) закрывающие решётки тоже нужно считать маркером.
- Для автоссылок и ссылок-ссылок (`[text][ref]`) формула «до конца узла» подходит, но проверьте на примерах.
- В `Event::Text` внутри ссылок может приходить несколько событий; поэтому берётся `last_child_end`.

### Проверка этапа

- Юнит-тесты на примерах: `**a**`, `*a*`, `~~a~~`, `` `a` ``, `[a](u)`, `## a`, вложенные `**a *b* c**`.
- Для каждого проверить: объединение `Marker` и `Style` покрывает узел без пропусков и пересечений для одного узла.

---

## 7. Этап 2: высоты блоков и виртуализация

### Проблема

- Крупные заголовки и многострочные блоки имеют разную высоту.
- Чтобы прокручивать и находить блок по координате, нужны высоты всех блоков.
- Полный shaping всего документа на каждый кадр недопустим.

### Что решает

- Высота блока хранится в кэше и считается один раз на текст и ширину.
- Видимый диапазон блоков находится бинарным поиском за O(log N).
- Shaping и paint выполняются только для видимых блоков.

### Структура: индекс высот

Для первой версии хватит массива накопленных высот (prefix sums) и пересчёта «хвоста» после изменения блока. Это O(N) на правку, но простая операция по `f32`, и на 10 000 блоков она занимает единицы микросекунд.

```rust
pub struct HeightIndex {
    heights: Vec<f32>,
    /// prefix[i] = сумма высот блоков [0..i), len = n + 1
    prefix: Vec<f32>,
}

impl HeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut idx = Self { heights, prefix: Vec::new() };
        idx.rebuild_from(0);
        idx
    }

    pub fn total(&self) -> f32 { *self.prefix.last().unwrap_or(&0.0) }

    pub fn top_of(&self, block: usize) -> f32 { self.prefix[block] }

    pub fn set_height(&mut self, block: usize, h: f32) {
        if (self.heights[block] - h).abs() < 0.01 { return; }
        self.heights[block] = h;
        self.rebuild_from(block);
    }

    /// Вставка/удаление блоков (при правке текста): заменяем диапазон блоков.
    pub fn splice(&mut self, range: Range<usize>, new_heights: Vec<f32>) {
        let start = range.start;
        self.heights.splice(range, new_heights);
        self.rebuild_from(start);
    }

    fn rebuild_from(&mut self, start: usize) {
        self.prefix.resize(self.heights.len() + 1, 0.0);
        if start == 0 { self.prefix[0] = 0.0; }
        for i in start..self.heights.len() {
            self.prefix[i + 1] = self.prefix[i] + self.heights[i];
        }
    }

    /// Индекс блока, содержащего вертикальную координату y.
    pub fn block_at(&self, y: f32) -> usize {
        // первый i такой, что prefix[i+1] > y
        let i = self.prefix.partition_point(|&p| p <= y);
        i.saturating_sub(1).min(self.heights.len().saturating_sub(1))
    }

    /// Диапазон видимых блоков с запасом overscan (в пикселях).
    pub fn visible_range(&self, scroll_top: f32, viewport_h: f32, overscan: f32) -> Range<usize> {
        let top = (scroll_top - overscan).max(0.0);
        let bottom = scroll_top + viewport_h + overscan;
        let first = self.block_at(top);
        let last = self.block_at(bottom) + 1;
        first..last.min(self.heights.len())
    }
}
```

Если после замеров окажется, что пересчёт «хвоста» дорог, замените `prefix` на дерево Фенвика (Fenwick tree): обновление и запрос за O(log N). Делать это заранее не нужно.

### Откуда берутся высоты

Первая оценка высоты блока без shaping (при открытии файла), чтобы быстро показать первый кадр:

```rust
fn estimate_height(block: &MarkdownBlock, line_height: f32, width: f32, char_w: f32) -> f32 {
    let chars = (block.source_range.len() as f32).max(1.0);
    let cols = (width / char_w).max(1.0);
    let lines = (chars / cols).ceil().max(1.0);
    let scale = match block.kind {
        BlockKind::Heading(1) => 1.8,
        BlockKind::Heading(2) => 1.5,
        BlockKind::Heading(3) => 1.25,
        _ => 1.0,
    };
    lines * line_height * scale
}
```

Затем при первом показе блока его высота уточняется реальным shaping (этап 3), `HeightIndex::set_height` обновляет индекс. Видимый «прыжок» при скролле вверх компенсируется поправкой `scroll_top` на разницу высот блоков выше viewport (см. раздел о рисках).

### Рендеринг списка в GPUI

Сверьтесь с вашей версией GPUI: API `list` / `ListState` менялся. Общая схема:

```rust
// инициализация (один раз или при reset)
let state = ListState::new(blocks.len(), ListAlignment::Top, px(300.0) /* overdraw */);

// в render:
list(state.clone(), move |ix, window, cx| {
    // отрисовка блока ix: берём spans, CursorState, кэш layout
    render_block(&doc.blocks[ix], &cursor, window, cx).into_any_element()
})
.size_full()
```

При правке блоков сообщайте списку о замене диапазона, а не пересоздавайте его целиком:

```rust
// блоки [a..b) заменены на new_count блоков
state.splice(a..b, new_count);
```

При смене ширины окна кэш высот в `ListState` сбрасывается (`reset` или аналог), а разбор Markdown не трогается.

Если встроенный `list` не подходит, можно реализовать собственный виртуализатор на основе `HeightIndex::visible_range`: вы сами вычисляете, какие блоки рисовать и с каким смещением по Y.

### Проверка этапа

- На документе 5000 строк число shaped блоков за кадр ≈ числу видимых блоков (десятки), а не тысячи (счётчик `shape_calls`).
- Время кадра не растёт при увеличении файла.

---

## 8. Этап 3: shaping и кэш layout

### Проблема

Shaping (превращение текста со стилями в позиционированные глифы) дорогой. Если выполнять его для каждого видимого блока на каждый кадр, кадр будет медленным даже при виртуализации.

### Что решает

Для неизменившегося блока (тот же текст, те же стили, та же ширина, тот же режим раскрытия) используется готовый результат.

### Алгоритм: spans + CursorState -> runs

Для блока нужно получить список «пробегов» (runs): последовательных сегментов текста с одинаковым стилем и видимостью.

```text
1. Собрать границы: все start и end всех spans блока + начало/конец блока.
2. Отсортировать границы, убрать дубликаты.
3. Для каждого сегмента между соседними границами:
   - найти покрывающие spans;
   - если среди них есть Marker, принадлежащий НЕактивному узлу -> сегмент скрыт (пропускаем);
   - иначе объединить стили покрывающих Style-spans (bold + italic ...).
4. Склеить соседние сегменты с одинаковым стилем.
```

```rust
pub struct VisualRun {
    pub source: Range<usize>,
    pub style: RunStyle,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub heading: Option<u8>,
    pub marker: bool,        // показанный маркер (можно рисовать приглушённым цветом)
}

pub fn build_runs(block: &MarkdownBlock, cursor: &CursorState) -> Vec<VisualRun> {
    let mut cuts: Vec<usize> = vec![block.source_range.start, block.source_range.end];
    for s in &block.spans {
        cuts.push(s.range.start);
        cuts.push(s.range.end);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut runs: Vec<VisualRun> = Vec::new();

    for w in cuts.windows(2) {
        let seg = w[0]..w[1];
        if seg.is_empty() { continue; }

        let mut style = RunStyle::default();
        let mut hidden = false;

        for s in block.spans.iter().filter(|s| s.range.start <= seg.start && seg.end <= s.range.end) {
            match s.kind {
                SpanKind::Marker => {
                    if cursor.is_node_active(s.node) {
                        style.marker = true;       // показываем приглушённо
                    } else {
                        hidden = true;
                    }
                }
                SpanKind::Style(k) => apply_style(&mut style, k),
                SpanKind::Atomic | SpanKind::Text => {}
            }
        }
        if hidden { continue; }

        match runs.last_mut() {
            Some(last) if last.style == style && last.source.end == seg.start => last.source.end = seg.end,
            _ => runs.push(VisualRun { source: seg, style }),
        }
    }
    runs
}
```

Для блоков с большим числом spans фильтр `iter().filter(...)` внутри цикла даёт O(S^2). Для первой версии допустимо (spans на блок обычно единицы или десятки). Если блок огромный (длинный абзац с сотнями ссылок), используйте sweep-line: обходите границы по порядку и поддерживайте множество активных spans.

### Ключ кэша

```rust
#[derive(Hash, PartialEq, Eq, Clone)]
pub struct LayoutKey {
    pub block_id: BlockId,
    pub text_hash: u64,         // хэш текста блока (или номер ревизии блока)
    pub active_hash: u64,       // какие узлы раскрыты в этом блоке
    pub width_px: u32,          // ширина в пикселях, округлённая
    pub scale_factor_x100: u32, // масштаб дисплея
    pub style_rev: u32,         // ревизия темы/шрифтов
}
```

Практичнее вместо `text_hash` хранить в блоке `rev: u64` и увеличивать при его перепарсинге: дешевле, чем хэшировать текст.

```rust
pub struct LayoutCache {
    map: HashMap<LayoutKey, Rc<BlockShaped>>,
    order: VecDeque<LayoutKey>,   // для простого LRU
    cap: usize,
}

impl LayoutCache {
    pub fn get_or_insert_with(
        &mut self,
        key: LayoutKey,
        make: impl FnOnce() -> BlockShaped,
    ) -> Rc<BlockShaped> {
        if let Some(v) = self.map.get(&key) {
            PERF.layout_cache_hit.fetch_add(1, Relaxed);
            return v.clone();
        }
        PERF.layout_cache_miss.fetch_add(1, Relaxed);
        let v = Rc::new(make());
        self.map.insert(key.clone(), v.clone());
        self.order.push_back(key);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() { self.map.remove(&old); }
        }
        v
    }
}
```

`BlockShaped` содержит результат shaping (для GPUI это `ShapedLine` или список `WrappedLine`, получаемых через `window.text_system()`; сверьтесь с вашей версией) и вычисленную высоту.

### Сначала проверьте встроенный кэш GPUI

GPUI уже кэширует layout строк по тексту, шрифту и прогонам стилей. Прежде чем писать свой `LayoutCache`:

1. Убедитесь по исходникам вашей версии, что кэш действительно переиспользуется между кадрами при одинаковых аргументах.
2. Проверьте, что вы передаёте одинаковые `TextRun`-ы для неизменившегося блока (одинаковая последовательность, длина и стиль). Если ваш код каждый раз строит чуть другой набор run-ов, кэш будет промахиваться.
3. Если встроенного кэша достаточно, свой не нужен: достаточно виртуализации и стабильных run-ов.

### Проверка этапа

- После нажатия стрелки в пределах одного блока: `miss <= 1` (блок поменял режим раскрытия).
- Стрелка между блоками: `miss <= 2`.
- При скролле без правок: `miss` только для новых блоков, входящих в viewport.

---

## 9. Этап 4: курсор и раскрытие синтаксиса

### Проблема

При движении курсора нужно раскрывать маркеры узла, в который вошла каретка, и скрывать маркеры узла, который она покинула. Нельзя из-за этого пересчитывать весь документ.

### Что решает

Определяет, какие узлы активны, находит их за O(log S) по блоку и инвалидирует только затронутые блоки.

### Правило активности узла

Узел активен, если каретка находится в `node.range` включая границы:

```text
node.range.start <= pos <= node.range.end
```

Включение границ важно: при движении стрелкой справа налево каретка сначала оказывается сразу после `**` (на границе), и маркеры уже раскрываются до того, как каретка попадёт внутрь. Тогда не возникает «прыжка» по скрытым символам, и не нужно делать все маркеры atomic.

### Алгоритм: найти активные узлы

1. Бинарным поиском найти блок по `pos` (по `source_range`).
2. В блоке найти spans, чей диапазон содержит `pos`, и собрать их `node`.
3. Добавить узлы, у которых `pos` совпадает с границей (`range.end == pos`).

```rust
impl MarkdownDocument {
    pub fn block_index_at(&self, pos: usize) -> Option<usize> {
        let i = self.blocks.partition_point(|b| b.source_range.end < pos);
        // блок i заканчивается не раньше pos
        (i < self.blocks.len() && self.blocks[i].source_range.start <= pos).then_some(i)
    }

    pub fn cursor_state(&self, pos: usize) -> CursorState {
        let Some(bi) = self.block_index_at(pos) else {
            return CursorState { pos, active_block: None, active_nodes: Default::default() };
        };
        let block = &self.blocks[bi];
        let mut nodes: SmallVec<[NodeId; 4]> = SmallVec::new();
        for s in &block.spans {
            if s.range.start <= pos && pos <= s.range.end && !nodes.contains(&s.node) {
                nodes.push(s.node);
            }
        }
        CursorState { pos, active_block: Some(block.id), active_nodes: nodes }
    }
}

impl CursorState {
    pub fn is_node_active(&self, node: NodeId) -> bool { self.active_nodes.contains(&node) }
}
```

Spans внутри блока отсортированы по `start`, так что перебор можно ограничить `partition_point` по `start <= pos`, если блок большой.

Заметка про заголовки: узел `Heading` занимает весь блок, поэтому на любой позиции внутри заголовка раскрывается `# `. Это ожидаемое поведение.

### Алгоритм: обновление при движении курсора

```rust
impl Editor {
    pub fn update_cursor(&mut self, new_pos: usize, cx: &mut Context<Self>) {
        let new = self.doc.cursor_state(new_pos);

        // Вариант 1: ничего не изменилось в визуальном смысле — двигаем только каретку.
        if new.active_block == self.cursor.active_block
            && new.active_nodes == self.cursor.active_nodes
        {
            self.cursor = new;
            cx.notify();           // перерисовать каретку; layout блоков из кэша
            return;
        }

        // Вариант 2: изменился набор активных узлов — инвалидируем старый и новый блок.
        let old = std::mem::replace(&mut self.cursor, new);
        for b in [old.active_block, self.cursor.active_block].into_iter().flatten() {
            self.invalidate_block_layout(b);       // сбросить кэш, не разбор
            PERF.blocks_invalidated.fetch_add(1, Relaxed);
        }
        cx.notify();
    }
}
```

Здесь `active_hash` в `LayoutKey` (этап 3) считается от `active_nodes` блока. Нет необходимости вручную чистить кэш: изменится ключ, и будет miss только для затронутых блоков. Явная `invalidate_block_layout` нужна только если вы кэшируете состояние иначе.

### Проверка этапа

Сценарий: курсор перемещается стрелкой по строке `Текст **жирный** текст`.

| Положение | Ожидаемо |
|---|---|
| До `**` (не на границе) | маркеры скрыты |
| На границе перед `**` | маркеры раскрыты |
| Внутри «жирный» | раскрыты |
| На границе после `**` | раскрыты |
| Дальше | скрыты |

Счётчики: `parse=0`, `spans=0`, `miss<=2`.

---

## 10. Этап 5: соответствие source <-> visual и навигация

### Проблема

Когда часть символов скрыта, смещение в исходнике и позиция на экране не совпадают. Нужно:

- рисовать каретку в правильном месте;
- по клику находить позицию в исходнике;
- обрабатывать стрелки и Backspace рядом со скрытыми диапазонами.

### Что решает

Двусторонняя карта «source offset <-> visual offset» по блоку, вычисляемая из spans и CursorState, без обхода документа.

### Алгоритм

Скрытые диапазоны блока (после применения CursorState) задают массив непересекающихся отрезков `hidden[i] = (start, end)`, отсортированных по start. Для каждого вычисляем накопленную длину скрытого до него.

```text
visual(source) = source - (сумма длин скрытых диапазонов, лежащих целиком до source)
                        - (часть скрытого диапазона, в который попал source, до source)

source(visual): бинарный поиск по массиву "visual-позиций начала скрытых диапазонов".
```

```rust
pub struct VisualMap {
    base: usize,                  // block.source_range.start
    hidden: Vec<Range<usize>>,    // непересекающиеся, отсортированы
    cum_before: Vec<usize>,       // cum_before[i] = суммарная длина hidden[0..i)
}

impl VisualMap {
    pub fn new(base: usize, mut hidden: Vec<Range<usize>>) -> Self {
        hidden.sort_by_key(|r| r.start);
        // слить пересекающиеся и соседние
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in hidden {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        let mut cum = Vec::with_capacity(merged.len());
        let mut acc = 0;
        for r in &merged { cum.push(acc); acc += r.len(); }
        Self { base, hidden: merged, cum_before: cum }
    }

    /// source offset -> visual offset (в байтах видимого текста блока)
    pub fn to_visual(&self, src: usize) -> usize {
        // сколько скрытых диапазонов начинаются не позже src
        let i = self.hidden.partition_point(|r| r.start <= src);
        let mut hidden_before = if i > 0 { self.cum_before[i - 1] } else { 0 };
        if i > 0 {
            let r = &self.hidden[i - 1];
            // добавляем длину самого диапазона, но не больше, чем до src
            hidden_before += r.end.min(src) - r.start;
        }
        src - self.base - hidden_before
    }

    /// visual offset -> source offset
    pub fn to_source(&self, vis: usize) -> usize {
        // идём по скрытым диапазонам: позиция source = vis + base + (длина скрытого перед ней)
        let mut src = vis + self.base;
        for (r, cum) in self.hidden.iter().zip(&self.cum_before) {
            // визуальная позиция начала этого скрытого диапазона
            let vis_start = r.start - self.base - cum;
            if vis >= vis_start { src = vis + self.base + cum + r.len(); } else { break; }
        }
        src
    }
}
```

Замечания:

- `to_source` на границе скрытого диапазона выбирает позицию после него. Для каретки перед маркером это нужно уточнить правилами ниже.
- Для длинных блоков вместо линейного цикла в `to_source` используйте бинарный поиск по `vis_start` (массив vis_start считается один раз вместе с `cum_before`).
- Работайте в границах UTF-8: смещения должны всегда указывать на границы символов. Проверяйте `source.is_char_boundary(...)` при создании диапазонов маркеров (для ASCII-маркеров это выполняется автоматически).

### Навигация стрелками

Благодаря правилу активности «на границе включительно» (этап 4) каретка не оказывается внутри скрытого маркера: при подходе к узлу он раскрывается, и каретка двигается по обычным символам. Поэтому для обычных узлов нужна стандартная навигация по символам исходника.

Исключения, где нужна особая логика:

| Случай | Поведение |
|---|---|
| `Atomic` (виджет, embed, скрытый URL при необходимости) | стрелка перепрыгивает диапазон целиком |
| Каретка попала в скрытый диапазон (например, после клика) | сместить каретку к ближайшей границе |
| Backspace у границы маркера | удалять маркер целиком или символ - решите по UX (в Obsidian при раскрытых маркерах это обычное удаление символа) |

```rust
/// После любого изменения позиции: если pos оказался внутри Atomic-диапазона, вынести наружу.
fn normalize_pos(block: &MarkdownBlock, pos: usize, moving_right: bool) -> usize {
    for s in &block.spans {
        if s.kind == SpanKind::Atomic && s.range.start < pos && pos < s.range.end {
            return if moving_right { s.range.end } else { s.range.start };
        }
    }
    pos
}
```

### Каретка и клик

- Рисование каретки: найти блок, получить `to_visual(pos)`, затем координаты каретки из shaped-строки (в GPUI у `ShapedLine`/`WrappedLine` есть методы позиции по индексу; сверьтесь с версией).
- Клик: координаты -> блок через `HeightIndex::block_at(y)` -> индекс в видимом тексте через shaped-строку (`index_for_x` или аналог) -> `to_source`.

### Проверка этапа

Юнит-тесты `VisualMap`: для всех `src` в блоке `to_source(to_visual(src)) == src` для видимых позиций; монотонность `to_visual`.

---

## 11. Этап 6: заголовки и другие layout-стили

### Проблема

Крупный шрифт заголовков меняет высоту строки. Если высота зависит от того, активен ли заголовок, то при движении курсора вся ниже лежащая часть документа «прыгает» по вертикали и требуется пересчёт геометрии.

### Что решает

Правило: размер шрифта и межстрочное расстояние блока не зависят от состояния активности. Раскрытие маркеров меняет только ширину текста (добавляется `# `), высота остаётся той же, если не меняется число строк после переноса.

### Реализация

```rust
fn font_size_for(kind: BlockKind, base: f32) -> f32 {
    match kind {
        BlockKind::Heading(1) => base * 1.8,
        BlockKind::Heading(2) => base * 1.5,
        BlockKind::Heading(3) => base * 1.25,
        BlockKind::Heading(_) => base * 1.1,
        _ => base,
    }
}
```

Размер зависит от `BlockKind`, а не от `CursorState`. Раскрытый `# ` рисуется тем же шрифтом (можно приглушённым цветом через `RunStyle::marker`).

### Нюанс: перенос строк

Добавление `# ` при раскрытии может увеличить число строк у заголовка, который находился на границе ширины. Тогда высота блока изменится на одну строку.

Обработка: после shaping активного блока сравните новую высоту с сохранённой. Если отличается, вызовите `HeightIndex::set_height` и подправьте `scroll_top` так, чтобы видимая позиция каретки не сместилась (см. «Риски»). Для первой версии допустимо принять это редкое смещение.

### Другие layout-affecting элементы

| Элемент | Высота зависит от | Комментарий |
|---|---|---|
| Заголовки | уровня | постоянная |
| Блоки кода | числа строк | менять только при правке |
| Изображения/embed | размера картинки | загрузка асинхронно, `set_height` после загрузки |
| Таблицы | содержимого | отдельный блок |

---

## 12. Этап 7: инкрементальная обработка правок

### Проблема

При вводе символа нельзя перепарсивать весь документ. Нужно определить, какие блоки затронуты, и обновить только их.

### Что решает

Время обработки правки зависит от размера затронутых блоков, а не файла.

### Алгоритм

Вход: правка `(range: Range<usize>, new_text: &str)` в координатах старого документа. `delta = new_text.len() as isize - range.len() as isize`.

```text
1. Применить правку к Rope.
2. Найти блоки, пересекающиеся с range: индексы [a..=b] (бинарный поиск по source_range).
3. Расширить на один блок в обе стороны (a-1, b+1), потому что правка может
   слить или разбить блоки (удалили пустую строку между абзацами и т.п.).
4. Взять срез исходника от blocks[a'].start до blocks[b'].end + delta.
5. Перепарсить срез (parse_blocks с base_offset = start срез).
6. Заменить blocks[a'..=b'] новыми блоками.
7. У всех блоков после b' сдвинуть source_range и spans на delta.
8. HeightIndex::splice(a'..=b', новые высоты). ListState::splice(...).
9. Проверить «опасные» правки (см. ниже) -> при необходимости расширить область.
```

```rust
pub fn apply_edit(&mut self, range: Range<usize>, new_text: &str) {
    let delta = new_text.len() as isize - range.len() as isize;
    self.source.replace(range.clone(), new_text);

    // 2-3. затронутые блоки с запасом
    let mut a = self.blocks.partition_point(|b| b.source_range.end < range.start);
    let mut b = self.blocks.partition_point(|b| b.source_range.start <= range.end);
    a = a.saturating_sub(1);
    b = (b + 1).min(self.blocks.len());       // b — эксклюзивная граница

    let start = self.blocks.get(a).map(|x| x.source_range.start).unwrap_or(0);
    let old_end = self.blocks.get(b.saturating_sub(1)).map(|x| x.source_range.end).unwrap_or(start);
    let new_end = (old_end as isize + delta) as usize;

    // 4-5. перепарсить срез
    let slice = self.source.slice_to_string(start..new_end);
    let new_blocks = parse_blocks(&slice, start, &mut self.ids);

    // 6. заменить
    let new_count = new_blocks.len();
    self.blocks.splice(a..b, new_blocks);

    // 7. сдвиг хвоста
    for blk in &mut self.blocks[a + new_count..] {
        shift_block(blk, delta);
    }

    // 8. высоты
    let heights: Vec<f32> = self.blocks[a..a + new_count].iter()
        .map(|blk| self.estimate_or_cached_height(blk))
        .collect();
    self.heights.splice(a..b, heights);
}

fn shift_block(b: &mut MarkdownBlock, delta: isize) {
    b.source_range = shift_range(&b.source_range, delta);
    for s in &mut b.spans { s.range = shift_range(&s.range, delta); }
}
```

Сдвиг хвоста O(число блоков после правки): это простой проход по числам. Если окажется дорогим, храните смещения блоков относительно начала предыдущего (ленивый сдвиг) или используйте SumTree/Rope-подобную структуру.

### «Опасные» правки, которые меняют разбор далеко от места правки

| Правка | Что может измениться |
|---|---|
| Ввод или удаление `` ``` `` | блоки кода захватывают или отпускают весь хвост документа |
| Ввод или удаление `---` вверху | front matter |
| Ввод `>`, `-`, `1.` в начале строки | тип блока меняется |
| Удаление пустой строки между абзацами | блоки сливаются |
| Ввод `<!--` или HTML-блок | комментарий уходит до закрытия |

Простая надёжная стратегия:

```rust
fn needs_wide_reparse(edit_text: &str, removed: &str) -> bool {
    edit_text.contains("```") || removed.contains("```")
        || edit_text.contains("<!--") || removed.contains("<!--")
        || edit_text.contains("---") || removed.contains("---")
}
```

Если `true`, перепарсить от затронутого блока до конца документа (или до следующего стабильного блока, у которого `kind` и `source_range` совпали со старыми). Это редкий случай, и он допустим.

Дополнительная проверка после перепарсинга: если последний новый блок заканчивается не на границе старого блока (`new_end`), расширьте область на следующий блок и повторите. Так вы ловите изменения, которые «сдвинули» границы дальше предположенных.

### Проверка этапа

- Тест: случайные правки на документе, после каждой сравнить `blocks` с полным перепарсингом `parse_blocks(whole_doc)` (property-тест). Структуры должны совпасть (kind и source_range).
- Счётчик `parse_calls` и размер среза при обычном вводе: один блок, не весь документ.

---

## 13. Этап 8: оптимизации после профилирования

Делать только если замеры из этапа 0 показывают проблему.

| Симптом | Решение |
|---|---|
| Prefix sums дорого пересчитывать при частых правках | дерево Фенвика или SumTree для высот |
| Блок с сотнями spans тормозит в `build_runs` | sweep-line вместо вложенного фильтра |
| Первый разбор большого файла блокирует UI | разбор в фоновом потоке по частям, первыми блоки около позиции курсора |
| Скролл дёргается при уточнении высот | хранить якорь (block_id + смещение внутри блока) и корректировать `scroll_top` после `set_height` |
| Много аллокаций в `render` | переиспользуемые буферы (`Vec` с `clear()`), `SmallVec` |
| Кэш layout растёт | LRU с лимитом (десятки блоков вокруг viewport) |
| Диапазоны для очень больших блоков | interval tree по `Anchor` |
| Огромный файл (от ~100 тыс. строк), где Markdown лишь в малой части строк | склеивать подряд идущие простые строки в один блок `BlockKind::PlainRun { lines }` без spans (высота = число строк * высота строки); консервативный классификатор строки (любое сомнение = отдавать в полный разбор); состояние fenced-блока считать линейным сканом на границах кусков; разбирать лениво и в фоне, начиная с блоков около viewport |

---

## 14. Тесты

### Юнит-тесты

```rust
#[test]
fn bold_produces_two_markers_and_style() {
    let src = "a **b** c";
    let blocks = parse_blocks(src, 0, &mut IdGen::default());
    let spans = &blocks[0].spans;
    let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].range, 2..4);
    assert_eq!(markers[1].range, 5..7);
}

#[test]
fn markers_hidden_when_cursor_outside() {
    let doc = MarkdownDocument::from_str("a **b** c");
    let cursor = doc.cursor_state(0);               // в начале
    let runs = build_runs(&doc.blocks[0], &cursor);
    assert!(runs.iter().all(|r| !r.style.marker));  // маркеры скрыты
}

#[test]
fn markers_shown_when_cursor_on_boundary() {
    let doc = MarkdownDocument::from_str("a **b** c");
    let cursor = doc.cursor_state(2);               // перед **
    let runs = build_runs(&doc.blocks[0], &cursor);
    assert!(runs.iter().any(|r| r.style.marker));
}

#[test]
fn visual_map_roundtrip() {
    let m = VisualMap::new(0, vec![2..4, 5..7]);
    assert_eq!(m.to_visual(4), 2);
    assert_eq!(m.to_visual(5), 2);
    assert_eq!(m.to_visual(7), 3);
    assert_eq!(m.to_source(2), 4);   // после скрытого диапазона
}

#[test]
fn height_index_block_at() {
    let h = HeightIndex::new(vec![10.0, 20.0, 30.0]);
    assert_eq!(h.block_at(0.0), 0);
    assert_eq!(h.block_at(10.0), 1);
    assert_eq!(h.block_at(29.9), 1);
    assert_eq!(h.block_at(30.0), 2);
}
```

### Property-тест инкрементальности (proptest)

```text
для случайного документа D и случайной правки E:
    A = parse_blocks(apply(D, E))                  // полный перепарсинг
    B = doc.apply_edit(E); doc.blocks              // инкрементально
    assert структуры A и B совпадают (kind, source_range, множество spans)
```

### Бенчмарки (criterion)

- `parse_blocks` на 50 / 500 / 5000 строк;
- `cursor_state` + `update_cursor` на 5000 строк (должно быть микросекунды и не зависеть от размера);
- `apply_edit` на 5000 строк при вводе символа в середине;
- `build_runs` для блока с 10 / 100 / 1000 spans.

---

## 15. Критерии готовности

- [ ] Обработчик стрелки: parse=0, span_rebuilds=0, инвалидировано <= 2 блока.
- [ ] Время кадра после стрелки не зависит от размера файла (50 и 5000 строк в одном порядке).
- [ ] Видимых shaped блоков за кадр = числу видимых блоков.
- [ ] Курсор на границе узла раскрывает маркеры, вне узла скрывает.
- [ ] Каретка и клик попадают в правильную позицию при скрытых маркерах.
- [ ] Ввод символа перепарсит только затронутые блоки; property-тест проходит.
- [ ] Заголовки не меняют высоту при движении курсора.
- [ ] Смена ширины окна не запускает перепарсинг.

---

## 16. Риски и открытые вопросы

| Риск | Что делать |
|---|---|
| API GPUI (`list`, `ListState`, shaping) отличается в вашей версии | прочитать исходники конкретного commit до реализации этапов 2-3 |
| Скролл «прыгает» при уточнении оценочных высот | хранить якорь (block_id + смещение) и корректировать `scroll_top`; либо измерять блоки вокруг viewport заранее |
| Многострочные inline-конструкции и вложенные списки | в первой версии не скрывать маркеры у сложных конструкций, добавлять по одной |
| Setext-заголовки, таблицы, HTML-блоки | сначала показывать как исходный текст |
| Неполный Markdown при наборе (незакрытый `**`) | CommonMark такой текст считает обычным; расчёт маркеров идёт только по реально распознанным узлам |
| UTF-8: смещения внутри многобайтовых символов | все диапазоны только на границах символов; проверять `is_char_boundary` |
| Undo/redo | работает над Rope; после отката вызывать тот же `apply_edit` |
| Выделение, охватывающее много блоков | выделение рисуется поверх видимых блоков по диапазонам; считать только для видимой области |
| Поиск и замена | работают по `source`, подсветка совпадений как отдельный слой |
| Очень большие файлы (десятки МБ) | запасной вариант: выше порога отключать Live Preview и показывать обычный текст с подсветкой только видимой области; `BlockKind` оставить расширяемым под `PlainRun` |

### Рекомендуемый порядок работ

1. Этап 0: счётчики и замеры текущего кода (зафиксировать «до»).
2. Этап 2 + 3 (виртуализация и проверка кэша GPUI): обычно дают основной выигрыш.
3. Этап 4: минимальное обновление при движении курсора.
4. Этап 1: перевод разбора на блоки (если ещё не сделано).
5. Этап 5-6: карта source/visual, заголовки.
6. Этап 7: инкрементальные правки.
7. Этап 8: только по результатам замеров.
