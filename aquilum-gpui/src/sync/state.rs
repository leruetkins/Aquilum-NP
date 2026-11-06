use crate::settings::SyncSettings;
use crate::sync::progress::Progress;
use crate::sync::records::Memory;
use crate::sync::cycle::Outcome;
use crate::sync::{RemoteError, SyncError};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncStatus {
    Off(Option<String>),
    Login,
    Ready,
    Running(Progress),
    Deletions { planned: usize, synced: usize },
    Offline,
    Unsynced { files: usize, first: String },
    Failed(String),
}

pub struct State {
    pub vault: Option<(PathBuf, SyncSettings)>,
    pub memory: Memory,
    pub running: Option<Progress>,
    pub problem: Option<SyncStatus>,
    pub allow_deletions: bool,
    pub due: Option<Instant>,
    pub epoch: u64,
    pub closed: bool,
    pub login: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            vault: None,
            memory: Memory::default(),
            running: None,
            problem: None,
            allow_deletions: false,
            due: None,
            epoch: 0,
            closed: false,
            login: None,
        }
    }
}

impl State {
    pub fn status(&self, connected: bool) -> SyncStatus {
        if self.login.is_some() {
            SyncStatus::Login
        } else if !connected {
            SyncStatus::Off(match &self.problem {
                Some(SyncStatus::Failed(error)) => Some(error.clone()),
                _ => None,
            })
        } else if let Some(progress) = self.running {
            SyncStatus::Running(progress)
        } else {
            self.problem.clone().unwrap_or(SyncStatus::Ready)
        }
    }

    pub fn restart(&mut self) {
        self.epoch += 1;
        self.memory = Memory::default();
        self.running = None;
        self.problem = None;
        self.due = Some(Instant::now());
    }

    pub fn settle(&mut self, passed: Result<Outcome, SyncError>) {
        self.running = None;
        self.problem = match passed {
            Ok(outcome) if outcome.stopped => return,
            Ok(outcome) if outcome.failed.is_empty() => {
                self.memory.done = Some(millis(SystemTime::now()));
                None
            }
            Ok(outcome) => match outcome.failed.iter().find(|(_, error)| !offline(error)) {
                Some((path, error)) => {
                    let first = format!("{path}: {error}").lines().next().unwrap_or_default().to_owned();
                    Some(SyncStatus::Unsynced { files: outcome.failed.len(), first })
                }
                None => Some(SyncStatus::Offline),
            },
            Err(SyncError::Deletions { planned, synced }) => Some(SyncStatus::Deletions { planned, synced }),
            Err(error) if offline(&error) => Some(SyncStatus::Offline),
            Err(error) => Some(SyncStatus::Failed(error.to_string())),
        };
    }
}

fn offline(error: &SyncError) -> bool {
    matches!(error, SyncError::Remote(RemoteError::Offline(_)))
}

pub fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map(|since| since.as_millis() as u64).unwrap_or(0)
}
