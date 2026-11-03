//! Fence-parity checkpoints, so a window can start mid-document.
//!
//! The renderer draws a window around the viewport (see [`crate::editor_window`]),
//! and the window's code-fence states used to be computed from `in_code_block =
//! false` — correct only if the window starts at the top of the file. A window
//! placed in the middle of a 400 MB note therefore painted the inside of a code
//! block as plain text.
//!
//! Knowing the state at an arbitrary line normally means scanning from line 0,
//! which is exactly the O(document) work the whole plan exists to remove. So the
//! scan is kept, but *checkpointed*: the state is recorded every
//! [`CHECKPOINT_LINES`] lines, and resolving any line costs a jump to the
//! nearest checkpoint plus at most one block of lines.
//!
//! The table is filled **lazily and in bounded slices**: only as far as the
//! window actually reaches, plus a little ahead of it per frame. Nothing here
//! ever scans the whole document on the way in, and nothing blocks on it.

use crate::line_index::LineIndex;
use crate::markdown::{CodeScanState, CodeScanner};

/// Lines between two stored checkpoint states.
///
/// The same trade-off as [`crate::line_index::LINE_BLOCK`]: larger means less
/// memory and a longer roll-forward. 1024 keeps the roll-forward well under the
/// cost of shaping a row.
pub const CHECKPOINT_LINES: usize = 1024;

/// Lines a single opportunistic extension may consume.
///
/// This is the "scan ahead while the user reads" budget. It is deliberately
/// small enough to be invisible inside a frame — a few tens of microseconds —
/// so the document is walked in the background of normal rendering rather than
/// in one blocking pass.
pub const SCAN_SLICE_LINES: usize = 4096;

/// Fence state every [`CHECKPOINT_LINES`] lines, filled on demand.
#[derive(Debug, Clone)]
pub struct CodeCheckpoints {
    /// `blocks[k]` is the scanner state **before** line `k * CHECKPOINT_LINES`.
    /// Always non-empty: index 0 is the state before the first line.
    blocks: Vec<CodeScanState>,
    /// Lines the scan has consumed. The state before line `scanned` is known.
    scanned: usize,
    /// The state after `scanned` lines — the resume point for the next slice.
    cursor: CodeScanState,
}

impl Default for CodeCheckpoints {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeCheckpoints {
    pub fn new() -> Self {
        Self {
            blocks: vec![CodeScanState::default()],
            scanned: 0,
            cursor: CodeScanState::default(),
        }
    }

    /// Lines the scan has reached. The state before any line below this is known.
    pub fn scanned_lines(&self) -> usize {
        self.scanned
    }

    /// Whether the scan has reached the end of the document.
    pub fn is_complete(&self, lines_total: usize) -> bool {
        self.scanned >= lines_total
    }

    /// Drop every checkpoint after `line`.
    ///
    /// An edit at line `E` leaves lines before `E` untouched, so checkpoints at
    /// or before it stay valid and the rest are simply forgotten; the next
    /// extension re-consumes them from the text. That is the "apply the edit to
    /// the in-flight scan" step: it costs O(1), not O(document).
    pub fn truncate_to(&mut self, line: usize) {
        let keep = (line / CHECKPOINT_LINES).min(self.blocks.len().saturating_sub(1));
        self.blocks.truncate(keep + 1);
        self.scanned = keep * CHECKPOINT_LINES;
        self.cursor = self.blocks[keep];
    }

    /// Forget everything. Used when the whole text is replaced wholesale, as in
    /// undo/redo, where no prefix can be trusted.
    pub fn reset(&mut self) {
        self.blocks.truncate(1);
        self.blocks[0] = CodeScanState::default();
        self.scanned = 0;
        self.cursor = CodeScanState::default();
    }

    /// State before `line`, or `None` if the scan has not reached it yet.
    ///
    /// Costs one checkpoint jump plus at most [`CHECKPOINT_LINES`] lines of
    /// scanning — never a scan from the top of the file.
    pub fn state_at(
        &self,
        content: &str,
        line_index: &LineIndex,
        line: usize,
    ) -> Option<CodeScanState> {
        if line > self.scanned {
            return None;
        }
        let k = (line / CHECKPOINT_LINES).min(self.blocks.len() - 1);
        let base = k * CHECKPOINT_LINES;
        if base >= line {
            return Some(self.blocks[k]);
        }
        let mut scanner = CodeScanner::from_state(self.blocks[k]);
        consume(&mut scanner, content, line_index, base, line, |_, _| {});
        Some(scanner.state())
    }

    /// Extend the scan until it has reached `to_line`, in one go.
    ///
    /// Called on the window-build path, where the state of the window's first
    /// line is a hard requirement. For a note opened at the top this is a few
    /// thousand lines; only a jump far into the document pays for the distance,
    /// and it pays once — afterwards the checkpoints are cached.
    pub fn ensure(&mut self, content: &str, line_index: &LineIndex, to_line: usize) {
        if to_line > self.scanned {
            self.extend_by(content, line_index, to_line - self.scanned);
        }
    }

    /// Extend the scan using a `crop::Rope`.
    pub fn ensure_rope(&mut self, rope: &crop::Rope, to_line: usize) {
        if to_line > self.scanned {
            self.extend_by_rope(rope, to_line - self.scanned);
        }
    }

