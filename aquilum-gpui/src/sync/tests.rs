use crate::settings::SyncSettings;
use crate::sync::cycle::{self, Outcome, Vault, Watch};
use crate::sync::hash::dropbox_hash;
use crate::sync::records::{Large, Records, Size};
use crate::sync::remote::{Content, Delta, Remote, RemoteError, RemoteFile, RemoteResult};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
struct State {
    files: BTreeMap<String, (Vec<u8>, String, u64)>,
    revisions: HashMap<String, Vec<u8>>,
    log: Vec<String>,
    next: u64,
    uploads: usize,
    expired: bool,
    refuse_uploads: bool,
    short_downloads: bool,
    offline: bool,
    slow: Duration,
    downloads: Vec<String>,
}

#[derive(Default)]
struct Fake {
    state: Mutex<State>,
}

impl Fake {
    fn put(&self, path: &str, text: &str) {
        self.store(path, text.as_bytes().to_vec());
    }

    fn put_bytes(&self, path: &str, bytes: &[u8]) {
        self.store(path, bytes.to_vec());
    }

    /// Stores content without adding it to the change log, so the next delta
    /// misses it and the engine must recover through a full listing.
    fn put_quietly(&self, path: &str, text: &str) {
        self.store(path, text.as_bytes().to_vec());
        self.state.lock().unwrap().log.pop();
    }

    fn store(&self, path: &str, bytes: Vec<u8>) {
        let mut state = self.state.lock().unwrap();
        state.next += 1;
        let rev = format!("r{}", state.next);
        state.revisions.insert(rev.clone(), bytes.clone());
        state.files.insert(path.to_owned(), (bytes, rev, 0));
        state.log.push(path.to_owned());
    }

    fn text(&self, path: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .files
            .get(path)
            .map(|(bytes, _, _)| String::from_utf8(bytes.clone()).unwrap())
    }

    fn paths(&self) -> Vec<String> {
        self.state.lock().unwrap().files.keys().cloned().collect()
    }

    fn uploads(&self) -> usize {
        self.state.lock().unwrap().uploads
    }

    fn refuse_uploads(&self, refuse: bool) {
        self.state.lock().unwrap().refuse_uploads = refuse;
    }

    fn short_downloads(&self, short: bool) {
        self.state.lock().unwrap().short_downloads = short;
    }

    fn go_offline(&self) {
        self.state.lock().unwrap().offline = true;
    }

    fn slow_transfers(&self, slow: Duration) {
        self.state.lock().unwrap().slow = slow;
    }

    fn downloads(&self) -> Vec<String> {
        self.state.lock().unwrap().downloads.clone()
    }

    fn remove_folder(&self, folder: &str) {
        let mut state = self.state.lock().unwrap();
        state.files.retain(|path, _| !path.starts_with(&format!("{folder}/")));
        state.log.push(folder.to_owned());
    }

    fn wipe_and_expire(&self) {
        let mut state = self.state.lock().unwrap();
        state.files.clear();
        state.revisions.clear();
        state.expired = true;
    }

    fn online(&self) -> RemoteResult<std::sync::MutexGuard<'_, State>> {
        let state = self.state.lock().unwrap();
        if state.offline {
            Err(RemoteError::Offline("no network".into()))
        } else {
            Ok(state)
        }
    }

    fn transfer<T>(&self, work: impl FnOnce() -> RemoteResult<T>) -> RemoteResult<T> {
        let slow = self.online()?.slow;
        std::thread::sleep(slow);
        work()
    }
}

fn meta(path: &str, bytes: &[u8], rev: &str, modified: u64) -> RemoteFile {
    RemoteFile {
        path: path.to_owned(),
        id: format!("id:{path}"),
        rev: rev.to_owned(),
        checksum: dropbox_hash(&bytes).unwrap(),
        size: bytes.len() as u64,
        modified,
    }
}

impl Remote for Fake {
    fn cursor(&self) -> RemoteResult<String> {
        Ok(self.online()?.log.len().to_string())
    }

    fn list(&self) -> RemoteResult<Vec<RemoteFile>> {
        let state = self.online()?;
        Ok(state
            .files
            .iter()
            .map(|(path, (bytes, rev, modified))| meta(path, bytes, rev, *modified))
            .collect())
    }

    fn delta(&self, cursor: &str) -> RemoteResult<Option<Delta>> {
        let mut state = self.online()?;
        if std::mem::take(&mut state.expired) {
            return Ok(None);
        }
        let from: usize = cursor.parse().map_err(|_| RemoteError::Failed("cursor".into()))?;
        if from > state.log.len() {
            return Ok(None);
        }
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        let touched: BTreeSet<&String> = state.log[from..].iter().collect();
        for path in touched {
            if let Some((bytes, rev, modified)) = state.files.get(path) {
                changed.push(meta(path, bytes, rev, *modified));
            } else {
                removed.push(path.clone());
            }
        }
        Ok(Some(Delta { changed, removed, cursor: state.log.len().to_string() }))
    }

