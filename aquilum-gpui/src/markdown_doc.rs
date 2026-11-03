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
        // 1. Применяем правку к исходному тексту
        self.source.replace_range(range, new_text);

        // 2. Полный надёжный парсинг AST документа без повреждения UTF-8 смещений и срезов
        crate::perf::PERF.parse_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let new_blocks = parse_blocks(&self.source, 0, &mut self.ids);
        self.heights = HeightIndex::from_blocks(&new_blocks, line_height, width, char_w);
        self.blocks = new_blocks;
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
mod tests;
