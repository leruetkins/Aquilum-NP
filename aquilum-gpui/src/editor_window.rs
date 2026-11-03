//! The window of the editor's visual cache — the only part of the document that
//! is ever split into lines, wrapped, parsed for code fences and measured.
//!
//! Opening a 600 MB note used to build all four caches for the whole file: a
//! `Vec<String>` with one heap buffer per line, a `VisualRow` per wrapped
//! sub-row, a code state per line and a height per visual row. That is seconds
//! of work and several times the size of the text, before a single row is
//! painted, and the renderer only ever looks at a viewport. This type keeps the
//! same structures for a bounded slice around the viewport instead; everything
//! outside it is an estimate (`EditorViewState::above_h` / `below_h`).
//!
//! See `TODO/perf-plan-large-file-open.md`.
//!
//! Two coordinate systems meet here and must not be confused:
//!
//! * **Visual row indices are window-local.** `height_index` addresses the
//!   window's own rows, so `top_of`, `v_idx_at_y` and `visible_range` all speak
//!   window coordinates, which is what keeps the `f64` geometry near zero and
//!   `slice_shift` small.
//! * **Line indices are global.** Every [`VisualRow`] carries the document line
//!   it came from, so hit-testing, the caret and line numbers keep working in
//!   document coordinates without translation. The window's own arrays are
//!   indexed by `global_line - start_line`; use [`EditorWindow::local`] for that
//!   instead of subtracting by hand.

use std::sync::Arc;

use crate::markdown::{self, CodeBlockState, VisualRow, VisualRowHeightIndex};

/// Lines kept on each side of the anchor when the window is placed.
///
/// 6000 lines is roughly 480 KB of text at 80 bytes per line, and thousands of
/// rows more than any viewport can show, so the window never needs to move
/// while the user scrolls within a screen or two.
pub const MARGIN_LINES: usize = 3000;

/// How far the anchor may travel towards an edge before the window is re-placed.
///
/// Re-placing costs a re-wrap and a re-measure of `MARGIN_LINES`, so it must not
/// happen on every scroll event. With half a margin of slack the window moves
/// once per `MARGIN_LINES / 2` lines scrolled.
pub const FOLLOW_SLACK: usize = MARGIN_LINES / 2;

/// The window's lines, addressed by **document** line number.
///
/// Storage stays window-local — that is the whole point — but every lookup takes
/// a document line, so call sites keep writing `lines[row.line_idx]` exactly as
/// they did when the array covered the document. Both index spaces are `usize`,
/// and an untyped `- start_line` somewhere in a thousand lines of renderer would
/// silently read a neighbouring line instead of failing; this type is what makes
/// that impossible.
#[derive(Debug, Clone, Default)]
pub struct WindowLines {
    start: usize,
    lines: Vec<String>,
}

impl WindowLines {
    /// Lines of `start..start + lines.len()`, in document order.
    pub fn new(start: usize, lines: Vec<String>) -> Self {
        Self { start, lines }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    /// One past the last document line held.
    pub fn end(&self) -> usize {
        self.start + self.lines.len()
    }

    /// Number of lines held, which for a windowed note is not the document's.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn covers(&self, line: usize) -> bool {
        line >= self.start && line < self.end()
    }

    /// Line text by document line number.
    pub fn get(&self, line: usize) -> Option<&String> {
        self.covers(line).then(|| &self.lines[line - self.start])
    }

    /// Window-local slice, in document order. Only for code that measures the
    /// window as a whole — line addressing goes through `get` / `Index`.
    pub fn as_slice(&self) -> &[String] {
        &self.lines
    }

    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.lines.iter()
    }

    /// Bytes held: the headers plus every per-line buffer.
    pub fn bytes(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<String>()
            + self.lines.iter().map(|s| s.capacity()).sum::<usize>()
    }

    /// Shifts the starting line index of the window lines.
    pub fn shift_start(&mut self, line_diff: isize) {
        self.start = (self.start as isize + line_diff).max(0) as usize;
    }
}

impl std::ops::Index<usize> for WindowLines {
    type Output = String;

    fn index(&self, line: usize) -> &String {
        self.get(line)
            .unwrap_or_else(|| panic!("line {line} is outside the window {}..{}", self.start, self.end()))
    }
}