    fn download(&self, file: &RemoteFile, out: &mut dyn Write) -> RemoteResult<()> {
        self.transfer(|| {
            let mut state = self.online()?;
            state.downloads.push(file.path.clone());
            let bytes = state.revisions.get(&file.rev).ok_or(RemoteError::Missing)?.clone();
            let length = if state.short_downloads { bytes.len() / 2 } else { bytes.len() };
            drop(state);
            Ok(out.write_all(&bytes[..length])?)
        })
    }

    fn upload(
        &self,
        path: &str,
        content: &dyn Content,
        modified: u64,
        replaces: Option<&RemoteFile>,
    ) -> RemoteResult<RemoteFile> {
        self.transfer(|| {
            let mut bytes = Vec::with_capacity(content.byte_len());
            content.read(0..content.byte_len(), &mut |block| {
                bytes.extend_from_slice(block);
                Ok(())
            })?;
            let mut state = self.online()?;
            if state.refuse_uploads {
                return Err(RemoteError::Failed("cloud unavailable".into()));
            }
            let current = state.files.get(path).map(|(_, rev, _)| rev.as_str());
            if current != replaces.map(|file| file.rev.as_str()) {
                return Err(RemoteError::Stale);
            }
            state.uploads += 1;
            state.next += 1;
            let rev = format!("r{}", state.next);
            state.revisions.insert(rev.clone(), bytes.clone());
            state.files.insert(path.to_owned(), (bytes.clone(), rev.clone(), modified));
            state.log.push(path.to_owned());
            Ok(meta(path, &bytes, &rev, modified))
        })
    }

    fn delete(&self, file: &RemoteFile) -> RemoteResult<()> {
        let mut state = self.online()?;
        if state.files.get(&file.path).map(|(_, rev, _)| rev.as_str()) != Some(file.rev.as_str()) {
            return Err(RemoteError::Stale);
        }
        state.files.remove(&file.path);
        state.log.push(file.path.clone());
        Ok(())
    }

    fn checksum(&self, content: &dyn Content) -> io::Result<String> {
        dropbox_hash(content)
    }
}

#[derive(Default)]
struct Probe {
    measured: Mutex<Option<(Vec<Large>, Size)>>,
    moves: Mutex<Vec<crate::sync::progress::Progress>>,
    until: Mutex<Option<usize>>,
}

impl Watch for Probe {
    fn measured(&self, large: Vec<Large>, size: Size) {
        *self.measured.lock().unwrap() = Some((large, size));
    }
    fn progress(&self, progress: crate::sync::progress::Progress) {
        self.moves.lock().unwrap().push(progress);
    }
    fn stopped(&self) -> bool {
        let done = self.moves.lock().unwrap().last().map_or(0, |progress| progress.done);
        self.until.lock().unwrap().is_some_and(|until| done >= until)
    }
}

const SCOPE: SyncSettings = SyncSettings {
    enabled: true,
    trash: false,
    history: false,
    interval: 300,
    remote_folder: None,
};

struct Device {
    root: PathBuf,
    records: Records,
    scope: SyncSettings,
    allow_deletions: std::cell::Cell<bool>,
    allowed: std::cell::RefCell<Vec<String>>,
    ignored: std::cell::RefCell<Vec<String>>,
    probe: Probe,
    directory: tempfile::TempDir,
}

impl Device {
    fn new() -> Self {
        Self::with(SCOPE)
    }

