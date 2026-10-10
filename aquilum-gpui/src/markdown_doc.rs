use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use smallvec::SmallVec;
use std::ops::Range;

pub type BlockId = u64;
pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    CodeFence,
    BlockQuote,
    ListItem,
    Rule,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Text,
    Marker,
    Style(StyleKind),
    Atomic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleKind {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Heading(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: SpanKind,
    pub node: NodeId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockLayout {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct MarkdownBlock {
    pub id: BlockId,
    pub source_range: Range<usize>,
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    pub layout: Option<BlockLayout>,
}

#[derive(Debug, Clone, Default)]
pub struct IdGen {
    next_block_id: BlockId,
    next_node_id: NodeId,
}

impl IdGen {
    pub fn new() -> Self {
        Self {
            next_block_id: 1,
            next_node_id: 1,
        }
    }

    pub fn next_block_id(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    pub fn next_node_id(&mut self) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CursorState {
    pub pos: usize,
    pub active_block: Option<BlockId>,
    pub active_nodes: SmallVec<[NodeId; 4]>,
}

impl CursorState {
    pub fn is_node_active(&self, node: NodeId) -> bool {
        self.active_nodes.contains(&node)
    }
}

pub fn markdown_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn block_kind(tag: &Tag<'_>) -> BlockKind {
    match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { level, .. } => BlockKind::Heading(heading_level_to_u8(*level)),
        Tag::CodeBlock(_) => BlockKind::CodeFence,
        Tag::BlockQuote(_) => BlockKind::BlockQuote,
        Tag::List(_) | Tag::Item => BlockKind::ListItem,
        _ => BlockKind::Other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FrameTag {
    Strong,
    Emph,
    Strike,
    Link,
    Heading(u8),
}

struct NodeFrame {
    id: NodeId,
    range: Range<usize>,
    tag: FrameTag,
    last_child_end: usize,
}

fn count_leading_bytes(source: &str, start: usize, byte: u8) -> usize {
    let bytes = source.as_bytes();
    let mut count = 0;
    while start + count < bytes.len() && bytes[start + count] == byte {
        count += 1;
    }
    count
}

fn count_marker_pair_bytes(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    if start >= bytes.len() || end > bytes.len() || start >= end {
        return 1;
    }
    let first = bytes[start];
    if first == b'*' || first == b'_' || first == b'~' {
        let mut open_len = 0;
        while start + open_len < end && bytes[start + open_len] == first {
            open_len += 1;
        }
        let mut close_len = 0;
        while end > start + close_len && bytes[end - 1 - close_len] == first {
            close_len += 1;
        }
        open_len.min(close_len).max(1)
    } else {
        1
    }
}

fn skip_hashes_and_spaces(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = start;
    while i < bytes.len() && (bytes[i] == b'#' || bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

fn marker(node: NodeId, range: Range<usize>) -> Span {
    Span {
        range,
        kind: SpanKind::Marker,
        node,
    }
}

fn style(node: NodeId, range: Range<usize>, kind: StyleKind) -> Span {
    Span {
        range,
        kind: SpanKind::Style(kind),
        node,
    }
}

pub fn build_spans(
    source: &str,
    base_offset: usize,
    events: &[(Event<'_>, Range<usize>)],
    ids: &mut IdGen,
) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut stack: Vec<NodeFrame> = Vec::new();

    for (event, local) in events {
        let range = (base_offset + local.start)..(base_offset + local.end);
        match event {
            Event::Start(Tag::Strong) => {
                let id = ids.next_node_id();
                stack.push(NodeFrame {
                    id,
                    range,
                    tag: FrameTag::Strong,
                    last_child_end: 0,
                });
            }
            Event::Start(Tag::Emphasis) => {
                let id = ids.next_node_id();
                stack.push(NodeFrame {
                    id,
                    range,
                    tag: FrameTag::Emph,
                    last_child_end: 0,
                });
            }
            Event::Start(Tag::Strikethrough) => {
                let id = ids.next_node_id();
                stack.push(NodeFrame {
                    id,
                    range,
                    tag: FrameTag::Strike,
                    last_child_end: 0,
                });
            }
            Event::Start(Tag::Link { .. }) => {
                let id = ids.next_node_id();
                let last_child_end = range.start + 1;
                stack.push(NodeFrame {
                    id,
                    range,
                    tag: FrameTag::Link,
                    last_child_end,
                });
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let id = ids.next_node_id();
                let lvl = heading_level_to_u8(*level);
                stack.push(NodeFrame {
                    id,
                    range,
                    tag: FrameTag::Heading(lvl),
                    last_child_end: 0,
                });
            }
            Event::Text(_) => {
                if let Some(top) = stack.last_mut() {
                    top.last_child_end = range.end;
                }
            }
            Event::Code(_) => {
                let id = ids.next_node_id();
                let local_start = local.start;
                let ticks = count_leading_bytes(source, local_start, b'`').max(1);
                if range.len() >= 2 * ticks {
                    spans.push(marker(id, range.start..range.start + ticks));
                    spans.push(style(id, range.start + ticks..range.end - ticks, StyleKind::Code));
                    spans.push(marker(id, range.end - ticks..range.end));
                }
                if let Some(top) = stack.last_mut() {
                    top.last_child_end = range.end;
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some(f) = stack.pop() {
                    let local_start = f.range.start - base_offset;
                    let local_end = f.range.end - base_offset;
                    let mlen = count_marker_pair_bytes(source, local_start, local_end);
                    if f.range.len() >= 2 * mlen {
                        spans.push(marker(f.id, f.range.start..f.range.start + mlen));
                        spans.push(style(f.id, f.range.start + mlen..f.range.end - mlen, StyleKind::Bold));
                        spans.push(marker(f.id, f.range.end - mlen..f.range.end));
                    }
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(f) = stack.pop() {
                    let local_start = f.range.start - base_offset;
                    let local_end = f.range.end - base_offset;
                    let mlen = count_marker_pair_bytes(source, local_start, local_end);
                    if f.range.len() >= 2 * mlen {
                        spans.push(marker(f.id, f.range.start..f.range.start + mlen));
                        spans.push(style(f.id, f.range.start + mlen..f.range.end - mlen, StyleKind::Italic));
                        spans.push(marker(f.id, f.range.end - mlen..f.range.end));
                    }
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(f) = stack.pop() {
                    let local_start = f.range.start - base_offset;
                    let local_end = f.range.end - base_offset;
                    let mlen = count_marker_pair_bytes(source, local_start, local_end);
                    if f.range.len() >= 2 * mlen {
                        spans.push(marker(f.id, f.range.start..f.range.start + mlen));
                        spans.push(style(f.id, f.range.start + mlen..f.range.end - mlen, StyleKind::Strike));
                        spans.push(marker(f.id, f.range.end - mlen..f.range.end));
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(f) = stack.pop() {
                    if f.range.len() > 1 && f.last_child_end >= f.range.start + 1 && f.last_child_end <= f.range.end {
                        spans.push(marker(f.id, f.range.start..f.range.start + 1));
                        spans.push(style(f.id, f.range.start + 1..f.last_child_end, StyleKind::Link));
                        spans.push(marker(f.id, f.last_child_end..f.range.end));
                    }
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(f) = stack.pop() {
                    let FrameTag::Heading(lvl) = f.tag else { continue };
                    let local_start = f.range.start - base_offset;
                    let local_text_start = skip_hashes_and_spaces(source, local_start);
                    let text_start = base_offset + local_text_start;
                    let text_start = text_start.min(f.range.end);
                    spans.push(marker(f.id, f.range.start..text_start));
                    if text_start < f.range.end {
                        spans.push(style(f.id, text_start..f.range.end, StyleKind::Heading(lvl)));
                    }
                }
            }
            _ => {}
        }
    }

    spans.sort_by_key(|s| s.range.start);
    spans
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
                depth = depth.saturating_sub(1);
                if let Some((_, _, evs)) = current.as_mut() {
                    evs.push((event, range));
                }
                if depth == 0 {
                    if let Some((r, kind, evs)) = current.take() {
                        let id = ids.next_block_id();
                        let spans = build_spans(source, base_offset, &evs, ids);
                        blocks.push(MarkdownBlock {
                            id,
                            source_range: (base_offset + r.start)..(base_offset + r.end),
                            kind,
                            spans,
                            layout: None,
                        });
                    }
                }
            }
            _ => {
                if depth == 0 {
                    let kind = if matches!(event, Event::Rule) {
                        BlockKind::Rule
                    } else {
                        BlockKind::Other
                    };
                    let id = ids.next_block_id();
                    let spans = build_spans(source, base_offset, &[(event, range.clone())], ids);
                    blocks.push(MarkdownBlock {
                        id,
                        source_range: (base_offset + range.start)..(base_offset + range.end),
                        kind,
                        spans,
                        layout: None,
                    });
                } else if let Some((_, _, evs)) = current.as_mut() {
                    evs.push((event, range));
                }
            }
        }
    }
    blocks
}

pub fn font_size_for(kind: BlockKind, base: f32) -> f32 {
    match kind {
        BlockKind::Heading(1) => base * 1.8,
        BlockKind::Heading(2) => base * 1.5,
        BlockKind::Heading(3) => base * 1.25,
        BlockKind::Heading(_) => base * 1.1,
        _ => base,
    }
}

pub fn line_height_for(kind: BlockKind, base: f32) -> f32 {
    match kind {
        BlockKind::Heading(1) => base * 1.8,
        BlockKind::Heading(2) => base * 1.5,
        BlockKind::Heading(3) => base * 1.25,
        BlockKind::Heading(_) => base * 1.1,
        _ => base,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTypography {
    pub font_size: f32,
    pub line_height: f32,
}

impl BlockTypography {
    pub fn for_block(kind: BlockKind, base_font_size: f32, base_line_height: f32) -> Self {
        Self {
            font_size: font_size_for(kind, base_font_size),
            line_height: line_height_for(kind, base_line_height),
        }
    }
}

pub fn estimate_height(block: &MarkdownBlock, line_height: f32, width: f32, char_w: f32) -> f32 {
    let chars = (block.source_range.len() as f32).max(1.0);
    let cols = (width / char_w).max(1.0);
    let lines = (chars / cols).ceil().max(1.0);
    lines * line_height_for(block.kind, line_height)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeightIndex {
    heights: Vec<f32>,
    /// prefix[i] = сумма высот блоков [0..i), len = heights.len() + 1
    prefix: Vec<f32>,
}

impl HeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut idx = Self {
            heights,
            prefix: Vec::new(),
        };
        idx.rebuild_from(0);
        idx
    }

    pub fn from_blocks(blocks: &[MarkdownBlock], line_height: f32, width: f32, char_w: f32) -> Self {
        let heights = blocks
            .iter()
            .map(|b| estimate_height(b, line_height, width, char_w))
            .collect();
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

    pub fn top_of(&self, block: usize) -> f32 {
        if block < self.prefix.len() {
            self.prefix[block]
        } else {
            self.total()
        }
    }

    pub fn height_of(&self, block: usize) -> f32 {
        self.heights.get(block).copied().unwrap_or(0.0)
    }

    pub fn set_height(&mut self, block: usize, h: f32) {
        if block >= self.heights.len() {
            return;
        }
        if (self.heights[block] - h).abs() < 0.01 {
            return;
        }
        self.heights[block] = h;
        self.rebuild_from(block);
    }

    /// Вставка/удаление блоков (при правке текста): заменяем диапазон блоков.
    pub fn splice(&mut self, range: Range<usize>, new_heights: Vec<f32>) {
        let start = range.start.min(self.heights.len());
        let end = range.end.min(self.heights.len()).max(start);
        self.heights.splice(start..end, new_heights);
        self.rebuild_from(start);
    }

    fn rebuild_from(&mut self, start: usize) {
        self.prefix.resize(self.heights.len() + 1, 0.0);
        if start == 0 {
            self.prefix[0] = 0.0;
        }
        for i in start..self.heights.len() {
            self.prefix[i + 1] = self.prefix[i] + self.heights[i];
        }
    }

    /// Индекс блока, содержащего вертикальную координату y.
    pub fn block_at(&self, y: f32) -> usize {
        if self.heights.is_empty() || y <= 0.0 {
            return 0;
        }
        // первый i такой, что prefix[i+1] > y
        let i = self.prefix.partition_point(|&p| p <= y);
        i.saturating_sub(1).min(self.heights.len().saturating_sub(1))
    }

    /// Диапазон видимых блоков с запасом overscan (в пикселях).
    pub fn visible_range(&self, scroll_top: f32, viewport_h: f32, overscan: f32) -> Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let top = (scroll_top - overscan).max(0.0);
        let bottom = scroll_top + viewport_h + overscan;
        let first = self.block_at(top);
        let last = self.block_at(bottom) + 1;
        first..last.min(self.heights.len())
    }
}

pub struct MarkdownDocument {
    pub source: String,
    pub blocks: Vec<MarkdownBlock>,
    pub heights: HeightIndex,
    pub ids: IdGen,
}

impl MarkdownDocument {
    pub fn new(source: String) -> Self {
        Self::with_metrics(source, 24.0, 800.0, 8.0)
    }

    pub fn with_metrics(source: String, line_height: f32, width: f32, char_w: f32) -> Self {
        let mut ids = IdGen::new();
        let blocks = parse_blocks(&source, 0, &mut ids);
        let heights = HeightIndex::from_blocks(&blocks, line_height, width, char_w);
        Self {
            source,
            blocks,
            heights,
            ids,
        }
    }

    pub fn recompute_heights(&mut self, line_height: f32, width: f32, char_w: f32) {
        self.heights = HeightIndex::from_blocks(&self.blocks, line_height, width, char_w);
    }

    pub fn block_index_at(&self, pos: usize) -> Option<usize> {
        let i = self.blocks.partition_point(|b| b.source_range.end < pos);
        (i < self.blocks.len() && self.blocks[i].source_range.start <= pos).then_some(i)
    }

    pub fn cursor_state(&self, pos: usize) -> CursorState {
        let Some(bi) = self.block_index_at(pos) else {
            return CursorState {
                pos,
                active_block: None,
                active_nodes: SmallVec::new(),
            };
        };
        let block = &self.blocks[bi];
        let mut nodes: SmallVec<[NodeId; 4]> = SmallVec::new();
        for s in &block.spans {
            if s.range.start <= pos && pos <= s.range.end && !nodes.contains(&s.node) {
                nodes.push(s.node);
            }
        }
        CursorState {
            pos,
            active_block: Some(block.id),
            active_nodes: nodes,
        }
    }

    pub fn update_cursor(
        &self,
        cursor: &mut CursorState,
        new_pos: usize,
    ) -> CursorUpdateOutcome {
        let new = self.cursor_state(new_pos);
        if new.active_block == cursor.active_block && new.active_nodes == cursor.active_nodes {
            let old_pos = cursor.pos;
            cursor.pos = new_pos;
            CursorUpdateOutcome::Unchanged {
                old_pos,
                new_pos,
            }
        } else {
            let mut invalidated: SmallVec<[BlockId; 2]> = SmallVec::new();
            if let Some(b) = cursor.active_block {
                invalidated.push(b);
            }
            if let Some(b) = new.active_block {
                if !invalidated.contains(&b) {
                    invalidated.push(b);
                }
            }
            crate::perf::PERF.blocks_invalidated.fetch_add(
                invalidated.len() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            let old = std::mem::replace(cursor, new);
            CursorUpdateOutcome::ActiveNodesChanged {
                invalidated_blocks: invalidated,
                old_nodes: old.active_nodes,
                new_nodes: cursor.active_nodes.clone(),
            }
        }
    }

    pub fn apply_edit(&mut self, range: Range<usize>, new_text: &str) {
        self.apply_edit_with_metrics(range, new_text, 24.0, 800.0, 8.0);
    }

    pub fn apply_edit_with_metrics(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        line_height: f32,
        width: f32,
        char_w: f32,
    ) {
        let delta = new_text.len() as isize - range.len() as isize;
        let removed = if range.end <= self.source.len() {
            &self.source[range.clone()]
        } else {
            ""
        };
        let is_wide = needs_wide_reparse(new_text, removed);

        // 1. Применяем правку к исходному тексту
        self.source.replace_range(range.clone(), new_text);

        // Если документ пустой или правка меняет глобальный синтаксис (```, ---, <!--)
        if self.blocks.is_empty() || is_wide {
            crate::perf::PERF.parse_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let new_blocks = parse_blocks(&self.source, 0, &mut self.ids);
            self.heights = HeightIndex::from_blocks(&new_blocks, line_height, width, char_w);
            self.blocks = new_blocks;
            return;
        }

        // 2-3. Затронутые блоки с запасом [a..b)
        let mut a = self.blocks.partition_point(|b| b.source_range.end < range.start);
        let mut b = self.blocks.partition_point(|b| b.source_range.start <= range.end);
        a = a.saturating_sub(1);
        b = (b + 1).min(self.blocks.len());

        let start = self.blocks.get(a).map(|x| x.source_range.start).unwrap_or(0);
        let old_end = self.blocks.get(b.saturating_sub(1)).map(|x| x.source_range.end).unwrap_or(start);
        let new_end = ((old_end as isize + delta).max(start as isize) as usize).min(self.source.len());

        // 4-5. Перепарсить локальный срез
        crate::perf::PERF.parse_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let slice = &self.source[start..new_end];
        let new_blocks = parse_blocks(slice, start, &mut self.ids);
        let new_count = new_blocks.len();

        // 6. Заменить затронутый диапазон новыми блоками
        self.blocks.splice(a..b, new_blocks);

        // 7. Сдвиг смещений хвоста документа
        for blk in &mut self.blocks[a + new_count..] {
            shift_block(blk, delta);
        }

        // 8. Обновление индекса высот
        let new_heights: Vec<f32> = self.blocks[a..a + new_count]
            .iter()
            .map(|blk| estimate_height(blk, line_height, width, char_w))
            .collect();
        self.heights.splice(a..b, new_heights);
    }
}

pub fn needs_wide_reparse(edit_text: &str, removed: &str) -> bool {
    edit_text.contains("```")
        || removed.contains("```")
        || edit_text.contains("<!--")
        || removed.contains("<!--")
        || edit_text.contains("---")
        || removed.contains("---")
}

pub fn shift_range(range: &Range<usize>, delta: isize) -> Range<usize> {
    let start = (range.start as isize + delta).max(0) as usize;
    let end = (range.end as isize + delta).max(start as isize) as usize;
    start..end
}

pub fn shift_block(b: &mut MarkdownBlock, delta: isize) {
    b.source_range = shift_range(&b.source_range, delta);
    for s in &mut b.spans {
        s.range = shift_range(&s.range, delta);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorUpdateOutcome {
    Unchanged {
        old_pos: usize,
        new_pos: usize,
    },
    ActiveNodesChanged {
        invalidated_blocks: SmallVec<[BlockId; 2]>,
        old_nodes: SmallVec<[NodeId; 4]>,
        new_nodes: SmallVec<[NodeId; 4]>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRun {
    pub source: Range<usize>,
    pub style: RunStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub heading: Option<u8>,
    pub marker: bool,
}

pub fn apply_style(style: &mut RunStyle, k: StyleKind) {
    match k {
        StyleKind::Bold => style.bold = true,
        StyleKind::Italic => style.italic = true,
        StyleKind::Strike => style.strike = true,
        StyleKind::Code => style.code = true,
        StyleKind::Link => style.link = true,
        StyleKind::Heading(lvl) => style.heading = Some(lvl),
    }
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
        if seg.is_empty() {
            continue;
        }

        let mut style = RunStyle::default();
        let mut hidden = false;

        for s in block.spans.iter().filter(|s| s.range.start <= seg.start && seg.end <= s.range.end) {
            match s.kind {
                SpanKind::Marker => {
                    if cursor.is_node_active(s.node) {
                        style.marker = true;
                    } else {
                        hidden = true;
                    }
                }
                SpanKind::Style(k) => apply_style(&mut style, k),
                SpanKind::Atomic | SpanKind::Text => {}
            }
        }
        if hidden {
            continue;
        }

        match runs.last_mut() {
            Some(last) if last.style == style && last.source.end == seg.start => {
                last.source.end = seg.end;
            }
            _ => runs.push(VisualRun { source: seg, style }),
        }
    }
    runs
}

pub fn active_nodes_hash(block: &MarkdownBlock, cursor: &CursorState) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for span in &block.spans {
        if cursor.is_node_active(span.node) {
            span.node.hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct LayoutKey {
    pub block_id: BlockId,
    pub text_hash: u64,
    pub active_hash: u64,
    pub width_px: u32,
    pub scale_factor_x100: u32,
    pub style_rev: u32,
}

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub struct LayoutCache<T> {
    map: HashMap<LayoutKey, Arc<T>>,
    order: VecDeque<LayoutKey>,
    cap: usize,
}

impl<T> LayoutCache<T> {
    pub fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub fn get(&self, key: &LayoutKey) -> Option<Arc<T>> {
        if let Some(v) = self.map.get(key) {
            crate::perf::PERF.layout_cache_hit.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some(v.clone())
        } else {
            crate::perf::PERF.layout_cache_miss.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            None
        }
    }

    pub fn insert(&mut self, key: LayoutKey, val: Arc<T>) {
        if self.map.contains_key(&key) {
            self.map.insert(key, val);
            return;
        }
        self.map.insert(key.clone(), val);
        self.order.push_back(key);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }

    pub fn get_or_insert_with(
        &mut self,
        key: LayoutKey,
        make: impl FnOnce() -> T,
    ) -> Arc<T> {
        if let Some(v) = self.map.get(&key) {
            crate::perf::PERF.layout_cache_hit.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return v.clone();
        }
        crate::perf::PERF.layout_cache_miss.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let v = Arc::new(make());
        self.map.insert(key.clone(), v.clone());
        self.order.push_back(key);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        v
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualMap {
    pub base: usize,
    pub hidden: Vec<Range<usize>>,
    pub cum_before: Vec<usize>,
}

impl VisualMap {
    pub fn new(base: usize, mut hidden: Vec<Range<usize>>) -> Self {
        hidden.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in hidden {
            if r.is_empty() {
                continue;
            }
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        let mut cum = Vec::with_capacity(merged.len());
        let mut acc = 0;
        for r in &merged {
            cum.push(acc);
            acc += r.len();
        }
        Self {
            base,
            hidden: merged,
            cum_before: cum,
        }
    }

    pub fn from_block(block: &MarkdownBlock, cursor: &CursorState) -> Self {
        let mut hidden = Vec::new();
        for s in &block.spans {
            if s.kind == SpanKind::Marker && !cursor.is_node_active(s.node) {
                hidden.push(s.range.clone());
            }
        }
        Self::new(block.source_range.start, hidden)
    }

    /// source offset -> visual offset (в байтах видимого текста блока)
    pub fn to_visual(&self, src: usize) -> usize {
        let src = src.max(self.base);
        let i = self.hidden.partition_point(|r| r.start <= src);
        let mut hidden_before = if i > 0 { self.cum_before[i - 1] } else { 0 };
        if i > 0 {
            let r = &self.hidden[i - 1];
            hidden_before += r.end.min(src).saturating_sub(r.start);
        }
        src.saturating_sub(self.base).saturating_sub(hidden_before)
    }

    /// visual offset -> source offset
    pub fn to_source(&self, vis: usize) -> usize {
        let mut src = vis + self.base;
        for (r, cum) in self.hidden.iter().zip(&self.cum_before) {
            let vis_start = r.start.saturating_sub(self.base).saturating_sub(*cum);
            if vis >= vis_start {
                src = vis + self.base + cum + r.len();
            } else {
                break;
            }
        }
        src
    }

    /// Если pos оказался внутри скрытого диапазона, вынести наружу к ближайшей границе.
    pub fn clamp_hidden_pos(&self, pos: usize, moving_right: bool) -> usize {
        for r in &self.hidden {
            if r.start < pos && pos < r.end {
                return if moving_right { r.end } else { r.start };
            }
        }
        pos
    }
}

/// После любого изменения позиции: если pos оказался внутри Atomic-диапазона, перепрыгнуть его.
pub fn normalize_pos(block: &MarkdownBlock, pos: usize, moving_right: bool) -> usize {
    for s in &block.spans {
        if s.kind == SpanKind::Atomic && s.range.start < pos && pos < s.range.end {
            return if moving_right { s.range.end } else { s.range.start };
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold_produces_two_markers_and_style() {
        let src = "a **b** c";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 2..4); // "**"
        assert_eq!(markers[1].range, 5..7); // "**"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Bold))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 4..5); // "b"
        assert_eq!(styles[0].node, markers[0].node);
        assert_eq!(styles[0].node, markers[1].node);
    }

    #[test]
    fn test_italic_produces_markers_and_style() {
        let src = "hello *world* test";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 6..7); // "*"
        assert_eq!(markers[1].range, 12..13); // "*"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Italic))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..12); // "world"
    }

    #[test]
    fn test_inline_code_produces_markers_and_style() {
        let src = "test `let x = 1;` end";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 5..6); // "`"
        assert_eq!(markers[1].range, 16..17); // "`"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Code))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 6..16); // "let x = 1;"
    }

    #[test]
    fn test_link_produces_markers_and_style() {
        let src = "click [here](https://example.com) now";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 6..7); // "["
        assert_eq!(markers[1].range, 11..33); // "](https://example.com)"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Link))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..11); // "here"
    }

    #[test]
    fn test_heading_produces_marker_and_style() {
        let src = "## Header Two";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::Heading(2));

        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].range, 0..3); // "## "

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Heading(2)))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 3..13); // "Header Two"
    }

    #[test]
    fn test_cursor_state_active_nodes_boundaries() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        // pos = 0 (на "a") -> активных узлов нет
        let c0 = doc.cursor_state(0);
        assert!(c0.active_nodes.is_empty());

        // pos = 2 (прямо перед "**") -> узел активен (граница start)
        let c2 = doc.cursor_state(2);
        assert_eq!(c2.active_nodes.len(), 1);

        // pos = 4 (внутри "b") -> узел активен
        let c4 = doc.cursor_state(4);
        assert_eq!(c4.active_nodes.len(), 1);

        // pos = 7 (прямо после закрывающего "**") -> узел активен (граница end)
        let c7 = doc.cursor_state(7);
        assert_eq!(c7.active_nodes.len(), 1);

        // pos = 8 (на пробеле после "**") -> активных узлов нет
        let c8 = doc.cursor_state(8);
        assert!(c8.active_nodes.is_empty());
    }

    #[test]
    fn test_strikethrough_produces_markers_and_style() {
        let src = "text ~~deleted~~ more";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;
        let markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker).collect();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].range, 5..7); // "~~"
        assert_eq!(markers[1].range, 14..16); // "~~"

        let styles: Vec<_> = spans.iter().filter(|s| matches!(s.kind, SpanKind::Style(StyleKind::Strike))).collect();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].range, 7..14); // "deleted"
    }

    #[test]
    fn test_nested_bold_and_italic() {
        let src = "**a *b* c**";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(src, 0, &mut ids);
        assert_eq!(blocks.len(), 1);
        let spans = &blocks[0].spans;

        // Strong markers
        let strong_markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker && s.range.len() == 2).collect();
        assert_eq!(strong_markers.len(), 2);
        assert_eq!(strong_markers[0].range, 0..2); // "**"
        assert_eq!(strong_markers[1].range, 9..11); // "**"

        // Emph markers
        let emph_markers: Vec<_> = spans.iter().filter(|s| s.kind == SpanKind::Marker && s.range.len() == 1).collect();
        assert_eq!(emph_markers.len(), 2);
        assert_eq!(emph_markers[0].range, 4..5); // "*"
        assert_eq!(emph_markers[1].range, 6..7); // "*"
    }

    #[test]
    fn test_multi_block_document_parsing() {
        let doc_text = "# Header\n\nParagraph with **bold**.\n\n```rust\nfn main() {}\n```\n";
        let mut ids = IdGen::new();
        let blocks = parse_blocks(doc_text, 0, &mut ids);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(blocks[2].kind, BlockKind::CodeFence);
    }

    #[test]
    fn test_height_index_block_at() {
        let h = HeightIndex::new(vec![10.0, 20.0, 30.0]);
        assert_eq!(h.total(), 60.0);
        assert_eq!(h.block_at(0.0), 0);
        assert_eq!(h.block_at(5.0), 0);
        assert_eq!(h.block_at(10.0), 1);
        assert_eq!(h.block_at(29.9), 1);
        assert_eq!(h.block_at(30.0), 2);
        assert_eq!(h.block_at(59.9), 2);
        assert_eq!(h.block_at(100.0), 2);
    }

    #[test]
    fn test_height_index_visible_range() {
        let h = HeightIndex::new(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        // scroll_top = 15.0, viewport_h = 30.0, overscan = 0.0
        // top = 15.0 (блок 1, top = 10..30), bottom = 45.0 (блок 2, top = 30..60)
        let r = h.visible_range(15.0, 30.0, 0.0);
        assert_eq!(r, 1..3);
    }

    #[test]
    fn test_height_index_splice_and_set_height() {
        let mut h = HeightIndex::new(vec![10.0, 20.0, 30.0]);
        h.set_height(1, 25.0);
        assert_eq!(h.height_of(1), 25.0);
        assert_eq!(h.top_of(2), 35.0);
        assert_eq!(h.total(), 65.0);

        // splice: заменяем блок 1 на два блока [15.0, 15.0]
        h.splice(1..2, vec![15.0, 15.0]);
        assert_eq!(h.len(), 4);
        assert_eq!(h.height_of(0), 10.0);
        assert_eq!(h.height_of(1), 15.0);
        assert_eq!(h.height_of(2), 15.0);
        assert_eq!(h.height_of(3), 30.0);
        assert_eq!(h.top_of(3), 40.0);
        assert_eq!(h.total(), 70.0);
    }

    #[test]
    fn test_height_index_empty() {
        let h = HeightIndex::new(vec![]);
        assert_eq!(h.total(), 0.0);
        assert_eq!(h.block_at(10.0), 0);
        assert_eq!(h.visible_range(0.0, 100.0, 0.0), 0..0);
    }

    #[test]
    fn test_build_runs_markers_hidden_when_cursor_outside() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let cursor = doc.cursor_state(0); // pos = 0
        let runs = build_runs(&doc.blocks[0], &cursor);
        // Маркеры скрыты! Ожидаем 3 пробега: "a ", "b" (bold), " c"
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].source, 0..2);
        assert!(!runs[0].style.marker && !runs[0].style.bold);

        assert_eq!(runs[1].source, 4..5); // "b"
        assert!(!runs[1].style.marker && runs[1].style.bold);

        assert_eq!(runs[2].source, 7..9);
        assert!(!runs[2].style.marker && !runs[2].style.bold);
    }

    #[test]
    fn test_build_runs_markers_shown_when_cursor_on_boundary() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let cursor = doc.cursor_state(2); // pos = 2 (перед "**")
        let runs = build_runs(&doc.blocks[0], &cursor);
        // Маркеры показаны! Ожидаем 5 пробегов: "a ", "**" (marker), "b" (bold), "**" (marker), " c"
        assert_eq!(runs.len(), 5);
        assert_eq!(runs[0].source, 0..2);

        assert_eq!(runs[1].source, 2..4); // "**"
        assert!(runs[1].style.marker);

        assert_eq!(runs[2].source, 4..5); // "b"
        assert!(runs[2].style.bold);

        assert_eq!(runs[3].source, 5..7); // "**"
        assert!(runs[3].style.marker);

        assert_eq!(runs[4].source, 7..9);
    }

    #[test]
    fn test_layout_cache_lru() {
        let mut cache: LayoutCache<String> = LayoutCache::new(2);
        let key1 = LayoutKey { block_id: 1, text_hash: 10, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };
        let key2 = LayoutKey { block_id: 2, text_hash: 20, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };
        let key3 = LayoutKey { block_id: 3, text_hash: 30, active_hash: 0, width_px: 800, scale_factor_x100: 100, style_rev: 1 };

        let val1 = cache.get_or_insert_with(key1.clone(), || "block1".to_string());
        assert_eq!(*val1, "block1");
        assert_eq!(cache.len(), 1);

        let val1_again = cache.get(&key1);
        assert!(val1_again.is_some());

        // Вставляем key2
        cache.insert(key2.clone(), Arc::new("block2".to_string()));
        assert_eq!(cache.len(), 2);

        // Вставляем key3 -> key1 должен быть вытеснен из LRU (емкость 2)
        cache.insert(key3.clone(), Arc::new("block3".to_string()));
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some());
        assert!(cache.get(&key3).is_some());
    }

    #[test]
    fn test_stage4_cursor_movement_step_by_step() {
        let text = "Hello **bold** world";
        let doc = MarkdownDocument::new(text.to_string());
        let mut cursor = doc.cursor_state(0);

        // Сбросим счетчики
        crate::perf::PERF.snapshot_and_reset();

        // 1. Движение внутри "Hello" (позиции 0..=5): маркеры скрыты, invalidation = 0
        for pos in 1..=5 {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
            let runs = build_runs(&doc.blocks[0], &cursor);
            assert!(runs.iter().all(|r| !r.style.marker));
        }

        // 2. Вход на границу "**" (позиция 6): узел активируется, инвалидируется ровно 1 блок
        let outcome = doc.update_cursor(&mut cursor, 6);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 1);
            }
            _ => panic!("Expected ActiveNodesChanged on boundary at pos 6"),
        }
        let runs = build_runs(&doc.blocks[0], &cursor);
        assert!(runs.iter().any(|r| r.style.marker));

        // 3. Движение внутри "**bold**" (позиции 7..=14): маркеры раскрыты, invalidation = 0!
        for pos in 7..=14 {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
            let runs = build_runs(&doc.blocks[0], &cursor);
            assert!(runs.iter().any(|r| r.style.marker));
        }

        // 4. Выход за пределы "**" (позиция 15, пробел перед "world"): узел деактивируется, инвалидируется ровно 1 блок
        let outcome = doc.update_cursor(&mut cursor, 15);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 1);
            }
            _ => panic!("Expected ActiveNodesChanged when leaving node at pos 15"),
        }
        let runs = build_runs(&doc.blocks[0], &cursor);
        assert!(runs.iter().all(|r| !r.style.marker));

        // 5. Движение в "world" (16..=20): маркеры скрыты, invalidation = 0
        for pos in 16..=text.len() {
            let outcome = doc.update_cursor(&mut cursor, pos);
            assert_eq!(outcome, CursorUpdateOutcome::Unchanged { old_pos: pos - 1, new_pos: pos });
        }

        // За все шаги парсер не вызывался ни разу!
        let counters = crate::perf::PERF.snapshot_and_reset();
        assert_eq!(counters[0], 0, "parse_calls must be 0");
        assert_eq!(counters[1], 0, "span_rebuilds must be 0");
    }

    #[test]
    fn test_stage4_multi_block_cursor_crossing() {
        let text = "# Heading\n\nSecond paragraph text.";
        let doc = MarkdownDocument::new(text.to_string());
        let mut cursor = doc.cursor_state(0); // блок 0

        crate::perf::PERF.snapshot_and_reset();

        // Переходим из блока 0 в блок 1 (позиция 12)
        let outcome = doc.update_cursor(&mut cursor, 12);
        match outcome {
            CursorUpdateOutcome::ActiveNodesChanged { invalidated_blocks, .. } => {
                assert_eq!(invalidated_blocks.len(), 2, "crossing block boundary invalidates old and new block");
                assert_eq!(invalidated_blocks[0], doc.blocks[0].id);
                assert_eq!(invalidated_blocks[1], doc.blocks[1].id);
            }
            _ => panic!("Expected ActiveNodesChanged when crossing block boundary"),
        }

        let counters = crate::perf::PERF.snapshot_and_reset();
        assert_eq!(counters[0], 0, "parse_calls must be 0");
        assert_eq!(counters[1], 0, "span_rebuilds must be 0");
    }

    #[test]
    fn test_stage5_visual_map_roundtrip() {
        let m = VisualMap::new(0, vec![2..4, 5..7]);
        assert_eq!(m.to_visual(0), 0);
        assert_eq!(m.to_visual(1), 1);
        assert_eq!(m.to_visual(2), 2); // начало первого скрытого
        assert_eq!(m.to_visual(4), 2); // конец первого скрытого
        assert_eq!(m.to_visual(5), 3); // "b" (длина 1), начало второго скрытого
        assert_eq!(m.to_visual(7), 3); // конец второго скрытого
        assert_eq!(m.to_visual(8), 4); // " c"

        assert_eq!(m.to_source(0), 0);
        assert_eq!(m.to_source(1), 1);
        assert_eq!(m.to_source(2), 4); // после первого скрытого диапазона
        assert_eq!(m.to_source(3), 7); // после второго скрытого диапазона

        // Проверка круглого цикла: to_visual(to_source(vis)) == vis
        for vis in 0..=4 {
            let src = m.to_source(vis);
            assert_eq!(m.to_visual(src), vis);
        }
    }

    #[test]
    fn test_stage5_visual_map_from_block() {
        let doc = MarkdownDocument::new("a **b** c".to_string());
        let block = &doc.blocks[0];

        // 1. Курсор вне узла -> маркеры скрыты -> VisualMap сжат
        let cursor_outside = doc.cursor_state(0);
        let map_outside = VisualMap::from_block(block, &cursor_outside);
        assert_eq!(map_outside.hidden.len(), 2);
        assert_eq!(map_outside.to_visual(4), 2); // 'b' находится на визуальной позиции 2

        // 2. Курсор на границе -> маркеры раскрыты -> VisualMap без скрытых диапазонов
        let cursor_active = doc.cursor_state(2);
        let map_active = VisualMap::from_block(block, &cursor_active);
        assert!(map_active.hidden.is_empty());
        assert_eq!(map_active.to_visual(4), 4);
        assert_eq!(map_active.to_source(4), 4);
    }

    #[test]
    fn test_stage5_clamp_hidden_and_normalize_atomic() {
        let m = VisualMap::new(0, vec![2..4]);
        // Внутри 2..4: moving_right -> 4, moving_left -> 2
        assert_eq!(m.clamp_hidden_pos(3, true), 4);
        assert_eq!(m.clamp_hidden_pos(3, false), 2);
        // Вне: остается без изменений
        assert_eq!(m.clamp_hidden_pos(5, true), 5);

        // Atomic span: курсор перепрыгивает
        let block = MarkdownBlock {
            id: 1,
            source_range: 0..30,
            kind: BlockKind::Paragraph,
            spans: vec![Span {
                range: 10..20,
                kind: SpanKind::Atomic,
                node: 1,
            }],
            layout: None,
        };
        assert_eq!(normalize_pos(&block, 15, true), 20);
        assert_eq!(normalize_pos(&block, 15, false), 10);
        assert_eq!(normalize_pos(&block, 5, true), 5);
    }

    #[test]
    fn test_stage6_typography_stable_regardless_of_cursor() {
        let doc = MarkdownDocument::new("# Heading 1\n\nParagraph text".to_string());
        let h1_block = &doc.blocks[0];

        let base_font = 16.0;
        let base_lh = 24.0;

        // Курсор снаружи заголовка (в абзаце на позиции 15)
        let cursor_outside = doc.cursor_state(15);
        assert!(!cursor_outside.is_node_active(h1_block.spans[0].node));

        let typo_outside = BlockTypography::for_block(h1_block.kind, base_font, base_lh);

        // Курсор внутри заголовка (на позиции 3)
        let cursor_inside = doc.cursor_state(3);
        assert!(cursor_inside.is_node_active(h1_block.spans[0].node));

        let typo_inside = BlockTypography::for_block(h1_block.kind, base_font, base_lh);

        // Размеры шрифта и высота строки СТРОГО идентичны
        assert_eq!(typo_outside.font_size, typo_inside.font_size);
        assert_eq!(typo_outside.line_height, typo_inside.line_height);
        assert_eq!(typo_inside.font_size, 16.0 * 1.8);
        assert_eq!(typo_inside.line_height, 24.0 * 1.8);
    }

    #[test]
    fn test_stage6_heading_hierarchy_scaling() {
        let base = 10.0;
        assert_eq!(font_size_for(BlockKind::Heading(1), base), 18.0);
        assert_eq!(font_size_for(BlockKind::Heading(2), base), 15.0);
        assert_eq!(font_size_for(BlockKind::Heading(3), base), 12.5);
        assert_eq!(font_size_for(BlockKind::Heading(4), base), 11.0);
        assert_eq!(font_size_for(BlockKind::Paragraph, base), 10.0);
        assert_eq!(font_size_for(BlockKind::CodeFence, base), 10.0);
    }

    #[test]
    fn test_stage7_incremental_edit_matches_full_reparse() {
        let initial_text = "# Title\n\nFirst paragraph text.\n\nSecond paragraph with **bold** text.\n\nThird paragraph here.\n";
        let mut doc = MarkdownDocument::new(initial_text.to_string());
        assert_eq!(doc.blocks.len(), 4);

        // Вставляем слово "MODIFIED " во второй абзац (перед "Second")
        let edit_pos = doc.blocks[2].source_range.start;
        doc.apply_edit(edit_pos..edit_pos, "MODIFIED ");

        // Сравниваем с полным перепарсингом
        let mut ids = IdGen::new();
        let full_blocks = parse_blocks(&doc.source, 0, &mut ids);

        assert_eq!(doc.blocks.len(), full_blocks.len(), "Block counts must match");
        for (i, (inc_b, full_b)) in doc.blocks.iter().zip(&full_blocks).enumerate() {
            assert_eq!(inc_b.kind, full_b.kind, "Block {i} kind mismatch");
            assert_eq!(inc_b.source_range, full_b.source_range, "Block {i} source_range mismatch");
            assert_eq!(inc_b.spans.len(), full_b.spans.len(), "Block {i} spans len mismatch");
            for (s_inc, s_full) in inc_b.spans.iter().zip(&full_b.spans) {
                assert_eq!(s_inc.kind, s_full.kind);
                assert_eq!(s_inc.range, s_full.range);
            }
        }
    }

    #[test]
    fn test_stage7_wide_reparse_trigger() {
        assert!(needs_wide_reparse("```rust\n", ""));
        assert!(needs_wide_reparse("", "```"));
        assert!(needs_wide_reparse("---", ""));
        assert!(needs_wide_reparse("<!--", ""));
        assert!(!needs_wide_reparse("обычный текст", ""));
    }

    #[test]
    fn test_stage7_deletion_incremental_edit() {
        let initial_text = "# Header\n\nAbc def ghi.\n\nEnd.\n";
        let mut doc = MarkdownDocument::new(initial_text.to_string());
        assert_eq!(doc.blocks.len(), 3);

        // Удаляем "def " из второго блока
        let b1_start = doc.blocks[1].source_range.start;
        doc.apply_edit(b1_start + 4..b1_start + 8, "");

        let mut ids = IdGen::new();
        let full_blocks = parse_blocks(&doc.source, 0, &mut ids);

        assert_eq!(doc.blocks.len(), full_blocks.len());
        for (inc_b, full_b) in doc.blocks.iter().zip(&full_blocks) {
            assert_eq!(inc_b.kind, full_b.kind);
            assert_eq!(inc_b.source_range, full_b.source_range);
        }
    }
}
