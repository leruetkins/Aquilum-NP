use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use super::notes::walk_markdown;
use super::import::attachments_dir;
use super::{restore_if_trashed, trash_pair};
use super::reference::references_in_file;

fn is_same_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn is_excluded_note(path: &Path, exclude: &[&Path]) -> bool {
    exclude.iter().any(|excluded| is_same_path(path, excluded))
}

// ────────────────────── In-memory Attachment Reference Index ──────────────────────

#[derive(Default)]
pub(crate) struct AttachmentRefIndex {
    vault_root: Option<PathBuf>,
    note_to_attachments: std::collections::HashMap<PathBuf, std::collections::HashSet<String>>,
    pub(super) attachment_to_notes: std::collections::HashMap<String, std::collections::HashSet<PathBuf>>,
    ready: bool,
}

static REF_INDEX: OnceLock<Mutex<AttachmentRefIndex>> = OnceLock::new();

pub(super) fn ref_index() -> &'static Mutex<AttachmentRefIndex> {
    REF_INDEX.get_or_init(|| Mutex::new(AttachmentRefIndex::default()))
}

pub fn invalidate_ref_index() {
    let mut guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    guard.ready = false;
    guard.vault_root = None;
    guard.note_to_attachments.clear();
    guard.attachment_to_notes.clear();
}

pub fn warm_ref_index(vault_root: PathBuf) {
    std::thread::Builder::new()
        .name("attachment-ref-index".into())
        .spawn(move || {
            ensure_ref_index(&vault_root);
        })
        .ok();
}

pub fn ensure_ref_index(vault_root: &Path) {
    {
        let guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
        if guard.ready && guard.vault_root.as_deref() == Some(vault_root) {
            return;
        }
    }

    let mut notes = Vec::new();
    walk_markdown(vault_root, vault_root, 0, &mut notes);
    let mut note_to_attachments = std::collections::HashMap::new();
    let mut attachment_to_notes: std::collections::HashMap<String, std::collections::HashSet<PathBuf>> =
        std::collections::HashMap::new();

    for path in notes {
        let mut refs = std::collections::HashSet::new();
        references_in_file(&path, &mut refs);
        for r in &refs {
            let key = r.to_lowercase();
            attachment_to_notes
                .entry(key.clone())
                .or_default()
                .insert(path.clone());
            if let Some(file_name) = std::path::Path::new(r).file_name().and_then(|s| s.to_str()) {
                let file_key = file_name.to_lowercase();
                if file_key != key {
                    attachment_to_notes
                        .entry(file_key)
                        .or_default()
                        .insert(path.clone());
                }
            }
        }
        note_to_attachments.insert(path, refs);
    }

    let mut guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    if guard.ready && guard.vault_root.as_deref() == Some(vault_root) {
        return;
    }
    for (note, new_refs) in guard.note_to_attachments.drain() {
        for r in &new_refs {
            let key = r.to_lowercase();
            attachment_to_notes
                .entry(key.clone())
                .or_default()
                .insert(note.clone());
            if let Some(file_name) = std::path::Path::new(&r).file_name().and_then(|s| s.to_str()) {
                let file_key = file_name.to_lowercase();
                if file_key != key {
                    attachment_to_notes
                        .entry(file_key)
                        .or_default()
                        .insert(note.clone());
                }
            }
        }
        note_to_attachments.insert(note, new_refs);
    }

    guard.vault_root = Some(vault_root.to_path_buf());
    guard.note_to_attachments = note_to_attachments;
    guard.attachment_to_notes = attachment_to_notes;
    guard.ready = true;
}

pub fn update_note_references(note_path: &Path, current_refs: &std::collections::HashSet<String>) {
    let mut guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    let existing_key = guard
        .note_to_attachments
        .keys()
        .find(|k| is_same_path(k, note_path))
        .cloned();

    let old_refs = if let Some(key) = existing_key {
        guard.note_to_attachments.remove(&key)
    } else {
        None
    };

    if let Some(old_refs) = old_refs {
        for old in old_refs {
            let key = old.to_lowercase();
            if let Some(notes) = guard.attachment_to_notes.get_mut(&key) {
                notes.retain(|p| !is_same_path(p, note_path));
                if notes.is_empty() {
                    guard.attachment_to_notes.remove(&key);
                }
            }
            if let Some(file_name) = std::path::Path::new(&old).file_name().and_then(|s| s.to_str()) {
                let file_key = file_name.to_lowercase();
                if file_key != key {
                    if let Some(notes) = guard.attachment_to_notes.get_mut(&file_key) {
                        notes.retain(|p| !is_same_path(p, note_path));
                        if notes.is_empty() {
                            guard.attachment_to_notes.remove(&file_key);
                        }
                    }
                }
            }
        }
    }

    for r in current_refs {
        let key = r.to_lowercase();
        guard
            .attachment_to_notes
            .entry(key.clone())
            .or_default()
            .insert(note_path.to_path_buf());
        if let Some(file_name) = std::path::Path::new(r).file_name().and_then(|s| s.to_str()) {
            let file_key = file_name.to_lowercase();
            if file_key != key {
                guard
                    .attachment_to_notes
                    .entry(file_key)
                    .or_default()
                    .insert(note_path.to_path_buf());
            }
        }
    }

    guard
        .note_to_attachments
        .insert(note_path.to_path_buf(), current_refs.clone());
}

