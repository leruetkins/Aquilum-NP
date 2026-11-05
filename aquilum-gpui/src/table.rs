//! Pure data model for inline markdown table editing.
//!
//! The document buffer remains the single source of truth: there is no local UI state
//! here, only pure functions mapping "lines -> table structure" and "structure -> line replacement edits".
//! All column coordinates are Unicode character (char) indices within lines, matching `markdown.rs`.
//!
//! Table boundaries are determined by `pulldown_cmark` (GFM rules), ensuring the renderer
//! and editor never disagree on where a table starts and ends.

use pulldown_cmark::Alignment;
use std::ops::Range;

// ───────────────────────────── Blocks ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

impl From<Alignment> for Align {
    fn from(a: Alignment) -> Self {
        match a {
            Alignment::None => Align::None,
            Alignment::Left => Align::Left,
            Alignment::Center => Align::Center,
            Alignment::Right => Align::Right,
        }
    }
}

/// A markdown table in the document spanning lines `first_line..=last_line`.
/// `first_line` is the header, `first_line + 1` is the delimiter/separator line,
/// followed by body rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBlock {
    pub first_line: usize,
    pub last_line: usize,
    pub aligns: Vec<Align>,
}

impl TableBlock {
    pub fn sep_line(&self) -> usize {
        self.first_line + 1
    }

    pub fn contains(&self, line: usize) -> bool {
        (self.first_line..=self.last_line).contains(&line)
    }

    /// Number of logical rows excluding the separator line (header counts as row 0).
    pub fn row_count(&self) -> usize {
        self.last_line - self.first_line
    }

    pub fn col_count(&self) -> usize {
        self.aligns.len()
    }

    /// Maps document line index to table logical row index (0 = header).
    /// Returns `None` for the delimiter line or lines outside the table block.
    pub fn row_of_line(&self, line: usize) -> Option<usize> {
        if !self.contains(line) || line == self.sep_line() {
            None
        } else if line == self.first_line {
            Some(0)
        } else {
            Some(line - self.first_line - 1)
        }
    }

    /// Maps table logical row index (0 = header, 1 = first body row) to document line index.
    pub fn line_of_row(&self, row: usize) -> usize {
        if row == 0 {
            self.first_line
        } else {
            self.first_line + 1 + row
        }
    }
}

/// Cache of tables in the document, refreshed by buffer revision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableIndex {
    pub rev: u64,
    pub blocks: Vec<TableBlock>,
    pub covered_range: (usize, usize),
    pub fully_scanned: bool,
}

