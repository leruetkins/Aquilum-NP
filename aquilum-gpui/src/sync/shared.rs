use crate::sync::hash::hash_bytes;
use crate::sync::records::Records;
use crate::sync::remote::Remote;
use crate::sync::state::State;
use crate::sync::SyncError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Creates the remote backend for a named cloud folder. Tests inject a fake
/// implementation; production builds use Dropbox.
pub type Open = Box<dyn Fn(&Path, &str) -> Result<Arc<dyn Remote>, SyncError> + Send + Sync>;

pub struct Shared {
    pub folder: PathBuf,
    pub device: String,
    open: Open,
    token: String,
    connected: AtomicBool,
    revision: AtomicU64,
    pub state: Mutex<State>,
    pub signal: Condvar,
}

impl Shared {
    pub fn new(folder: PathBuf, open: Open, token: impl Into<String>) -> Self {
        let _ = fs::create_dir_all(&folder);
        let token = token.into();
        let connected = folder.join(&token).is_file();
        Self {
            device: device_id(&folder),
            folder,
            open,
            token,
            connected: AtomicBool::new(connected),
            revision: AtomicU64::new(0),
            state: Mutex::new(State::default()),
            signal: Condvar::new(),
        }
    }

    pub fn remote(&self, name: &str) -> Result<Arc<dyn Remote>, SyncError> {
        (self.open)(&self.folder, name)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::Relaxed);
        self.bump();
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn bump(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
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
        self.bump();
        self.signal.notify_all();
    }
}

pub fn canonical(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Comparison form of a path: canonical when possible, without the Windows
/// extended-length prefix. `notify` and the UI pass ordinary paths, while
/// `canonicalize` returns `\\?\…` on Windows; mixing them breaks
/// `starts_with` checks in the watcher.
pub fn normalize(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    strip_extended(path)
}

#[cfg(windows)]
fn strip_extended(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        if let Some(rest) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        return PathBuf::from(rest);
    }
    path
}

#[cfg(not(windows))]
fn strip_extended(path: PathBuf) -> PathBuf {
    path
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