/// Code-fence state per window line, addressed by **document** line number.
///
/// The same bargain as [`WindowLines`]: window-local storage, document-line
/// lookups, so the renderer's `states[row.line_idx]` stays correct.
#[derive(Debug, Clone, Default)]
pub struct WindowCodeStates {
    start: usize,
    states: Vec<Option<CodeBlockState>>,
}

impl WindowCodeStates {
    /// States of `start..start + states.len()`, in document order.
    pub fn new(start: usize, states: Vec<Option<CodeBlockState>>) -> Self {
        Self { start, states }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.start + self.states.len()
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn covers(&self, line: usize) -> bool {
        line >= self.start && line < self.end()
    }

    pub fn get(&self, line: usize) -> Option<&Option<CodeBlockState>> {
        self.covers(line).then(|| &self.states[line - self.start])
    }

    pub fn as_slice(&self) -> &[Option<CodeBlockState>] {
        &self.states
    }

    /// Bytes held.
    pub fn bytes(&self) -> usize {
        self.states.capacity() * std::mem::size_of::<Option<CodeBlockState>>()
    }

    /// Shifts the starting line index of the window code states.
    pub fn shift_start(&mut self, line_diff: isize) {
        self.start = (self.start as isize + line_diff).max(0) as usize;
    }
}

impl std::ops::Index<usize> for WindowCodeStates {
    type Output = Option<CodeBlockState>;

    fn index(&self, line: usize) -> &Option<CodeBlockState> {
        self.get(line).unwrap_or_else(|| {
            panic!("line {line} is outside the window {}..{}", self.start, self.end())
        })
    }
}

/// A bounded slice of the document, fully materialised.
///
/// `Default` is the empty window: no lines, no rows, no height. An empty window
/// is what a note has before its first frame.
#[derive(Debug, Clone, Default)]
pub struct EditorWindow {
    /// First global line held by the window.
    pub start_line: usize,
    /// One past the last global line held by the window.
    pub end_line: usize,
    /// Byte offset of `start_line` in the document.
    pub start_byte: usize,
    /// Byte offset just past the last character of `end_line - 1`.
    pub end_byte: usize,

    /// The window's lines, addressed by document line number.
    pub lines: Arc<WindowLines>,
    /// Wrapped rows of those lines, in order. `line_idx` is **global**.
    pub visual_rows: Arc<Vec<VisualRow>>,
    /// Code-fence state per window line, addressed by document line number.
    pub code_states: Arc<WindowCodeStates>,
    /// Row geometry of the window, addressed by **window-local** row index.
    pub height_index: Arc<VisualRowHeightIndex>,

    pub word_wrap: bool,
    pub wrap_cols: usize,
}

impl EditorWindow {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Number of lines held. Zero for the empty window.
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    /// Whether `line` is inside the window.
    pub fn covers_line(&self, line: usize) -> bool {
        !self.is_empty() && line >= self.start_line && line < self.end_line
    }

    /// Index of `line` in the window's own arrays.
    ///
    /// Callers must have checked [`EditorWindow::covers_line`] first.
    pub fn local(&self, line: usize) -> usize {
        debug_assert!(
            self.covers_line(line),
            "line {line} outside window {}..{}",
            self.start_line,
            self.end_line
        );
        line.saturating_sub(self.start_line)
    }

    /// Line text by document line number, or `None` outside the window.
    pub fn line(&self, line: usize) -> Option<&str> {
        self.lines.get(line).map(|s| s.as_str())
    }

    /// Code state by document line number, or `None` outside the window.
    pub fn code_state(&self, line: usize) -> Option<&CodeBlockState> {
        self.code_states.get(line).and_then(|s| s.as_ref())
    }

    /// Window-local visual row index of `line`'s first row, or `None` outside
    /// the window.
    pub fn first_row_of_line(&self, line: usize) -> Option<usize> {
        if !self.covers_line(line) {
            return None;
        }
        let at = self.visual_rows.partition_point(|v| v.line_idx < line);
        (at < self.visual_rows.len() && self.visual_rows[at].line_idx == line).then_some(at)
    }

    /// Total height of the window's rows, in logical pixels.
    pub fn total_height(&self) -> f64 {
        self.height_index.total()
    }

