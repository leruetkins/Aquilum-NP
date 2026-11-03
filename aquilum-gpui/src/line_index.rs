//! Line addressing for documents of any size, in O(1) memory per `LINE_BLOCK` lines.
//!
//! The editor used to keep a `Vec<String>` with one heap `String` per line plus a
//! `Vec<usize>` with one start offset per line. Both are O(lines): for an 8M-line
//! note that is 8M allocations and 66 MB of offsets before a single row is drawn,
//! and the whole thing is rebuilt on every edit. See
//! `TODO/perf-plan-large-file-open.md`.
//!
//! This index stores only every `LINE_BLOCK`-th line start. An exact start is then
//! found by scanning at most `LINE_BLOCK` lines forward from the nearest checkpoint
//! with `memchr`, which is a few microseconds. Memory for the 8M-line example:
//! `8M / 1024 * 8` bytes ≈ 62 KB instead of 66 MB.
//!
//! The index is purely positional: it holds no reference to the text, so every
//! lookup takes the text it describes. That keeps it cheap to store next to the
//! text and impossible to get out of sync with a borrowed copy.

/// Lines between two stored checkpoints. Trade-off: larger means less memory and a
/// longer local scan. 1024 keeps the scan well under the cost of a shape call.
pub const LINE_BLOCK: usize = 1024;

/// Byte offsets of line starts, every [`LINE_BLOCK`] lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineIndex {
    /// `starts[b]` is the byte offset of line `b * LINE_BLOCK`.
    starts: Vec<usize>,
    /// Byte length of the text the index was built from.
    len: usize,
    /// Total number of lines (`'\n'` count + 1, so an empty text is one line).
    lines: usize,
}

impl LineIndex {
    /// Empty index for uninitialized buffers.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build the index in one `memchr` pass over `text`.
    pub fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut starts = Vec::with_capacity(bytes.len() / (LINE_BLOCK * 16) + 1);
        starts.push(0);
        let mut line = 0usize;
        for at in memchr::memchr_iter(b'\n', bytes) {
            line += 1;
            if line % LINE_BLOCK == 0 {
                starts.push(at + 1);
            }
        }
        Self {
            starts,
            len: bytes.len(),
            lines: line + 1,
        }
    }

    /// Build line index directly from a `crop::Rope` in O(lines / LINE_BLOCK) time.
    pub fn new_from_rope(rope: &crop::Rope) -> Self {
        let len = rope.byte_len();
        let lines = rope.line_len() + usize::from(len == 0 || rope.byte(len - 1) == b'\n');
        let mut starts = Vec::with_capacity(lines / LINE_BLOCK + 1);
        for block in 0..=(lines / LINE_BLOCK) {
            let line = block * LINE_BLOCK;
            if line < lines {
                starts.push(rope.byte_of_line(line));
            }
        }
        if starts.is_empty() {
            starts.push(0);
        }
        Self {
            starts,
            len,
            lines,
        }
    }

    /// Update line index from `rope` after an edit at `edit_byte`.
    pub fn splice_rope(&mut self, rope: &crop::Rope, edit_byte: usize) {
        let len = rope.byte_len();
        let lines = rope.line_len() + usize::from(len == 0 || rope.byte(len - 1) == b'\n');
        self.len = len;
        self.lines = lines;

        let checkpoint_idx = self.starts.partition_point(|&s| s <= edit_byte).saturating_sub(1);
        self.starts.truncate(checkpoint_idx + 1);
        for block in (checkpoint_idx + 1)..=(lines / LINE_BLOCK) {
            let line = block * LINE_BLOCK;
            if line < lines {
                self.starts.push(rope.byte_of_line(line));
            }
        }
    }

    /// Number of lines. An empty text has one (empty) line, matching
    /// `str::split('\n')`.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// Byte length of the indexed text.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Byte offset of the start of `line`. Past the last line it returns the
    /// text length, which is where the trailing empty line starts.
    pub fn start(&self, text: &str, line: usize) -> usize {
        if line >= self.lines {
            return self.len;
        }
        let block = line / LINE_BLOCK;
        let base = self.starts[block];
        let target = line - block * LINE_BLOCK;
        if target == 0 {
            return base;
        }
        let bytes = text.as_bytes();
        let tail = &bytes[base.min(bytes.len())..];
        let mut seen = 0usize;
        for nl in memchr::memchr_iter(b'\n', tail) {
            seen += 1;
            if seen == target {
                return base + nl + 1;
            }
        }
        self.len
    }

    /// Byte offset just past `line` (i.e. including its newline), or the text
    /// length for the last line.
    pub fn end(&self, text: &str, line: usize) -> usize {
        if line + 1 >= self.lines {
            self.len
        } else {
            self.start(text, line + 1).saturating_sub(1).max(self.start(text, line))
        }
    }

    /// Index of the line containing `byte`, clamped to the last line.
    pub fn line_of(&self, text: &str, byte: usize) -> usize {
        let byte = byte.min(self.len);
        let block = self
            .starts
            .partition_point(|&start| start <= byte)
            .saturating_sub(1);
        let base = self.starts[block];
        let bytes = text.as_bytes();
        let from = base.min(bytes.len());
        let to = byte.max(from);
        let extra = memchr::memchr_iter(b'\n', &bytes[from..to]).count();
        (block * LINE_BLOCK + extra).min(self.lines.saturating_sub(1))
    }

    /// Bytes held by the index. Reported by the open-path benchmark.
    pub fn bytes(&self) -> usize {
        self.starts.capacity() * std::mem::size_of::<usize>()
    }

    /// Incrementally update the line index after an edit at `edit_byte`.
    ///
    /// If the edit does not add or remove newlines, no scanning of `text` is
    /// required: checkpoint offsets after `edit_byte` are simply shifted by `delta`.
    /// If newlines were added or removed, scanning restarts from the last valid checkpoint
    /// strictly before or at `edit_byte`, keeping all prior checkpoints intact.
    pub fn splice(&mut self, text: &str, edit_byte: usize, removed: &str, inserted: &str) {
        let delta = inserted.len() as isize - removed.len() as isize;
        let removed_has_nl = removed.contains('\n');
        let inserted_has_nl = inserted.contains('\n');

        if !removed_has_nl && !inserted_has_nl {
            self.len = (self.len as isize + delta).max(0) as usize;
            let first_shifted = self.starts.partition_point(|&s| s <= edit_byte);
            for s in &mut self.starts[first_shifted..] {
                *s = (*s as isize + delta).max(0) as usize;
            }
            return;
        }

        let checkpoint_idx = self.starts.partition_point(|&s| s <= edit_byte).saturating_sub(1);
        let base_byte = self.starts[checkpoint_idx];
        let mut line = checkpoint_idx * LINE_BLOCK;

        self.starts.truncate(checkpoint_idx + 1);

        let bytes = text.as_bytes();
        let tail = if base_byte < bytes.len() { &bytes[base_byte..] } else { &[] };
        for at in memchr::memchr_iter(b'\n', tail) {
            line += 1;
            if line % LINE_BLOCK == 0 {
                self.starts.push(base_byte + at + 1);
            }
        }
        self.lines = line + 1;
        self.len = bytes.len();
    }
}

#[cfg(test)]
mod tests;