    fn with(scope: SyncSettings) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root,
            records: Records::memory().unwrap(),
            scope,
            allow_deletions: std::cell::Cell::new(false),
            allowed: std::cell::RefCell::new(Vec::new()),
            ignored: std::cell::RefCell::new(Vec::new()),
            probe: Probe::default(),
            directory,
        }
    }

    fn write(&self, path: &str, text: &str) {
        self.write_bytes(path, text.as_bytes());
    }

    fn write_bytes(&self, path: &str, bytes: &[u8]) {
        let file = self.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    fn delete(&self, path: &str) {
        std::fs::remove_file(self.root.join(path)).unwrap();
    }

    fn trash(&self, path: &str) {
        crate::trash::trash_note(&self.root, &self.root.join(path), path).unwrap();
    }

    fn allow(&self, paths: &[&str]) {
        self.allowed.borrow_mut().extend(paths.iter().map(|path| (*path).to_owned()));
    }

    fn ignore(&self, path: &str) {
        self.ignored.borrow_mut().push(path.to_owned());
    }

    fn modified(&self, path: &str) -> u64 {
        crate::sync::scan::stat(&std::fs::metadata(self.root.join(path)).unwrap())
            .unwrap()
            .modified
    }

    fn set_modified(&self, path: &str, millis: u64) {
        crate::sync::scan::stamp(&self.root.join(path), millis).unwrap();
    }

    fn staging(&self) -> PathBuf {
        self.directory.path().join("staging")
    }

    fn try_sync(&self, remote: &dyn Remote) -> Result<Outcome, crate::sync::SyncError> {
        self.probe.moves.lock().unwrap().clear();
        let staging = self.staging();
        let vault = Vault {
            root: &self.root,
            remote,
            records: &self.records,
            staging: &staging,
            scope: self.scope.clone(),
            watch: &self.probe,
            allow_deletions: self.allow_deletions.get(),
            allowed: &self.allowed.borrow(),
            ignored: &self.ignored.borrow(),
            device: "test",
            language: "ru",
        };
        cycle::run(&vault)
    }

    fn sync(&self, remote: &dyn Remote, allow_deletions: bool) -> Result<Outcome, crate::sync::SyncError> {
        self.allow_deletions.set(allow_deletions);
        self.try_sync(remote)
    }

    fn sync_until(&self, remote: &dyn Remote, done: usize) -> Outcome {
        *self.probe.until.lock().unwrap() = Some(done);
        let outcome = self.try_sync(remote).unwrap();
        *self.probe.until.lock().unwrap() = None;
        outcome
    }

    fn files(&self) -> Vec<String> {
        fn walk(root: &Path, directory: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if crate::sync::scan::is_hidden_name(&name) {
                    continue;
                }
                if path.is_dir() {
                    walk(root, &path, out);
                } else if path.is_file() {
                    out.push(crate::sync::scan::relative(root, &path));
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.root, &self.root, &mut files);
        files.sort();
        files
    }

    fn kept_text(&self) -> String {
        fn walk(root: &Path, directory: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == ".vault" || name == ".git" || name == ".obsidian" {
                    continue;
                }
                if path.is_dir() {
                    walk(root, &path, out);
                } else if path.is_file() {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.root, &self.root, &mut files);
        files
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn copies(&self) -> Vec<String> {
        self.files().into_iter().filter(|path| path.contains("(конфликт")).collect()
    }

    fn trashed(&self) -> bool {
        !self.trash_text().is_empty()
    }

    fn trash_text(&self) -> String {
        fn walk(directory: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.is_file() {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.root.join(".trash"), &mut files);
        files
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .collect::<Vec<_>>()
            .join("|")
    }
}

#[test]
fn first_sync_sends_local_files_and_pulls_remote_ones() {
    let remote = Fake::default();
    remote.put("Облако.md", "из облака");
    let device = Device::new();
    device.write("Папка/Здесь.md", "отсюда");

    let outcome = device.sync(&remote, false).unwrap();
    assert!(outcome.failed.is_empty());
    assert_eq!(remote.text("Папка/Здесь.md").as_deref(), Some("отсюда"));
    assert_eq!(device.read("Облако.md").as_deref(), Some("из облака"));
}

#[test]
fn a_quiet_second_sync_does_nothing() {
    let remote = Fake::default();
    let device = Device::new();
    device.write("Заметка.md", "текст");
    device.sync(&remote, false).unwrap();
    let before = remote.uploads();
    let outcome = device.sync(&remote, false).unwrap();
    assert_eq!((outcome.progress.total, outcome.failed.len(), remote.uploads()), (0, 0, before));
}

#[test]
fn edits_travel_between_devices() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Идея.md", "черновик");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    phone.write("Идея.md", "черновик, дописанный с телефона");
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();
    assert_eq!(laptop.read("Идея.md").as_deref(), Some("черновик, дописанный с телефона"));
}

#[test]
fn edits_on_both_sides_merge_line_by_line() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Список.md", "один\nдва\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    laptop.write("Список.md", "ОДИН\nдва\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.write("Список.md", "один\nдва\nтри!\n");
    assert!(phone.sync(&remote, false).unwrap().failed.is_empty());
    laptop.sync(&remote, false).unwrap();
    assert_eq!(phone.read("Список.md").as_deref(), Some("ОДИН\nдва\nтри!\n"));
    assert_eq!(remote.text("Список.md").as_deref(), Some("ОДИН\nдва\nтри!\n"));
}

#[test]
fn deleting_most_files_waits_for_confirmation() {
    let remote = Fake::default();
    let device = Device::new();
    for index in 0..10 {
        device.write(&format!("Заметка {index}.md"), "текст");
    }
    device.sync(&remote, false).unwrap();
    for index in 0..6 {
        device.delete(&format!("Заметка {index}.md"));
    }
    assert!(matches!(device.sync(&remote, false), Err(crate::sync::SyncError::Deletions { planned: 6, synced: 10 })));
    assert_eq!(remote.state.lock().unwrap().files.len(), 10);
}

#[test]
fn an_empty_cloud_after_a_reset_does_not_delete_local_files() {
    let remote = Fake::default();
    let device = Device::new();
    device.write("Заметка.md", "текст");
    device.sync(&remote, false).unwrap();
    remote.wipe_and_expire();
    assert!(device.sync(&remote, false).is_err());
    assert_eq!(device.read("Заметка.md").as_deref(), Some("текст"));
}

#[test]
fn deletions_go_to_the_trash_on_the_other_side() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Первая.md", "один");
    laptop.write("Вторая.md", "два");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();

    phone.delete("Первая.md");
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();
    laptop.delete("Вторая.md");
    laptop.sync(&remote, false).unwrap();

    assert_eq!(laptop.files(), Vec::<String>::new());
    assert!(laptop.trashed());
    assert_eq!(remote.paths(), Vec::<String>::new());
}

#[test]
fn an_edit_wins_over_a_deletion() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "было");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();

    laptop.delete("Заметка.md");
    laptop.sync(&remote, false).unwrap();
    phone.write("Заметка.md", "было и стало");
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();

    assert_eq!(remote.text("Заметка.md").as_deref(), Some("было и стало"));
    assert_eq!(laptop.read("Заметка.md").as_deref(), Some("было и стало"));
}

