//! Allocation-free case folding matcher.
//!
//! Replaces whole-string `to_lowercase()` scans in the search path: every
//! character of the text is folded on the fly and paired with the byte range
//! of the ORIGINAL character it came from, so matches, snippets and highlight
//! ranges never need to map indices back through a folded copy (whose byte
//! length can differ from the original under exotic casings, e.g.
//! `İ` → `i` + combining dot).
//!
//! Modeled on the reference `core/src/text/matcher/fold.rs` (ideas only, no
//! code): a fold table for the first 0x800 code points built once into a
//! `OnceLock`, `char::to_lowercase()` for everything above it. Unlike the
//! reference the table is built with the standard lowercase mapping, not
//! tantivy's `AsciiFoldingFilter` — diacritic folding is out of scope here.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::OnceLock;

/// Case folding: `Exact` compares characters as-is (the `Aa` toggle), `Lower`
/// folds both sides with the Unicode lowercase mapping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Exact,
    Lower,
}

/// Match anchoring: anywhere in the text, at a word start, or as a whole word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Substring,
    WordPrefix,
    WholeWord,
}

/// Fold table covers the BMP range where most case-paired scripts live
/// (Latin, Cyrillic, Greek).
const TABLE_CP: usize = 0x800;
/// Widest lowercase expansion of a single char (`İ` → `i` + combining dot).
/// The real Unicode maximum for `char::to_lowercase` is 3; the buffer has
/// headroom and writes are guarded anyway.
const MAX_EXPANSION: usize = 8;

/// Placeholder for a table slot whose lowercase mapping is longer than one
/// char; the expansion is computed on demand. A NUL folds to itself and would
/// hit this marker too, which only costs it the slow path.
const EXPANDS: char = '\0';

fn fold_table() -> &'static [char] {
    static TABLE: OnceLock<Vec<char>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..TABLE_CP)
            .map(|cp| {
                let c = char::from_u32(cp as u32).expect("code points below 0x800 are scalars");
                let mut lower = c.to_lowercase();
                match (lower.next(), lower.next()) {
                    (Some(one), None) => one,
                    _ => EXPANDS,
                }
            })
            .collect()
    })
}

/// Lowercase mapping of `c` when it is exactly one char.
///
/// This is the per-character fast path of the matcher: ASCII and the fold
/// table answer with a single load instead of walking `to_lowercase()`.
fn lower_single(c: char) -> Option<char> {
    if c.is_ascii() {
        return Some(c.to_ascii_lowercase());
    }
    let cached = *fold_table().get(c as usize)?;
    (cached != EXPANDS).then_some(cached)
}

/// Folds one char into a single char when possible (`Exact` always can).
fn fold_single(c: char, fold: Fold) -> Option<char> {
    match fold {
        Fold::Exact => Some(c),
        Fold::Lower => lower_single(c),
    }
}

/// Folds one char into at most [`MAX_EXPANSION`] chars. Returns the filled
/// prefix length of `out`.
fn expand(c: char, fold: Fold, out: &mut [char; MAX_EXPANSION]) -> usize {
    if let Some(one) = fold_single(c, fold) {
        out[0] = one;
        return 1;
    }
    let mut n = 0;
    for fc in c.to_lowercase() {
        if n < MAX_EXPANSION {
            out[n] = fc;
            n += 1;
        }
    }
    n
}

/// One folded character of the text with its provenance in the original.
#[derive(Clone, Copy)]
struct Item {
    ch: char,
    /// Byte range of the original character this folded char came from.
    start: usize,
    end: usize,
    /// A word can start at this folded char: it opens its original char and
    /// the previous original char (if any) is not alphanumeric.
    word_start: bool,
    /// A word can end after this folded char: it closes its original char
    /// and the next original char (if any) is not alphanumeric.
    word_end: bool,
}

