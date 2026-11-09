//! Vault-scoped operations exposed as JSON strings.
//!
//! Every function returns one envelope:
//! `{"ok":true,"data":...}` or `{"ok":false,"error":"..."}`. The Android side
//! never needs a second error channel and can render a failure verbatim.

use crate::vault::{FileInfo, VaultCore};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

struct CoreState {
    vault: Option<VaultCore>,
    root: Option<String>,
}

static STATE: OnceLock<Mutex<CoreState>> = OnceLock::new();

fn state() -> &'static Mutex<CoreState> {
    STATE.get_or_init(|| {
        Mutex::new(CoreState {
            vault: None,
            root: None,
        })
    })
}

fn ok<T: Serialize>(data: T) -> String {
    json!({ "ok": true, "data": data }).to_string()
}

pub(crate) fn err(message: impl Into<String>) -> String {
    json!({ "ok": false, "error": message.into() }).to_string()
}

fn with_vault<T>(f: impl FnOnce(&VaultCore) -> Result<T, String>) -> Result<T, String> {
    let guard = state()
        .lock()
        .map_err(|_| "core state is poisoned".to_string())?;
    let vault = guard
        .vault
        .as_ref()
        .ok_or_else(|| "no vault is open".to_string())?;
    f(vault)
}

pub fn version() -> String {
    ok(json!({ "version": VERSION }))
}

pub fn open_vault(path: &str) -> String {
    let root = Path::new(path);
    if !root.is_dir() {
        return err(format!("Not a directory: {path}"));
    }
    let vault = VaultCore::open(root.to_path_buf());
    let name = vault.name.clone();
    // Opening must stay O(1): the Android shell scans the tree once on its own
    // IO thread. Counting markdown files here meant a full recursive walk
    // before the first frame could be shown.
    match state().lock() {
        Ok(mut guard) => {
            guard.vault = Some(vault);
            guard.root = Some(path.to_owned());
            ok(json!({ "name": name, "root": path }))
        }
        Err(_) => err("core state is poisoned"),
    }
}

/// Creates the directory (if needed) and a welcome note, then opens it. This is
/// the friction-free first-run path: the app needs no storage permission to
/// use its own `filesDir`, and the user can point it at a shared folder later.
pub fn create_vault(path: &str) -> String {
    let root = Path::new(path);
    if let Err(error) = std::fs::create_dir_all(root) {
        return err(format!("Could not create {path}: {error}"));
    }
    let welcome = root.join("Welcome.md");
    if !welcome.exists() {
        let body = "# Welcome to vibecodium\n\n\
                    This vault is served by the Rust core over JNI.\n\n\
                    - Tap the drawer button to browse notes\n\
                    - Use + for a new note and the tab button to switch\n\
                    - The right panel shows the document outline\n";
        if let Err(error) = std::fs::write(&welcome, body) {
            return err(format!("Could not write the welcome note: {error}"));
        }
    }
    open_vault(path)
}

pub fn close_vault() -> String {
    match state().lock() {
        Ok(mut guard) => {
            guard.vault = None;
            guard.root = None;
            ok(json!({ "closed": true }))
        }
        Err(_) => err("core state is poisoned"),
    }
}

pub fn vault_root() -> String {
    match state().lock() {
        Ok(guard) => match guard.root.as_deref() {
            Some(root) => ok(json!({ "root": root })),
            None => err("no vault is open"),
        },
        Err(_) => err("core state is poisoned"),
    }
}

pub fn list_notes() -> String {
    match with_vault(|vault| {
        // The mobile app can receive files from sync or another app while the
        // vault is open; the desktop snapshot cache must not hide them.
        vault.invalidate_markdown_cache();
        let mut notes: Vec<FileInfo> = vault.list_markdown_files().iter().cloned().collect();
        notes.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
        Ok(notes)
    }) {
        Ok(notes) => ok(json!({ "notes": notes })),
        Err(error) => err(error),
    }
}

pub fn read_note(path: &str) -> String {
    match with_vault(|vault| vault.read_file(path).map_err(|e| e.to_string())) {
        Ok(text) => ok(json!({ "path": path, "text": text })),
        Err(error) => err(error),
    }
}

pub fn write_note(path: &str, text: &str) -> String {
    match with_vault(|vault| vault.modify_file(path, text).map_err(|e| e.to_string())) {
        Ok(()) => ok(json!({ "path": path })),
        Err(error) => err(error),
    }
}

pub fn create_note(path: &str, text: &str) -> String {
    match with_vault(|vault| {
        vault
            .create_file(path, text)
            .map(|_| path.to_owned())
            .map_err(|e| e.to_string())
    }) {
        Ok(path) => {
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            ok(json!({ "path": path, "name": name }))
        }
        Err(error) => err(error),
    }
}

pub fn delete_note(path: &str) -> String {
    match with_vault(|vault| {
        // Obsidian keeps deletions recoverable; fall back to a hard delete only
        // when the vault cannot create its trash folder.
        match vault.trash_file(path) {
            Ok(()) => Ok(()),
            Err(_) => vault.delete_file(path).map_err(|e| e.to_string()),
        }
    }) {
        Ok(()) => ok(json!({ "path": path })),
        Err(error) => err(error),
    }
}

