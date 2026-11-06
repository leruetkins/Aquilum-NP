mod api;
mod hash;
mod types;

use self::api::{copy, json, Api, Body, CONTENT};
use self::hash::content_hash;
use self::types::{Cursor, Entry, Metadata, Session};
use crate::sync::account::Account;
use crate::sync::identity;
use crate::sync::oauth::Client;
use crate::sync::remote::{Content, Delta, Remote, RemoteError, RemoteFile, RemoteResult};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, Write};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use ureq::Agent;

/// The default key is the project's own Dropbox app. Register it with
/// **App folder** access (least privilege) and the loopback redirect URI from
/// `sync::oauth::redirect()` in the Dropbox app console. Build with
/// `VIBECODIUM_DROPBOX_CLIENT_ID=<app key>` to override it. A client secret is
/// normally not needed for the PKCE public client;
/// `VIBECODIUM_DROPBOX_CLIENT_SECRET` is available for a confidential app.
pub const CLIENT: Client = Client {
    authorize: "https://www.dropbox.com/oauth2/authorize",
    token: "https://api.dropboxapi.com/oauth2/token",
    id: match option_env!("VIBECODIUM_DROPBOX_CLIENT_ID") {
        Some(id) => id,
        None => "hqch0u08opkzvpn",
    },
    secret: match option_env!("VIBECODIUM_DROPBOX_CLIENT_SECRET") {
        Some(secret) => secret,
        None => "",
    },
    params: &[("token_access_type", "offline")],
};

const SINGLE_UPLOAD: usize = 150 * 1024 * 1024;
const UPLOAD_PART: usize = 64 * 1024 * 1024;

pub struct Dropbox {
    api: Api,
    root: String,
}

impl Dropbox {
    pub fn new(account: Account, agent: Agent, folder: &str) -> Self {
        Self { api: Api::new(account, agent), root: format!("/{folder}") }
    }