/// Streams the folded form of `text`, char by char, without allocating.
#[derive(Clone)]
struct Cursor<'a> {
    text: &'a str,
    /// Cached fold table: reaching it through the `OnceLock` costs an atomic
    /// load plus a branch on every character of the document.
    table: &'static [char],
    byte: usize,
    fold: Fold,
    /// Original char before the current one (`None` at the text start).
    prev: Option<char>,
    /// Whether a word can start at the current original char.
    cur_word_start: bool,
    /// Lowercase expansion of the current original char.
    exp: [char; MAX_EXPANSION],
    exp_len: usize,
    exp_ix: usize,
    /// Byte range of the current original char.
    start: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str, fold: Fold) -> Self {
        Self {
            text,
            table: fold_table(),
            byte: 0,
            fold,
            prev: None,
            cur_word_start: true,
            exp: ['\0'; MAX_EXPANSION],
            exp_len: 0,
            exp_ix: 0,
            start: 0,
            end: 0,
        }
    }

    fn next(&mut self) -> Option<Item> {
        if self.exp_ix >= self.exp_len {
            if self.byte >= self.text.len() {
                return None;
            }
            let c = self.text[self.byte..].chars().next().unwrap();
            let end = self.byte + c.len_utf8();
            let word_start = self.prev.map_or(true, |p| !p.is_alphanumeric());
            self.prev = Some(c);
            self.start = self.byte;
            self.end = end;
            self.byte = end;
            self.cur_word_start = word_start;
            self.exp_len = expand(c, self.fold, &mut self.exp);
            self.exp_ix = 0;
        }
        let ix = self.exp_ix;
        self.exp_ix += 1;
        Some(Item {
            ch: self.exp[ix],
            start: self.start,
            end: self.end,
            word_start: ix == 0 && self.cur_word_start,
            word_end: ix + 1 == self.exp_len
                && self.text[self.end..]
                    .chars()
                    .next()
                    .map_or(true, |c| !c.is_alphanumeric()),
        })
    }

    /// Folds one char to a single char, or `None` when it expands to several.
    /// Same mapping as [`fold_single`], with the table already at hand.
    #[inline]
    fn fold_char(&self, c: char) -> Option<char> {
        match self.fold {
            Fold::Exact => Some(c),
            Fold::Lower => {
                if c.is_ascii() {
                    Some(c.to_ascii_lowercase())
                } else {
                    let cached = *self.table.get(c as usize)?;
                    (cached != EXPANDS).then_some(cached)
                }
            }
        }
    }

    /// Next folded char with its byte range and no word-boundary bookkeeping.
    ///
    /// Word flags cost two Unicode case lookups and an extra decode per
    /// character; substring search never reads them, so it walks the text
    /// with this instead of [`Cursor::next`].
    fn next_flat(&mut self) -> Option<(char, usize, usize)> {
        if self.exp_ix < self.exp_len {
            let ix = self.exp_ix;
            self.exp_ix += 1;
            return Some((self.exp[ix], self.start, self.end));
        }
        let c = self.text[self.byte..].chars().next()?;
        let start = self.byte;
        let end = start + c.len_utf8();
        self.start = start;
        self.end = end;
        self.byte = end;
        if let Some(folded) = self.fold_char(c) {
            self.exp_len = 0;
            self.exp_ix = 0;
            return Some((folded, start, end));
        }
        self.exp_len = expand(c, self.fold, &mut self.exp);
        self.exp_ix = 1;
        Some((self.exp[0], start, end))
    }
}

/// Folds `text` lazily, pairing every folded char with the byte range of the
/// original character it came from.
pub struct Folded<'a> {
    cursor: Cursor<'a>,
}

impl Iterator for Folded<'_> {
    type Item = (char, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        self.cursor
            .next()
            .map(|item| (item.ch, item.start..item.end))
    }
}

/// Starts a [`Folded`] stream over `text`.
pub fn folded(text: &str, fold: Fold) -> Folded<'_> {
    Folded {
        cursor: Cursor::new(text, fold),
    }
}

/// Folds a whole string into its folded chars (no range tracking). Used for
/// short strings — dedup keys, needles, sort keys.
pub fn fold_str(s: &str, fold: Fold) -> String {
    if fold == Fold::Exact {
        return s.to_owned();
    }
    folded(s, fold).map(|(c, _)| c).collect()
}

/// Folds a needle into the flat char sequence a [`Cursor`] emits: expansions
/// are inlined, so `İ` becomes `i` + combining dot and matches itself.
fn fold_needle(needle: &str, fold: Fold) -> Vec<char> {
    let mut out = Vec::with_capacity(needle.len());
    let mut buf = ['\0'; MAX_EXPANSION];
    for c in needle.chars() {
        let n = expand(c, fold, &mut buf);
        out.extend_from_slice(&buf[..n]);
    }
    out
}

