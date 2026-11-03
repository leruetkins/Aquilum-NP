use std::path::Path;
use super::import::ImportedImage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    pub target: String,
    /// `![[name]]` — a bare name resolved through `resolve_attachment`.
    pub wiki: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl ImageReference {
    /// Markup to write back into the note.
    pub fn markup(&self) -> String {
        match (self.width, self.height) {
            (Some(w), Some(h)) => format!("![[{}|{}x{}]]", self.target, w, h),
            (Some(w), None) => format!("![[{}|{}]]", self.target, w),
            _ if self.wiki => format!("![[{}]]", self.target),
            _ => format!("![]({})", self.target),
        }
    }

    pub fn with_size(&self, width: Option<u32>, height: Option<u32>) -> Self {
        Self {
            width,
            height,
            ..self.clone()
        }
    }

    pub fn with_target(&self, target: String) -> Self {
        Self {
            target,
            ..self.clone()
        }
    }

    /// Displayed size in px inside a text column of `max_width`.
    pub fn display_size(&self, natural: (u32, u32), max_width: f32) -> (f32, f32) {
        let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
        let explicit_w = self.width.filter(|w| *w > 0).map(|w| w as f32);
        let explicit_h = self.height.filter(|h| *h > 0).map(|h| h as f32);
        let (mut w, mut h) = match (explicit_w, explicit_h) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * nh / nw),
            (None, Some(h)) => (h * nw / nh, h),
            (None, None) => (nw, nh),
        };
        let max_width = max_width.max(1.0);
        if w > max_width {
            h *= max_width / w;
            w = max_width;
        }
        (w.max(1.0), h.max(1.0))
    }
}

/// Parses a line that consists only of one image reference.
///
/// Returns `None` for ordinary text, for markup inside a longer line, and for
/// remote URLs (those are not attachments).
pub fn parse_image_reference(line: &str) -> Option<ImageReference> {
    let trimmed = line.trim();
    if !trimmed.starts_with('!') {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("![[") {
        let inner = rest.strip_suffix("]]")?;
        if inner.contains('[') || inner.contains(']') || inner.contains('\n') {
            return None;
        }
        let mut parts = inner.split('|');
        let target = parts.next()?.trim();
        if target.is_empty() {
            return None;
        }
        let (mut width, mut height) = (None, None);
        if let Some(spec) = parts.next() {
            let spec = spec.trim();
            if let Some((w, h)) = spec.split_once('x').or_else(|| spec.split_once('X')) {
                width = w.trim().parse::<u32>().ok().filter(|v| *v > 0);
                height = h.trim().parse::<u32>().ok().filter(|v| *v > 0);
            } else if let Ok(w) = spec.parse::<u32>() {
                if w > 0 {
                    width = Some(w);
                }
            }
            // A non-numeric part is an Obsidian-style caption; captions are
            // stored in `.edit.json`, so it is only a hint here and ignored.
        }
        return Some(ImageReference {
            target: target.to_string(),
            wiki: true,
            width,
            height,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("![") {
        let (alt, url) = rest.split_once("](")?;
        let url = url.strip_suffix(')')?;
        let _ = alt;
        if url.starts_with("http://") || url.starts_with("https://") || url.is_empty() {
            return None;
        }
        return Some(ImageReference {
            target: url.trim().to_string(),
            wiki: false,
            width: None,
            height: None,
        });
    }
    None
}

/// `![[name.png]]` written at the caret, one per line.
pub fn insert_text_for(images: &[ImportedImage]) -> String {
    images
        .iter()
        .map(|image| format!("![[{}]]", image.name))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefScan {
    Normal,
    Bang,
    Open,
    Wiki,
    WikiSize,
    Close,
    AfterClose,
    Inline,
}

/// Names referenced by `![[name]]` and `![[name|size]]` in one text.
pub fn references_in_text(text: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    scan_references(text.chars(), &mut out);
    out
}

/// Same scan over the document rope. Only image references allocate.
pub fn references_in_rope(rope: &crop::Rope) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    scan_references(rope.chars(), &mut out);
    out
}

fn scan_references(chars: impl Iterator<Item = char>, out: &mut std::collections::HashSet<String>) {
    let mut state = RefScan::Normal;
    let mut buffer = String::new();
    let finish = |buffer: &mut String, out: &mut std::collections::HashSet<String>| {
        let name = buffer.trim();
        if !name.is_empty() && !name.starts_with("http://") && !name.starts_with("https://") {
            out.insert(name.to_string());
        }
        buffer.clear();
    };
    for ch in chars {
        state = match state {
            RefScan::Normal => {
                if ch == '!' {
                    RefScan::Bang
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Bang => {
                if ch == '[' {
                    RefScan::Open
                } else if ch == '!' {
                    RefScan::Bang
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Open => {
                if ch == '[' {
                    buffer.clear();
                    RefScan::Wiki
                } else if ch == ']' {
                    RefScan::AfterClose
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Wiki => match ch {
                '|' => RefScan::WikiSize,
                ']' => RefScan::Close,
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                other => {
                    buffer.push(other);
                    RefScan::Wiki
                }
            },
            RefScan::WikiSize => match ch {
                ']' => RefScan::Close,
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                _ => RefScan::WikiSize,
            },
            RefScan::Close => {
                if ch == ']' {
                    finish(&mut buffer, out);
                } else {
                    buffer.clear();
                }
                RefScan::Normal
            }
            RefScan::AfterClose => {
                if ch == '(' {
                    buffer.clear();
                    RefScan::Inline
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Inline => match ch {
                ')' => {
                    finish(&mut buffer, out);
                    RefScan::Normal
                }
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                other => {
                    buffer.push(other);
                    RefScan::Inline
                }
            },
        };
    }
}

/// Collects every image reference from a markdown file without loading the
/// whole file into memory. Files larger than 50MB are skipped for safety.
pub fn references_in_file(path: &Path, out: &mut std::collections::HashSet<String>) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > 50 * 1024 * 1024 {
            return;
        }
    }
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    while let Ok(n) = reader.read_line(&mut line) {
        if n == 0 {
            break;
        }
        if (line.contains("![[") || line.contains("![")) && line.len() <= 65536 {
            scan_references(line.chars(), out);
        }
        line.clear();
    }
}