pub fn rename_note(old_path: &str, new_path: &str) -> String {
    match with_vault(|vault| {
        vault
            .rename_file(old_path, new_path)
            .map(|_| new_path.to_owned())
            .map_err(|e| e.to_string())
    }) {
        Ok(path) => ok(json!({ "path": path })),
        Err(error) => err(error),
    }
}

pub fn search(query: &str, limit: u32) -> String {
    let query = query.trim();
    if query.is_empty() {
        return ok(json!({ "matches": [] }));
    }
    let needle = query.to_lowercase();
    let limit = limit.clamp(1, 200) as usize;

    match with_vault(|vault| {
        let files = vault.list_markdown_files();
        let mut matches: Vec<Value> = Vec::new();

        'files: for file in files.iter() {
            let name_hit = file.name.to_lowercase().contains(&needle);
            let text = vault.read_file(&file.path).unwrap_or_default();
            let mut hit_line: Option<(usize, String)> = None;

            for (index, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    hit_line = Some((index + 1, snippet(line)));
                    break;
                }
            }

            match hit_line {
                Some((line, snippet)) => matches.push(json!({
                    "path": file.path,
                    "name": file.name,
                    "line": line,
                    "snippet": snippet,
                })),
                None if name_hit => matches.push(json!({
                    "path": file.path,
                    "name": file.name,
                    "line": 0,
                    "snippet": "",
                })),
                None => continue 'files,
            }

            if matches.len() >= limit {
                break;
            }
        }

        Ok(matches)
    }) {
        Ok(matches) => ok(json!({ "matches": matches })),
        Err(error) => err(error),
    }
}

/// Extracts ATX headings for the outline panel. Fenced code blocks are
/// skipped so a `# comment` inside a snippet is not reported as a heading.
pub fn outline(text: &str) -> String {
    let mut headings: Vec<Value> = Vec::new();
    let mut in_fence = false;

    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&hashes) {
            continue;
        }
        if trimmed.chars().nth(hashes) != Some(' ') {
            continue;
        }

        let title = trimmed[hashes..]
            .trim()
            .trim_end_matches('#')
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }
        headings.push(json!({
            "level": hashes,
            "text": title,
            "line": index + 1,
        }));
    }

    ok(json!({ "headings": headings }))
}

pub fn stats(text: &str) -> String {
    let lines = text.lines().count();
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    ok(json!({ "lines": lines, "words": words, "chars": chars }))
}

/// Markdown rows produced by the desktop classifier (`classify`/`inline`/
/// `syntax_spans`), consumed by both the Compose preview and source view.
pub fn markdown(text: &str) -> String {
    ok(json!({ "lines": crate::markdown::model(text) }))
}

/// One call for everything the editor needs after a note is opened.
///
/// The shell used to call `stats`, `outline` and `markdown` separately, which
/// parsed the same document three times across three JNI crossings. This
/// builds the markdown model once and derives the outline from its heading
/// rows, so opening a note costs one parse.
pub fn analyze(text: &str) -> String {
    let lines = crate::markdown::model(text);
    let mut headings: Vec<Value> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.kind != "heading" {
            continue;
        }
        let title = line.content.trim().trim_end_matches('#').trim();
        if title.is_empty() {
            continue;
        }
        headings.push(json!({
            "level": line.level,
            "text": title,
            "line": index + 1,
        }));
    }
    let stats = json!({
        "lines": lines.len(),
        "words": text.split_whitespace().count(),
        "chars": text.chars().count(),
    });
    ok(json!({ "stats": stats, "headings": headings, "lines": lines }))
}

fn snippet(line: &str) -> String {
    let trimmed = line.trim();
    let mut out: String = trimmed.chars().take(160).collect();
    if trimmed.chars().count() > 160 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_skips_fences() {
        let text = "# One\n\n```\n# not a heading\n```\n## Two ##\n";
        let value: Value = serde_json::from_str(&outline(text)).unwrap();
        let headings = value["data"]["headings"].as_array().unwrap();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0]["level"], 1);
        assert_eq!(headings[0]["text"], "One");
        assert_eq!(headings[1]["text"], "Two");
    }

    #[test]
    fn stats_counts_words() {
        let value: Value = serde_json::from_str(&stats("one two\nthree")).unwrap();
        assert_eq!(value["data"]["words"], 3);
        assert_eq!(value["data"]["lines"], 2);
    }

    #[test]
    fn analyze_reuses_the_model_for_headings_and_stats() {
        let text = "# One\n\n```\n# not a heading\n```\n## Two ##\nthree\n";
        let value: Value = serde_json::from_str(&analyze(text)).unwrap();
        let data = &value["data"];
        assert_eq!(data["stats"]["lines"], 7);
        assert_eq!(data["stats"]["words"], 12);
        let headings = data["headings"].as_array().unwrap();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0]["text"], "One");
        assert_eq!(headings[1]["text"], "Two");
        assert_eq!(headings[1]["line"], 6);
        assert_eq!(data["lines"].as_array().unwrap().len(), 7);
    }
}