/// Substring scan that probes each needle by cloning the cursor.
///
/// The windowed scan below buffers the longest needle horizon for every
/// character; here the common case is one comparison against the first needle
/// char, and the cursor clone is paid only where a match can actually start.
/// That is what keeps a plain word search on a large file responsive.
fn scan_substring(
    hay: &str,
    needles: &[Vec<char>],
    fold: Fold,
    mut on_match: impl FnMut(Range<usize>) -> bool,
) {
    let mut cursor = Cursor::new(hay, fold);
    while let Some((first, start, end)) = cursor.next_flat() {
        let mut hit_end = None;
        for needle in needles {
            let Some(&needle_first) = needle.first() else {
                continue;
            };
            if needle_first != first {
                continue;
            }
            if needle.len() == 1 {
                hit_end = Some(end);
                break;
            }
            let mut probe = cursor.clone();
            let mut probe_end = end;
            let mut matched = true;
            for &want in &needle[1..] {
                match probe.next_flat() {
                    Some((ch, _, e)) if ch == want => probe_end = e,
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }
            if matched {
                hit_end = Some(probe_end);
                cursor = probe;
                break;
            }
        }
        if let Some(end) = hit_end {
            if !on_match(start..end) {
                return;
            }
        }
    }
}

/// Non-overlapping matches of `needles` in `hay`, longest-first at each start
/// position. `needles` must be pre-folded and sorted by folded length
/// descending. Calls `on_match` with byte ranges into the original; stops
/// when it returns `false`.
fn scan_matches(
    hay: &str,
    needles: &[Vec<char>],
    fold: Fold,
    mode: Mode,
    mut on_match: impl FnMut(Range<usize>) -> bool,
) {
    if needles.is_empty() {
        return;
    }
    let max_len = needles.iter().map(|n| n.len()).max().unwrap_or(0);
    if max_len == 0 {
        return;
    }
    if mode == Mode::Substring {
        scan_substring(hay, needles, fold, on_match);
        return;
    }
    let mut cursor = Cursor::new(hay, fold);
    // Items from the current scan start up to the longest needle horizon.
    let mut window: VecDeque<Item> = VecDeque::with_capacity(max_len + 1);
    loop {
        while window.len() < max_len {
            match cursor.next() {
                Some(item) => window.push_back(item),
                None => break,
            }
        }
        if window.is_empty() {
            return;
        }
        let mut matched_len = 0;
        for needle in needles {
            let n = needle.len();
            if n == 0 || n > window.len() {
                continue;
            }
            let first = window[0];
            if matches!(mode, Mode::WordPrefix | Mode::WholeWord) && !first.word_start {
                continue;
            }
            if needle.iter().zip(window.iter()).all(|(&nc, it)| it.ch == nc) {
                if mode == Mode::WholeWord && !window[n - 1].word_end {
                    continue;
                }
                matched_len = n;
                break;
            }
        }
        if matched_len > 0 {
            let start = window[0].start;
            let end = window[matched_len - 1].end;
            if !on_match(start..end) {
                return;
            }
            for _ in 0..matched_len {
                window.pop_front();
            }
        } else {
            window.pop_front();
        }
    }
}

/// First match of `needle` in `hay`, as a byte range into the original.
/// `None` when there is no match (an empty needle never matches).
pub fn find(hay: &str, needle: &str, fold: Fold, mode: Mode) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    let needle = fold_needle(needle, fold);
    let mut hit = None;
    scan_matches(hay, std::slice::from_ref(&needle), fold, mode, |r| {
        hit = Some(r);
        false
    });
    hit
}

/// Whether `needle` occurs in `hay`. An empty needle matches as a substring
/// (like `str::contains`); word modes never match an empty needle.
pub fn contains(hay: &str, needle: &str, fold: Fold, mode: Mode) -> bool {
    if needle.is_empty() {
        return mode == Mode::Substring;
    }
    let needle = fold_needle(needle, fold);
    let mut hit = false;
    scan_matches(hay, std::slice::from_ref(&needle), fold, mode, |_| {
        hit = true;
        false
    });
    hit
}

