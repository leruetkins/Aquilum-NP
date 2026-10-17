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
mod tests {
    use super::{LineIndex, LINE_BLOCK};

    /// Reference implementation: the thing the index must agree with.
    fn naive_lines(text: &str) -> Vec<usize> {
        let mut starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    fn check(text: &str) {
        let index = LineIndex::new(text);
        let naive = naive_lines(text);
        assert_eq!(index.lines(), naive.len(), "line count for {:?}", &text[..text.len().min(40)]);
        assert_eq!(index.len(), text.len());
        for (line, &want) in naive.iter().enumerate() {
            assert_eq!(index.start(text, line), want, "start of line {line}");
        }
        // Past the last line the start is the text length.
        assert_eq!(index.start(text, naive.len()), text.len());
        // Every byte maps to the line whose range contains it.
        for byte in 0..=text.len() {
            let line = index.line_of(text, byte);
            assert!(line < naive.len());
            assert!(naive[line] <= byte, "byte {byte} reported as line {line}");
            if line + 1 < naive.len() {
                assert!(byte < naive[line + 1], "byte {byte} belongs to line {}", line + 1);
            }
        }
    }

    #[test]
    fn matches_a_naive_scan_on_edge_cases() {
        for text in [
            "",
            "a",
            "\n",
            "a\n",
            "\na",
            "a\nb",
            "a\nb\n",
            "a\n\n\nb",
            "строка\r\nвторая", // CR is content here, only '\n' splits
            "без перевода строки",
        ] {
            check(text);
        }
    }

    #[test]
    fn matches_a_naive_scan_across_checkpoints() {
        // Enough lines to cross several `LINE_BLOCK` boundaries, with lines of
        // varying length so a checkpoint cannot accidentally line up.
        let mut text = String::new();
        for i in 0..(LINE_BLOCK * 3 + 7) {
            text.push_str(&"x".repeat(i % 17));
            text.push('\n');
        }
        check(&text);
    }

    #[test]
    fn lookup_work_is_bounded_by_one_block() {
        // A huge document must not make a lookup scan the whole text. Build the
        // lines lazily: 200k lines of 8 bytes is only 1.6 MB, but the point is
        // that the checkpoint scan is bounded, not the document size.
        let mut text = String::with_capacity(2 * 1024 * 1024);
        for i in 0..200_000 {
            text.push_str(&format!("{i:07}\n"));
        }
        let index = LineIndex::new(&text);
        assert_eq!(index.lines(), 200_001);
        // The last line, i.e. the worst case for the forward scan.
        assert_eq!(index.start(&text, 200_000), text.len());
        assert_eq!(index.line_of(&text, text.len()), 200_000);
        // A middle line, checked against the naive answer.
        let mid = 137_000;
        let want = text
            .match_indices('\n')
            .nth(mid - 1)
            .map(|(i, _)| i + 1)
            .unwrap();
        assert_eq!(index.start(&text, mid), want);
        // The index is tiny: a handful of bytes per block, not per line.
        assert!(
            index.bytes() < text.len() / 8,
            "index {} bytes for {} bytes of text",
            index.bytes(),
            text.len()
        );
    }

    /// Readiness criterion of stage 2: five million lines are addressed exactly
    /// and in bounded work. The lines have a fixed length here, so the expected
    /// offset is a multiplication — an independent check instead of a second
    /// copy of the same scan.
    #[test]
    fn five_million_lines_are_addressed_exactly_and_cheaply() {
        const LINE: &str = "0123456\n"; // 8 bytes, so line `l` starts at `l * 8`.
        const COUNT: usize = 5_000_000;
        let mut text = String::with_capacity(COUNT * LINE.len());
        for _ in 0..COUNT {
            text.push_str(LINE);
        }
        let index = LineIndex::new(&text);

        assert_eq!(index.lines(), COUNT + 1);
        assert_eq!(index.len(), text.len());
        for line in [
            0,
            1,
            LINE_BLOCK - 1,
            LINE_BLOCK,
            LINE_BLOCK + 1,
            COUNT / 2,
            COUNT - 1,
            COUNT,
        ] {
            assert_eq!(
                index.start(&text, line),
                line * LINE.len(),
                "start of line {line}"
            );
        }
        for byte in [
            0,
            7,
            8,
            LINE_BLOCK * LINE.len(),
            text.len() - 1,
            text.len(),
        ] {
            assert_eq!(index.line_of(&text, byte), byte / LINE.len(), "byte {byte}");
        }
        // Checkpoints only: tens of kilobytes for a 40 MB document.
        assert!(
            index.bytes() < 256 * 1024,
            "index is {} bytes for {} bytes of text",
            index.bytes(),
            text.len()
        );
    }

    #[test]
    fn splice_matches_new_for_all_edit_types() {
        let mut text = String::new();
        for i in 0..(LINE_BLOCK * 3 + 50) {
            text.push_str(&format!("line_{i:04}\n"));
        }
        let mut spliced = LineIndex::new(&text);

        // 1. Single character insertion without newline (in block 0)
        let edit_byte = 10;
        let inserted = "X";
        let removed = "";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, removed, inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "insert without newline block 0");

        // 2. Character deletion without newline (in block 2, across checkpoints)
        let edit_byte = LINE_BLOCK * 10 + 20;
        let removed = text[edit_byte..edit_byte + 3].to_string();
        text.replace_range(edit_byte..edit_byte + 3, "");
        spliced.splice(&text, edit_byte, &removed, "");
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "delete without newline block 2");

        // 3. Newline insertion (pressing Enter)
        let edit_byte = LINE_BLOCK * 2 + 15;
        let inserted = "\n";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, "", inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "insert newline");

        // 4. Multiline paste
        let edit_byte = 50;
        let inserted = "first\nsecond\nthird\n";
        text.insert_str(edit_byte, inserted);
        spliced.splice(&text, edit_byte, "", inserted);
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "multiline paste");

        // 5. Multiline deletion (deleting across lines)
        let edit_byte = 40;
        let removed = text[edit_byte..edit_byte + 100].to_string();
        text.replace_range(edit_byte..edit_byte + 100, "");
        spliced.splice(&text, edit_byte, &removed, "");
        let expected = LineIndex::new(&text);
        assert_eq!(spliced, expected, "multiline delete");
    }
}
