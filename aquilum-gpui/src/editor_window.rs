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
        headings: Option<&crate::settings::HeadingsSettings>,
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
        let mut visual_rows = markdown::build_visual_rows(&line_refs, word_wrap, wrap_cols);
        let mut scanner = markdown::CodeScanner::from_state(initial);
        let code_states: Vec<Option<CodeBlockState>> =
            line_refs.iter().map(|line| scanner.step(line)).collect();
        // The height index resolves every row back to its line text, so it has
        // to be built while `line_idx` still counts from the window's first
        // line. It is only after this that rows are lifted to document
        // coordinates.
        let height_index = VisualRowHeightIndex::from_rows(&lines, &visual_rows, headings);
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

    /// Bytes held by the window, for the open-path benchmark.
    pub fn bytes(&self) -> usize {
        let rows = self.visual_rows.capacity() * std::mem::size_of::<VisualRow>();
        self.lines.bytes() + rows + self.code_states.bytes() + self.height_index.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorWindow, FOLLOW_SLACK, MARGIN_LINES};
    use crate::line_index::LineIndex;

    fn doc(lines: usize) -> String {
        let mut text = String::new();
        for i in 0..lines {
            text.push_str(&format!("line {i}\n"));
        }
        text
    }

    /// The window holds exactly the lines it claims, in document order, and
    /// every other array lines up with it.
    #[test]
    fn window_lines_and_rows_agree_with_the_document() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());

        assert_eq!(w.start_line, 4000);
        assert_eq!(w.end_line, 7000);
        assert_eq!(w.line_count(), 3000);
        assert_eq!(w.lines.len(), 3000);
        assert_eq!(w.code_states.len(), 3000);
        for line in 4000..7000 {
            assert_eq!(w.line(line), Some(format!("line {line}").as_str()));
        }
        // Byte offsets are the ones `LineIndex` reports for those lines.
        assert_eq!(w.start_byte, index.start(&text, 4000));
        assert_eq!(w.end_byte, index.end(&text, 6999));
        assert_eq!(&text[w.start_byte..w.end_byte].split('\n').count(), &3000);
    }

    /// Rows carry document line numbers, so a row found by index can be used
    /// directly against a global line — that is the contract hit-testing and
    /// the caret rely on.
    #[test]
    fn visual_rows_carry_global_line_numbers() {
        let text = doc(5000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 2000, 2600, false, 0, None, crate::markdown::CodeScanState::default());

        assert!(!w.visual_rows.is_empty());
        assert_eq!(w.visual_rows.first().unwrap().line_idx, 2000);
        assert_eq!(w.visual_rows.last().unwrap().line_idx, 2599);
        // Row order is by document line, so the global index is monotone.
        let mut prev = 0;
        for row in w.visual_rows.iter() {
            assert!(row.line_idx >= prev, "row order broke at {}", row.line_idx);
            prev = row.line_idx;
        }
        // And the row geometry is window-local: 600 rows of 24 px.
        assert_eq!(w.height_index.len(), 600);
        assert_eq!(w.height_index.total(), 600.0 * 24.0);
        assert_eq!(w.height_index.top_of(0), 0.0);
        assert_eq!(w.height_index.top_of(600), 600.0 * 24.0);
        assert_eq!(w.first_row_of_line(2000), Some(0));
        assert_eq!(w.first_row_of_line(2599), Some(599));
        assert_eq!(w.first_row_of_line(1999), None);
        assert_eq!(w.first_row_of_line(2600), None);
    }

    /// Wrapping is measured inside the window only, and rows of one line stay
    /// contiguous so `find_visual_row_for_cursor` still works on global lines.
    #[test]
    fn wrapped_window_keeps_lines_contiguous() {
        let text = "aaaa\nbbbbbbbbbb\ncccc\n";
        let index = LineIndex::new(text);
        let w = EditorWindow::build(text, &index, 0, 4, true, 4, None, crate::markdown::CodeScanState::default());
        // "aaaa" (1 row), "bbbbbbbbbb" (3 rows), "cccc" (1 row), "" (1 row).
        assert_eq!(w.visual_rows.len(), 6);
        let line_of = |v: usize| w.visual_rows[v].line_idx;
        assert_eq!((line_of(0), line_of(1), line_of(2), line_of(3)), (0, 1, 1, 1));
        assert_eq!((line_of(4), line_of(5)), (2, 3));
        let found = crate::markdown::find_visual_row_for_cursor(&w.visual_rows, 1, 7);
        assert_eq!(found, Some((2, 3)));
    }

    /// At the document edges the window is one-sided instead of out of range.
    #[test]
    fn bounds_clamp_at_the_document_edges() {
        assert_eq!(EditorWindow::bounds_for(0, 10), (0, 10));
        assert_eq!(EditorWindow::bounds_for(5, 10), (0, 10));
        assert_eq!(
            EditorWindow::bounds_for(1_000_000, 1_000_001),
            (1_000_000 - MARGIN_LINES, 1_000_001)
        );
        assert_eq!(EditorWindow::bounds_for(0, 0), (0, 0));
        assert_eq!(EditorWindow::bounds_for(999, 1), (0, 1));
    }

    /// A note that fits inside two margins is never windowed, so ordinary
    /// documents keep an exact height and no estimate at all.
    #[test]
    fn a_small_document_is_never_windowed() {
        let fits = 2 * MARGIN_LINES + 1;
        for total in [1, 2, 1000, fits] {
            assert_eq!(
                EditorWindow::bounds_for(0, total),
                (0, total),
                "total {total} must stay whole"
            );
            assert_eq!(
                EditorWindow::bounds_for(total - 1, total),
                (0, total),
                "total {total} must stay whole from the end too"
            );
        }
        // One line past that it does start windowing.
        let (s, e) = EditorWindow::bounds_for(fits, fits + 1);
        assert!(s > 0 || e < fits + 1);
    }

    /// Re-placing must be idempotent and must stop once the window already
    /// reaches the end of the document — otherwise the last screenful would be
    /// rebuilt on every frame.
    #[test]
    fn follow_is_idempotent_and_settles_at_the_end() {
        let text = doc(10_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        // Start where `bounds_for` would put the window, not anywhere we like:
        // re-placing is defined relative to that function.
        let (s, e) = EditorWindow::bounds_for(0, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w.start_line, w.end_line), (0, MARGIN_LINES + 1));
        assert!(!w.should_move_to(0, total, false, 0));
        assert!(!w.should_move_to(1, total, false, 0));
        assert!(!w.should_move_to(100, total, false, 0));
        assert!(!w.should_move_to(FOLLOW_SLACK, total, false, 0));
        // Far enough down to want a new window: it may move.
        assert!(w.should_move_to(total - 1, total, false, 0));

        // At the end the bounds cannot grow, so asking again changes nothing.
        let (s, e) = EditorWindow::bounds_for(total - 1, total);
        let w2 = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!(w2.end_line, total);
        assert!(!w2.should_move_to(total - 1, total, false, 0));
        assert!(!w2.should_move_to(total - 2, total, false, 0));
        assert!(!w2.should_move_to(total - 100, total, false, 0));
        assert!(!w2.should_move_to(total - 1 - FOLLOW_SLACK, total, false, 0));

        // Scrolling back up eventually asks for a new window, and that window
        // is the same one `bounds_for` describes.
        assert!(w2.should_move_to(0, total, false, 0));
        let (s3, e3) = EditorWindow::bounds_for(0, total);
        let w3 = EditorWindow::build(&text, &index, s3, e3, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w3.start_line, w3.end_line), (0, MARGIN_LINES + 1));
        assert!(!w3.should_move_to(0, total, false, 0));
    }

    /// Changing the wrap settings always re-places, because row geometry is
    /// only valid for the settings it was measured with.
    #[test]
    fn wrap_change_forces_a_rebuild() {
        let text = doc(100);
        let index = LineIndex::new(&text);
        let total = index.lines();
        let w = EditorWindow::build(&text, &index, 0, total, false, 0, None, crate::markdown::CodeScanState::default());
        assert!(w.should_move_to(0, total, true, 80));
        assert!(w.should_move_to(0, total, false, 40));
        assert!(!w.should_move_to(0, total, false, 0));
    }

    /// Lookups take document lines, not window offsets. Both are `usize`, so
    /// mixing them is exactly the mistake these types exist to prevent.
    #[test]
    fn lines_and_states_are_addressed_by_document_line() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());

        assert_eq!(w.lines[4000], "line 4000");
        assert_eq!(w.lines[6999], "line 6999");
        assert_eq!(w.lines.start(), 4000);
        assert_eq!(w.lines.end(), 7000);
        assert_eq!(w.lines.get(3999), None);
        assert_eq!(w.lines.get(7000), None);
        assert!(w.code_states[4000].is_none());
        assert_eq!(w.code_states.get(3999), None);
        assert_eq!(w.code_states.start(), 4000);
        // `local` is the window offset, and it is what the arrays do not take.
        assert_eq!(w.local(4000), 0);
        assert_eq!(w.local(6999), 2999);
    }

    /// Addressing a line the window does not hold is a panic, not a silent read
    /// of whichever line happens to sit at that offset.
    #[test]
    #[should_panic(expected = "outside the window")]
    fn indexing_outside_the_window_panics() {
        let text = doc(9000);
        let index = LineIndex::new(&text);
        let w = EditorWindow::build(&text, &index, 4000, 7000, false, 0, None, crate::markdown::CodeScanState::default());
        let _ = &w.lines[0];
    }

    /// The empty window is what a note has before its first frame, and it must
    /// answer every question instead of panicking.
    #[test]
    fn empty_window_is_inert() {
        let w = EditorWindow::default();
        assert!(w.is_empty());
        assert_eq!(w.line_count(), 0);
        assert_eq!(w.line(0), None);
        assert_eq!(w.code_state(0), None);
        assert_eq!(w.first_row_of_line(0), None);
        assert_eq!(w.total_height(), 0.0);
        assert_eq!(w.bytes(), 0);
        assert!(w.should_move_to(0, 10, false, 0));
    }

    #[test]
    fn test_typing_stability_near_boundaries_and_middle() {
        let text = doc(20_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        // Window anchored at top (line 0)
        let (s, e) = EditorWindow::bounds_for(0, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());
        assert_eq!((w.start_line, w.end_line), (0, MARGIN_LINES + 1));

        // When typing on line 1, 10, 50, 500, or 1000, should_move_to must NOT trigger
        for anchor in [0, 1, 10, 50, 500, 1000, 1499] {
            assert!(!w.should_move_to(anchor, total, false, 0), "anchor {anchor} should not move window");
        }

        // Window anchored at middle (line 10_000)
        let (s2, e2) = EditorWindow::bounds_for(10_000, total);
        let w2 = EditorWindow::build(&text, &index, s2, e2, false, 0, None, crate::markdown::CodeScanState::default());

        // When typing within FOLLOW_SLACK of 10_000, window must not move
        for anchor in [9000, 9500, 10_000, 10_500, 11_000] {
            assert!(!w2.should_move_to(anchor, total, false, 0), "middle anchor {anchor} should not move window");
        }
    }

    #[test]
    fn test_code_states_and_lines_bounds_with_distant_selection() {
        let text = doc(50_000);
        let index = LineIndex::new(&text);
        let total = index.lines();

        let (s, e) = EditorWindow::bounds_for(10_000, total);
        let w = EditorWindow::build(&text, &index, s, e, false, 0, None, crate::markdown::CodeScanState::default());

        // A selection far outside the window (e.g. lines 40_000..45_000)
        let sel_start_line = 40_000;
        let sel_end_line = 45_000;

        let win_start = w.code_states.start();
        let win_end = w.code_states.end();
        let scan_start = sel_start_line.max(win_start);
        let scan_end = sel_end_line.min(win_end.saturating_sub(1));

        // Clamped range must be empty when selection is entirely outside
        assert!(scan_start > scan_end);

        // Safe query via get() returns None without panic
        assert_eq!(w.lines.get(sel_start_line), None);
        assert_eq!(w.code_states.get(sel_start_line), None);
    }
}