impl TableIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh(&mut self, rev: u64, lines: &[&str]) {
        if self.rev != rev {
            self.blocks = find_tables(lines);
            self.rev = rev;
            self.covered_range = (0, lines.len());
            self.fully_scanned = true;
        }
    }

    pub fn parse_candidate_run(
        blocks: &mut Vec<TableBlock>,
        rope: &crop::Rope,
        start_line: usize,
        end_line: usize,
    ) {
        if start_line > end_line {
            return;
        }
        let total_lines = rope.line_len() + usize::from(rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) != b'\n');
        if end_line >= total_lines {
            return;
        }
        let start_byte = if start_line < rope.line_len() { rope.byte_of_line(start_line) } else { rope.byte_len() };
        let end_byte = if end_line + 1 < rope.line_len() { rope.byte_of_line(end_line + 1) } else { rope.byte_len() };
        if start_byte >= end_byte {
            return;
        }
        let slice = rope.byte_slice(start_byte..end_byte).to_string();
        let lines: Vec<&str> = slice.lines().collect();
        let found = find_tables(&lines);
        for mut t in found {
            t.first_line += start_line;
            t.last_line += start_line;
            blocks.push(t);
        }
    }

    /// Windowed scanning: parses tables in candidate runs within [start_line, end_line].
    /// Avoids traversing 100k lines when only a small window is shown.
    pub fn scan_window(&mut self, rev: u64, rope: &crop::Rope, start_line: usize, end_line: usize) {
        if self.rev == rev
            && (self.fully_scanned
                || (start_line >= self.covered_range.0 && end_line <= self.covered_range.1))
        {
            return;
        }

        let total_lines = rope.line_len() + usize::from(rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) != b'\n');
        if total_lines == 0 {
            self.rev = rev;
            self.blocks.clear();
            self.covered_range = (0, 0);
            self.fully_scanned = true;
            return;
        }

        if total_lines <= 3000 {
            self.scan_rope(rev, rope);
            return;
        }

        self.rev = rev;

        // Expand bounds by up to 200 lines to never split a candidate table
        let mut p_start = start_line.saturating_sub(100);
        while p_start > 0 {
            let prev = p_start - 1;
            let start_byte = if prev < rope.line_len() { rope.byte_of_line(prev) } else { rope.byte_len() };
            let end_byte = if prev + 1 < rope.line_len() { rope.byte_of_line(prev + 1) } else { rope.byte_len() };
            let slice = rope.byte_slice(start_byte..end_byte);
            if !slice.bytes().any(|b| b == b'|') {
                break;
            }
            p_start = prev;
            if start_line.saturating_sub(p_start) > 200 {
                break;
            }
        }

        let mut p_end = (end_line + 100).min(total_lines.saturating_sub(1));
        while p_end + 1 < total_lines {
            let next = p_end + 1;
            let start_byte = if next < rope.line_len() { rope.byte_of_line(next) } else { rope.byte_len() };
            let end_byte = if next + 1 < rope.line_len() { rope.byte_of_line(next + 1) } else { rope.byte_len() };
            let slice = rope.byte_slice(start_byte..end_byte);
            if !slice.bytes().any(|b| b == b'|') {
                break;
            }
            p_end = next;
            if p_end.saturating_sub(end_line) > 200 {
                break;
            }
        }

        // Retain existing blocks strictly outside [p_start, p_end]
        self.blocks.retain(|b| b.last_line < p_start || b.first_line > p_end);

        // Scan candidate runs in p_start..=p_end
        let mut cur_run_start = None;
        let mut cur_run_has_delim = false;

        for l_idx in p_start..=p_end {
            let start_byte = if l_idx < rope.line_len() { rope.byte_of_line(l_idx) } else { rope.byte_len() };
            let end_byte = if l_idx + 1 < rope.line_len() { rope.byte_of_line(l_idx + 1) } else { rope.byte_len() };
            let slice = rope.byte_slice(start_byte..end_byte);
            let mut has_pipe = false;
            let mut has_hyphen = false;
            let mut has_non_ws = false;
            for chunk in slice.chunks() {
                for &b in chunk.as_bytes() {
                    if b == b'|' {
                        has_pipe = true;
                    } else if b == b'-' {
                        has_hyphen = true;
                    }
                    if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                        has_non_ws = true;
                    }
                }
            }

            if has_non_ws && has_pipe {
                if cur_run_start.is_none() {
                    cur_run_start = Some(l_idx);
                    cur_run_has_delim = false;
                }
                if has_hyphen {
                    cur_run_has_delim = true;
                }
            } else {
                if let Some(start) = cur_run_start {
                    let end = l_idx.saturating_sub(1);
                    if cur_run_has_delim && end >= start + 1 {
                        Self::parse_candidate_run(&mut self.blocks, rope, start, end);
                    }
                    cur_run_start = None;
                    cur_run_has_delim = false;
                }
            }
        }

        if let Some(start) = cur_run_start {
            if cur_run_has_delim && p_end >= start + 1 {
                Self::parse_candidate_run(&mut self.blocks, rope, start, p_end);
            }
        }

        self.blocks.sort_by_key(|b| b.first_line);
        self.covered_range = (p_start, p_end);
        self.fully_scanned = p_start == 0 && p_end + 1 >= total_lines;
    }

    /// Fast, zero-allocation pre-filter across rope lines.
    /// Only extracts and parses candidate paragraphs that contain `|` and `-`.
    pub fn scan_rope(&mut self, rev: u64, rope: &crop::Rope) {
        if self.rev == rev && self.fully_scanned {
            return;
        }
        self.blocks.clear();
        self.rev = rev;
        self.fully_scanned = true;

        let total_lines = rope.line_len() + usize::from(rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) != b'\n');
        self.covered_range = (0, total_lines);
        if total_lines == 0 {
            return;
        }

        let mut cur_run_start = None;
        let mut cur_run_has_delim = false;
        let mut l_idx: usize = 0;

        for line in rope.lines() {
            let mut has_pipe = false;
            let mut has_hyphen = false;
            let mut has_non_ws = false;
            for chunk in line.chunks() {
                for &b in chunk.as_bytes() {
                    if b == b'|' {
                        has_pipe = true;
                    } else if b == b'-' {
                        has_hyphen = true;
                    }
                    if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                        has_non_ws = true;
                    }
                }
            }

            if has_non_ws && has_pipe {
                if cur_run_start.is_none() {
                    cur_run_start = Some(l_idx);
                    cur_run_has_delim = false;
                }
                if has_hyphen {
                    cur_run_has_delim = true;
                }
            } else {
                if let Some(start) = cur_run_start {
                    let end = l_idx.saturating_sub(1);
                    if cur_run_has_delim && end >= start + 1 {
                        Self::parse_candidate_run(&mut self.blocks, rope, start, end);
                    }
                    cur_run_start = None;
                    cur_run_has_delim = false;
                }
            }
            l_idx += 1;
        }

        if let Some(start) = cur_run_start {
            if cur_run_has_delim && l_idx > start + 1 {
                Self::parse_candidate_run(&mut self.blocks, rope, start, l_idx.saturating_sub(1));
            }
        }
    }

    /// Incremental edit update: shifts table blocks above the change and rescans
    /// only the paragraph that contains it.
    ///
    /// `start_line` is the line where the changed text begins; `end_line` is the
    /// line where it ends in the new rope. The whole span has to be rescanned, not
    /// just `start_line`: an inserted block can start several blank lines below the
    /// insertion point (a pasted table, for example), and scanning forward from the
    /// insertion line alone stops at the first blank line and leaves it unindexed.
    pub fn apply_edit(
        &mut self,
        rev: u64,
        start_line: usize,
        end_line: usize,
        line_diff: isize,
        rope: &crop::Rope,
    ) {
        self.rev = rev;
        let total_lines = rope.line_len() + usize::from(rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) != b'\n');
        if total_lines == 0 {
            self.blocks.clear();
            self.covered_range = (0, 0);
            self.fully_scanned = true;
            return;
        }

        // 1. Scan backwards to find paragraph start (blank line or line without pipe)
        let mut p_start = start_line;
        while p_start > 0 {
            let prev = p_start - 1;
            let start_byte = if prev < rope.line_len() { rope.byte_of_line(prev) } else { rope.byte_len() };
            let end_byte = if prev + 1 < rope.line_len() { rope.byte_of_line(prev + 1) } else { rope.byte_len() };
            let slice = rope.byte_slice(start_byte..end_byte);
            let mut has_pipe = false;
            let mut has_non_ws = false;
            for b in slice.bytes() {
                if b == b'|' { has_pipe = true; }
                if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                    has_non_ws = true;
                }
            }
            if !has_non_ws || !has_pipe {
                break;
            }
            p_start = prev;
            if start_line.saturating_sub(p_start) > 200 {
                break;
            }
        }

        // Scan forwards to find paragraph end (blank line or line without pipe).
        // The walk starts at the end of the changed span so inserted lines below
        // the first one are inside the rescanned range. An insertion that ends with
        // a newline points one line past the last real line, so clamp it.
        let mut p_end = end_line.max(start_line).min(total_lines.saturating_sub(1));
        while p_end + 1 < total_lines {
            let next = p_end + 1;
            let start_byte = if next < rope.line_len() { rope.byte_of_line(next) } else { rope.byte_len() };
            let end_byte = if next + 1 < rope.line_len() { rope.byte_of_line(next + 1) } else { rope.byte_len() };
            let slice = rope.byte_slice(start_byte..end_byte);
            let mut has_pipe = false;
            let mut has_non_ws = false;
            for b in slice.bytes() {
                if b == b'|' { has_pipe = true; }
                if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                    has_non_ws = true;
                }
            }
            if !has_non_ws || !has_pipe {
                break;
            }
            p_end = next;
            if p_end.saturating_sub(end_line) > 200 {
                break;
            }
        }

        // 2. Partition existing blocks
        let mut new_blocks = Vec::with_capacity(self.blocks.len() + 1);
        let old_p_end = if line_diff < 0 {
            p_end.saturating_add((-line_diff) as usize)
        } else {
            p_end.saturating_sub(line_diff as usize)
        };

        for mut b in self.blocks.drain(..) {
            if b.last_line < p_start {
                new_blocks.push(b);
            } else if b.first_line > old_p_end {
                b.first_line = (b.first_line as isize + line_diff).max(0) as usize;
                b.last_line = (b.last_line as isize + line_diff).max(0) as usize;
                new_blocks.push(b);
            }
        }
        self.blocks = new_blocks;

        // 3. Rescan candidate run p_start..=p_end
        if p_end >= p_start + 1 {
            let mut has_delim = false;
            for l in p_start..=p_end {
                let start_byte = if l < rope.line_len() { rope.byte_of_line(l) } else { rope.byte_len() };
                let end_byte = if l + 1 < rope.line_len() { rope.byte_of_line(l + 1) } else { rope.byte_len() };
                let slice = rope.byte_slice(start_byte..end_byte);
                if slice.bytes().any(|b| b == b'-') {
                    has_delim = true;
                    break;
                }
            }
            if has_delim {
                Self::parse_candidate_run(&mut self.blocks, rope, p_start, p_end);
                self.blocks.sort_by_key(|b| b.first_line);
            }
        }

        let shift = |line: usize| (line as isize + line_diff).max(0) as usize;
        if self.covered_range.0 > start_line {
            self.covered_range.0 = shift(self.covered_range.0);
        }
        if self.covered_range.1 >= start_line {
            self.covered_range.1 = shift(self.covered_range.1);
        }
        self.covered_range.0 = self.covered_range.0.min(p_start);
        self.covered_range.1 = self.covered_range.1.max(p_end);
    }

    pub fn at(&self, line: usize) -> Option<&TableBlock> {
        let i = self.blocks.partition_point(|t| t.last_line < line);
        self.blocks.get(i).filter(|t| t.first_line <= line)
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// Checks if a cell in a separator line is valid GFM separator syntax:
/// optional leading ':', at least one '-', optional trailing ':'.
pub fn is_separator_cell(cell: &str) -> bool {
    let s = cell.trim();
    let hyphens = s.trim_start_matches(':').trim_end_matches(':');
    !hyphens.is_empty() && hyphens.bytes().all(|b| b == b'-')
}

/// Parses alignment for a separator cell.
pub fn align_of(cell: &str) -> Align {
    let s = cell.trim();
    match (s.starts_with(':'), s.ends_with(':')) {
        (true, true) => Align::Center,
        (true, false) => Align::Left,
        (false, true) => Align::Right,
        _ => Align::None,
    }
}

/// If `line` is a valid GFM separator row, returns `Some(Vec<Align>)`.
/// A valid separator row must:
/// - Be indented by at most 3 spaces
/// - Contain `|`
/// - Have all cells be valid separator cells (`:---:`, etc.)
pub fn parse_separator_aligns(line: &str) -> Option<Vec<Align>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let indent = line.len() - line.trim_start().len();
    if indent > 3 {
        return None;
    }
    if !trimmed.contains('-') || !trimmed.contains('|') {
        return None;
    }

    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = match inner.strip_suffix('|') {
        Some(body) if !body.ends_with('\\') => body,
        _ => inner,
    };
    let parts: Vec<&str> = inner.split('|').collect();
    if parts.is_empty() {
        return None;
    }
    let mut aligns = Vec::with_capacity(parts.len());
    for part in parts {
        let s = part.trim();
        let hyphens = s.trim_start_matches(':').trim_end_matches(':');
        if hyphens.is_empty() || !hyphens.bytes().all(|b| b == b'-') {
            return None;
        }
        aligns.push(align_of(s));
    }
    if aligns.is_empty() {
        None
    } else {
        Some(aligns)
    }
}