#[test]
fn without_a_common_version_both_texts_survive() {
    let remote = Fake::default();
    remote.put("Спор.md", "из облака");
    let device = Device::new();
    device.write("Спор.md", "с устройства");

    let outcome = device.sync(&remote, false).unwrap();

    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    assert_eq!(device.read("Спор.md").as_deref(), Some("с устройства"));
    assert_eq!(device.copies().len(), 1);
    let copy = device.copies().into_iter().next().unwrap();
    assert_eq!(device.read(&copy).as_deref(), Some("из облака"));
}

#[test]
fn equal_files_meet_without_uploading() {
    let remote = Fake::default();
    remote.put("Равно.md", "одинаково");
    let device = Device::new();
    device.write("Равно.md", "одинаково");

    let outcome = device.sync(&remote, false).unwrap();

    assert!(outcome.failed.is_empty());
    assert_eq!(remote.uploads(), 0);
}

#[test]
fn a_failed_upload_keeps_the_cursor_and_the_next_sync_finishes() {
    let remote = Fake::default();
    remote.put("Заметка.md", "из облака");
    let device = Device::new();
    device.write("Заметка.md", "с устройства");
    remote.refuse_uploads(true);
    assert!(!device.sync(&remote, false).unwrap().failed.is_empty());

    remote.refuse_uploads(false);
    let second = device.sync(&remote, false).unwrap();
    assert!(second.failed.is_empty(), "{:?}", second.failed);
    assert_eq!(remote.text("Заметка.md").as_deref(), Some("с устройства"));
}

#[test]
fn an_edit_with_the_same_size_and_time_is_not_stuck_forever() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "abc\n");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    let synced = laptop.modified("Заметка.md");
    laptop.write("Заметка.md", "abd\n");
    laptop.set_modified("Заметка.md", synced);

    phone.write("Заметка.md", "abc\nс телефона\n");
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();
    let last = laptop.sync(&remote, false).unwrap();

    assert!(last.failed.is_empty(), "{:?}", last.failed);
    let text = laptop.read("Заметка.md").unwrap();
    assert!(text.contains("с телефона"));
    assert!(text.contains("abd") || laptop.copies().iter().any(|copy| laptop.read(copy).unwrap().contains("abd")));
}

#[test]
fn a_cloud_file_missing_from_the_changes_is_found_by_a_full_listing() {
    let remote = Fake::default();
    let device = Device::new();
    device.write("Старая.md", "есть");
    device.sync(&remote, false).unwrap();

    remote.put_quietly("Новая.md", "из облака");
    device.write("Новая.md", "с устройства");
    assert_eq!(device.sync(&remote, false).unwrap().failed.len(), 1);
    let second = device.sync(&remote, false).unwrap();

    assert!(second.failed.is_empty(), "{:?}", second.failed);
    assert_eq!(device.read("Новая.md").as_deref(), Some("с устройства"));
    assert_eq!(
        device.copies().iter().map(|copy| device.read(copy).unwrap()).collect::<Vec<_>>(),
        ["из облака"]
    );
}

