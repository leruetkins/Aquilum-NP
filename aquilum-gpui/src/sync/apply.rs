use crate::sync::conflict::resolve;
use crate::sync::cycle::{Facts, Vault};
use crate::sync::decide::Action;
use crate::sync::hash::{hash_bytes, hash_file};
use crate::sync::records::Record;
use crate::sync::remote::{Content, FileContent, RemoteError, RemoteFile};
use crate::sync::scan::{absolute, service, stamp, LocalFile, UNKNOWN};
use crate::sync::transfer::Net;
use crate::sync::write::{create_content, write_content_checked, WriteOutcome};
use crate::sync::SyncError;
use std::fs;
use std::path::{Path, PathBuf};

pub enum Step {
    Write(Staged),
    Remember(String, RemoteFile),
    Forget,
    DeleteLocal,
    Resolve,
    Match,
}

pub struct Staged(PathBuf);

impl Staged {
    pub fn open(&self) -> Result<FileContent, SyncError> {
        Ok(FileContent::open(&self.0)?)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn fetch(net: &Net<'_>, facts: &Facts<'_>, action: Action) -> Result<Step, SyncError> {
    Ok(match action {
        Action::Pull => Step::Write(stage(net, present(facts.remote)?)?),
        Action::Push => {
            let (hash, uploaded) = upload(net, facts)?;
            Step::Remember(hash, uploaded)
        }
        Action::DeleteRemote => delete_remote(net, facts)?,
        Action::Forget => Step::Forget,
        Action::DeleteLocal => Step::DeleteLocal,
        Action::Conflict => Step::Resolve,
        Action::Match => Step::Match,
    })
}

pub fn apply(vault: &Vault<'_>, facts: &Facts<'_>, step: Step) -> Result<(), SyncError> {
    match step {
        Step::Write(staged) => pull(vault, facts, &staged),
        Step::Remember(hash, uploaded) => {
            remember(vault, facts.path, hash, present(facts.local)?, uploaded)
        }
        Step::Forget => Ok(vault.records.remove(facts.path)?),
        Step::DeleteLocal => delete_local(vault, facts),
        Step::Resolve => resolve(vault, facts),
        Step::Match => remember(
            vault,
            facts.path,
            local_hash(vault.root, facts)?,
            present(facts.local)?,
            present(facts.remote)?.clone(),
        ),
    }
}

pub fn present<T>(value: Option<T>) -> Result<T, SyncError> {
    value.ok_or(SyncError::Changed)
}

pub fn remember(
    vault: &Vault<'_>,
    path: &str,
    hash: String,
    local: LocalFile,
    mut remote: RemoteFile,
) -> Result<(), SyncError> {
    remote.path = path.to_owned();
    Ok(vault.records.put(&Record { hash, size: local.size, modified: local.modified, remote })?)
}

pub fn local_hash(root: &Path, facts: &Facts<'_>) -> Result<String, SyncError> {
    match (&facts.hash, &facts.record) {
        (Some(hash), _) => Ok(hash.clone()),
        (None, Some(record)) => Ok(record.hash.clone()),
        (None, None) => Ok(hash_file(&absolute(root, facts.path))?),
    }
}

pub fn upload(net: &Net<'_>, facts: &Facts<'_>) -> Result<(String, RemoteFile), SyncError> {
    let local = present(facts.local)?;
    let hash = local_hash(net.root, facts)?;
    let content = FileContent::open(&absolute(net.root, facts.path))?;
    Ok((hash, net.remote.upload(facts.path, &content, local.modified, facts.remote)?))
}

pub fn stage(net: &Net<'_>, remote: &RemoteFile) -> Result<Staged, SyncError> {
    fs::create_dir_all(net.staging)?;
    let staged = Staged(net.staging.join(hash_bytes(format!("{}\n{}", remote.path, remote.rev).as_bytes())));
    net.remote.download(remote, &mut fs::File::create(&staged.0)?)?;
    whole(remote, fs::metadata(&staged.0)?.len() as usize)?;
    Ok(staged)
}

pub fn whole(remote: &RemoteFile, length: usize) -> Result<(), SyncError> {
    if length as u64 == remote.size {
        return Ok(());
    }
    Err(RemoteError::Failed(format!("{} was not downloaded completely", remote.path)).into())
}

fn pull(vault: &Vault<'_>, facts: &Facts<'_>, staged: &Staged) -> Result<(), SyncError> {
    let remote = present(facts.remote)?;
    let path = absolute(vault.root, facts.path);
    let content = staged.open()?;
    let hash = write_local(vault, facts, &path, &content)?;
    stamp(&path, remote.modified)?;
    remember(vault, facts.path, hash, UNKNOWN, remote.clone())
}

fn write_local(
    vault: &Vault<'_>,
    facts: &Facts<'_>,
    path: &Path,
    content: &dyn Content,
) -> Result<String, SyncError> {
    if facts.local.is_none() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        return match create_content(path, content) {
            WriteOutcome::Written(hash) => Ok(hash),
            WriteOutcome::Conflict { actual } => {
                let _ = actual;
                Err(SyncError::Changed)
            }
            WriteOutcome::Failed(error) => Err(SyncError::File(error)),
        };
    }
    let expected = local_hash(vault.root, facts)?;
    match write_content_checked(path, content, Some(&expected)) {
        WriteOutcome::Written(hash) => Ok(hash),
        WriteOutcome::Conflict { actual } => {
            let _ = actual;
            Err(SyncError::Changed)
        }
        WriteOutcome::Failed(error) => Err(SyncError::File(error)),
    }
}

fn delete_local(vault: &Vault<'_>, facts: &Facts<'_>) -> Result<(), SyncError> {
    let record = present(facts.record)?;
    let path = absolute(vault.root, facts.path);
    if !path.exists() {
        vault.records.remove(facts.path)?;
        return Ok(());
    }
    if hash_file(&path)? != record.hash {
        return Err(SyncError::Changed);
    }
    if service(facts.path) {
        fs::remove_file(&path)?;
    } else {
        trash_local(vault.root, facts.path)?;
    }
    Ok(vault.records.remove(facts.path)?)
}

fn delete_remote(net: &Net<'_>, facts: &Facts<'_>) -> Result<Step, SyncError> {
    let remote = present(facts.remote)?;
    if absolute(net.root, facts.path).exists() {
        return Err(SyncError::Changed);
    }
    net.remote.delete(remote)?;
    Ok(Step::Forget)
}

/// Moves a vault file to `.trash/<relative>`, avoiding overwrites with a
/// numbered suffix. `.trash` itself is excluded from scans by default.
fn trash_local(root: &Path, relative: &str) -> Result<(), SyncError> {
    let path = absolute(root, relative);
    if !path.exists() {
        return Ok(());
    }
    let trash = root.join(".trash");
    let mut target = trash.join(relative);
    if target.exists() {
        let parent = target.parent().map(Path::to_path_buf).unwrap_or(trash);
        let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = target.extension().map(|s| s.to_string_lossy().into_owned());
        for attempt in 1..10_000u32 {
            let name = match &ext {
                Some(ext) => format!("{stem} {attempt}.{ext}"),
                None => format!("{stem} {attempt}"),
            };
            let candidate = parent.join(name);
            if !candidate.exists() {
                target = candidate;
                break;
            }
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&path, &target)?;
    Ok(())
}