pub fn remove_note_from_ref_index(note_path: &Path) {
    let mut guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    let existing_key = guard
        .note_to_attachments
        .keys()
        .find(|k| is_same_path(k, note_path))
        .cloned();

    if let Some(key) = existing_key {
        if let Some(old_refs) = guard.note_to_attachments.remove(&key) {
            for old in old_refs {
                let key = old.to_lowercase();
                if let Some(notes) = guard.attachment_to_notes.get_mut(&key) {
                    notes.retain(|p| !is_same_path(p, note_path));
                    if notes.is_empty() {
                        guard.attachment_to_notes.remove(&key);
                    }
                }
                if let Some(file_name) = std::path::Path::new(&old).file_name().and_then(|s| s.to_str()) {
                    let file_key = file_name.to_lowercase();
                    if file_key != key {
                        if let Some(notes) = guard.attachment_to_notes.get_mut(&file_key) {
                            notes.retain(|p| !is_same_path(p, note_path));
                            if notes.is_empty() {
                                guard.attachment_to_notes.remove(&file_key);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn rename_note_in_ref_index(old_path: &Path, new_path: &Path) {
    let mut guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    let existing_key = guard
        .note_to_attachments
        .keys()
        .find(|k| is_same_path(k, old_path))
        .cloned();

    if let Some(key) = existing_key {
        if let Some(refs) = guard.note_to_attachments.remove(&key) {
            for r in &refs {
                let key = r.to_lowercase();
                if let Some(notes) = guard.attachment_to_notes.get_mut(&key) {
                    notes.retain(|p| !is_same_path(p, old_path));
                    notes.insert(new_path.to_path_buf());
                }
                if let Some(file_name) = std::path::Path::new(r).file_name().and_then(|s| s.to_str()) {
                    let file_key = file_name.to_lowercase();
                    if file_key != key {
                        if let Some(notes) = guard.attachment_to_notes.get_mut(&file_key) {
                            notes.retain(|p| !is_same_path(p, old_path));
                            notes.insert(new_path.to_path_buf());
                        }
                    }
                }
            }
            guard
                .note_to_attachments
                .insert(new_path.to_path_buf(), refs);
        }
    }
}

/// True when some other note in the vault references `name`.
/// Checks the in-memory reference index in O(1) time with zero disk reads.
pub fn referenced_elsewhere(vault_root: &Path, name: &str, exclude: &[&Path]) -> bool {
    ensure_ref_index(vault_root);
    let guard = ref_index().lock().unwrap_or_else(|p| p.into_inner());
    let key = name.to_lowercase();
    if let Some(notes) = guard.attachment_to_notes.get(&key) {
        for path in notes {
            if !is_excluded_note(path, exclude) {
                return true;
            }
        }
    }
    false
}

static CUT_PROTECTED: OnceLock<Mutex<std::collections::HashSet<(PathBuf, String)>>> =
    OnceLock::new();

fn cut_protected_cell() -> &'static Mutex<std::collections::HashSet<(PathBuf, String)>> {
    CUT_PROTECTED.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// A Cut copied the reference away: keep the pair until it is pasted into
/// another note (then it is moved), instead of trashing it on the next save.
pub fn protect_cut(note_path: &Path, name: &str) {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((note_path.to_path_buf(), name.to_string()));
}

pub fn release_cut(note_path: &Path, name: &str) {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&(note_path.to_path_buf(), name.to_string()));
}

fn is_cut_protected(note_path: &Path, name: &str) -> bool {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&(note_path.to_path_buf(), name.to_string()))
}

/// A save removed `name`: move the pair to the trash unless another note still
/// uses it or it came from a shared folder.
pub fn defer_removed_reference(
    vault_root: &Path,
    note_path: &Path,
    folder: &str,
    name: &str,
    exclude_notes: &[&Path],
) {
    let beside = attachments_dir(note_path, folder).join(name);
    if !beside.is_file() {
        return;
    }
    if is_cut_protected(note_path, name) {
        return;
    }
    if referenced_elsewhere(vault_root, name, exclude_notes) {
        return;
    }
    let _ = trash_pair(vault_root, folder, &beside);
}

/// Applies the attachment lifecycle after a successful save: references that
/// disappeared from the saved text go to the trash (unless another note still
/// uses them or auto_trash is disabled), and references that came back are restored from the trash.
pub fn reconcile_saved_references(
    vault_root: Option<&Path>,
    note_path: &Path,
    folder: &str,
    previous: &std::collections::HashSet<String>,
    current: &std::collections::HashSet<String>,
    auto_trash: bool,
) {
    let Some(vault_root) = vault_root else {
        return;
    };
    update_note_references(note_path, current);
    let exclude = [note_path];
    if auto_trash {
        for name in previous.difference(current) {
            defer_removed_reference(vault_root, note_path, folder, name, &exclude);
        }
    }
    for name in current {
        restore_if_trashed(vault_root, note_path, folder, name);
    }
}