#[test]
fn a_failed_upload_leaves_no_extra_conflict_copies() {
    let remote = Fake::default();
    remote.put("Спор.md", "из облака");
    let device = Device::new();
    device.write("Спор.md", "с устройства");

    remote.refuse_uploads(true);
    device.sync(&remote, false).unwrap();
    device.sync(&remote, false).unwrap();
    assert!(device.copies().is_empty());
    remote.refuse_uploads(false);
    device.sync(&remote, false).unwrap();

    assert_eq!(device.copies().len(), 1);
}

#[test]
fn identical_edits_on_both_devices_make_no_copy() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "один\nдва\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();

    laptop.write("Заметка.md", "один\nДВА\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.write("Заметка.md", "один\nДВА\nтри\nчетыре\n");
    phone.sync(&remote, false).unwrap();

    assert_eq!(phone.read("Заметка.md").as_deref(), Some("один\nДВА\nтри\nчетыре\n"));
    assert!(phone.copies().is_empty());
}

#[test]
fn a_note_that_is_not_utf8_keeps_both_versions() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "текст");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();

    laptop.write_bytes("Заметка.md", &[0xFF, 0xFE, 0x01]);
    laptop.sync(&remote, false).unwrap();
    phone.write_bytes("Заметка.md", &[0xFF, 0xFE, 0x02, 0x03]);
    let outcome = phone.sync(&remote, false).unwrap();

    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    assert_eq!(phone.copies().len(), 1);
}

#[test]
fn a_truncated_base_version_never_reaches_the_note() {
    let remote = Fake::default();
    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "один\nдва\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    laptop.write("Заметка.md", "ОДИН\nдва\nтри\n");
    laptop.sync(&remote, false).unwrap();
    phone.write("Заметка.md", "один\nдва\nтри!\n");

    remote.short_downloads(true);
    assert!(!phone.sync(&remote, false).unwrap().failed.is_empty());
    assert_eq!(phone.read("Заметка.md").as_deref(), Some("один\nдва\nтри!\n"));
    remote.short_downloads(false);
    phone.sync(&remote, false).unwrap();

    assert_eq!(phone.read("Заметка.md").as_deref(), Some("ОДИН\nдва\nтри!\n"));
}

#[test]
fn a_download_left_by_a_crash_is_cleared_by_the_next_pass() {
    let remote = Fake::default();
    let device = Device::new();
    std::fs::create_dir_all(device.staging()).unwrap();
    std::fs::write(device.staging().join("interrupted"), "part").unwrap();

    device.sync(&remote, false).unwrap();

    assert!(!device.staging().join("interrupted").exists());
}

#[test]
fn a_folder_deleted_in_the_cloud_takes_its_notes_along() {
    let remote = Fake::default();
    let device = Device::new();
    device.write("Папка/Первая.md", "один");
    device.write("Папка/Вложенная/Вторая.md", "два");
    device.write("Остаётся.md", "три");
    device.sync(&remote, false).unwrap();

    remote.remove_folder("Папка");
    device.sync(&remote, false).unwrap();

    assert_eq!(device.files(), vec!["Остаётся.md".to_owned()]);
    assert!(device.trashed());
}

#[test]
fn a_stopped_pass_starts_no_new_transfers_and_the_next_one_finishes() {
    let remote = Fake::default();
    remote.slow_transfers(Duration::from_millis(20));
    let device = Device::new();
    for index in 0..20 {
        device.write(&format!("Заметка {index}.md"), "текст");
    }

    let stopped = device.sync_until(&remote, 1);

    assert!(stopped.stopped);
    assert!((1..=crate::sync::transfer::TRANSFERS).contains(&remote.paths().len()), "{}", remote.paths().len());
    assert_eq!(device.records.state("cursor").unwrap(), None);
    let finished = device.sync(&remote, false).unwrap();
    assert!(!finished.stopped && finished.failed.is_empty());
    assert_eq!(remote.paths().len(), 20);
}

#[test]
fn an_emptied_vault_folder_deletes_nothing_in_the_cloud() {
    let remote = Fake::default();
    let device = Device::new();
    for index in 0..10 {
        device.write(&format!("Папка/Заметка {index}.md"), "текст");
    }
    device.sync(&remote, false).unwrap();
    std::fs::remove_dir_all(device.root.join("Папка")).unwrap();

    assert!(device.try_sync(&remote).is_err());
    assert_eq!(remote.paths().len(), 10);
}

