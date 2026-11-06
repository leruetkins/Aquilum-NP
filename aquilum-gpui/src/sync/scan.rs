use crate::settings::SyncSettings;
use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalFile {
    pub size: u64,
    pub modified: u64,
}

pub const UNKNOWN: LocalFile = LocalFile { size: 0, modified: 0 };

const HIDDEN_NAMES: [&str; 6] = [".vault", ".obsidian", ".git", ".trash", ".DS_Store", "target"];

pub fn is_hidden_name(name: &str) -> bool {
    HIDDEN_NAMES.iter().any(|hidden| *hidden == name)
        || name.starts_with("._")
        || name == "node_modules"
}

pub fn is_hidden(relative: &str) -> bool {
    relative.split('/').any(is_hidden_name)
}

impl SyncSettings {
    pub fn included(&self, relative: &str) -> bool {
        let mut parts = relative.split('/');
        let first = parts.next().unwrap_or_default();
        if first == ".vault" {
            return self.history && parts.next() == Some("history");
        }
        if first == ".trash" {
            return self.trash;
        }
        !is_hidden(relative)
    }

    pub fn fingerprint(&self) -> String {
        format!("trash={};history={}", self.trash, self.history)
    }

    fn areas(&self, root: &Path) -> Vec<PathBuf> {
        let mut areas = Vec::new();
        if self.trash {
            areas.push(root.join(".trash"));
        }
        if self.history {
            areas.push(root.join(".vault").join("history"));
        }
        areas
    }
}

pub fn service(relative: &str) -> bool {
    let mut parts = relative.split('/');
    match parts.next() {
        Some(".trash") => true,
        Some(".vault") => parts.next() == Some("history"),
        _ => false,
    }
}

pub fn scan(root: &Path, scope: SyncSettings) -> io::Result<HashMap<String, LocalFile>> {
    let mut files = HashMap::new();
    walk(root, root, |relative| scope.included(relative), &mut files)?;
    for area in scope.areas(root).iter().filter(|area| area.is_dir()) {
        walk(root, area, |relative| scope.included(relative), &mut files)?;
    }
    Ok(files)
}

fn walk(
    root: &Path,
    from: &Path,
    keep: impl Fn(&str) -> bool,
    files: &mut HashMap<String, LocalFile>,
) -> io::Result<()> {
    let mut stack = vec![from.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let relative = relative(root, &path);
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_dir() {
                if !is_hidden_name(&name) {
                    stack.push(path);
                }
            } else if entry.file_type()?.is_file() && keep(&relative) {
                files.insert(relative, stat(&entry.metadata()?)?);
            }
        }
    }
    Ok(())
}

pub fn stat(metadata: &Metadata) -> io::Result<LocalFile> {
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0);
    Ok(LocalFile { size: metadata.len(), modified })
}

pub fn stamp(path: &Path, modified: u64) -> io::Result<()> {
    if modified == 0 {
        return Ok(());
    }
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(UNIX_EPOCH + Duration::from_millis(modified))
}

pub fn relative(root: &Path, path: &Path) -> String {
    let parts: Vec<_> = path.strip_prefix(root).unwrap_or(path).iter().map(|part| part.to_string_lossy()).collect();
    parts.join("/")
}

/// Joins a relative path below `root`, dropping empty, `.` and `..`
/// components so a malformed remote path cannot escape the vault.
pub fn absolute(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SyncSettings;

    fn scope(trash: bool) -> SyncSettings {
        SyncSettings { enabled: true, trash, history: true, interval: 300 }
    }

    #[test]
    fn hidden_service_folders_stay_out_and_attachments_stay_in() {
        let root = tempfile::tempdir().unwrap();
        for path in [
            ".vault/data.json",
            ".vault/history/h.md",
            ".obsidian/app.json",
            ".trash/old.md",
            "attachments/pic.png",
            "notes/a.md",
        ] {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "x").unwrap();
        }
        let files = scan(root.path(), scope(false)).unwrap();
        assert!(files.contains_key("notes/a.md"));
        assert!(files.contains_key("attachments/pic.png"));
        assert!(files.contains_key(".vault/history/h.md"));
        assert!(!files.contains_key(".vault/data.json"));
        assert!(!files.contains_key(".obsidian/app.json"));
        assert!(!files.contains_key(".trash/old.md"));

        let files = scan(root.path(), scope(true)).unwrap();
        assert!(files.contains_key(".trash/old.md"));

        let no_history = SyncSettings { history: false, ..scope(false) };
        assert!(!scan(root.path(), no_history).unwrap().contains_key(".vault/history/h.md"));
    }

    #[test]
    fn paths_cannot_escape_the_vault() {
        let root = Path::new("C:/vault");
        assert_eq!(absolute(root, "a/../../b.md"), root.join("a").join("b.md"));
        assert!(service(".trash/x.md"));
        assert!(service(".vault/history/x.md"));
        assert!(!service("notes/x.md"));
    }
}