/// Non-overlapping occurrence count of `needle` in `hay` (0 for an empty
/// needle).
pub fn count(hay: &str, needle: &str, fold: Fold) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let needle = fold_needle(needle, fold);
    let mut n = 0;
    scan_matches(hay, std::slice::from_ref(&needle), fold, Mode::Substring, |_| {
        n += 1;
        true
    });
    n
}

/// Non-overlapping byte ranges in the original for every occurrence of any
/// needle, longest needle winning at each position, capped at `limit`.
pub fn ranges(hay: &str, needles: &[&str], fold: Fold, limit: usize) -> Vec<Range<usize>> {
    ranges_mode(hay, needles, fold, Mode::Substring, limit)
}

/// [`ranges`] with explicit anchoring: `Mode::WholeWord` reports only
/// occurrences that start and end on word boundaries.
pub fn ranges_mode(
    hay: &str,
    needles: &[&str],
    fold: Fold,
    mode: Mode,
    limit: usize,
) -> Vec<Range<usize>> {
    let mut folded: Vec<Vec<char>> = needles
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| fold_needle(s, fold))
        .collect();
    folded.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut out = Vec::new();
    if limit > 0 {
        scan_matches(hay, &folded, fold, mode, |r| {
            out.push(r);
            out.len() < limit
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_lower(hay: &str, needle: &str) -> Option<Range<usize>> {
        find(hay, needle, Fold::Lower, Mode::Substring)
    }

    #[test]
    fn folds_case_for_latin_and_cyrillic() {
        assert!(contains("Hello World", "world", Fold::Lower, Mode::Substring));
        assert!(!contains("Hello World", "world", Fold::Exact, Mode::Substring));
        assert!(contains("Привет Мир", "мир", Fold::Lower, Mode::Substring));
        assert!(contains("Привет Мир", "ПРИВЕТ", Fold::Lower, Mode::Substring));
        assert!(!contains("Привет Мир", "ПРИВЕТ", Fold::Exact, Mode::Substring));
        // Cyrillic e and yo stay distinct, exactly like `to_lowercase` today.
        assert!(!contains("Ель", "ёль", Fold::Lower, Mode::Substring));
    }

    #[test]
    fn expansion_maps_to_one_original_char() {
        // `İ` folds to `i` + combining dot (two folded chars, one original).
        let r = find_lower("İstanbul", "i\u{0307}").expect("İ must match i-dot");
        assert_eq!(r, 0..2);
        // A plain `i` does NOT match into `İ`: the combining dot stays in the
        // folded stream, exactly like `"i̇stanbul".contains("ist")` is false
        // with the old whole-string `to_lowercase` scan. Parity, not a bug.
        assert_eq!(find_lower("İstanbul", "ist"), None);
        // The range always points at original bytes, not folded ones.
        assert_eq!(&"İstanbul"[r], "İ");
    }

    #[test]
    fn ranges_respect_char_boundaries() {
        // Emoji (4 bytes) must never be cut in the middle.
        let hay = "a😀b 😀c";
        let rs = ranges(hay, &["b", "c"], Fold::Lower, usize::MAX);
        for r in &rs {
            assert!(hay.is_char_boundary(r.start) && hay.is_char_boundary(r.end));
        }
        assert_eq!(rs.len(), 2);
    }

    #[test]
    fn word_prefix_needs_a_word_start() {
        assert!(contains("hello world", "wor", Fold::Lower, Mode::WordPrefix));
        assert!(!contains("hello world", "orld", Fold::Lower, Mode::WordPrefix));
        assert!(contains("hello-world", "wor", Fold::Lower, Mode::WordPrefix));
        assert!(contains("World", "wor", Fold::Lower, Mode::WordPrefix));
    }

    #[test]
    fn whole_word_needs_both_boundaries() {
        assert!(contains("a cat scattered", "cat", Fold::Lower, Mode::Substring));
        assert!(contains("a cat scattered", "cat", Fold::Lower, Mode::WordPrefix));
        assert!(contains("a cat scattered", "cat", Fold::Lower, Mode::WholeWord));
        assert!(contains("a cat scattered", "scat", Fold::Lower, Mode::WordPrefix));
        // Mid-word starts and ends do not count for word modes.
        assert!(!contains("a cat scattered", "at", Fold::Lower, Mode::WordPrefix));
        assert!(!contains("a cat scattered", "at", Fold::Lower, Mode::WholeWord));
        assert!(!contains("a cat scattered", "atte", Fold::Lower, Mode::WholeWord));
        assert!(!contains("a cat scattered", "scat", Fold::Lower, Mode::WholeWord));
    }

    #[test]
    fn ranges_mode_applies_case_and_word_rules() {
        let hay = "Cat cat CAT";
        assert_eq!(ranges(hay, &["cat"], Fold::Lower, usize::MAX).len(), 3);
        assert_eq!(
            ranges_mode(hay, &["cat"], Fold::Exact, Mode::Substring, usize::MAX).len(),
            1,
            "only the lowercase word survives a case-sensitive scan"
        );
        let words = ranges_mode(hay, &["cat"], Fold::Lower, Mode::WholeWord, usize::MAX);
        assert_eq!(words, vec![0..3, 4..7, 8..11]);
        let exact_words = ranges_mode(hay, &["cat"], Fold::Exact, Mode::WholeWord, usize::MAX);
        assert_eq!(exact_words, vec![4..7]);

        // Word-start mode drops a hit that begins inside another word, and
        // whole-word mode additionally demands the end boundary.
        let hay = "cat scattered cat";
        assert_eq!(
            ranges_mode(hay, &["cat"], Fold::Lower, Mode::WordPrefix, usize::MAX),
            vec![0..3, 14..17]
        );
        assert_eq!(
            ranges_mode(hay, &["sca"], Fold::Lower, Mode::WordPrefix, usize::MAX),
            vec![4..7]
        );
        assert_eq!(
            ranges_mode(hay, &["cat"], Fold::Lower, Mode::WholeWord, usize::MAX),
            vec![0..3, 14..17]
        );

        // A limit still truncates the word-mode list.
        assert_eq!(
            ranges_mode(hay, &["cat"], Fold::Lower, Mode::WholeWord, 1).len(),
            1
        );
    }

    #[test]
    fn ranges_longest_wins_and_non_overlapping() {
        let rs = ranges("abcd", &["ab", "abcd"], Fold::Lower, usize::MAX);
        assert_eq!(rs, vec![0..4]);
        let rs = ranges("abcabc", &["bc", "abc"], Fold::Lower, usize::MAX);
        assert_eq!(rs, vec![0..3, 3..6]);
    }

    #[test]
    fn ranges_match_highlight_reference_semantics() {
        // Same scenario as goto_search's highlight test: 2 hits, Cyrillic.
        let rs = ranges("Приватизация ПРИ", &["при"], Fold::Lower, usize::MAX);
        assert_eq!(rs.len(), 2);
        assert_eq!(&"Приватизация ПРИ"[rs[0].clone()], "При");
    }

    #[test]
    fn counts_non_overlapping() {
        // Same non-overlapping rule as the old `count_occurrences`: after a
        // hit the scan resumes past the whole needle.
        assert_eq!(count("aaa", "aa", Fold::Lower), 1);
        assert_eq!(count("aaaa", "aa", Fold::Lower), 2);
        assert_eq!(count("aAa", "aa", Fold::Lower), 1);
        assert_eq!(count("aAa", "aa", Fold::Exact), 0);
        assert_eq!(count("hello", "", Fold::Lower), 0);
        assert_eq!(count("привет привет", "привет", Fold::Lower), 2);
    }

    #[test]
    fn empty_and_oversized_needles() {
        assert!(find_lower("abc", "").is_none());
        assert!(contains("abc", "", Fold::Lower, Mode::Substring));
        assert!(!contains("abc", "", Fold::Lower, Mode::WordPrefix));
        assert!(find_lower("ab", "abc").is_none());
        assert!(find_lower("", "a").is_none());
        assert_eq!(count("", "a", Fold::Lower), 0);
    }

    #[test]
    fn fold_str_matches_iterator_output() {
        assert_eq!(fold_str("İ", Fold::Lower), "i\u{0307}");
        assert_eq!(fold_str("ПРИ", Fold::Lower), "при");
        assert_eq!(fold_str("ПРИ", Fold::Exact), "ПРИ");
    }
}