    /// Shifts document line and byte indices of the window when an edit occurs strictly above it.
    /// Invariant R5: the text inside the window, code fences, and row height index remain untouched;
    /// only document line numbers and byte offsets shift.
    pub fn shift_lines_and_bytes(&mut self, line_diff: isize, byte_diff: isize) {
        if self.is_empty() {
            return;
        }
        self.start_line = (self.start_line as isize + line_diff).max(0) as usize;
        self.end_line = (self.end_line as isize + line_diff).max(0) as usize;
        self.start_byte = (self.start_byte as isize + byte_diff).max(0) as usize;
        self.end_byte = (self.end_byte as isize + byte_diff).max(0) as usize;

        if line_diff != 0 {
            if let Some(lines) = Arc::get_mut(&mut self.lines) {
                lines.shift_start(line_diff);
            } else {
                let mut lines = (*self.lines).clone();
                lines.shift_start(line_diff);
                self.lines = Arc::new(lines);
            }

            if let Some(states) = Arc::get_mut(&mut self.code_states) {
                states.shift_start(line_diff);
            } else {
                let mut states = (*self.code_states).clone();
                states.shift_start(line_diff);
                self.code_states = Arc::new(states);
            }

            if let Some(rows) = Arc::get_mut(&mut self.visual_rows) {
                for row in rows.iter_mut() {
                    row.line_idx = (row.line_idx as isize + line_diff).max(0) as usize;
                }
            } else {
                let mut rows = (*self.visual_rows).clone();
                for row in rows.iter_mut() {
                    row.line_idx = (row.line_idx as isize + line_diff).max(0) as usize;
                }
                self.visual_rows = Arc::new(rows);
            }
        }
    }

    /// Window bounds that hold `anchor_line` with a margin on each side.
    ///
    /// The result is clamped to the document, so at either end the window is
    /// one-sided. It is a pure function of the anchor, which is what keeps
    /// re-placing idempotent: asking twice for the same anchor yields the same
    /// window and no rebuild.
    ///
    /// A document that fits inside two margins is **not** windowed at all: it
    /// returns its full range. That keeps every ordinary note — anything up to
    /// `2 * MARGIN_LINES + 1` lines — materialised exactly as before, with an
    /// exact height and no estimate anywhere. Windowing only ever applies to
    /// documents that are actually too big to hold.
    pub fn bounds_for(anchor_line: usize, lines_total: usize) -> (usize, usize) {
        if lines_total == 0 {
            return (0, 0);
        }
        if lines_total <= 2 * MARGIN_LINES + 1 {
            return (0, lines_total);
        }
        let anchor = anchor_line.min(lines_total - 1);
        let start = anchor.saturating_sub(MARGIN_LINES);
        let end = anchor.saturating_add(MARGIN_LINES).saturating_add(1).min(lines_total);
        (start, end.max(start + 1))
    }

    /// Whether the window must be re-placed for this anchor.
    ///
    /// Re-placing is expensive, so this is deliberately conservative: while the
    /// anchor sits comfortably inside, nothing happens; once it comes within
    /// `FOLLOW_SLACK` of an edge the window is re-placed, but only if the new
    /// bounds actually differ — otherwise a window that already reaches the end
    /// of the document would be rebuilt on every frame.
    pub fn should_move_to(
        &self,
        anchor_line: usize,
        lines_total: usize,
        word_wrap: bool,
        wrap_cols: usize,
    ) -> bool {
        if self.is_empty() || self.word_wrap != word_wrap || self.wrap_cols != wrap_cols {
            return true;
        }
        if self.covers_line(anchor_line) {
            let top_ok = self.start_line == 0 || anchor_line >= self.start_line + FOLLOW_SLACK;
            let bottom_ok =
                self.end_line >= lines_total || anchor_line + FOLLOW_SLACK < self.end_line;
            if top_ok && bottom_ok {
                return false;
            }
        }
        Self::bounds_for(anchor_line, lines_total) != (self.start_line, self.end_line)
    }

