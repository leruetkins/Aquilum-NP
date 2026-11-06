use crate::sync::remote::RemoteFile;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const SQLITE_PRAGMAS: &str = "PRAGMA journal_mode = WAL;
     PRAGMA synchronous = NORMAL;
     PRAGMA foreign_keys = ON;
     PRAGMA busy_timeout = 3000;
     PRAGMA journal_size_limit = 4194304;";

const SELECT: &str =
    "SELECT path, hash, size, modified, remote_id, remote_rev, checksum, remote_size, remote_modified FROM records";

pub struct Record {
    pub hash: String,
    pub size: u64,
    pub modified: u64,
    pub remote: RemoteFile,
}

pub struct Records {
    connection: Connection,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Memory {
    pub done: Option<u64>,
    pub large: Vec<Large>,
    pub size: Size,
    pub allowed: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Large {
    pub path: String,
    pub size: u64,
    pub kept: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub synced: u64,
    pub files: u64,
}

impl Records {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::prepare(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn memory() -> rusqlite::Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(connection: Connection) -> rusqlite::Result<Self> {
        connection.execute_batch(SQLITE_PRAGMAS)?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS records (
                 path TEXT PRIMARY KEY,
                 hash TEXT NOT NULL,
                 size INTEGER NOT NULL,
                 modified INTEGER NOT NULL,
                 remote_id TEXT NOT NULL,
                 remote_rev TEXT NOT NULL,
                 checksum TEXT NOT NULL,
                 remote_size INTEGER NOT NULL,
                 remote_modified INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Self { connection })
    }

    pub fn all(&self) -> rusqlite::Result<HashMap<String, Record>> {
        let mut statement = self.connection.prepare(SELECT)?;
        let rows = statement.query_map([], |row| record(row).map(|record| (record.remote.path.clone(), record)))?;
        rows.collect()
    }

    pub fn get(&self, path: &str) -> rusqlite::Result<Option<Record>> {
        self.connection.query_row(&format!("{SELECT} WHERE path = ?1"), [path], record).optional()
    }

    pub fn holds_under(&self, folder: &str) -> rusqlite::Result<bool> {
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE path > ?1 AND path < ?2)",
            [format!("{folder}/"), format!("{folder}0")],
            |row| row.get(0),
        )
    }

    pub fn put(&self, record: &Record) -> rusqlite::Result<()> {
        let remote = &record.remote;
        self.connection.prepare_cached("INSERT OR REPLACE INTO records VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?.execute(
            params![
                remote.path,
                record.hash,
                record.size as i64,
                record.modified as i64,
                remote.id,
                remote.rev,
                remote.checksum,
                remote.size as i64,
                remote.modified as i64
            ],
        )?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> rusqlite::Result<()> {
        self.connection.prepare_cached("DELETE FROM records WHERE path = ?1")?.execute([path]).map(|_| ())
    }

    pub fn forget(&self, path: &str) -> rusqlite::Result<()> {
        self.remove(path)
    }

    pub fn forget_stat(&self, path: &str) -> rusqlite::Result<()> {
        self.connection.execute("UPDATE records SET size = 0, modified = 0 WHERE path = ?1", [path]).map(|_| ())
    }

    pub fn forget_cursor(&self) -> rusqlite::Result<()> {
        self.connection.execute("DELETE FROM state WHERE key = 'cursor'", []).map(|_| ())
    }

    pub fn state(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.connection.query_row("SELECT value FROM state WHERE key = ?1", [key], |row| row.get(0)).optional()
    }

    pub fn recall(&self) -> Memory {
        self.state("memory").ok().flatten().and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
    }

    pub fn remember(&self, memory: &Memory) -> rusqlite::Result<()> {
        let json = serde_json::to_string(memory).unwrap_or_default();
        self.connection.execute("INSERT OR REPLACE INTO state VALUES ('memory', ?1)", [json]).map(|_| ())
    }

    pub fn commit(&self, cursor: &str, scope: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO state VALUES ('cursor', ?1), ('scope', ?2)",
            params![cursor, scope],
        ).map(|_| ())
    }
}

fn record(row: &Row<'_>) -> rusqlite::Result<Record> {
    Ok(Record {
        hash: row.get(1)?,
        size: row.get::<_, i64>(2)? as u64,
        modified: row.get::<_, i64>(3)? as u64,
        remote: RemoteFile {
            path: row.get(0)?,
            id: row.get(4)?,
            rev: row.get(5)?,
            checksum: row.get(6)?,
            size: row.get::<_, i64>(7)? as u64,
            modified: row.get::<_, i64>(8)? as u64,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(path: &str, rev: &str) -> RemoteFile {
        RemoteFile {
            path: path.to_owned(),
            id: format!("id:{path}"),
            rev: rev.to_owned(),
            checksum: "checksum".to_owned(),
            size: 10,
            modified: 1_700_000_000_000,
        }
    }

    #[test]
    fn stores_and_reads_a_record() {
        let records = Records::memory().unwrap();
        records
            .put(&Record { hash: "hash".into(), size: 10, modified: 20, remote: remote("a/x.md", "r1") })
            .unwrap();
        let stored = records.get("a/x.md").unwrap().unwrap();
        assert_eq!((stored.hash.as_str(), stored.size, stored.modified), ("hash", 10, 20));
        assert_eq!(stored.remote.rev, "r1");
        assert!(records.holds_under("a").unwrap());
        assert!(!records.holds_under("b").unwrap());
    }

    #[test]
    fn commits_cursor_and_scope_together() {
        let records = Records::memory().unwrap();
        records.commit("cursor-1", "trash=true").unwrap();
        assert_eq!(records.state("cursor").unwrap().as_deref(), Some("cursor-1"));
        assert_eq!(records.state("scope").unwrap().as_deref(), Some("trash=true"));
    }

    #[test]
    fn remembers_memory_and_forgets_records() {
        let records = Records::memory().unwrap();
        let memory = Memory {
            done: Some(1),
            large: vec![Large { path: "video.mp4".into(), size: 20, kept: true }],
            size: Size { synced: 10, files: 1 },
            allowed: vec!["video.mp4".into()],
        };
        records.remember(&memory).unwrap();
        assert_eq!(records.recall().done, Some(1));
        records
            .put(&Record { hash: "h".into(), size: 1, modified: 1, remote: remote("a.md", "r1") })
            .unwrap();
        records.forget("a.md").unwrap();
        assert!(records.get("a.md").unwrap().is_none());
    }
}