#[test]
fn a_large_file_stays_on_the_device_and_is_listed() {
    use crate::sync::cycle::LARGE;
    let remote = Fake::default();
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize + 1));
    device.write("Заметка.md", "текст");

    let outcome = device.sync(&remote, false).unwrap();

    assert!(outcome.failed.is_empty());
    assert_eq!(remote.paths(), vec!["Заметка.md".to_owned()]);
    assert!(device.read("Видео.mp4").is_some());
    let (large, _) = device.probe.measured.lock().unwrap().clone().unwrap();
    assert!(large.iter().any(|large| large.path == "Видео.mp4" && large.kept));
}

#[test]
fn a_large_file_in_the_cloud_is_downloaded_and_listed_as_in_the_cloud() {
    use crate::sync::cycle::LARGE;
    let remote = Fake::default();
    remote.put_bytes("Видео.mp4", &vec![b'v'; LARGE as usize + 1]);
    let device = Device::new();

    device.sync(&remote, false).unwrap();
    device.sync(&remote, false).unwrap();

    assert!(device.root.join("Видео.mp4").exists());
    let (large, _) = device.probe.measured.lock().unwrap().clone().unwrap();
    assert!(large.iter().any(|large| large.path == "Видео.mp4" && !large.kept));
}

#[test]
fn an_ignored_large_file_stays_on_disk_and_out_of_the_cloud() {
    use crate::sync::cycle::LARGE;
    let remote = Fake::default();
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize + 1));
    device.ignore("Видео.mp4");

    let outcome = device.sync(&remote, false).unwrap();

    assert!(outcome.failed.is_empty());
    assert!(remote.paths().is_empty());
    assert!(device.read("Видео.mp4").is_some());
    let (large, size) = device.probe.measured.lock().unwrap().clone().unwrap();
    assert_eq!(large.len(), 1);
    assert!(large[0].ignored);
    assert_eq!(size.synced, 0);
}

#[test]
fn sending_a_large_file_by_hand_uploads_it() {
    use crate::sync::cycle::LARGE;
    let remote = Fake::default();
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize + 1));
    device.sync(&remote, false).unwrap();
    assert!(remote.paths().is_empty());

    device.allow(&["Видео.mp4"]);
    device.sync(&remote, false).unwrap();

    assert_eq!(remote.paths(), vec!["Видео.mp4".to_owned()]);
}

struct Random(u64);

impl Random {
    fn next(&mut self, limit: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (self.0 >> 33) % limit.max(1)
    }
}

fn play_random(seed: u64) {
    const NOTES: [&str; 4] = ["Один.md", "Два.md", "Папка/Три.md", "Папка/Четыре.md"];
    let mut random = Random(seed);
    let remote = Fake::default();
    let devices = [Device::new(), Device::new(), Device::new()];
    let mut typed = Vec::new();

    for step in 0..40 {
        let device = &devices[random.next(3) as usize];
        let note = NOTES[random.next(NOTES.len() as u64) as usize];
        match random.next(10) {
            0..=4 => {
                let line = format!("строка {seed}-{step}");
                let mut lines: Vec<String> = device.read(note).map(|text| text.lines().map(str::to_owned).collect()).unwrap_or_default();
                let at = random.next(lines.len() as u64 + 1) as usize;
                lines.insert(at, line.clone());
                device.write(note, &(lines.join("\n") + "\n"));
                typed.push(line);
            }
            5 if device.read(note).is_some() => device.trash(note),
            _ => {
                remote.refuse_uploads(random.next(5) == 0);
                device.sync(&remote, false).unwrap();
            }
        }
    }
    remote.refuse_uploads(false);
    let quiet = (0..10).any(|_| {
        let outcomes: Vec<_> = devices.iter().map(|device| device.sync(&remote, false).unwrap()).collect();
        outcomes.iter().all(|outcome| outcome.failed.is_empty() && outcome.progress.total == 0)
    });
    assert!(quiet, "seed {seed}: devices did not settle");

    for device in &devices {
        assert_eq!(device.files(), remote.paths(), "seed {seed}: files diverged");
        for path in device.files() {
            assert_eq!(device.read(&path), remote.text(&path), "seed {seed}: text differs for {path}");
        }
    }
    let kept: String = devices.iter().map(Device::kept_text).collect::<Vec<_>>().join("\n");
    for line in typed {
        if !kept.lines().any(|kept| kept == line) {
            for (index, device) in devices.iter().enumerate() {
                eprintln!(
                    "seed {seed} device {index} files={:?} copies={:?} kept={:?}",
                    device.files(),
                    device.copies(),
                    device.kept_text()
                );
            }
            eprintln!("remote={:?}", remote.paths());
            panic!("seed {seed}: lost line {line}");
        }
    }
}