    /// Reads the vault marker stored in the remote folder. A missing marker
    /// is not an error: the folder may predate identities or be brand new.
    pub fn marker(&self) -> RemoteResult<Option<String>> {
        let arg = json!({ "path": self.marker_path() });
        let response = match self
            .api
            .call(&format!("{CONTENT}files/download"), Some(&arg), Body::Empty)
        {
            Ok(response) => response,
            Err(RemoteError::Missing) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        copy(&mut response.into_reader(), &mut bytes)?;
        let value = String::from_utf8(bytes)
            .map_err(|error| RemoteError::Failed(format!("invalid vault marker: {error}")))?;
        let value = value.trim();
        if !identity::valid_id(value) {
            return Err(RemoteError::Failed("invalid vault marker".into()));
        }
        Ok(Some(value.to_owned()))
    }

    /// Writes the vault marker, creating the app folder when needed.
    pub fn set_marker(&self, id: &str) -> RemoteResult<()> {
        if !identity::valid_id(id) {
            return Err(RemoteError::Failed("invalid vault identity".into()));
        }
        self.ensure_root()?;
        let arg = json!({
            "path": self.marker_path(),
            "mode": { ".tag": "overwrite" },
            "autorename": false,
            "mute": true,
            "strict_conflict": false
        });
        let bytes = id.as_bytes();
        self.api
            .call(&format!("{CONTENT}files/upload"), Some(&arg), Body::Bytes(&bytes, 0..bytes.len()))?;
        Ok(())
    }

    fn marker_path(&self) -> String {
        format!("{}/{}", self.root, identity::MARKER)
    }

    fn ensure_root(&self) -> RemoteResult<()> {
        match self
            .api
            .rpc::<Value>("files/create_folder_v2", &json!({ "path": self.root }))
        {
            Ok(_) | Err(RemoteError::Stale) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn file(&self, metadata: Metadata) -> Option<RemoteFile> {
        Some(RemoteFile {
            path: self.relative(&metadata.path_display)?,
            id: metadata.id,
            rev: metadata.rev,
            checksum: metadata.content_hash,
            size: metadata.size,
            modified: OffsetDateTime::parse(&metadata.client_modified, &Rfc3339)
                .map(|at| at.unix_timestamp().max(0) as u64 * 1000)
                .unwrap_or(0),
        })
    }

    fn relative(&self, display: &str) -> Option<String> {
        let skip = self.root.chars().count() + 1;
        let prefix: String = display.chars().take(skip).collect();
        if prefix.to_lowercase() != format!("{}/", self.root).to_lowercase() {
            return None;
        }
        let relative: String = display.chars().skip(skip).collect();
        if relative.is_empty() || relative.split('/').any(|part| part.is_empty() || part == "..") {
            return None;
        }
        Some(relative)
    }

    fn upload_parts(&self, content: &dyn Content, commit: Value) -> RemoteResult<Metadata> {
        let url = |route: &str| format!("{CONTENT}files/upload_session/{route}");
        let length = content.byte_len();
        let part = |start: usize| Body::Bytes(content, start..(start + UPLOAD_PART).min(length));
        let session: Session = json(self.api.call(&url("start"), Some(&json!({ "close": false })), part(0))?)?;
        let mut offset = UPLOAD_PART;
        while offset + UPLOAD_PART < length {
            let cursor = json!({ "cursor": { "session_id": session.session_id, "offset": offset }, "close": false });
            self.api.call(&url("append_v2"), Some(&cursor), part(offset))?;
            offset += UPLOAD_PART;
        }
        let finish = json!({ "cursor": { "session_id": session.session_id, "offset": offset }, "commit": commit });
        json(self.api.call(&url("finish"), Some(&finish), part(offset))?)
    }
}

impl Remote for Dropbox {
    fn cursor(&self) -> RemoteResult<String> {
        let latest = |api: &Api| {
            api.rpc::<Cursor>("files/list_folder/get_latest_cursor", &json!({ "path": self.root, "recursive": true }))
        };
        let cursor = match latest(&self.api) {
            Err(RemoteError::Missing) => match self.api.rpc::<Value>("files/create_folder_v2", &json!({ "path": self.root })) {
                Ok(_) | Err(RemoteError::Stale) => latest(&self.api)?,
                Err(error) => return Err(error),
            },
            cursor => cursor?,
        };
        Ok(cursor.cursor)
    }

    fn list(&self) -> RemoteResult<Vec<RemoteFile>> {
        let mut files = Vec::new();
        let first = self.api.rpc("files/list_folder", &json!({ "path": self.root, "recursive": true }))?;
        self.api.pages(first, |entry| {
            if let Entry::File(metadata) = entry {
                files.extend(self.file(metadata));
            }
            Ok(())
        })?;
        Ok(files)
    }

    fn delta(&self, cursor: &str) -> RemoteResult<Option<Delta>> {
        let first = match self.api.rpc("files/list_folder/continue", &json!({ "cursor": cursor })) {
            Err(RemoteError::Missing) => return Ok(None),
            page => page?,
        };
        let root = self.root.to_lowercase();
        let (mut changed, mut removed) = (BTreeMap::new(), Vec::new());
        let cursor = self.api.pages(first, |entry| {
            match entry {
                Entry::File(metadata) => {
                    if let Some(file) = self.file(metadata) {
                        changed.insert(file.path.clone(), file);
                    }
                }
                Entry::Deleted { path_lower, .. } if path_lower == root => {
                    return Err(RemoteError::Failed(format!("folder {} disappeared from Dropbox; pass stopped", self.root)));
                }
                Entry::Deleted { path_display, .. } => {
                    if let Some(path) = self.relative(&path_display) {
                        let folder = format!("{path}/");
                        changed.retain(|known: &String, _| known != &path && !known.starts_with(&folder));
                        removed.push(path);
                    }
                }
                Entry::Folder { .. } => {}
            }
            Ok(())
        })?;
        Ok(Some(Delta { changed: changed.into_values().collect(), removed, cursor }))
    }

    fn download(&self, file: &RemoteFile, out: &mut dyn Write) -> RemoteResult<()> {
        let arg = json!({ "path": format!("rev:{}", file.rev) });
        let response = self.api.call(&format!("{CONTENT}files/download"), Some(&arg), Body::Empty)?;
        copy(&mut response.into_reader(), out)
    }

    fn upload(
        &self,
        path: &str,
        content: &dyn Content,
        modified: u64,
        replaces: Option<&RemoteFile>,
    ) -> RemoteResult<RemoteFile> {
        let mode = match replaces {
            Some(file) => json!({ ".tag": "update", "update": file.rev }),
            None => json!("add"),
        };
        let commit = json!({
            "path": format!("{}/{path}", self.root),
            "mode": mode,
            "autorename": false,
            "strict_conflict": true,
            "mute": true,
            "client_modified": timestamp(modified)
        });
        let metadata = match content.byte_len() {
            length if length <= SINGLE_UPLOAD => {
                json(self.api.call(&format!("{CONTENT}files/upload"), Some(&commit), Body::Bytes(content, 0..length))?)?
            }
            _ => self.upload_parts(content, commit)?,
        };
        self.file(metadata)
            .ok_or_else(|| RemoteError::Failed(format!("Dropbox wrote {path} outside the sync folder")))
    }

    fn delete(&self, file: &RemoteFile) -> RemoteResult<()> {
        match self.api.rpc::<Value>(
            "files/delete_v2",
            &json!({ "path": format!("{}/{}", self.root, file.path), "parent_rev": file.rev }),
        ) {
            Err(RemoteError::Missing) => Ok(()),
            deleted => deleted.map(drop),
        }
    }

    fn checksum(&self, content: &dyn Content) -> io::Result<String> {
        content_hash(content)
    }

    fn remote_identity(&self) -> RemoteResult<Option<String>> {
        self.marker()
    }

    fn set_remote_identity(&self, id: &str) -> RemoteResult<()> {
        self.set_marker(id)
    }
}

#[cfg(test)]
impl Dropbox {
    /// Removes the whole test folder. Only used by the ignored live test.
    pub fn remove(&self) -> RemoteResult<()> {
        self.api
            .rpc::<Value>("files/delete_v2", &json!({ "path": self.root }))
            .map(drop)
    }
}

fn timestamp(modified: u64) -> String {
    let at = OffsetDateTime::from_unix_timestamp((modified / 1000) as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let format = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    at.format(&format).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::timestamp;

    #[test]
    fn modification_time_round_trips_as_utc() {
        assert_eq!(timestamp(1_767_225_601_000), "2026-01-01T00:00:01Z");
    }
}

#[allow(dead_code)]
fn _client_connect_placeholder() {}
