use crate::sync::account::Account;
use crate::sync::cycle::{self, Outcome, Vault, Watch};
use crate::sync::dropbox::{Dropbox, CLIENT};
use crate::sync::progress::Progress;
use crate::sync::records::{Large, Records, Size};
use crate::sync::service::agent;
use crate::sync::shared::{cache_key, Shared};
use crate::sync::SyncError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SHOW_EVERY: Duration = Duration::from_millis(100);
const RECHECK: Duration = Duration::from_secs(5);

struct Job {
    root: std::path::PathBuf,
    scope: crate::settings::SyncSettings,
    allowed: Vec<String>,
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

pub fn work(shared: Arc<Shared>) {
    while let Some(job) = wait(&shared) {
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
                return Some(Job { root, scope, allowed, allow_deletions, epoch: state.epoch });
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
    let account = Account::open(CLIENT, agent(), shared.folder.join("dropbox.json"))
        .map_err(SyncError::File)?
        .ok_or(crate::sync::RemoteError::Auth)?;
    let name = job.root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    if name.is_empty() {
        return Err(crate::sync::RemoteError::Failed(
            "the workspace folder has no name; Dropbox sync is refused for a drive root".into(),
        )
        .into());
    }
    let remote = Dropbox::new(account, agent(), &name);
    let staging = shared.folder.join("staging");
    let vault = Vault {
        root: &job.root,
        remote: &remote,
        records: &records,
        staging: &staging,
        scope: job.scope,
        watch,
        allow_deletions: job.allow_deletions,
        allowed: &job.allowed,
        device: &shared.device,
    };
    cycle::run(&vault)
}
