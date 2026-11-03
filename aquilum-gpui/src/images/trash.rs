use std::path::{Path, PathBuf};
use super::import::{attachments_dir, folder_name};
use super::ref_index::{
    defer_removed_reference, ensure_ref_index, ref_index, referenced_elsewhere,
    remove_note_from_ref_index, rename_note_in_ref_index,
};
use super::edit::edit_path;
use super::reference::{references_in_file, references_in_text};
use super::resolve::{invalidate_index, is_image_path};

// ───────────────────────────── References & lifecycle ─────────────────────────────


pub(crate) fn trash_dir(vault_root: &Path, folder: &str) -> PathBuf {
    vault_root.join(folder_name(folder)).join(".trash")
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned());
    for n in 1..10_000u32 {
        let candidate_name = match &ext {
            Some(ext) => format!("{stem} {n}.{ext}"),
            None => format!("{stem} {n}"),
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(name)
}

/// Moves one attachment (and its sidecar) into the global attachment trash.
pub fn trash_pair(vault_root: &Path, folder: &str, source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?.to_string_lossy().into_owned();
    let dir = trash_dir(vault_root, folder);
    std::fs::create_dir_all(&dir).ok()?;
    let target = unique_path(&dir, &name);
    std::fs::rename(source, &target).ok()?;
    let source_edit = edit_path(source);
    if source_edit.is_file() {
        let target_edit = edit_path(&target);
        let _ = std::fs::rename(&source_edit, &target_edit);
    }
    invalidate_index();
    Some(target)
}

/// Returns a trashed attachment next to the note when the note references it
/// again (undo after a save).
pub fn restore_if_trashed(
    vault_root: &Path,
    note_path: &Path,
    folder: &str,
    name: &str,
) -> Option<PathBuf> {
    let destination = attachments_dir(note_path, folder).join(name);
    if destination.exists() {
        return Some(destination);
    }
    let trashed = trash_dir(vault_root, folder).join(name);
    if !trashed.is_file() {
        return None;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    std::fs::rename(&trashed, &destination).ok()?;
    let trashed_edit = edit_path(&trashed);
    if trashed_edit.is_file() {
        let destination_edit = edit_path(&destination);
        let _ = std::fs::rename(&trashed_edit, &destination_edit);
    }
    invalidate_index();
    Some(destination)
}



fn is_image_reference_at(buf: &[u8], pos: usize, needle_len: usize) -> bool {
    // 1. Boundary check: character immediately preceding needle cannot be part of the filename
    if pos > 0 {
        let prev = buf[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' || prev == b'.' {
            return false;
        }
    }
    // 2. Boundary check: character immediately following needle cannot be part of the filename
    let end_pos = pos + needle_len;
    if end_pos < buf.len() {
        let next = buf[end_pos];
        if next.is_ascii_alphanumeric() || next == b'_' || next == b'-' || next == b'.' {
            return false;
        }
    }

    // 3. Look backwards up to 1024 bytes for `![` on the same line
    let lookback_start = pos.saturating_sub(1024);
    let prefix = &buf[lookback_start..pos];
    let Some(bang_bracket_idx) = memchr::memmem::rfind(prefix, b"![") else {
        return false;
    };
    let between = &prefix[bang_bracket_idx + 2..];
    // No newlines allowed between `![` and the reference name
    if memchr::memchr(b'\n', between).is_some() || memchr::memchr(b'\r', between).is_some() {
        return false;
    }

    // 4. Look forwards up to 512 bytes on the same line for closing `]` or `)`
    let lookahead_end = (end_pos + 512).min(buf.len());
    let suffix = &buf[end_pos..lookahead_end];
    let suffix = if let Some(newline_idx) = memchr::memchr(b'\n', suffix) {
        &suffix[..newline_idx]
    } else {
        suffix
    };

    suffix.contains(&b']') || suffix.contains(&b')')
}

/// Checks whether a single markdown file references `name`, without loading
/// the whole file into memory, using streaming SIMD scanning.
pub fn note_references_attachment(path: &Path, name: &str) -> bool {
    let needle = name.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };

    let mut needle_lower = needle.to_vec();
    needle_lower.make_ascii_lowercase();
    let finder = memchr::memmem::Finder::new(&needle_lower);

    let mut buf = [0u8; 64 * 1024];
    let mut lower_buf = [0u8; 64 * 1024];
    let min_overlap = 2048.max(needle.len() + 64);
    if min_overlap >= buf.len() {
        return false;
    }
    let mut overlap = 0;

    loop {
        let read = match std::io::Read::read(&mut file, &mut buf[overlap..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return false,
        };
        let valid = overlap + read;
        if valid >= needle.len() {
            // An attachment in a note is always referenced via `![` (markdown or wikilink).
            // If the buffer does not even contain `![`, it cannot contain any image references.
            if memchr::memmem::find(&buf[..valid], b"![").is_some() {
                lower_buf[..valid].copy_from_slice(&buf[..valid]);
                lower_buf[..valid].make_ascii_lowercase();

                let mut search_from = 0;
                while search_from + needle.len() <= valid {
                    if let Some(rel_pos) = finder.find(&lower_buf[search_from..valid]) {
                        let pos = search_from + rel_pos;
                        if is_image_reference_at(&lower_buf[..valid], pos, needle.len()) {
                            return true;
                        }
                        search_from = pos + 1;
                    } else {
                        break;
                    }
                }
            }
            overlap = min_overlap.min(valid);
            buf.copy_within(valid - overlap..valid, 0);
        } else {
            overlap = valid;
        }
    }
    false
}




/// Moves the pairs referenced by a note from `old_note`'s attachments to
/// `new_note`'s attachments. Shared files stay where they are.
pub fn move_note_attachments(
    vault_root: &Path,
    old_note: &Path,
    new_note: &Path,
    folder: &str,
) -> usize {
    let mut names = std::collections::HashSet::new();
    references_in_file(new_note, &mut names);
    if names.is_empty() {
        return 0;
    }
    let old_dir = attachments_dir(old_note, folder);
    let new_dir = attachments_dir(new_note, folder);
    if old_dir == new_dir {
        return 0;
    }
    let mut moved = 0;
    for name in names {
        let source = old_dir.join(&name);
        if !source.is_file() {
            continue;
        }
        if referenced_elsewhere(vault_root, &name, &[old_note, new_note]) {
            continue;
        }
        if std::fs::create_dir_all(&new_dir).is_err() {
            continue;
        }
        let target = if new_dir.join(&name).exists() {
            continue;
        } else {
            new_dir.join(&name)
        };
        if std::fs::rename(&source, &target).is_ok() {
            let source_edit = edit_path(&source);
            if source_edit.is_file() {
                let _ = std::fs::rename(&source_edit, edit_path(&target));
            }
            moved += 1;
        }
    }
    if moved > 0 {
        invalidate_index();
        rename_note_in_ref_index(old_note, new_note);
    }
    moved
}

/// Sends every attachment referenced by a deleted note to the trash.
pub fn trash_note_attachments(
    vault_root: &Path,
    note_path: &Path,
    content: &str,
    folder: &str,
    exclude_notes: &[&Path],
) {
    for name in references_in_text(content) {
        defer_removed_reference(vault_root, note_path, folder, &name, exclude_notes);
    }
    remove_note_from_ref_index(note_path);
}

/// Attachment files in the vault that no note references.
pub fn unused_attachments(vault_root: &Path, folder: &str) -> Vec<PathBuf> {
    ensure_ref_index(vault_root);
    let guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    let mut files = Vec::new();
    let mut dirs = vec![vault_root.join(folder_name(folder))];
    scan_attachment_dirs(vault_root, vault_root, &folder_name(folder), 0, &mut dirs);
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".edit.json") {
                continue;
            }
            let key = name.to_lowercase();
            if is_image_path(&path) && !guard.attachment_to_notes.contains_key(&key) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn scan_attachment_dirs(
    root: &Path,
    dir: &Path,
    folder: &str,
    depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "vendor" {
            continue;
        }
        if name == folder {
            out.push(path.clone());
        }
        scan_attachment_dirs(root, &path, folder, depth + 1, out);
    }
}

/// Removes attachment trash files older than `days`.
pub fn clean_trash(vault_root: &Path, folder: &str, days: u32) -> usize {
    let dir = trash_dir(vault_root, folder);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days.max(1) as u64 * 24 * 60 * 60);
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > limit);
        if stale && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Moves a pair to another note's attachments when no other note references