pub fn is_separator(line: &str) -> bool {
    parse_separator_aligns(line).is_some()
}

/// Checks if a line is a candidate table row (top-level, not blockquote/list/code fence).
pub fn is_table_row_candidate(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let indent = line.len() - line.trim_start().len();
    if indent > 3 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    if first == b'>' {
        return false;
    }
    if (first == b'-' || first == b'*' || first == b'+') && trimmed.len() > 1 && trimmed.as_bytes()[1] == b' ' {
        return false;
    }
    if first.is_ascii_digit() {
        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
        }
    }
    trimmed.contains('|')
}

/// Finds all top-level tables in the document using fast streaming GFM line scanning.
/// Tables inside blockquotes, lists, or fenced code blocks are skipped.
pub fn find_tables(lines: &[&str]) -> Vec<TableBlock> {
    let mut out = Vec::new();
    let n = lines.len();
    let mut i = 0;
    let mut in_fence: Option<char> = None;

    while i < n {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_char = trimmed.chars().next().unwrap();
            match in_fence {
                Some(c) if c == fence_char => in_fence = None,
                None => in_fence = Some(fence_char),
                _ => {}
            }
            i += 1;
            continue;
        }

        if in_fence.is_some() {
            i += 1;
            continue;
        }

        if i + 1 < n && is_table_row_candidate(lines[i]) && !is_separator(lines[i]) {
            if let Some(aligns) = parse_separator_aligns(lines[i + 1]) {
                let first = i;
                let mut last = i + 1;
                let mut j = i + 2;

                while j < n {
                    let line_j = lines[j];
                    if line_j.trim().is_empty() {
                        break;
                    }
                    if !is_table_row_candidate(line_j) || is_separator(line_j) {
                        break;
                    }
                    last = j;
                    j += 1;
                }

                out.push(TableBlock {
                    first_line: first,
                    last_line: last,
                    aligns,
                });
                i = last + 1;
                continue;
            }
        }

        i += 1;
    }

    out
}