    /// Materialise lines `start_line..end_line` of `content`.
    ///
    /// Splits, wraps, scans code fences and measures only that slice. The slice
    /// boundaries come from [`crate::line_index::LineIndex`], so finding them
    /// costs one checkpoint jump rather than a scan from the top of the file.
    ///
    /// The fence scan starts from `initial` — the state at `start_line`, from
    /// [`crate::code_scan::CodeCheckpoints`] — so a window that opens in the
    /// middle of a code block knows it is inside one. Passing the default state
    /// is only correct for a window that starts at the top of the document.
    pub fn build(
        content: &str,
        line_index: &crate::line_index::LineIndex,
        start_line: usize,
        end_line: usize,
        word_wrap: bool,
        wrap_cols: usize,
        typography: Option<&crate::settings::EditorTypography>,
        initial: crate::markdown::CodeScanState,
    ) -> Self {
        let lines_total = line_index.lines();
        if lines_total == 0 || start_line >= end_line {
            return Self::default();
        }
        let start_line = start_line.min(lines_total - 1);
        let end_line = end_line.clamp(start_line + 1, lines_total);

        let start_byte = line_index.start(content, start_line).min(content.len());
        // `LineIndex::end` is the offset of the newline that closes the line (or
        // the text length for the last one), so the slice stops before it and
        // `split('\n')` yields exactly `end_line - start_line` items.
        let end_byte = line_index
            .end(content, end_line - 1)
            .clamp(start_byte, content.len());

        let lines: Vec<String> = content[start_byte..end_byte]
            .split('\n')
            .map(String::from)
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        // The fence scan comes first: the canonical wrap needs to know which
        // lines are code, because a code line paints its markup literally and
        // therefore wraps by raw width.
        let mut scanner = markdown::CodeScanner::from_state(initial);
        let code_states: Vec<Option<CodeBlockState>> =
            line_refs.iter().map(|line| scanner.step(line)).collect();
        let mut visual_rows = markdown::build_visual_rows_canonical(
            &line_refs,
            &code_states,
            word_wrap,
            wrap_cols,
            typography.map(|typography| &typography.headings),
            false,
            |_| false,
        );
        // The height index resolves every row back to its line text, so it has
        // to be built while `line_idx` still counts from the window's first
        // line. It is only after this that rows are lifted to document
        // coordinates.
        let height_index = VisualRowHeightIndex::from_rows(&lines, &visual_rows, typography, false);
        for row in &mut visual_rows {
            row.line_idx += start_line;
        }

        Self {
            start_line,
            end_line,
            start_byte,
            end_byte,
            lines: Arc::new(WindowLines { start: start_line, lines }),
            visual_rows: Arc::new(visual_rows),
            code_states: Arc::new(WindowCodeStates { start: start_line, states: code_states }),
            height_index: Arc::new(height_index),
            word_wrap,
            wrap_cols,
        }
    }

    /// Build a window directly from a bounded window slice string with table filtering, avoiding any O(document) allocation.
    pub fn build_from_slice_with_tables(
        slice: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
        end_byte: usize,
        word_wrap: bool,
        wrap_cols: usize,
        typography: Option<&crate::settings::EditorTypography>,
        source_mode: bool,
        initial: crate::markdown::CodeScanState,
        table_row_info: impl Fn(usize, &str) -> Option<crate::markdown::TableRowVisualKind>,
        image_row_height: impl Fn(usize, &str) -> Option<f32>,
    ) -> Self {
        let lines_total = end_line.max(start_line);
        if lines_total == 0 || start_line >= end_line {
            return Self::default();
        }

        let lines: Vec<String> = slice
            .split('\n')
            .map(String::from)
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let mut scanner = markdown::CodeScanner::from_state(initial);
        let code_states: Vec<Option<CodeBlockState>> =
            line_refs.iter().map(|line| scanner.step(line)).collect();
        let mut visual_rows = markdown::build_visual_rows_canonical(
            &line_refs,
            &code_states,
            word_wrap,
            wrap_cols,
            typography.map(|typography| &typography.headings),
            source_mode,
            |idx| {
                let text = line_refs.get(idx).copied().unwrap_or("");
                table_row_info(start_line + idx, text).is_some()
                    || image_row_height(start_line + idx, text).is_some()
            },
        );
        let height_index = VisualRowHeightIndex::from_rows_with_tables(
            &lines,
            &visual_rows,
            typography,
            source_mode,
            |idx, text| table_row_info(start_line + idx, text),
            |idx, text| image_row_height(start_line + idx, text),
        );
        for row in &mut visual_rows {
            row.line_idx += start_line;
        }

        Self {
            start_line,
            end_line,
            start_byte,
            end_byte,
            lines: Arc::new(WindowLines { start: start_line, lines }),
            visual_rows: Arc::new(visual_rows),
            code_states: Arc::new(WindowCodeStates { start: start_line, states: code_states }),
            height_index: Arc::new(height_index),
            word_wrap,
            wrap_cols,
        }
    }

