use super::*;
use super::inline::*;

pub fn parse_inline_syntax_spans(mut s: &str) -> Vec<SyntaxSpan> {
    let whole = s;
    let mut spans = Vec::new();
    let mut plain_accum = String::new();

    while !s.is_empty() {
        // Backslash escape: `\` before ASCII punctuation makes that char literal. The
        // backslash is a marker (hidden off the active line, shown muted on it, like the
        // other syntax) and the escaped char is plain text — so `` \` `` renders as a
        // literal backtick and opens no code span, and `\*` keeps its asterisk instead of
        // starting an emphasis span. This branch must stay ahead of every span opener.
        if s.starts_with('\\') {
            if let Some(c) = s[1..].chars().next() {
                if is_escapable_punctuation(c) {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    spans.push(SyntaxSpan::Marker("\\".to_string()));
                    spans.push(SyntaxSpan::Text(c.to_string()));
                    s = &s[1 + c.len_utf8()..];
                    continue;
                }
            }
        }

        // School-grammar member: `::s{red/blue}[text]`. The closing `]` ends the
        // fragment (there is no `::` after it). An unknown type letter or a
        // malformed construct is not consumed here, so it accumulates as plain text.
        if s.starts_with("::") {
            if let Some(scan) = scan_grammar_tag(s) {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker(scan.marker.to_string()));
                spans.push(SyntaxSpan::Grammar {
                    kind: scan.kind,
                    line_color: scan.line_color,
                    text_color: scan.text_color,
                    text: scan.content.to_string(),
                    content: parse_grammar_content(scan.content),
                });
                spans.push(SyntaxSpan::Marker("]".to_string()));
                s = &s[scan.consumed..];
                continue;
            }
        }

        // WikiLink: [[target]] or [[target|alias]]
        if s.starts_with("[[") {
            if let Some(close_idx) = s[2..].find("]]") {
                let inner = &s[2..2 + close_idx];
                if !inner.is_empty() && !inner.contains('\n') {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    if let Some((target, alias)) = inner.split_once('|') {
                        let open_marker = format!("[[{}|", target);
                        spans.push(SyntaxSpan::Marker(open_marker));
                        spans.push(SyntaxSpan::Link {
                            text: alias.to_string(),
                            url: target.to_string(),
                        });
                        spans.push(SyntaxSpan::Marker("]]".to_string()));
                    } else {
                        spans.push(SyntaxSpan::Marker("[[".to_string()));
                        spans.push(SyntaxSpan::Link {
                            text: inner.to_string(),
                            url: inner.to_string(),
                        });
                        spans.push(SyntaxSpan::Marker("]]".to_string()));
                    }
                    let total_consumed = 2 + close_idx + 2;
                    s = &s[total_consumed..];
                    continue;
                }
            }
        }

        // Link: [label](url)
        if s.starts_with('[') {
            if let Some(close_bracket) = s[1..].find(']') {
                let after_bracket = &s[1 + close_bracket + 1..];
                if after_bracket.starts_with('(') {
                    if let Some(close_paren) = after_bracket[1..].find(')') {
                        if !plain_accum.is_empty() {
                            spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                        }
                        let label = &s[1..1 + close_bracket];
                        let url = &after_bracket[1..1 + close_paren];
                        spans.push(SyntaxSpan::Marker("[".to_string()));
                        spans.push(SyntaxSpan::Link {
                            text: label.to_string(),
                            url: url.to_string(),
                        });
                        let close_marker = format!("]({})", url);
                        spans.push(SyntaxSpan::Marker(close_marker));
                        let total_consumed = 1 + close_bracket + 1 + 1 + close_paren + 1;
                        s = &s[total_consumed..];
                        continue;
                    }
                }
            }
        }

        // HTML Underline tag: <u>...</u>
        if s.starts_with("<u>") {
            if let Some(end_idx) = s[3..].find("</u>") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("<u>".to_string()));
                let content = &s[3..3 + end_idx];
                spans.push(SyntaxSpan::Underline(content.to_string()));
                spans.push(SyntaxSpan::Marker("</u>".to_string()));
                s = &s[3 + end_idx + 4..];
                continue;
            }
        }

        // Autolink: `<https://example.com>` / `<mail@example.com>`. The brackets are
        // markup, so they hide while the caret is away like any other link marker.
        if s.starts_with('<') {
            if let Some((shown, target)) = scan_angle_autolink(s) {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("<".to_string()));
                spans.push(SyntaxSpan::Link {
                    text: shown.to_string(),
                    url: target,
                });
                spans.push(SyntaxSpan::Marker(">".to_string()));
                s = &s[1 + shown.len() + 1..];
                continue;
            }
        }

        // Highlight: ==...==
        if s.starts_with("==") {
            if let Some(end_idx) = s[2..].find("==") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("==".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Highlight(content.to_string()));
                spans.push(SyntaxSpan::Marker("==".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Inline code: a run of N backticks, closed by the next run of exactly N. The
        // run length is the whole point: `` ``code with `backticks` `` `` is one span holding
        // literal single backticks, and reading it as a lone delimiter produced an empty
        // span plus a wrongly-short one (the content must not be re-scanned either).
        let run = backtick_run_len(s);
        if run > 0 {
            match find_closing_backtick_run(&s[run..], run) {
                Some(content_len) => {
                    if !plain_accum.is_empty() {
                        spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                    }
                    let delimiter = "`".repeat(run);
                    let code_content = &s[run..run + content_len];
                    spans.push(SyntaxSpan::Marker(delimiter.clone()));
                    spans.push(SyntaxSpan::InlineCode(code_content.to_string()));
                    spans.push(SyntaxSpan::Marker(delimiter));
                    s = &s[run + content_len + run..];
                    continue;
                }
                // No run of the same length: the backticks are literal text, and the
                // whole run is consumed so it cannot be re-read as a shorter delimiter.
                None => {
                    plain_accum.push_str(&s[..run]);
                    s = &s[run..];
                    continue;
                }
            }
        }

        // Bold italic: ***...***
        if s.starts_with("***") {
            if let Some(end_idx) = s[3..].find("***") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("***".to_string()));
                let content = &s[3..3 + end_idx];
                spans.push(SyntaxSpan::BoldItalic(content.to_string()));
                spans.push(SyntaxSpan::Marker("***".to_string()));
                s = &s[6 + end_idx..];
                continue;
            }
        }

        // Bold: **...**
        if s.starts_with("**") {
            if let Some(end_idx) = s[2..].find("**") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("**".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Bold(content.to_string()));
                spans.push(SyntaxSpan::Marker("**".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Italic: *...*
        if s.starts_with('*') {
            if let Some(end_idx) = s[1..].find('*') {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("*".to_string()));
                let content = &s[1..1 + end_idx];
                spans.push(SyntaxSpan::Italic(content.to_string()));
                spans.push(SyntaxSpan::Marker("*".to_string()));
                s = &s[2 + end_idx..];
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if s.starts_with("~~") {
            if let Some(end_idx) = s[2..].find("~~") {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                let content = &s[2..2 + end_idx];
                spans.push(SyntaxSpan::Strikethrough(content.to_string()));
                spans.push(SyntaxSpan::Marker("~~".to_string()));
                s = &s[4 + end_idx..];
                continue;
            }
        }

        // Bare autolink: GFM links a URL that is not inside other markup, and the
        // original editor shows `https://example.com` / `www.example.com` accented and
        // clickable without any `[...]()` around it.
        if let Some(len) = scan_bare_url(s) {
            // The prefix must start a word: `nothttps://` is plain text. Openers count
            // as a boundary too, so a URL right after `(` or an emphasis marker links.
            let before = &whole[..whole.len() - s.len()];
            let at_boundary = before
                .chars()
                .next_back()
                .map_or(true, |c| c.is_whitespace() || "([<*_~".contains(c));
            if at_boundary {
                if !plain_accum.is_empty() {
                    spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                }
                let shown = &s[..len];
                let url = if shown.starts_with("www.") {
                    format!("https://{shown}")
                } else {
                    shown.to_string()
                };
                spans.push(SyntaxSpan::Link {
                    text: shown.to_string(),
                    url,
                });
                s = &s[len..];
                continue;
            }
        }

        // Tag: #tag (e.g. #name, #surname, #nickname)
        if s.starts_with('#') {
            let prev_ok = plain_accum.chars().last().map_or_else(
                || {
                    spans.last().map_or(true, |span| {
                        let last_char = match span {
                            SyntaxSpan::Text(t)
                            | SyntaxSpan::Bold(t)
                            | SyntaxSpan::Italic(t)
                            | SyntaxSpan::BoldItalic(t)
                            | SyntaxSpan::InlineCode(t)
                            | SyntaxSpan::Strikethrough(t)
                            | SyntaxSpan::Underline(t)
                            | SyntaxSpan::Highlight(t)
                            | SyntaxSpan::Tag(t) => t.chars().last(),
                            SyntaxSpan::Marker(m) => m.chars().last(),
                            SyntaxSpan::Link { text, .. } => text.chars().last(),
                            SyntaxSpan::Grammar { text, .. } => text.chars().last(),
                        };
                        last_char.map_or(true, |c| !c.is_alphanumeric() && c != '#' && c != '/' && c != '\\')
                    })
                },
                |c| !c.is_alphanumeric() && c != '#' && c != '/' && c != '\\',
            );
            if prev_ok {
                if let Some(first_char) = s[1..].chars().next() {
                    if first_char.is_alphabetic() || first_char == '_' {
                        let mut tag_len = 1;
                        for c in s[1..].chars() {
                            if c.is_alphanumeric() || c == '_' || c == '-' || c == '/' {
                                tag_len += c.len_utf8();
                            } else {
                                break;
                            }
                        }
                        while tag_len > 1 && (s.as_bytes()[tag_len - 1] == b'-' || s.as_bytes()[tag_len - 1] == b'/') {
                            tag_len -= 1;
                        }
                        if tag_len > 1 {
                            if !plain_accum.is_empty() {
                                spans.push(SyntaxSpan::Text(std::mem::take(&mut plain_accum)));
                            }
                            let tag_str = &s[..tag_len];
                            spans.push(SyntaxSpan::Tag(tag_str.to_string()));
                            s = &s[tag_len..];
                            continue;
                        }
                    }
                }
            }
        }

        // Regular character
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            plain_accum.push(c);
            s = chars.as_str();
        } else {
            break;
        }
    }

    if !plain_accum.is_empty() {
        spans.push(SyntaxSpan::Text(plain_accum));
    }

    spans
}
