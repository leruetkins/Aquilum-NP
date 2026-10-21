//! Body tokenizer for the in-memory search index (stage B).
//!
//! Splits text into alphanumeric tokens, case-folds them with the stage-A
//! matcher and counts frequencies per field (title / path / body). No
//! stemming: prefix queries cover word starts, and per the plan single-char
//! tokens are kept (dropping them would kill one-letter body queries).

use std::collections::HashMap;

/// Index format version. Bumping it invalidates a persisted index; the index
/// is rebuilt in memory on every vault open, so this is documentation of the
/// rules below.
pub const ANALYZER_VERSION: u32 = 1;

/// Field slots inside a token's counts: `[title, path, body]`.
pub type FieldCounts = [u16; 3];

/// Folds `body` into token frequencies.
///
/// Sanitization before splitting (idea from the reference `analyzer/sanitize.rs`,
/// reimplemented): leading YAML frontmatter is cut, and markdown link/image
/// URLs `](…)` are dropped so file paths inside links do not pollute the body
/// terms. A token immediately preceded by `#` is additionally indexed as
/// `#<token>` so the `tag:` operator keeps matching literal hash mentions.
pub fn tokenize_document(title: &str, rel: &str, body: &str) -> HashMap<String, FieldCounts> {
    let mut counts: HashMap<String, FieldCounts> = HashMap::new();
    let mut buf = String::new();
    let mut preceded_by_hash = false;

    let count_token = |buf: &str, field: usize, counts: &mut HashMap<String, FieldCounts>| {
        if buf.is_empty() {
            return;
        }
        let entry = counts.entry(buf.to_owned()).or_insert([0; 3]);
        entry[field] = entry[field].saturating_add(1);
    };

    let sources: [(&str, usize); 3] = [
        (title, 0),
        (rel, 1),
        (&strip_markdown_urls(strip_frontmatter(body)), 2),
    ];
    for (text, field) in sources {
        let text: &str = text;
        for c in text.chars() {
            if c.is_alphanumeric() {
                // Fold on the fly into the reusable buffer (one allocation
                // per document instead of one per token).
                for fc in c.to_lowercase() {
                    buf.push(fc);
                }
            } else {
                if !buf.is_empty() {
                    count_token(&buf, field, &mut counts);
                    if field == 2 && preceded_by_hash {
                        let tag = format!("#{}", buf);
                        count_token(&tag, field, &mut counts);
                    }
                    buf.clear();
                }
                preceded_by_hash = field == 2 && c == '#';
            }
        }
        if !buf.is_empty() {
            count_token(&buf, field, &mut counts);
            buf.clear();
        }
        preceded_by_hash = false;
    }
    counts
}

/// Cuts a leading `---` … `---` YAML frontmatter block, if present.
fn strip_frontmatter(body: &str) -> &str {
    let mut lines = body.lines();
    if !lines.next().map_or(false, |l| l.trim() == "---") {
        return body;
    }
    let mut cut = 0usize;
    let mut inside = true;
    for line in body.lines() {
        cut += line.len() + 1;
        if !inside {
            break;
        }
        if line.trim() == "---" {
            inside = false;
        }
    }
    if inside {
        // Unclosed frontmatter: treat the whole body as frontmatter-free
        // except the opening fence, which the loop already consumed.
        return body;
    }
    body.get(cut..).unwrap_or("")
}

/// Removes markdown link/image URLs: `](url)` runs are dropped so `](docs/x)`
/// does not add path tokens to the body. Link text stays.
fn strip_markdown_urls(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(ix) = rest.find("](") {
        out.push_str(&rest[..ix]);
        let after_paren = &rest[ix + 2..];
        match after_paren.find(')') {
            Some(close) => {
                // Keep the bracket, drop the URL part.
                out.push(']');
                rest = &after_paren[close + 1..];
            }
            None => {
                // Unclosed bracket: keep everything verbatim.
                out.push_str(&rest[ix..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as M;

    fn counts(map: &M<String, FieldCounts>, key: &str) -> FieldCounts {
        *map.get(key).unwrap_or(&[0; 3])
    }

    #[test]
    fn counts_per_field() {
        let t = tokenize_document("Alpha Note", "docs/alpha note.md", "alpha beta alpha");
        assert_eq!(counts(&t, "alpha"), [1, 1, 2]);
        assert_eq!(counts(&t, "beta"), [0, 0, 1]);
        assert_eq!(counts(&t, "note"), [1, 1, 0]);
    }

    #[test]
    fn frontmatter_and_link_urls_are_stripped() {
        let body = "---\ntitle: secret\n---\nsee [docs](https://example.com/x) here";
        let t = tokenize_document("", "", body);
        assert_eq!(counts(&t, "secret"), [0, 0, 0], "frontmatter dropped");
        assert_eq!(counts(&t, "example"), [0, 0, 0], "URL dropped");
        assert_eq!(counts(&t, "docs"), [0, 0, 1], "link text kept");
        assert_eq!(counts(&t, "here"), [0, 0, 1]);
    }

    #[test]
    fn hash_tokens_are_indexed_for_tags() {
        let t = tokenize_document("", "", "tag #rust here");
        assert_eq!(counts(&t, "rust"), [0, 0, 1]);
        assert_eq!(counts(&t, "#rust"), [0, 0, 1], "#token added");
        // A `#` with no token after it must not corrupt the next word.
        let t = tokenize_document("", "", "# rust");
        assert_eq!(counts(&t, "#rust"), [0, 0, 0]);
        assert_eq!(counts(&t, "rust"), [0, 0, 1]);
    }

    #[test]
    fn single_char_tokens_are_kept() {
        let t = tokenize_document("", "", "a b a");
        assert_eq!(counts(&t, "a"), [0, 0, 2]);
        assert_eq!(counts(&t, "b"), [0, 0, 1]);
    }
}