    /// Build a window directly from a bounded window slice string, avoiding any O(document) allocation.
    pub fn build_from_slice(
        slice: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
        end_byte: usize,
        word_wrap: bool,
        wrap_cols: usize,
        typography: Option<&crate::settings::EditorTypography>,
        initial: crate::markdown::CodeScanState,
    ) -> Self {
        Self::build_from_slice_with_tables(
            slice,
            start_line,
            end_line,
            start_byte,
            end_byte,
            word_wrap,
            wrap_cols,
            typography,
            false,
            initial,
            |_, _| None,
            |_, _| None,
        )
    }

    /// Incremental update of a single line inside the window (e.g. typing a character without newline).
    /// Avoids rebuilding all 6000 window lines and re-parsing markdown across the window.
    ///
    /// The line text, its visual rows and the height index are updated in place:
    /// the heights of the replaced rows are recomputed for the new row set only.
    /// Returns `false` when the caller must fall back to a full rebuild — the
    /// line is not covered, its rows cannot be located, or the edit touches a
    /// code fence (a fence line changes the code state of this and every
    /// following window line, which only a rebuild can recompute).
    pub fn update_single_line(
        &mut self,
        line: usize,
        new_line_text: String,
        delta: isize,
        typography: Option<&crate::settings::EditorTypography>,
        table_row_info: impl Fn(usize, &str) -> Option<markdown::TableRowVisualKind>,
    ) -> bool {
        if !self.covers_line(line) {
            return false;
        }
        let local_line = line - self.start_line;
        if local_line >= self.lines.lines.len() {
            return false;
        }

        let default_typography;
        let typography = match typography {
            Some(typography) => typography,
            None => {
                default_typography = crate::settings::EditorTypography::reference();
                &default_typography
            }
        };

        let old_is_fence = self.lines.lines[local_line].trim_start().starts_with("```");
        let new_is_fence = new_line_text.trim_start().starts_with("```");
        if old_is_fence || new_is_fence {
            return false;
        }

        self.end_byte = (self.end_byte as isize + delta).max(self.start_byte as isize) as usize;

        let table_kind = table_row_info(line, &new_line_text);

        // Table rows never wrap: a table line is always one visual row whose
        // height comes from the wrapped lines of its tallest cell. If the row is
        // already a single table row of the expected height, only the text
        // changed: there is nothing to re-wrap and no height to recompute
        // (R18, N9).
        if let Some(kind) = table_kind {
            let expected_h = kind.height(typography.table_line_height);
            if let Some(first) = self.first_row_of_line(line) {
                let last = self.visual_rows.iter().rposition(|r| r.line_idx == line).unwrap_or(first);
                if first == last && self.height_index.height_of(first) == expected_h {
                    let char_end = new_line_text.chars().count();
                    let row = &mut Arc::make_mut(&mut self.visual_rows)[first];
                    row.sub_idx = 0;
                    row.is_last_sub = true;
                    row.char_start = 0;
                    row.char_end = char_end;
                    Arc::make_mut(&mut self.lines).lines[local_line] = new_line_text;
                    return true;
                }
            }
        }

        let new_rows = {
            let ranges = self.wrap_ranges_for(line, &new_line_text, None, typography, table_kind);
            Self::rows_from_ranges(line, &ranges)
        };

        let vrows_mut = Arc::make_mut(&mut self.visual_rows);
        let first_row = vrows_mut.iter().position(|r| r.line_idx == line);
        let last_row = vrows_mut.iter().rposition(|r| r.line_idx == line);
        let (first_row, last_row) = match (first_row, last_row) {
            (Some(f), Some(l)) => (f, l),
            _ => return false,
        };

        let mut adjusted_new_rows = new_rows;
        for r in &mut adjusted_new_rows {
            r.line_idx = line;
        }

        // Every sub-row of one line carries the same height: it derives from the
        // line text and the table kind only, exactly as the full rebuild computes
        // it in `VisualRowHeightIndex::from_rows_with_tables`.
        let row_h = markdown::visual_row_height(&new_line_text, Some(typography), false, table_kind);
        let new_heights = vec![row_h; adjusted_new_rows.len()];

        vrows_mut.splice(first_row..=last_row, adjusted_new_rows);
        Arc::make_mut(&mut self.lines).lines[local_line] = new_line_text;
        Arc::make_mut(&mut self.height_index).replace_rows(
            first_row,
            last_row - first_row + 1,
            &new_heights,
        );
        true
    }

