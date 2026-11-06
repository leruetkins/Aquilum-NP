use crate::sync::hash::hash_bytes;
use crate::sync::records::Records;
use crate::sync::state::State;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};

pub struct Shared {
    pub folder: PathBuf,
    pub device: String,
    pub state: Mutex<State>,
    pub signal: Condvar,
}

impl Shared {
    pub fn new(folder: PathBuf) -> Self {
        let _ = fs::create_dir_all(&folder);
        Self {
            device: device_id(&folder),
            folder,
            state: Mutex::new(State::default()),
            signal: Condvar::new(),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn connected(&self) -> bool {
        self.folder.join("dropbox.json").is_file()
    }

    pub fn records(&self, root: &Path) -> Option<Records> {
        if !self.connected() {
            return None;
        }
        let key = cache_key(root);
        Records::open(&self.folder.join(format!("{key}.sqlite3"))).ok()
    }

    pub fn change(&self, edit: impl FnOnce(&mut State)) {
        let mut state = self.lock();
        edit(&mut state);
        if let Some(records) = state.vault.as_ref().and_then(|(root, _)| self.records(root)) {
            let _ = records.remember(&state.memory);
        }
        drop(state);
        self.signal.notify_all();
    }
}

pub fn canonical(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

pub fn cache_key(root: &Path) -> String {
    hash_bytes(canonical(root).to_string_lossy().as_bytes())
}

fn device_id(folder: &Path) -> String {
    let path = folder.join("device");
    if let Ok(value) = fs::read_to_string(&path) {
        let value = value.trim();
        if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return value.to_owned();
        }
    }
    let value = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
    let _ = fs::create_dir_all(folder);
    let _ = fs::write(&path, &value);
    value
}
