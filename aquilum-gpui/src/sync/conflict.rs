use crate::sync::apply::{local_hash, present, remember, stage, upload, whole};
use crate::sync::cycle::{Facts, Vault};
use crate::sync::merge::{apply_edits, merge_external_change};
use crate::sync::remote::{FileContent, RemoteError, RemoteFile};
use crate::sync::scan::{absolute, service, stat, UNKNOWN};
use crate::sync::write::{create_bytes, create_content, write_content_checked, WriteOutcome};
use crate::sync::SyncError;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const MERGE_LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictCause {
    Diverged,
    Displaced,
}

pub struct ConflictReport {
    pub cause: ConflictCause,
    #[allow(dead_code)]
    pub synced_hash: Option<String>,
}

impl ConflictCause {
    fn title(self) -> &'static str {
        match self {
            Self::Diverged => "the file and the editor changed at the same time",
            Self::Displaced => "lines did not fit during the merge",
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            Self::Diverged => {
                "Both the file on disk and the open document changed after the last sync. \
                 The editor version could not be merged without a common base, so it is preserved here."
            }
            Self::Displaced => {
                "An external change was merged into the note and displaced local lines. \
                 The rest of the note is intact; the displaced lines are preserved here."
            }
        }
    }
}

pub struct LocalMinute {
    pub date: String,
    pub time: String,
}

impl LocalMinute {
    pub fn now() -> Self {
        let at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        Self {
            date: format!("{:04}-{:02}-{:02}", at.year(), u8::from(at.month()), at.day()),
            time: format!("{:02}:{:02}", at.hour(), at.minute()),
        }
    }
}

fn is_note(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

fn title(path: &Path) -> String {
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

fn copy_stem(stem: &str, moment: &LocalMinute, device: &str) -> String {
    format!("{stem} (конфликт {} {} {device})", moment.date, moment.time.replace(':', "-"))
}

pub fn resolve(vault: &Vault<'_>, facts: &Facts<'_>) -> Result<(), SyncError> {
    let remote = present(facts.remote)?;
    let path = absolute(vault.root, facts.path);
    match texts(vault, facts, &path, remote)? {
        Some(texts) => merge(vault, facts, &path, remote, &texts),
        None => keep_both(vault, facts, &path, remote),
    }
}

struct Texts {
    base: String,
    theirs: String,
    ours: String,
}

fn texts(
    vault: &Vault<'_>,
    facts: &Facts<'_>,
    path: &Path,
    remote: &RemoteFile,
) -> Result<Option<Texts>, SyncError> {
    let (Some(record), Some(local)) = (facts.record, facts.local) else {
        return Ok(None);
    };
    let small = local.size <= MERGE_LIMIT && remote.size <= MERGE_LIMIT && record.remote.size <= MERGE_LIMIT;
    if !small || !is_note(path) || service(facts.path) {
        return Ok(None);
    }
    let base = match text(vault, &record.remote) {
        Err(SyncError::Remote(RemoteError::Missing)) => return Ok(None),
        base => base?,
    };
    let ours = fs::read(path).ok().and_then(|bytes| String::from_utf8(bytes).ok());
    Ok(match (base, text(vault, remote)?, ours) {
        (Some(base), Some(theirs), Some(ours)) => Some(Texts { base, theirs, ours }),
        _ => None,
    })
}

fn text(vault: &Vault<'_>, file: &RemoteFile) -> Result<Option<String>, SyncError> {
    let mut bytes = Vec::new();
    vault.remote.download(file, &mut bytes)?;
    whole(file, bytes.len())?;
    Ok(String::from_utf8(bytes).ok())
}

fn merge(
    vault: &Vault<'_>,
    facts: &Facts<'_>,
    path: &Path,
    remote: &RemoteFile,
    texts: &Texts,
) -> Result<(), SyncError> {
    let expected = local_hash(vault.root, facts)?;
    let merged = merge_external_change(&texts.base, &texts.theirs, &texts.ours);
    let merged_text = apply_edits(&texts.ours, &merged.edits);
    let written = match write_content_checked(path, &merged_text, Some(&expected)) {
        WriteOutcome::Written(hash) => hash,
        WriteOutcome::Conflict { .. } => return Err(SyncError::Changed),
        WriteOutcome::Failed(error) => return Err(SyncError::File(error)),
    };
    if !merged.displaced.is_empty() {
        preserve_copy(
            vault,
            path,
            &merged.displaced.join("\n"),
            ConflictReport { cause: ConflictCause::Displaced, synced_hash: None },
        );
    }
    let modified = stat(&fs::metadata(path)?)?.modified;
    let uploaded = vault.remote.upload(facts.path, &FileContent::open(path)?, modified, Some(remote))?;
    remember(vault, facts.path, written, UNKNOWN, uploaded)
}

fn keep_both(vault: &Vault<'_>, facts: &Facts<'_>, path: &Path, remote: &RemoteFile) -> Result<(), SyncError> {
    let label = copy_stem(&title(path), &LocalMinute::now(), vault.device);
    let extension = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_else(|| ".md".to_owned());
    let net = vault.net();
    let staged = stage(&net, remote)?;
    let content = staged.open()?;
    let copy = free_copy(path, &label, &extension, |target| create_content(target, &content))?;
    let (hash, uploaded) = upload(&net, facts).inspect_err(|_| {
        let _ = fs::remove_file(&copy);
    })?;
    remember(vault, facts.path, hash, present(facts.local)?, uploaded)
}

pub fn preserve_copy(
    vault: &Vault<'_>,
    path: &Path,
    body: &str,
    report: ConflictReport,
) -> Option<PathBuf> {
    if body.chars().all(|symbol| symbol.is_whitespace()) {
        return None;
    }
    let moment = LocalMinute::now();
    let stem = title(path);
    let label = copy_stem(&stem, &moment, vault.device);
    let header = format!(
        "---\nконфликт: {} {}\nзаметка: \"{}\"\nпричина: \"{}\"\n---\n\n> This is a conflict copy. The text below was not written into the note.\n> **Why.** {}\n> **When.** {} {}\n\n## Текст, который не попал в заметку\n\n",
        moment.date,
        moment.time,
        stem,
        report.cause.title(),
        report.cause.explanation(),
        moment.date,
        moment.time,
    );
    let content = format!("{header}{body}\n");
    free_copy(path, &label, ".md", |target| create_bytes(target, content.as_bytes())).ok()
}

fn free_copy(
    note: &Path,
    label: &str,
    extension: &str,
    create: impl FnMut(&Path) -> WriteOutcome,
) -> Result<PathBuf, SyncError> {
    let mut create = create;
    for attempt in 0..1000u32 {
        let target = crate::sync::write::sibling_path(note, label, extension, attempt as usize);
        if target.exists() {
            continue;
        }
        match create(&target) {
            WriteOutcome::Written(_) => return Ok(target),
            WriteOutcome::Conflict { .. } => continue,
            WriteOutcome::Failed(error) => return Err(SyncError::File(error)),
        }
    }
    Err(SyncError::Changed)
}