#[test]
fn many_devices_converge_and_lose_no_line() {
    for seed in 1..=5 {
        play_random(seed);
    }
}

#[test]
#[ignore]
fn many_devices_converge_and_lose_no_line_long() {
    for seed in 6..=30 {
        play_random(seed);
    }
}

#[test]
fn an_offline_cloud_keeps_the_local_file() {
    let remote = Fake::default();
    remote.go_offline();
    let device = Device::new();
    device.write("Заметка.md", "локально");

    let result = device.sync(&remote, false);

    assert!(matches!(
        result,
        Err(crate::sync::SyncError::Remote(crate::sync::RemoteError::Offline(_)))
    ));
    assert_eq!(device.read("Заметка.md").as_deref(), Some("локально"));
    assert!(remote.downloads().is_empty());
}

#[test]
fn a_lost_network_is_not_an_error() {
    let mut state = crate::sync::state::State::default();
    state.settle(Err(crate::sync::RemoteError::Offline("dns".into()).into()));
    assert_eq!(state.status(true), crate::sync::SyncStatus::Offline);

    let mut state = crate::sync::state::State::default();
    state.settle(Ok(Outcome {
        failed: vec![
            ("Один.md".into(), crate::sync::SyncError::Remote(crate::sync::RemoteError::Offline("dns".into()))),
            ("Два.md".into(), crate::sync::SyncError::Remote(crate::sync::RemoteError::Offline("dns".into()))),
        ],
        ..Outcome::default()
    }));
    assert_eq!(state.status(true), crate::sync::SyncStatus::Offline);
}

#[test]
fn file_errors_are_summed_up_with_the_first_real_one() {
    let mut state = crate::sync::state::State::default();
    state.settle(Ok(Outcome {
        failed: vec![
            ("Один.md".into(), crate::sync::SyncError::Remote(crate::sync::RemoteError::Offline("dns".into()))),
            ("Два.md".into(), crate::sync::SyncError::Remote(crate::sync::RemoteError::Failed("Dropbox 400: bad\nmore".into()))),
            ("Три.md".into(), crate::sync::SyncError::Remote(crate::sync::RemoteError::Stale)),
        ],
        ..Outcome::default()
    }));
    assert_eq!(
        state.status(true),
        crate::sync::SyncStatus::Unsynced { files: 3, first: "Два.md: Dropbox 400: bad".into() }
    );
}

const QUIET: std::time::Duration = std::time::Duration::from_millis(250);

fn until(what: &str, condition: impl Fn() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn started_service(remote: std::sync::Arc<Fake>, folder: &Path) -> crate::sync::service::SyncService {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join("dropbox.json"), "{}").unwrap();
    let service = crate::sync::service::SyncService::with(
        folder.to_path_buf(),
        Box::new(move |_, _| Ok(std::sync::Arc::clone(&remote) as std::sync::Arc<dyn Remote>)),
        "dropbox.json",
    );
    service.start();
    service
}

fn synced(service: &crate::sync::service::SyncService) -> bool {
    service.status() == crate::sync::SyncStatus::Ready && service.last().is_some()
}

fn finish(service: crate::sync::service::SyncService) {
    let shared = std::sync::Arc::clone(service.shared());
    drop(service);
    until("sync worker stopped", || std::sync::Arc::strong_count(&shared) == 1);
}

#[test]
fn an_injected_remote_runs_the_actual_service() {
    let remote = std::sync::Arc::new(Fake::default());
    let device = Device::new();
    device.write("Заметка.md", "текст");
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());

    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("the first pass finished", || synced(&service));

    assert_eq!(remote.text("Заметка.md").as_deref(), Some("текст"));
    finish(service);
}

#[test]
fn the_next_pass_waits_for_the_vault_interval() {
    let remote = std::sync::Arc::new(Fake::default());
    let device = Device::new();
    device.write("Заметка.md", "текст");
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());
    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("the first pass finished", || synced(&service));
    std::thread::sleep(QUIET);

    let left = service.due().map(|due| due.saturating_duration_since(std::time::Instant::now()));
    assert!(left.is_some_and(|left| left > std::time::Duration::from_secs(290)), "{left:?}");

    let ready = service.status();
    service.set_vault(Some((device.root.clone(), SyncSettings { interval: 30, ..SCOPE })), "ru");
    let left = service.due().map(|due| due.saturating_duration_since(std::time::Instant::now()));
    assert!(
        left.is_some_and(|left| left > std::time::Duration::from_secs(29) && left <= std::time::Duration::from_secs(30)),
        "{left:?}"
    );
    std::thread::sleep(QUIET);
    assert_eq!(service.status(), ready, "changing the interval must not start a pass");
    finish(service);
}

