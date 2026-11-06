//! Stable identity for the cloud folder that belongs to one vault.
//!
//! A Dropbox App Folder is shared by every workspace in the app, so the
//! remote folder name alone cannot distinguish two local vaults with the same
//! directory name. The identity file travels in the remote folder and is
//! adopted by a fresh, empty local vault; a vault that already has its own
//! identity refuses to merge into a folder owned by another one.

use crate::settings::SyncSettings;
use crate::sync::scan;
use std::fs;
use std::io;
use std::path::Path;

/// Marker stored in the remote folder. Kept out of the synced file set.
pub const MARKER: &str = ".vibecodium-vault-id";
/// Local identity file. Living under `.vault` keeps it out of the scan.
pub const LOCAL: &str = ".vault/sync-id";
/// Local resolved folder name, written when a collision forces a fallback.
pub const FOLDER: &str = ".vault/sync-folder";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Both sides already agree.
    Keep(String),
    /// The remote folder owns the identity; adopt it.
    Adopt(String),
    /// Neither side has an identity yet.
    Create,
    /// The remote folder belongs to a different vault.
    Conflict { remote: String },
}

/// Decides what to do with the local and remote identities.
///
/// A local vault without an identity may adopt the remote one when it is
/// empty, or when its records prove that this workspace already synced through
/// this folder before. Otherwise it would silently mix two unrelated vaults
/// that happen to share a directory name.
pub fn resolve(
    local: Option<&str>,
    remote: Option<&str>,
    local_has_files: bool,
    known: bool,
) -> Resolution {
    match (local, remote) {
        (Some(local), Some(remote)) if local == remote => Resolution::Keep(local.to_owned()),
        (Some(_), Some(remote)) => Resolution::Conflict { remote: remote.to_owned() },
        (Some(local), None) => Resolution::Keep(local.to_owned()),
        (None, Some(remote)) if local_has_files && !known => {
            Resolution::Conflict { remote: remote.to_owned() }
        }
        (None, Some(remote)) => Resolution::Adopt(remote.to_owned()),
        (None, None) => Resolution::Create,
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn valid_id(value: &str) -> bool {
    let value = value.trim();
    (8..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Reads the local identity, treating a corrupt file as absent so it can be
/// recreated instead of blocking every pass.
pub fn read(root: &Path) -> io::Result<Option<String>> {
    let path = root.join(LOCAL);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value = String::from_utf8(bytes).ok();
    Ok(value.map(|value| value.trim().to_owned()).filter(|value| valid_id(value)))
}

pub fn store(root: &Path, id: &str) -> io::Result<()> {
    if !valid_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid vault identity"));
    }
    crate::sync::write::write_bytes(&root.join(LOCAL), id.trim().as_bytes())
}

pub fn read_folder(root: &Path) -> io::Result<Option<String>> {
    let path = root.join(FOLDER);
    let value = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value = value.trim().to_owned();
    Ok(valid_folder(&value).then_some(value))
}

pub fn store_folder(root: &Path, folder: &str) -> io::Result<()> {
    if !valid_folder(folder) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid cloud folder name"));
    }
    crate::sync::write::write_bytes(&root.join(FOLDER), folder.as_bytes())
}

/// The folder used in the app folder unless the workspace overrides it or a
/// previous collision pinned a fallback name.
pub fn folder_name(root: &Path, remote_folder: Option<&str>) -> io::Result<String> {
    if let Some(name) = remote_folder {
        let name = name.trim();
        if valid_folder(name) {
            return Ok(name.to_owned());
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid cloud folder name: {name}"),
        ));
    }
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the workspace folder has no name; Dropbox sync is refused for a drive root",
            )
        })?;
    if valid_folder(&name) {
        Ok(name)
    } else {
        let digest = crate::sync::hash::hash_bytes(name.as_bytes());
        Ok(format!("Vault-{}", &digest[..8]))
    }
}

/// Appends a short piece of the vault identity, used when the default folder
/// is already owned by another vault.
pub fn fallback_name(folder: &str, id: &str) -> String {
    let short: String = id.chars().take(8).collect();
    format!("{folder}-{short}")
}

pub fn local_has_files(root: &Path, scope: &SyncSettings) -> io::Result<bool> {
    Ok(!scan::scan(root, scope)?.is_empty())
}

fn valid_folder(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable_and_corruption_is_recoverable() {
        let directory = tempfile::tempdir().unwrap();
        let id = new_id();
        store(directory.path(), &id).unwrap();
        assert_eq!(read(directory.path()).unwrap().as_deref(), Some(id.as_str()));

        fs::write(directory.path().join(LOCAL), "not-an-id").unwrap();
        assert_eq!(read(directory.path()).unwrap(), None);
    }

    #[test]
    fn empty_vault_adopts_the_remote_identity_but_a_used_one_refuses() {
        assert_eq!(resolve(None, Some("aaaa1111"), false, false), Resolution::Adopt("aaaa1111".into()));
        assert_eq!(
            resolve(None, Some("aaaa1111"), true, false),
            Resolution::Conflict { remote: "aaaa1111".into() }
        );
        assert_eq!(
            resolve(None, Some("aaaa1111"), true, true),
            Resolution::Adopt("aaaa1111".into()),
            "records prove this workspace already used the folder"
        );
        assert_eq!(
            resolve(Some("bbbb2222"), Some("aaaa1111"), false, false),
            Resolution::Conflict { remote: "aaaa1111".into() }
        );
        assert_eq!(
            resolve(Some("aaaa1111"), Some("aaaa1111"), true, false),
            Resolution::Keep("aaaa1111".into())
        );
        assert_eq!(resolve(None, None, true, false), Resolution::Create);
    }

    #[test]
    fn fallback_and_folder_names_are_usable_segments() {
        assert_eq!(fallback_name("Заметки", "abcdef0123456789"), "Заметки-abcdef01");
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Мой vault");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(folder_name(&root, None).unwrap(), "Мой vault");
        assert_eq!(folder_name(&root, Some("Other")).unwrap(), "Other");
        assert!(folder_name(&root, Some("a/b")).is_err());
    }
}
