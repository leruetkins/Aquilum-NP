use crate::sync::account::Account;
use crate::sync::cycle::{self, Outcome, Vault, Watch};
use crate::sync::dropbox::{Dropbox, CLIENT};
use crate::sync::identity::{self, Resolution};
use crate::sync::progress::Progress;
use crate::sync::records::{Large, Records, Size};
use crate::sync::remote::{Remote, RemoteError};
use crate::sync::service::agent;
use crate::sync::shared::{cache_key, Shared};
use crate::sync::SyncError;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SHOW_EVERY: Duration = Duration::from_millis(100);
const RECHECK: Duration = Duration::from_secs(5);

struct Job {
    root: std::path::PathBuf,
    scope: crate::settings::SyncSettings,
    language: String,
    allowed: Vec<String>,
    ignored: Vec<String>,
    allow_deletions: bool,
    epoch: u64,
}

struct Pass<'a> {
    shared: &'a Shared,
    epoch: u64,
    shown: Mutex<Instant>,
}

impl Watch for Pass<'_> {
    fn measured(&self, large: Vec<Large>, size: Size) {
        self.shared.change(|state| {
            if state.epoch == self.epoch {
                (state.memory.large, state.memory.size) = (large, size);
            }
        });
    }

    fn progress(&self, progress: Progress) {
        let mut state = self.shared.lock();
        if state.epoch == self.epoch {
            state.running = Some(progress);
        }
        drop(state);
        self.shared.bump();
        let mut shown = self.shown.lock().unwrap_or_else(|error| error.into_inner());
        if progress.done == progress.total || shown.elapsed() >= SHOW_EVERY {
            *shown = Instant::now();
        }
    }

    fn stopped(&self) -> bool {
        let state = self.shared.lock();
        state.closed || state.epoch != self.epoch
    }
}

/// Production provider factory used by `SyncService::new`.
pub fn dropbox(folder: &Path, name: &str) -> Result<Arc<dyn Remote>, SyncError> {
    let account = Account::open(CLIENT, agent(), folder.join("dropbox.json"))
        .map_err(SyncError::File)?
        .ok_or(RemoteError::Auth)?;
    Ok(Arc::new(Dropbox::new(account, agent(), name)))
}

pub fn work(shared: Arc<Shared>) {    while let Some(job) = wait(&shared) {
        let watch = Pass { shared: &shared, epoch: job.epoch, shown: Mutex::new(Instant::now()) };
        let passed = pass(&shared, &job, &watch);
        shared.change(|state| {
            if state.epoch == job.epoch {
                state.settle(passed);
            }
        });
    }
}

fn wait(shared: &Shared) -> Option<Job> {
    let mut state = shared.lock();
    loop {
        if state.closed {
            return None;
        }
        let now = Instant::now();
        match (state.vault.clone(), state.due) {
            (Some((root, scope)), Some(due)) if due <= now && shared.connected() => {
                state.due = Some(now + Duration::from_secs(scope.interval.max(5)));
                state.running = Some(Progress::default());
                let (allowed, allow_deletions) = (state.memory.allowed.clone(), std::mem::take(&mut state.allow_deletions));
                let ignored = state.memory.ignored.clone();
                shared.bump();
                return Some(Job {
                    root,
                    scope,
                    language: state.language.clone(),
                    allowed,
                    ignored,
                    allow_deletions,
                    epoch: state.epoch,
                });
            }
            (Some(_), Some(due)) if due > now => {
                state = shared
                    .signal
                    .wait_timeout(state, (due - now).min(RECHECK))
                    .unwrap_or_else(|error| error.into_inner())
                    .0;
            }
            _ => {
                state = shared.signal.wait(state).unwrap_or_else(|error| error.into_inner());
            }
        }
    }
}

fn pass(shared: &Shared, job: &Job, watch: &dyn Watch) -> Result<Outcome, SyncError> {
    if !shared.connected() {
        return Err(crate::sync::RemoteError::Auth.into());
    }
    let records_path = shared.folder.join(format!("{}.sqlite3", cache_key(&job.root)));
    let records = Records::open(&records_path)?;
    let remote = identity_remote(shared, job, &records)?;
    let staging = shared.folder.join("staging");
    let vault = Vault {
        root: &job.root,
        remote: &*remote,
        records: &records,
        staging: &staging,
        scope: job.scope.clone(),
        watch,
        allow_deletions: job.allow_deletions,
        allowed: &job.allowed,
        ignored: &job.ignored,
        device: &shared.device,
        language: &job.language,
    };
    cycle::run(&vault)
}

/// Resolves which remote folder belongs to this vault before any file is
/// touched. The default folder is the workspace name; when another vault
/// already owns it, the local vault is pinned to a suffixed folder instead of
/// silently mixing both file sets.
fn identity_remote(shared: &Shared, job: &Job, records: &Records) -> Result<Arc<dyn Remote>, SyncError> {
    let name = identity::folder_name(&job.root, job.scope.remote_folder.as_deref())?;
    let local = identity::read(&job.root)?;
    let pinned = job
        .scope
        .remote_folder
        .clone()
        .or_else(|| identity::read_folder(&job.root).ok().flatten());
    let candidate = local.clone().unwrap_or_else(identity::new_id);
    let folder = pinned.clone().unwrap_or_else(|| name.clone());
    let remote = shared.remote(&folder)?;
    let marker = remote.remote_identity()?;
    let known = if local.is_none() && marker.is_some() {
        records.all()?.keys().any(|path| job.scope.included(path))
    } else {
        false
    };
    let has_files = if local.is_none() && marker.is_some() && !known {
        identity::local_has_files(&job.root, &job.scope)?
    } else {
        false
    };

    match identity::resolve(local.as_deref(), marker.as_deref(), has_files, known) {
        Resolution::Keep(id) => {
            if marker.is_none() {
                remote.set_remote_identity(&id)?;
            }
            if local.is_none() {
                identity::store(&job.root, &id)?;
            }
            Ok(remote)
        }
        Resolution::Adopt(id) => {
            identity::store(&job.root, &id)?;
            Ok(remote)
        }
        Resolution::Create => {
            identity::store(&job.root, &candidate)?;
            remote.set_remote_identity(&candidate)?;
            Ok(remote)
        }
        Resolution::Conflict { remote: other } => {
            if pinned.is_some() {
                return Err(RemoteError::Failed(format!(
                    "cloud folder {folder} belongs to another vault ({other}); change the cloud folder name"
                ))
                .into());
            }
            let fallback = identity::fallback_name(&name, &candidate);
            let fallback_remote = shared.remote(&fallback)?;
            let fallback_marker = fallback_remote.remote_identity()?;
            match identity::resolve(local.as_deref(), fallback_marker.as_deref(), has_files, known) {
                Resolution::Keep(id) => {
                    if fallback_marker.is_none() {
                        fallback_remote.set_remote_identity(&id)?;
                    }
                    if local.is_none() {
                        identity::store(&job.root, &id)?;
                    }
                }
                Resolution::Adopt(id) => identity::store(&job.root, &id)?,
                Resolution::Create => {
                    identity::store(&job.root, &candidate)?;
                    fallback_remote.set_remote_identity(&candidate)?;
                }
                Resolution::Conflict { remote: other } => {
                    return Err(RemoteError::Failed(format!(
                        "cloud folder {fallback} belongs to another vault ({other}); change the cloud folder name"
                    ))
                    .into())
                }
            }
            identity::store_folder(&job.root, &fallback)?;
            Ok(fallback_remote)
        }
    }
}