/// it. Returns the name in the target folder.
pub fn move_attachment(
    vault_root: Option<&Path>,
    from_note: &Path,
    to_note: &Path,
    folder: &str,
    name: &str,
) -> Option<String> {
    let folder = folder_name(folder);
    let source = attachments_dir(from_note, &folder).join(name);
    if !source.is_file() {
        return None;
    }
    let target_dir = attachments_dir(to_note, &folder);
    // Both notes keep attachments in the same folder: nothing to move and no
    // need to scan the whole vault for other references.
    if source.parent() == Some(target_dir.as_path()) {
        return Some(name.to_string());
    }
    let root = vault_root?;
    if referenced_elsewhere(root, name, &[from_note, to_note]) {
        return None;
    }
    std::fs::create_dir_all(&target_dir).ok()?;
    let target = if target_dir.join(name).exists() {
        unique_path(&target_dir, name)
    } else {
        target_dir.join(name)
    };
    std::fs::rename(&source, &target).ok()?;
    let source_edit = edit_path(&source);
    if source_edit.is_file() {
        let _ = std::fs::rename(&source_edit, edit_path(&target));
    }
    invalidate_index();
    Some(target.file_name()?.to_string_lossy().into_owned())
}

/// Deletes every file in the attachment trash.
pub fn clear_trash(vault_root: &Path, folder: &str) -> usize {
    let dir = trash_dir(vault_root, folder);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}