/// Fast local check: parses only the paragraph (between blank lines) around `line`.
/// Note: Does not inspect fenced code blocks; if called across arbitrary lines,
/// filter against code blocks.
pub fn table_at(lines: &[&str], line: usize) -> Option<TableBlock> {
    if line >= lines.len() {
        return None;
    }
    let mut a = line;
    while a > 0 && !lines[a - 1].trim().is_empty() {
        a -= 1;
    }
    let mut b = line;
    while b + 1 < lines.len() && !lines[b + 1].trim().is_empty() {
        b += 1;
    }
    find_tables(&lines[a..=b])
        .into_iter()
        .find(|t| t.contains(line - a))
        .map(|mut t| {
            t.first_line += a;
            t.last_line += a;
            t
        })
}

// ───────────────────────────── Cells ─────────────────────────────

/// Row cell in character indices.
/// `outer` spans everything between adjacent `|` delimiters; `content` is trimmed of surrounding whitespace.
/// For an empty cell, `content` is an empty range where the caret can be placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub outer: Range<usize>,
    pub content: Range<usize>,
}

/// Splits a table row into cells. Escaped pipes `\|` do not split.
/// Leading and trailing `|` are optional under GFM rules.
pub fn split_row(line: &str) -> Vec<Cell> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();

    let mut pipes: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < n {
        match chars[i] {
            '\\' => i += 2,
            '|' => {
                pipes.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }

    let first_ns = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    let last_ns = chars.iter().rposition(|c| !c.is_whitespace());
    let lead = pipes.first() == Some(&first_ns);
    let trail = last_ns.is_some() && pipes.last().copied() == last_ns && (pipes.len() > 1 || !lead);

    let mut bounds: Vec<(usize, usize)> = Vec::new();
    let mut seg_start = if lead { first_ns + 1 } else { 0 };
    for &p in pipes.iter().skip(lead as usize) {
        bounds.push((seg_start, p));
        seg_start = p + 1;
    }
    if !trail {
        bounds.push((seg_start.min(n), n));
    }

    bounds
        .into_iter()
        .map(|(s, e)| {
            let (mut cs, mut ce) = (s, e);
            while cs < ce && chars[cs].is_whitespace() {
                cs += 1;
            }
            while ce > cs && chars[ce - 1].is_whitespace() {
                ce -= 1;
            }
            let content = if cs == ce {
                let p = (s + 1).min(e);
                p..p
            } else {
                cs..ce
            };
            Cell { outer: s..e, content }
        })
        .collect()
}

/// Identifies which cell owns the char position `col`.
/// Position immediately after `|` belongs to the next cell; position right before `|` belongs to current cell.
pub fn cell_index_at(cells: &[Cell], col: usize) -> usize {
    if cells.is_empty() {
        return 0;
    }
    for (i, c) in cells.iter().enumerate() {
        if col <= c.outer.end {
            return i;
        }
    }
    cells.len() - 1
}

fn slice_chars(s: &str, r: &Range<usize>) -> String {
    s.chars().skip(r.start).take(r.end - r.start).collect()
}

/// Rectangular range of cells: rows `row_start..row_end`, cols `col_start..col_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

impl CellRect {
    pub fn new(row1: usize, col1: usize, row2: usize, col2: usize) -> Self {
        Self {
            row_start: row1.min(row2),
            row_end: row1.max(row2) + 1,
            col_start: col1.min(col2),
            col_end: col1.max(col2) + 1,
        }
    }

    pub fn contains_cell(&self, row: usize, col: usize) -> bool {
        (self.row_start..self.row_end).contains(&row) && (self.col_start..self.col_end).contains(&col)
    }
}

#[inline]
pub(crate) fn get_table_line<'a>(block: &TableBlock, lines: &'a [&'a str], doc_line: usize) -> &'a str {
    if lines.len() > block.last_line {
        lines[doc_line]
    } else if doc_line >= block.first_line && doc_line - block.first_line < lines.len() {
        lines[doc_line - block.first_line]
    } else {
        ""
    }
}

// ───────────────────────────── Model ─────────────────────────────

/// Logical content of a table. `rows[0]` is the header; the separator line is not stored.
/// Every row contains exactly `aligns.len()` cells: short rows are padded, surplus cells
/// are trimmed on serialization (matching GFM rendering behavior).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableModel {
    pub aligns: Vec<Align>,
    pub rows: Vec<Vec<String>>,
}

impl TableModel {
    pub fn parse(block: &TableBlock, lines: &[&str]) -> Self {
        let cols = block.col_count();
        let rows = (0..block.row_count())
            .map(|r| {
                let line = get_table_line(block, lines, block.line_of_row(r));
                let mut cells: Vec<String> =
                    split_row(line).iter().map(|c| slice_chars(line, &c.content)).collect();
                cells.resize(cols, String::new());
                cells
            })
            .collect();
        Self { aligns: block.aligns.clone(), rows }
    }

    pub fn col_count(&self) -> usize {
        self.aligns.len()
    }

    /// When `pad = true`, pads columns to uniform widths measured by `width`.
    pub fn serialize(&self, pad: bool, width: &dyn Fn(&str) -> usize) -> Vec<String> {
        let n = self.col_count();
        let mut w = vec![3usize; n];
        if pad {
            for row in &self.rows {
                for (i, c) in row.iter().enumerate() {
                    w[i] = w[i].max(width(c));
                }
            }
        }
        let fmt_row = |cells: &[String]| {
            let mut s = String::from("|");
            for (i, c) in cells.iter().enumerate() {
                s.push(' ');
                s.push_str(c);
                if pad {
                    for _ in width(c)..w[i] {
                        s.push(' ');
                    }
                }
                s.push_str(" |");
            }
            s
        };
        let sep = {
            let mut s = String::from("|");
            for i in 0..n {
                s.push(' ');
                let d = w[i];
                match self.aligns[i] {
                    Align::None => s.push_str(&"-".repeat(d)),
                    Align::Left => {
                        s.push(':');
                        s.push_str(&"-".repeat(d - 1));
                    }
                    Align::Right => {
                        s.push_str(&"-".repeat(d - 1));
                        s.push(':');
                    }
                    Align::Center => {
                        s.push(':');
                        s.push_str(&"-".repeat(d - 2));
                        s.push(':');
                    }
                }
                s.push_str(" |");
            }
            s
        };

        let mut out = Vec::with_capacity(self.rows.len() + 1);
        out.push(fmt_row(&self.rows[0]));
        out.push(sep);
        for r in &self.rows[1..] {
            out.push(fmt_row(r));
        }
        out
    }

    // ── Structural operations ──

    /// Inserts an empty body row; `at >= 1` (cannot insert before header row 0).
    pub fn insert_row(&mut self, at: usize) -> bool {
        if at == 0 || at > self.rows.len() {
            return false;
        }
        self.rows.insert(at, vec![String::new(); self.col_count()]);
        true
    }

    /// Header row 0 cannot be deleted.
    pub fn delete_row(&mut self, row: usize) -> bool {
        if row == 0 || row >= self.rows.len() {
            return false;
        }
        self.rows.remove(row);
        true
    }

    pub fn insert_col(&mut self, at: usize) -> bool {
        if at > self.col_count() {
            return false;
        }
        self.aligns.insert(at, Align::None);
        for r in &mut self.rows {
            r.insert(at, String::new());
        }
        true
    }

    /// The last remaining column cannot be deleted.
    pub fn delete_col(&mut self, col: usize) -> bool {
        if self.col_count() <= 1 || col >= self.col_count() {
            return false;
        }
        self.aligns.remove(col);
        for r in &mut self.rows {
            r.remove(col);
        }
        true
    }

    pub fn move_col(&mut self, from: usize, to: usize) -> bool {
        let n = self.col_count();
        if from >= n || to >= n {
            return false;
        }
        let a = self.aligns.remove(from);
        self.aligns.insert(to, a);
        for r in &mut self.rows {
            let c = r.remove(from);
            r.insert(to, c);
        }
        true
    }

    pub fn set_align(&mut self, col: usize, a: Align) -> bool {
        match self.aligns.get_mut(col) {
            Some(x) => {
                *x = a;
                true
            }
            None => false,
        }
    }

    /// Sorts body rows by column value.
    /// If all non-empty entries in the column parse as numbers, sorts numerically;
    /// otherwise performs case-insensitive textual sort.
    pub fn sort_by_col(&mut self, col: usize, ascending: bool) -> bool {
        if col >= self.col_count() || self.rows.len() < 3 {
            return false;
        }
        let numeric = self.rows[1..]
            .iter()
            .filter(|r| !r[col].trim().is_empty())
            .all(|r| r[col].trim().parse::<f64>().is_ok());
        let body = &mut self.rows[1..];
        if numeric {
            let key = |r: &Vec<String>| r[col].trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            body.sort_by(|a, b| key(a).total_cmp(&key(b)));
        } else {
            body.sort_by_cached_key(|r| r[col].to_lowercase());
        }
        if !ascending {
            body.reverse();
        }
        true
    }

    /// Clears the contents of all cells inside `rect`.
    pub fn clear_rect(&mut self, rect: &CellRect) {
        let r_end = rect.row_end.min(self.rows.len());
        let c_end = rect.col_end.min(self.col_count());
        for r in rect.row_start..r_end {
            for c in rect.col_start..c_end {
                self.rows[r][c].clear();
            }
        }
    }

    /// Pastes a TSV block starting at `(row_start, col_start)`. Expands rows and columns if needed.
    pub fn paste_tsv(&mut self, row_start: usize, col_start: usize, tsv: &str) -> Option<(usize, usize)> {
        let pasted = TableModel::from_tsv(tsv)?;
        let mut max_r = row_start;
        let mut max_c = col_start;
        for (dr, src_row) in pasted.rows.iter().enumerate() {
            let r = row_start + dr;
            while r >= self.rows.len() {
                self.insert_row(self.rows.len());
            }
            for (dc, cell) in src_row.iter().enumerate() {
                let c = col_start + dc;
                while c >= self.col_count() {
                    self.insert_col(self.col_count());
                }
                self.rows[r][c] = cell.clone();
                max_c = max_c.max(c);
            }
            max_r = max_r.max(r);
        }
        Some((max_r, max_c))
    }

    // ── TSV (clipboard, spreadsheet interchange) ──

    /// Parses TSV where the first line becomes the header. Pipes are escaped automatically.
    pub fn from_tsv(text: &str) -> Option<Self> {
        let mut rows: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').map(sanitize_cell_input).collect())
            .collect();
        let cols = rows.iter().map(Vec::len).max()?;
        for r in &mut rows {
            r.resize(cols, String::new());
        }
        Some(Self { aligns: vec![Align::None; cols], rows })
    }

    pub fn to_tsv(&self, rows: Range<usize>, cols: Range<usize>) -> String {
        self.rows[rows.start.min(self.rows.len())..rows.end.min(self.rows.len())]
            .iter()
            .map(|r| {
                r[cols.start.min(r.len())..cols.end.min(r.len())]
                    .iter()
                    .map(|c| c.replace("\\|", "|"))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Sanitizes user-typed or pasted input for a table cell: `|` -> `\|`, newlines -> `<br>`.
pub fn sanitize_cell_input(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_backslash = false;
    for ch in s.replace("\r\n", "\n").chars() {
        match ch {
            '|' if !prev_backslash => out.push_str("\\|"),
            '\n' | '\r' => out.push_str("<br>"),
            c => out.push(c),
        }
        prev_backslash = ch == '\\' && !prev_backslash;
    }
    out
}

// ───────────────────────────── Edits ─────────────────────────────

/// Full replacement of an entire contiguous line range of a table.
/// Replacing all table lines in one operation preserves single-step undo history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub first_line: usize,
    pub old_line_count: usize,
    pub new_lines: Vec<String>,
}

impl LineEdit {
    pub fn apply(&self, lines: &mut Vec<String>) {
        lines.splice(self.first_line..self.first_line + self.old_line_count, self.new_lines.clone());
    }
}

pub fn rewrite(block: &TableBlock, model: &TableModel, pad: bool, width: &dyn Fn(&str) -> usize) -> LineEdit {
    LineEdit {
        first_line: block.first_line,
        old_line_count: block.last_line - block.first_line + 1,
        new_lines: model.serialize(pad, width),
    }
}

pub fn char_width(s: &str) -> usize {
    s.chars().count()
}

// ───────────────────────────── Navigation ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    To { row: usize, cell: usize },
    /// Appends a new row at the bottom and places the cursor at `cell`.
    AppendRow { cell: usize },
    /// Exits the table (e.g. Shift+Tab in the first header cell).
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Start,
    End,
}

fn current_cell(block: &TableBlock, lines: &[&str], line: usize, col: usize) -> Option<(usize, usize)> {
    let row = block.row_of_line(line)?;
    let line_str = get_table_line(block, lines, line);
    let cell = cell_index_at(&split_row(line_str), col).min(block.col_count() - 1);
    Some((row, cell))
}

/// Tab / Shift+Tab navigation from an already known `(row, cell)`.
/// Tab in the last cell of the last row appends a row.
pub fn tab_from(block: &TableBlock, row: usize, cell: usize, back: bool) -> Nav {
    let cols = block.col_count();
    if back {
        if cell > 0 {
            Nav::To { row, cell: cell - 1 }
        } else if row > 0 {
            Nav::To { row: row - 1, cell: cols - 1 }
        } else {
            Nav::Exit
        }
    } else if cell + 1 < cols {
        Nav::To { row, cell: cell + 1 }
    } else if row + 1 < block.row_count() {
        Nav::To { row: row + 1, cell: 0 }
    } else {
        Nav::AppendRow { cell: 0 }
    }
}

/// Tab / Shift+Tab navigation. Tab in the last cell of the last row appends a row.
pub fn tab(block: &TableBlock, lines: &[&str], line: usize, col: usize, back: bool) -> Option<Nav> {
    let (row, cell) = current_cell(block, lines, line, col)?;
    Some(tab_from(block, row, cell, back))
}

/// Enter navigation from an already known `(row, cell)`.
/// Appends a row when moving down from the last row.
pub fn enter_from(block: &TableBlock, row: usize, cell: usize, up: bool) -> Nav {
    if up {
        if row > 0 { Nav::To { row: row - 1, cell } } else { Nav::Exit }
    } else if row + 1 < block.row_count() {
        Nav::To { row: row + 1, cell }
    } else {
        Nav::AppendRow { cell }
    }
}

/// Enter navigates to the cell directly below (appends a row if on the last row).
/// Shift+Enter navigates to the cell directly above.
pub fn enter(block: &TableBlock, lines: &[&str], line: usize, col: usize, up: bool) -> Option<Nav> {
    let (row, cell) = current_cell(block, lines, line, col)?;
    Some(enter_from(block, row, cell, up))
}

/// Computes target cursor position `(document_line, char_column)` within the cell.
pub fn nav_cursor(block: &TableBlock, lines: &[&str], row: usize, cell: usize, place: Place) -> (usize, usize) {
    let line = block.line_of_row(row);
    let line_str = get_table_line(block, lines, line);
    let cells = split_row(line_str);
    let col = match cells.get(cell) {
        Some(c) => match place {
            Place::Start => c.content.start,
            Place::End => c.content.end,
        },
        None => line_str.chars().count(),
    };
    (line, col)
}

// ───────────────────────────── Edit Guard ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditGuard {
    Allow,
    /// Edit touches a `|` delimiter or the separator row — deny or handle via structural command.
    Deny,
}

/// Validates raw character range edits within a table line.
/// Edits are allowed only entirely inside a single cell's `content` or within whitespace inside outer bounds.
/// Backspace at the beginning of a cell or Delete at the end would delete the `|` delimiter and is denied.
pub fn guard_edit(block: &TableBlock, line_idx: usize, line: &str, sel: Range<usize>) -> EditGuard {
    if !block.contains(line_idx) || line_idx == block.sep_line() {
        return EditGuard::Deny;
    }
    let cells = split_row(line);
    let i = cell_index_at(&cells, sel.start);
    match cells.get(i) {
        Some(c) if sel.start >= c.outer.start && sel.end <= c.outer.end => EditGuard::Allow,
        _ => EditGuard::Deny,
    }
}

// ───────────────────────────── Tests ─────────────────────────────

#[cfg(test)]
mod tests;

