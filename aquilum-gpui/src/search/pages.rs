//! Line-boundary paging of file bodies.
//!
//! Modeled on the reference `core/src/search/pages` (idea only): a file is
//! streamed and cut into pages at line ends — a page closes once it holds at
//! least [`MIN_PAGE`] bytes, and a runaway line is clamped at [`MAX_LINE`].
//! Index building and snippet location share this splitter, so both see the
//! same page text and a term found in the index can be located by streaming
//! the file again — the whole file gets indexed without ever holding it in
//! memory.

use std::io::{self, Read};
use std::path::Path;

/// A page closes at a line end once it holds at least this many bytes.
pub(crate) const MIN_PAGE: usize = 256 * 1024;
/// A single line longer than this is clamped at a char boundary (the
/// reference clamps runaway lines the same way). Bounds the page size:
/// pages stay under `MIN_PAGE + MAX_LINE`.
pub(crate) const MAX_LINE: usize = 1024 * 1024;
/// Read block size.
const BLOCK: usize = 256 * 1024;

/// `str::is_char_boundary` over the pending buffer (the cut may point one
/// past the filled bytes — still a valid boundary for `take`).
fn is_char_boundary(buf: &[u8], at: usize) -> bool {
    at == 0 || at >= buf.len() || (buf[at] as i8) >= -0x40
}

/// Streams a reader as pages (see module docs).
pub(crate) struct PageStream<R: Read> {
    reader: R,
    buf: Vec<u8>,
    /// Start of the page being accumulated.
    start: usize,
    /// Search cursor inside the current page (past already-checked lines).
    probe: usize,
    filled: usize,
    ended: bool,
}

impl PageStream<io::BufReader<std::fs::File>> {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        Ok(Self::new(io::BufReader::with_capacity(
            BLOCK,
            std::fs::File::open(path)?,
        )))
    }
}

impl<R: Read> PageStream<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            reader,
            buf: Vec::new(),
            start: 0,
            probe: 0,
            filled: 0,
            ended: false,
        }
    }

    /// Next page: a line-bounded chunk of at least [`MIN_PAGE`] bytes (the
    /// tail page may be shorter). `None` — the stream is exhausted.
    pub(crate) fn next_page(&mut self) -> io::Result<Option<String>> {
        if self.ended && self.start >= self.filled {
            return Ok(None);
        }
        loop {
            if let Some(at) = memchr::memchr(b'\n', &self.buf[self.probe..self.filled]) {
                let end = self.probe + at + 1;
                if end - self.start >= MIN_PAGE {
                    return Ok(Some(self.take(end)));
                }
                self.probe = end;
            } else if self.filled - self.start >= MAX_LINE {
                // Runaway line: clamp the page at a char boundary.
                let mut cut = self.start + MAX_LINE;
                while cut > self.start
                    && (cut >= self.buf.len() || !is_char_boundary(&self.buf, cut))
                {
                    cut -= 1;
                }
                return Ok(Some(self.take(cut)));
            } else if self.ended {
                if self.filled == self.start {
                    return Ok(None);
                }
                return Ok(Some(self.take(self.filled)));
            } else {
                self.fill()?;
            }
        }
    }

    fn take(&mut self, end: usize) -> String {
        let text = String::from_utf8_lossy(&self.buf[self.start..end]).into_owned();
        self.buf.drain(..end);
        self.filled -= end;
        self.start = 0;
        self.probe = 0;
        text
    }

    fn fill(&mut self) -> io::Result<()> {
        let at = self.buf.len();
        self.buf.resize(at + BLOCK, 0);
        let read = self.reader.read(&mut self.buf[at..])?;
        self.buf.truncate(at + read);
        self.filled = self.buf.len();
        self.ended = read == 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pages(data: &[u8]) -> Vec<String> {
        let mut stream = PageStream::new(Cursor::new(data.to_vec()));
        let mut out = Vec::new();
        while let Some(page) = stream.next_page().unwrap() {
            out.push(page);
        }
        out
    }

    #[test]
    fn small_file_is_one_page() {
        let got = pages(b"alpha\nbeta\n");
        assert_eq!(got, vec!["alpha\nbeta\n".to_string()]);
    }

    #[test]
    fn empty_input_yields_no_pages() {
        assert!(pages(b"").is_empty());
    }

    #[test]
    fn big_file_splits_at_line_ends_and_preserves_content() {
        let line = "filler words here\n";
        let mut data = String::from("first line\n");
        while data.len() < 3 * MIN_PAGE {
            data.push_str(line);
        }
        data.push_str("last line\n");
        let got = pages(data.as_bytes());
        assert!(got.len() >= 3, "expected several pages, got {}", got.len());
        // Every page but the last ends at a line boundary; content preserved.
        let joined: String = got.concat();
        assert_eq!(joined, data);
        for page in &got[..got.len() - 1] {
            assert!(page.len() >= MIN_PAGE, "page too small: {}", page.len());
            assert!(page.ends_with('\n'));
        }
    }

    #[test]
    fn runaway_line_is_clamped_at_char_boundary() {
        // 2-byte chars: a byte-boundary cut would produce a replacement char.
        let data = "ж".repeat(3 * MAX_LINE);
        let got = pages(data.as_bytes());
        assert!(got.len() >= 2);
        for page in &got {
            assert!(page.len() <= MAX_LINE, "page not clamped: {}", page.len());
            assert!(!page.contains('\u{FFFD}'), "cut inside a char");
        }
        assert_eq!(got.concat(), data);
    }
}
