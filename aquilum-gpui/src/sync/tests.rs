use crate::settings::SyncSettings;
use crate::sync::cycle::{self, Outcome, Vault, Watch};
use crate::sync::hash::dropbox_hash;
use crate::sync::records::{Large, Records, Size};
use crate::sync::remote::{Content, Delta, Remote, RemoteError, RemoteFile, RemoteResult};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Default)]
struct State {
    files: BTreeMap<String, (Vec<u8>, String, u64)>,
    revisions: HashMap<String, Vec<u8>>,
    log: Vec<String>,
    next: u64,
    expired: bool,
}

#[derive(Default)]
struct Fake {
    state: Mutex<State>,
}

impl Fake {
    fn put(&self, path: &str, text: &str) {
        let mut state = self.state.lock().unwrap();
        state.next += 1;
        let rev = format!("r{}", state.next);
        let bytes = text.as_bytes().to_vec();
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

    fn uploads(&self) -> usize {
        self.state.lock().unwrap().log.len()
    }

    fn wipe(&self) {
        let mut state = self.state.lock().unwrap();
        state.files.clear();
        state.revisions.clear();
        state.log.clear();
        state.expired = true;
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
        Ok(self.state.lock().unwrap().log.len().to_string())
    }

    fn list(&self) -> RemoteResult<Vec<RemoteFile>> {
        let state = self.state.lock().unwrap();
        Ok(state
            .files
            .iter()
            .map(|(path, (bytes, rev, modified))| meta(path, bytes, rev, *modified))
            .collect())
    }

    fn delta(&self, cursor: &str) -> RemoteResult<Option<Delta>> {
        let mut state = self.state.lock().unwrap();
        if std::mem::take(&mut state.expired) {
            return Ok(None);
        }
        let from: usize = cursor.parse().map_err(|_| RemoteError::Failed("cursor".into()))?;
        if from > state.log.len() {
            return Ok(None);
        }
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        let mut seen = BTreeMap::new();
        for path in &state.log[from..] {
            seen.insert(path.clone(), ());
        }
        for path in seen.keys() {
            if let Some((bytes, rev, modified)) = state.files.get(path) {
                changed.push(meta(path, bytes, rev, *modified));
            } else {
                removed.push(path.clone());
            }
        }
        Ok(Some(Delta { changed, removed, cursor: state.log.len().to_string() }))
    }

    fn download(&self, file: &RemoteFile, out: &mut dyn Write) -> RemoteResult<()> {
        let state = self.state.lock().unwrap();
        let bytes = state.revisions.get(&file.rev).ok_or(RemoteError::Missing)?;
        out.write_all(bytes)?;
        Ok(())
    }

    fn upload(
        &self,
        path: &str,
        content: &dyn Content,
        modified: u64,
        replaces: Option<&RemoteFile>,
    ) -> RemoteResult<RemoteFile> {
        let mut bytes = Vec::with_capacity(content.byte_len());
        content.read(0..content.byte_len(), &mut |block| {
            bytes.extend_from_slice(block);
            Ok(())
        })?;
        let mut state = self.state.lock().unwrap();
        let current = state.files.get(path).map(|(_, rev, _)| rev.as_str());
        if current != replaces.map(|file| file.rev.as_str()) {
            return Err(RemoteError::Stale);
        }
        state.next += 1;
        let rev = format!("r{}", state.next);
        state.revisions.insert(rev.clone(), bytes.clone());
        state.files.insert(path.to_owned(), (bytes.clone(), rev.clone(), modified));
        state.log.push(path.to_owned());
        Ok(meta(path, &bytes, &rev, modified))
    }

    fn delete(&self, file: &RemoteFile) -> RemoteResult<()> {
        let mut state = self.state.lock().unwrap();
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

struct Probe;

impl Watch for Probe {
    fn measured(&self, _: Vec<Large>, _: Size) {}
    fn progress(&self, _: crate::sync::progress::Progress) {}
    fn stopped(&self) -> bool {
        false
    }
}

const SCOPE: SyncSettings = SyncSettings { enabled: true, trash: false, interval: 300 };

struct Device {
    root: PathBuf,
    records: Records,
    directory: tempfile::TempDir,
}

impl Device {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        Self { root, records: Records::memory().unwrap(), directory }
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    fn delete(&self, path: &str) {
        std::fs::remove_file(self.root.join(path)).unwrap();
    }

    fn sync(&self, remote: &dyn Remote, allow_deletions: bool) -> Result<Outcome, crate::sync::SyncError> {
        let staging = self.directory.path().join("staging");
        let vault = Vault {
            root: &self.root,
            remote,
            records: &self.records,
            staging: &staging,
            scope: SCOPE,
            watch: &Probe,
            allow_deletions,
            allowed: &[],
            device: "test",
        };
        cycle::run(&vault)
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
    remote.wipe();
    assert!(device.sync(&remote, false).is_err());
    assert_eq!(device.read("Заметка.md").as_deref(), Some("текст"));
}