    /// Raw char ranges the line wraps into for one painted state.
    ///
    /// The canonical state (`paint == None`) is what a window build produces;
    /// a spec is passed for a line under the caret, inside the selection or
    /// revealed by Alt+click.
    fn wrap_ranges_for(
        &self,
        line: usize,
        text: &str,
        paint: Option<&markdown::PaintedLineSpec>,
        typography: &crate::settings::EditorTypography,
        table_kind: Option<markdown::TableRowVisualKind>,
    ) -> Vec<(usize, usize)> {
        let char_count = text.chars().count();
        let state = self.code_states.get(line).and_then(|s| s.as_ref());
        let classified = markdown::classify_line(text, state.map(|s| (s.kind, s.lang.as_deref())));
        if table_kind.is_some()
            || !(self.word_wrap && self.wrap_cols > 0)
            || markdown::line_never_wraps(&classified)
        {
            return vec![(0, char_count)];
        }
        let (active, reveal) = match paint {
            Some(p) => (p.active, p.reveal.clone()),
            None => (false, markdown::InlineReveal::None),
        };
        let metrics =
            markdown::painted_line_metrics(text, &classified, active, &reveal, &typography.headings);
        markdown::wrap_line_ranges_painted(text, &metrics, self.wrap_cols)
    }

    fn rows_from_ranges(line: usize, ranges: &[(usize, usize)]) -> Vec<markdown::VisualRow> {
        let count = ranges.len();
        ranges
            .iter()
            .enumerate()
            .map(|(sub_idx, &(char_start, char_end))| markdown::VisualRow {
                line_idx: line,
                sub_idx,
                is_last_sub: sub_idx + 1 == count,
                char_start,
                char_end,
            })
            .collect()
    }

    /// Re-wrap one line for a new painted state (caret, selection, Alt+click)
    /// without touching any other line.
    ///
    /// Returns the row counts before and after so the caller can compensate the
    /// scroll when the reflowed line sits above the viewport.
    pub fn rewrap_line(
        &mut self,
        line: usize,
        paint: &markdown::PaintedLineSpec,
        typography: Option<&crate::settings::EditorTypography>,
        table_row_info: impl Fn(usize, &str) -> Option<markdown::TableRowVisualKind>,
    ) -> Option<(usize, usize)> {
        if !self.covers_line(line) {
            return None;
        }
        let default_typography;
        let typography = match typography {
            Some(typography) => typography,
            None => {
                default_typography = crate::settings::EditorTypography::reference();
                &default_typography
            }
        };
        let text = self.lines.get(line)?.to_string();
        let table_kind = table_row_info(line, &text);
        let ranges = self.wrap_ranges_for(line, &text, Some(paint), typography, table_kind);
        let new_rows = Self::rows_from_ranges(line, &ranges);

        let vrows_mut = Arc::make_mut(&mut self.visual_rows);
        let first_row = vrows_mut.iter().position(|r| r.line_idx == line)?;
        let last_row = vrows_mut.iter().rposition(|r| r.line_idx == line)?;
        let old_count = last_row - first_row + 1;
        if old_count == new_rows.len() && vrows_mut[first_row..=last_row] == new_rows[..] {
            return Some((old_count, old_count));
        }
        let row_h = markdown::visual_row_height(&text, Some(typography), false, table_kind);
        let new_count = new_rows.len();
        vrows_mut.splice(first_row..=last_row, new_rows);
        let heights = vec![row_h; new_count];
        Arc::make_mut(&mut self.height_index).replace_rows(first_row, old_count, &heights);
        Some((old_count, new_count))
    }

    /// Bytes held by the window, for the open-path benchmark.
    pub fn bytes(&self) -> usize {
        let rows = self.visual_rows.capacity() * std::mem::size_of::<VisualRow>();
        self.lines.bytes() + rows + self.code_states.bytes() + self.height_index.bytes()
    }
}

#[cfg(test)]
mod tests;