    /// Fence state before `line`, scanning forward from the nearest checkpoint using `crop::Rope`.
    pub fn state_at_rope(&self, rope: &crop::Rope, line: usize) -> Option<CodeScanState> {
        if line > self.scanned {
            return None;
        }
        let k = (line / CHECKPOINT_LINES).min(self.blocks.len().saturating_sub(1));
        let base = k * CHECKPOINT_LINES;
        if base >= line {
            return Some(self.blocks[k]);
        }
        let mut scanner = CodeScanner::from_state(self.blocks[k]);
        let base_byte = if base < rope.line_len() { rope.byte_of_line(base) } else { rope.byte_len() };
        let line_byte = if line < rope.line_len() { rope.byte_of_line(line) } else { rope.byte_len() };
        let slice = if base_byte < line_byte { rope.byte_slice(base_byte..line_byte).to_string() } else { String::new() };
        consume_slice(&mut scanner, &slice, base, line, |_, _| {});
        Some(scanner.state())
    }

    /// Extend the scan by at most `max_lines`, returning how many were consumed.
    ///
    /// The opportunistic half of the design: called once per frame so that a
    /// document is walked ahead of the user without any thread, any channel and
    /// any copy of the text — the text is already here, and a slice of a few
    /// thousand lines is far too small to be noticed in a frame.
    pub fn extend_slice(
        &mut self,
        content: &str,
        line_index: &LineIndex,
        max_lines: usize,
    ) -> usize {
        self.extend_by(content, line_index, max_lines)
    }

    /// Extend the scan by at most `max_lines` using `crop::Rope`.
    pub fn extend_slice_rope(&mut self, rope: &crop::Rope, max_lines: usize) -> usize {
        self.extend_by_rope(rope, max_lines)
    }

    fn extend_by_rope(&mut self, rope: &crop::Rope, max_lines: usize) -> usize {
        let lines_total = rope.line_len() + usize::from(rope.byte_len() == 0 || rope.byte(rope.byte_len() - 1) == b'\n');
        if max_lines == 0 || self.scanned >= lines_total {
            return 0;
        }
        let from = self.scanned;
        let to = from.saturating_add(max_lines).min(lines_total);

        let from_byte = if from < rope.line_len() { rope.byte_of_line(from) } else { rope.byte_len() };
        let to_byte = if to < rope.line_len() { rope.byte_of_line(to) } else { rope.byte_len() };
        let slice = if from_byte < to_byte { rope.byte_slice(from_byte..to_byte).to_string() } else { String::new() };

        let mut scanner = CodeScanner::from_state(self.cursor);
        let consumed = consume_slice(
            &mut scanner,
            &slice,
            from,
            to,
            |line, state| {
                if line % CHECKPOINT_LINES == 0 {
                    self.blocks.push(state);
                }
            },
        );

        self.cursor = scanner.state();
        self.scanned = from + consumed;
        consumed
    }

    fn extend_by(&mut self, content: &str, line_index: &LineIndex, max_lines: usize) -> usize {
        let lines_total = line_index.lines();
        if max_lines == 0 || self.scanned >= lines_total {
            return 0;
        }
        let from = self.scanned;
        let to = from.saturating_add(max_lines).min(lines_total);

        let mut scanner = CodeScanner::from_state(self.cursor);
        let consumed = consume(
            &mut scanner,
            content,
            line_index,
            from,
            to,
            |line, state| {
                if line % CHECKPOINT_LINES == 0 {
                    self.blocks.push(state);
                }
            },
        );

        self.cursor = scanner.state();
        self.scanned = from + consumed;
        consumed
    }

    /// Bytes held, for the open-path benchmark.
    pub fn bytes(&self) -> usize {
        self.blocks.capacity() * std::mem::size_of::<CodeScanState>()
    }
}

/// Consume lines `from..to`, reporting the state after each checkpoint line.
///
/// Walks the text with `memchr` instead of building a `Vec<&str>`: this runs on
/// the window-build path, so it must not allocate. Returns the number of lines
/// actually consumed, which is `to - from` unless `to` is past the end of the
/// text.
fn consume(
    scanner: &mut CodeScanner,
    content: &str,
    line_index: &LineIndex,
    from: usize,
    to: usize,
    mut at_checkpoint: impl FnMut(usize, CodeScanState),
) -> usize {
    if to <= from {
        return 0;
    }
    let lines_total = line_index.lines();
    let bytes = content.as_bytes();
    let begin = line_index.start(content, from).min(bytes.len());
    let end = line_index.start(content, to).min(bytes.len());

    let mut cursor = begin;
    let mut line = from;
    let mut newlines = memchr::memchr_iter(b'\n', &bytes[begin..end]);
    while line < to {
        let line_end = match newlines.next() {
            Some(nl) => begin + nl,
            // No more newlines: either the slice stops on a line boundary
            // (nothing left to consume here) or this is the document's last,
            // unterminated line, which still counts as a line.
            None if to < lines_total => break,
            None => bytes.len(),
        };
        scanner.step(&content[cursor..line_end]);
        cursor = line_end + 1;
        line += 1;
        at_checkpoint(line, scanner.state());
    }
    line - from
}

fn consume_slice(
    scanner: &mut CodeScanner,
    slice: &str,
    from: usize,
    to: usize,
    mut at_checkpoint: impl FnMut(usize, CodeScanState),
) -> usize {
    if to <= from || slice.is_empty() {
        return 0;
    }
    let mut line = from;
    let mut cursor = 0;
    for (nl_idx, _) in slice.match_indices('\n') {
        let line_text = &slice[cursor..nl_idx];
        scanner.step(line_text);
        cursor = nl_idx + 1;
        line += 1;
        at_checkpoint(line, scanner.state());
        if line >= to {
            break;
        }
    }
    if line < to && cursor < slice.len() {
        scanner.step(&slice[cursor..]);
        line += 1;
        at_checkpoint(line, scanner.state());
    }
    line - from
}

#[cfg(test)]
mod tests;
