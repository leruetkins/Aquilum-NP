use std::path::{Path, PathBuf};

pub(crate) fn is_note_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

pub(crate) fn walk_markdown(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "vendor" {
            continue;
        }
        if path.is_dir() {
            walk_markdown(root, &path, depth + 1, out);
        } else if path.starts_with(root) && is_note_file(&path) {
            // Skip binary/oversized files (> 50 MB, matching vault.rs BINARY_LIMIT).
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size <= 50 * 1024 * 1024 {
                out.push(path);
            }
        }
    }
}
