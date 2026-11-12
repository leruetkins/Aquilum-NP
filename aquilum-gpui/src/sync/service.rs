use crate::settings::SyncSettings;
use crate::sync::account::Account;
use crate::sync::dropbox::CLIENT;
use crate::sync::records::{Large, Size};
use crate::sync::shared::{Open, Shared};
use crate::sync::state::SyncStatus;
use crate::sync::oauth::Tokens;
use crate::sync::transfer::TRANSFERS;
use crate::sync::worker;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct SyncService {
    shared: Arc<Shared>,
    started: Mutex<bool>,
}

impl SyncService {
    pub fn new(app_data: &Path) -> Self {
        Self::with(
            app_data.join("sync"),
            Box::new(worker::dropbox),
            "dropbox.json",
        )
    }

    /// Builds a service with an injected remote factory. Production uses
    /// Dropbox; tests use a fake remote without network access.
    pub(crate) fn with(folder: PathBuf, open: Open, token: impl Into<String>) -> Self {
        Self {
            shared: Arc::new(Shared::new(folder, open, token)),
            started: Mutex::new(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    pub fn start(&self) {
        let mut started = self.started.lock().unwrap_or_else(|error| error.into_inner());
        if *started {
            return;
        }
        *started = true;
        let shared = Arc::clone(&self.shared);
        thread::Builder::new()
            .name("vibecodium-sync".to_owned())
            .spawn(move || worker::work(shared))
            .expect("sync worker thread");
    }

    pub fn status(&self) -> SyncStatus {
        self.shared.lock().status(self.shared.connected())
    }

    pub fn revision(&self) -> u64 {
        self.shared.revision()
    }

    pub fn large(&self) -> Vec<Large> {
        self.shared.lock().memory.large.clone()
    }

    pub fn size(&self) -> Size {
        self.shared.lock().memory.size
    }

    pub fn last(&self) -> Option<u64> {
        self.shared.lock().memory.done
    }

    pub fn due(&self) -> Option<Instant> {
        let state = self.shared.lock();
        let waiting = state.vault.is_some()
            && matches!(
                state.status(self.shared.connected()),
                SyncStatus::Ready | SyncStatus::Offline | SyncStatus::Unsynced { .. } | SyncStatus::Failed(_)
            );
        state.due.filter(|_| waiting)
    }

    pub fn set_vault(&self, vault: Option<(PathBuf, SyncSettings)>, language: &str) {
        let vault = vault.filter(|(_, sync)| sync.enabled);
        if let Some((root, sync)) = &vault {
            if self.reschedule(root, sync.clone(), language) {
                return;
            }
        }
        let memory = vault
            .as_ref()
            .and_then(|(root, _)| self.shared.records(root))
            .map(|records| records.recall())
            .unwrap_or_default();
        self.shared.change(|state| {
            state.restart();
            state.language = language.to_owned();
            state.memory = memory;
            state.vault = vault.map(|(root, sync)| (root, sync));
        });
    }

    fn reschedule(&self, root: &Path, sync: SyncSettings, language: &str) -> bool {
        let mut state = self.shared.lock();
        let Some((current, scope)) = &mut state.vault else {
            return false;
        };
        let same_scope = SyncSettings { interval: sync.interval, ..scope.clone() };
        if current.as_path() != root || same_scope != sync {
            return false;
        }
        let interval_changed = scope.interval != sync.interval;
        scope.interval = sync.interval;
        state.language = language.to_owned();
        if interval_changed && sync.interval == 0 {
            state.due = None;
        }
        drop(state);
        self.shared.bump();
        if sync.interval == 0 {
            if interval_changed {
                self.shared.signal.notify_all();
            }
        } else {
            self.request(Duration::from_secs(sync.interval.max(5)));
        }
        true
    }

    pub fn request(&self, delay: Duration) {
        let mut state = self.shared.lock();
        state.due = Some(Instant::now() + delay);
        drop(state);
        self.shared.signal.notify_all();
    }

    pub fn allow_deletions(&self) {
        self.shared.change(|state| {
            state.allow_deletions = true;
            state.due = Some(Instant::now());
        });
    }

    pub fn allow(&self, paths: Vec<String>) {
        self.shared.change(|state| {
            state
                .memory
                .large
                .iter_mut()
                .filter(|large| paths.contains(&large.path))
                .for_each(|large| {
                    large.kept = false;
                    large.ignored = false;
                });
            state.memory.allowed.extend(paths);
            state.due = Some(Instant::now());
        });
    }

    /// Pauses synchronization for the listed paths. The files stay on disk;
    /// a pass will not upload, download, or delete them until they are
    /// explicitly resumed.
    pub fn ignore(&self, paths: Vec<String>) {
        self.shared.change(|state| {
            for path in &paths {
                if !state.memory.ignored.contains(path) {
                    state.memory.ignored.push(path.clone());
                }
                if let Some(large) = state.memory.large.iter_mut().find(|large| large.path == *path) {
                    large.ignored = true;
                }
            }
            state.memory.allowed.retain(|path| !paths.contains(path));
            state.due = Some(Instant::now());
        });
    }

    /// Returns paused paths to normal synchronization.
    pub fn unignore(&self, paths: Vec<String>) {
        self.shared.change(|state| {
            state.memory.ignored.retain(|path| !paths.contains(path));
            for large in state.memory.large.iter_mut().filter(|large| paths.contains(&large.path)) {
                large.ignored = false;
            }
            state.due = Some(Instant::now());
        });
    }

    pub fn connect(&self) -> io::Result<String> {
        self.stop_login();
        let login = CLIENT.login()?;
        let url = login.url.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_thread = Arc::clone(&cancel);
        let shared = Arc::clone(&self.shared);
        let token = {
            let mut state = self.shared.lock();
            state.problem = None;
            state.epoch
        };
        let waiting = thread::spawn(move || {
            let agent = agent();
            let tokens = login.finish(&agent, &cancel_thread).map_err(|error| error.to_string());
            let connected = tokens.and_then(|tokens| {
                start_over(&shared.folder, shared.token(), tokens)
                    .map_err(|error| error.to_string())
            });
            shared.set_connected(connected.is_ok());
            shared.change(|state| {
                state.login = None;
                match connected {
                    Ok(()) => state.restart(),
                    Err(_) if cancel_thread.load(Ordering::Relaxed) => {
                        state.epoch += 1;
                    }
                    Err(error) => {
                        state.epoch += 1;
                        state.problem = Some(SyncStatus::Failed(error));
                    }
                }
            });
        });
        let mut state = self.shared.lock();
        // The login can finish before this line (a fast local callback). In
        // that case `restart` already advanced the epoch and there is no
        // pending login left to display.
        if state.epoch == token {
            state.login = Some((cancel, waiting));
        }
        drop(state);
        self.shared.bump();
        Ok(url)
    }

    pub fn disconnect(&self) -> io::Result<()> {
        self.stop_login();
        let removed = match fs::remove_file(self.shared.folder.join(self.shared.token())) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        let connected = self.shared.folder.join(self.shared.token()).is_file();
        self.shared.set_connected(connected);
        self.shared.change(|state| state.restart());
        removed
    }

    fn stop_login(&self) {
        let login = self.shared.lock().login.take();
        if let Some((cancel, waiting)) = login {
            cancel.store(true, Ordering::Relaxed);
            let _ = waiting.join();
        }
    }

    pub fn close(&self) {
        self.stop_login();
        let mut state = self.shared.lock();
        state.closed = true;
        drop(state);
        self.shared.signal.notify_all();
    }

    pub fn prune(&self, keep: &HashSet<String>) {
        let Ok(entries) = fs::read_dir(&self.shared.folder) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some((key, _)) = name.split_once(".sqlite3") {
                if !keep.contains(key) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

impl Drop for SyncService {
    fn drop(&mut self) {
        self.close();
    }
}

fn start_over(folder: &Path, token: &str, tokens: Tokens) -> io::Result<()> {
    match fs::remove_dir_all(folder) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir_all(folder)?;
    Account::new(CLIENT, agent(), folder.join(token), tokens).map(drop)
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(60))
        .max_idle_connections_per_host(TRANSFERS)
        .user_agent(concat!("Vibecodium/", env!("CARGO_PKG_VERSION"), " (Dropbox sync)"))
        .build()
}
