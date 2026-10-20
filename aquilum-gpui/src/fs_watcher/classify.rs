//! Classification of raw `notify` events into [`FsBatch`] buckets.
//!
//! Runs on the notify callback thread (or the delivery thread via coalesce)
//! and must stay cheap and allocation-light: it filters noise (reads, hidden
//! files, non-note writes) before anything reaches the UI.

use super::batch::FsBatch;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind};
use std::path::Path;

/// Extensions the editor treats as notes for content tracking. Other files
/// still produce deletion/rename events (any open document must follow), but
/// their content writes are ignored — no open document can be affected.
fn is_note(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

/// Hidden files and our own atomic-save temporary files share the leading dot;
/// both must never surface as user-visible events.
fn is_hidden(root: &Path, path: &Path) -> bool {
    let _ = root;
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// A rename/create whose target has no extension is treated as a folder move:
/// the event may arrive after the target no longer exists (or is not created
/// yet), so `is_dir()` cannot be trusted; notes are always `md`/`txt`.
fn looks_like_folder(path: &Path) -> bool {
    path.extension().is_none()
}

/// Recognises the atomic-save temporary file written by this very process
/// (`save_manager.rs` template: `.{file_name}.{pid}.tmp`). A rename from such
/// a file is our own completed save, not a user rename.
fn is_our_atomic_tmp(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(body) = name.strip_suffix(".tmp") else {
        return false;
    };
    let Some((_, pid)) = body.rsplit_once('.') else {
        return false;
    };
    pid == std::process::id().to_string()
}

/// Maps one notify event to at most one batch. `Err` (queue overflow, internal
/// watcher error) must never be dropped silently: it means events may have
/// been lost, so the only safe answer is a full rescan.
pub fn classify(result: notify::Result<Event>, root: &Path) -> Option<FsBatch> {
    let Ok(event) = result else {
        return Some(FsBatch::rescan());
    };
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return None;
    }

    let mut batch = FsBatch::default();
    match event.kind {
        EventKind::Remove(RemoveKind::Folder) => return Some(FsBatch::rescan()),
        EventKind::Remove(_) => {
            for path in event.paths {
                if is_hidden(root, &path) {
                    continue;
                }
                batch.deleted.push(path);
            }
        }
        EventKind::Create(kind) => match kind {
            CreateKind::Folder => return Some(FsBatch::rescan()),
            _ => {
                for path in event.paths {
                    if is_hidden(root, &path) {
                        continue;
                    }
                    if is_note(&path) {
                        batch.modified.push(path);
                    }
                }
            }
        },
        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            RenameMode::Both => {
                let mut paths = event.paths.into_iter();
                if let (Some(from), Some(to)) = (paths.next(), paths.next()) {
                    if is_our_atomic_tmp(&from) {
                        if is_note(&to) {
                            batch.modified.push(to);
                        }
                    } else if looks_like_folder(&to) {
                        return Some(FsBatch::rescan());
                    } else {
                        batch.renamed.push((from, to));
                    }
                }
            }
            RenameMode::From => {
                for path in event.paths {
                    if is_hidden(root, &path) {
                        continue;
                    }
                    batch.deleted.push(path);
                }
            }
            RenameMode::To => {
                for path in event.paths {
                    if is_hidden(root, &path) || !is_note(&path) {
                        continue;
                    }
                    batch.modified.push(path);
                }
            }
            RenameMode::Any | RenameMode::Other => {
                let mut paths = event.paths.into_iter();
                match (paths.next(), paths.next()) {
                    (Some(from), Some(to)) => {
                        if is_our_atomic_tmp(&from) {
                            if is_note(&to) {
                                batch.modified.push(to);
                            }
                        } else if looks_like_folder(&to) {
                            return Some(FsBatch::rescan());
                        } else {
                            batch.renamed.push((from, to));
                        }
                    }
                    (Some(single), None) => {
                        if !is_hidden(root, &single) && is_note(&single) {
                            batch.modified.push(single);
                        }
                    }
                    (None, _) => {}
                }
            }
        },
        EventKind::Modify(_) => {
            for path in event.paths {
                if is_hidden(root, &path) || !is_note(&path) {
                    continue;
                }
                batch.modified.push(path);
            }
        }
        // Access/Other are filtered above; Any carries no reliable structure,
        // so nothing safe can be concluded from it.
        EventKind::Any => {}
        EventKind::Access(_) | EventKind::Other => {}
    }

    if batch.is_empty() {
        None
    } else {
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::DataChange;
    use std::path::PathBuf;

    fn event(kind: EventKind, paths: &[&str]) -> notify::Result<Event> {
        Ok(Event {
            kind,
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: Default::default(),
        })
    }

    fn written(paths: &[&str]) -> notify::Result<Event> {
        event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths,
        )
    }

    #[test]
    fn ignores_reads() {
        let read = EventKind::Access(notify::event::AccessKind::Read);
        assert!(classify(event(read, &["a.md"]), Path::new("")).is_none());
    }

    #[test]
    fn a_write_to_a_note_is_a_modification_and_an_image_is_ignored() {
        let batch = classify(written(&["заметка.md"]), Path::new("")).unwrap();
        assert_eq!(batch.modified, vec![PathBuf::from("заметка.md")]);

        assert!(classify(written(&["cover.png"]), Path::new("")).is_none());
    }

    #[test]
    fn hidden_paths_never_surface() {
        assert!(classify(written(&[".foo.md"]), Path::new("")).is_none());
        assert!(classify(
            event(EventKind::Remove(RemoveKind::File), &[".foo.md.tmp"]),
            Path::new("")
        )
        .is_none());
    }

    #[test]
    fn file_removal_is_a_deletion_and_folder_removal_is_a_rescan() {
        let batch = classify(
            event(EventKind::Remove(RemoveKind::File), &["заметка.md"]),
            Path::new(""),
        )
        .unwrap();
        assert_eq!(batch.deleted, vec![PathBuf::from("заметка.md")]);

        let batch = classify(
            event(EventKind::Remove(RemoveKind::Folder), &["Проекты"]),
            Path::new(""),
        )
        .unwrap();
        assert!(batch.rescan);
    }

    #[test]
    fn a_new_note_is_a_modification_and_a_new_folder_is_a_rescan() {
        let batch = classify(
            event(EventKind::Create(CreateKind::File), &["новая.md"]),
            Path::new(""),
        )
        .unwrap();
        assert_eq!(batch.modified, vec![PathBuf::from("новая.md")]);

        let batch = classify(
            event(EventKind::Create(CreateKind::Folder), &["Проекты"]),
            Path::new(""),
        )
        .unwrap();
        assert!(batch.rescan);
    }

    #[test]
    fn our_atomic_save_rename_becomes_a_modification_not_a_rename() {
        let pid = std::process::id();
        let tmp = format!(".заметка.md.{pid}.tmp");
        let batch = classify(
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &[tmp.as_str(), "заметка.md"],
            ),
            Path::new(""),
        )
        .unwrap();
        assert!(batch.renamed.is_empty());
        assert_eq!(batch.modified, vec![PathBuf::from("заметка.md")]);
    }

    #[test]
    fn a_foreign_rename_stays_a_rename() {
        let batch = classify(
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["старое.md", "новое.md"],
            ),
            Path::new(""),
        )
        .unwrap();
        assert_eq!(
            batch.renamed,
            vec![(PathBuf::from("старое.md"), PathBuf::from("новое.md"))]
        );
        assert!(batch.modified.is_empty() && batch.deleted.is_empty());
    }

    #[test]
    fn a_watcher_error_asks_for_a_rescan_instead_of_being_dropped() {
        let batch = classify(Err(notify::Error::generic("queue overflow")), Path::new("")).unwrap();
        assert!(batch.rescan);
    }
}