#[test]
fn a_zero_interval_cancels_automatic_passes() {
    let remote = std::sync::Arc::new(Fake::default());
    let device = Device::new();
    device.write("Заметка.md", "текст");
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());
    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("the first pass finished", || synced(&service));
    assert!(service.due().is_some(), "a positive interval schedules the next pass");

    service.set_vault(Some((device.root.clone(), SyncSettings { interval: 0, ..SCOPE })), "ru");
    assert!(service.due().is_none(), "interval 0 must cancel the scheduled pass");
    finish(service);
}

#[test]
fn large_files_and_the_size_are_known_without_the_cloud() {
    use crate::sync::cycle::LARGE;
    let remote = std::sync::Arc::new(Fake::default());
    remote.go_offline();
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize / 2 + 1));
    device.write("Заметка.md", "текст");
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());

    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("the pass failed without network", || {
        service.status() == crate::sync::SyncStatus::Offline
    });

    assert_eq!(
        service.large(),
        vec![Large { path: "Видео.mp4".to_owned(), size: LARGE + 2, kept: true, ignored: false }]
    );
    assert_eq!(service.size().synced, 10);
    finish(service);
}

#[test]
fn ignoring_a_large_file_from_the_service_pauses_it() {
    use crate::sync::cycle::LARGE;
    let remote = std::sync::Arc::new(Fake::default());
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize + 1));
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());
    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("large file listed", || service.large().first().is_some_and(|large| large.kept));

    service.ignore(vec!["Видео.mp4".to_owned()]);

    assert!(service.large()[0].ignored);
    assert!(device.read("Видео.mp4").is_some());
    assert!(remote.paths().is_empty());
    finish(service);
}

#[test]
fn sending_a_large_file_marks_it_at_once_and_uploads_it() {
    use crate::sync::cycle::LARGE;
    let remote = std::sync::Arc::new(Fake::default());
    let device = Device::new();
    device.write("Видео.mp4", &"в".repeat(LARGE as usize / 2 + 1));
    let folder = tempfile::tempdir().unwrap();
    let service = started_service(std::sync::Arc::clone(&remote), folder.path());
    service.set_vault(Some((device.root.clone(), SCOPE)), "ru");
    until("large file listed", || service.large().first().is_some_and(|large| large.kept));
    until("the pass finished", || synced(&service));

    service.allow(vec!["Видео.mp4".to_owned()]);

    assert!(!service.large()[0].kept);
    until("large file uploaded", || remote.paths() == vec!["Видео.mp4".to_owned()]);
    finish(service);
}


#[test]
#[ignore]
fn two_devices_sync_through_dropbox() {
    use crate::sync::dropbox::{Dropbox, CLIENT};
    use std::path::PathBuf;

    let path = std::env::var("VIBECODIUM_DROPBOX_ACCOUNT")
        .expect("set VIBECODIUM_DROPBOX_ACCOUNT to a Dropbox token JSON path");
    let account = crate::sync::account::Account::open(
        CLIENT,
        crate::sync::service::agent(),
        PathBuf::from(path),
    )
    .unwrap()
    .expect("connect Dropbox in the app first, or provide a valid token file");
    let folder = format!("Vibecodium check {}", uuid::Uuid::new_v4().simple());
    let remote = Dropbox::new(account, crate::sync::service::agent(), &folder);

    struct Remove<'a>(&'a Dropbox);
    impl Drop for Remove<'_> {
        fn drop(&mut self) {
            let _ = self.0.remove();
        }
    }
    let _remove = Remove(&remote);

    let (laptop, phone) = (Device::new(), Device::new());
    laptop.write("Заметка.md", "один\nдва\nтри\n");
    laptop.write("Папка/Вложение.bin", "\u{0}\u{1}\u{2}");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    assert_eq!(phone.read("Заметка.md").as_deref(), Some("один\nдва\nтри\n"));

    laptop.write("Заметка.md", "ОДИН\nдва\nтри\n");
    phone.write("Заметка.md", "один\nдва\nтри!\n");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();
    assert_eq!(laptop.read("Заметка.md").as_deref(), Some("ОДИН\nдва\nтри!\n"));
    assert_eq!(phone.read("Заметка.md").as_deref(), Some("ОДИН\nдва\nтри!\n"));

    phone.delete("Папка/Вложение.bin");
    phone.sync(&remote, false).unwrap();
    laptop.sync(&remote, false).unwrap();
    assert!(laptop.read("Папка/Вложение.bin").is_none());

    laptop.delete("Заметка.md");
    laptop.sync(&remote, false).unwrap();
    phone.sync(&remote, false).unwrap();
    assert_eq!(phone.files(), Vec::<String>::new());
}
