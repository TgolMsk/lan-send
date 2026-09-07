//! The SQLite database: transfer history, known devices and partial uploads
//! (ADR-0006). Schema changes are applied as numbered migrations tracked in
//! `PRAGMA user_version`.

use super::StoreError;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE devices (
    fingerprint   TEXT PRIMARY KEY,
    alias         TEXT NOT NULL,
    custom_alias  TEXT,
    device_type   TEXT,
    device_model  TEXT,
    version       TEXT,
    host          TEXT,
    port          INTEGER,
    protocol      TEXT,
    favorite      INTEGER NOT NULL DEFAULT 0,
    paired        INTEGER NOT NULL DEFAULT 0,
    first_seen    INTEGER NOT NULL,
    last_seen     INTEGER NOT NULL
);

CREATE TABLE transfers (
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,
    direction        TEXT NOT NULL,
    peer_fingerprint TEXT NOT NULL,
    peer_alias       TEXT NOT NULL,
    file_name        TEXT NOT NULL,
    path             TEXT,
    size             INTEGER NOT NULL,
    mime             TEXT NOT NULL,
    status           TEXT NOT NULL,
    error            TEXT,
    started_at       INTEGER NOT NULL,
    finished_at      INTEGER
);
CREATE INDEX transfers_started_at ON transfers (started_at DESC);

CREATE TABLE partial_uploads (
    part_path          TEXT PRIMARY KEY,
    final_path         TEXT NOT NULL,
    sender_fingerprint TEXT NOT NULL,
    file_name          TEXT NOT NULL,
    size               INTEGER NOT NULL,
    sha256             TEXT,
    received           INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);
CREATE INDEX partial_uploads_sender ON partial_uploads (sender_fingerprint, sha256, size);
";

/// Seconds since the Unix epoch, the timestamp format of every table.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Send,
    Receive,
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "send" => Self::Send,
            _ => Self::Receive,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferStatus {
    Finished,
    Failed,
    Cancelled,
    Skipped,
}

impl TransferStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "finished" => Self::Finished,
            "cancelled" => Self::Cancelled,
            "skipped" => Self::Skipped,
            _ => Self::Failed,
        }
    }
}

/// One file of one transfer session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferRecord {
    pub id: String,
    pub session_id: String,
    pub direction: Direction,
    pub peer_fingerprint: String,
    pub peer_alias: String,
    /// The name as offered, including any relative directory.
    pub file_name: String,
    /// The local file: where it was saved, or the source that was sent.
    pub path: Option<PathBuf>,
    pub size: u64,
    pub mime: String,
    pub status: TransferStatus,
    pub error: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

/// A device seen at least once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownDevice {
    pub fingerprint: String,
    pub alias: String,
    /// A name the user chose; shown instead of `alias` when set.
    pub custom_alias: Option<String>,
    pub device_type: Option<String>,
    pub device_model: Option<String>,
    pub version: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub protocol: Option<String>,
    pub favorite: bool,
    pub paired: bool,
    pub first_seen: i64,
    pub last_seen: i64,
}

impl KnownDevice {
    /// The name to display.
    pub fn display_name(&self) -> &str {
        self.custom_alias.as_deref().unwrap_or(&self.alias)
    }
}

/// An upload that did not complete; its `.part` file can be resumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialUpload {
    pub part_path: PathBuf,
    pub final_path: PathBuf,
    pub sender_fingerprint: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub received: u64,
    pub updated_at: i64,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens (creating and migrating as needed) the database at `path`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::from_connection(conn)
    }

    /// A private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, StoreError> {
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ----- transfers -----

    pub fn record_transfer(&self, record: &TransferRecord) -> Result<(), StoreError> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO transfers
             (id, session_id, direction, peer_fingerprint, peer_alias, file_name, path, size,
              mime, status, error, started_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.session_id,
                record.direction.as_str(),
                record.peer_fingerprint,
                record.peer_alias,
                record.file_name,
                record
                    .path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                to_i64(record.size),
                record.mime,
                record.status.as_str(),
                record.error,
                record.started_at,
                record.finished_at,
            ],
        )?;
        Ok(())
    }

    /// The most recent transfers, newest first.
    pub fn list_transfers(&self, limit: usize) -> Result<Vec<TransferRecord>, StoreError> {
        let conn = self.conn.lock();
        let mut statement = conn.prepare(
            "SELECT id, session_id, direction, peer_fingerprint, peer_alias, file_name, path,
                    size, mime, status, error, started_at, finished_at
             FROM transfers ORDER BY started_at DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![to_i64(limit as u64)], |row| {
            Ok(TransferRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                direction: Direction::parse(&row.get::<_, String>(2)?),
                peer_fingerprint: row.get(3)?,
                peer_alias: row.get(4)?,
                file_name: row.get(5)?,
                path: row.get::<_, Option<String>>(6)?.map(PathBuf::from),
                size: to_u64(row.get(7)?),
                mime: row.get(8)?,
                status: TransferStatus::parse(&row.get::<_, String>(9)?),
                error: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Deletes everything but the newest `keep` transfers. Returns how many
    /// were removed.
    pub fn prune_transfers(&self, keep: usize) -> Result<usize, StoreError> {
        let removed = self.conn.lock().execute(
            "DELETE FROM transfers WHERE id NOT IN
                (SELECT id FROM transfers ORDER BY started_at DESC, rowid DESC LIMIT ?1)",
            params![to_i64(keep as u64)],
        )?;
        Ok(removed)
    }

    pub fn delete_transfer(&self, id: &str) -> Result<bool, StoreError> {
        let removed = self
            .conn
            .lock()
            .execute("DELETE FROM transfers WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    pub fn clear_transfers(&self) -> Result<usize, StoreError> {
        Ok(self.conn.lock().execute("DELETE FROM transfers", [])?)
    }

    // ----- devices -----

    /// Inserts a device or refreshes what discovery learned about it. The
    /// user-owned columns (custom alias, favorite, paired, first seen) are
    /// kept.
    pub fn upsert_device(&self, device: &KnownDevice) -> Result<(), StoreError> {
        self.conn.lock().execute(
            "INSERT INTO devices
             (fingerprint, alias, custom_alias, device_type, device_model, version, host, port,
              protocol, favorite, paired, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(fingerprint) DO UPDATE SET
                alias = excluded.alias,
                device_type = excluded.device_type,
                device_model = excluded.device_model,
                version = excluded.version,
                host = excluded.host,
                port = excluded.port,
                protocol = excluded.protocol,
                last_seen = excluded.last_seen",
            params![
                device.fingerprint,
                device.alias,
                device.custom_alias,
                device.device_type,
                device.device_model,
                device.version,
                device.host,
                device.port.map(i64::from),
                device.protocol,
                device.favorite,
                device.paired,
                device.first_seen,
                device.last_seen,
            ],
        )?;
        Ok(())
    }

    pub fn device(&self, fingerprint: &str) -> Result<Option<KnownDevice>, StoreError> {
        let conn = self.conn.lock();
        let mut statement = conn.prepare(&format!("{DEVICE_SELECT} WHERE fingerprint = ?1"))?;
        statement
            .query_row(params![fingerprint], device_from_row)
            .optional()
            .map_err(Into::into)
    }

    /// Every known device, favorites first, then most recently seen.
    pub fn list_devices(&self) -> Result<Vec<KnownDevice>, StoreError> {
        let conn = self.conn.lock();
        let mut statement = conn.prepare(&format!(
            "{DEVICE_SELECT} ORDER BY favorite DESC, last_seen DESC"
        ))?;
        let rows = statement.query_map([], device_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Favorites plus devices seen within `recent`, whose addresses are
    /// worth probing at startup.
    pub fn devices_to_probe(&self, recent: Duration) -> Result<Vec<KnownDevice>, StoreError> {
        let since = unix_now() - to_i64(recent.as_secs());
        let conn = self.conn.lock();
        let mut statement = conn.prepare(&format!(
            "{DEVICE_SELECT} WHERE host IS NOT NULL AND (favorite = 1 OR last_seen >= ?1)"
        ))?;
        let rows = statement.query_map(params![since], device_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_favorite(&self, fingerprint: &str, favorite: bool) -> Result<bool, StoreError> {
        let changed = self.conn.lock().execute(
            "UPDATE devices SET favorite = ?2 WHERE fingerprint = ?1",
            params![fingerprint, favorite],
        )?;
        Ok(changed > 0)
    }

    pub fn set_paired(&self, fingerprint: &str, paired: bool) -> Result<bool, StoreError> {
        let changed = self.conn.lock().execute(
            "UPDATE devices SET paired = ?2 WHERE fingerprint = ?1",
            params![fingerprint, paired],
        )?;
        Ok(changed > 0)
    }

    pub fn set_custom_alias(
        &self,
        fingerprint: &str,
        custom_alias: Option<&str>,
    ) -> Result<bool, StoreError> {
        let changed = self.conn.lock().execute(
            "UPDATE devices SET custom_alias = ?2 WHERE fingerprint = ?1",
            params![fingerprint, custom_alias],
        )?;
        Ok(changed > 0)
    }

    pub fn remove_device(&self, fingerprint: &str) -> Result<bool, StoreError> {
        let removed = self.conn.lock().execute(
            "DELETE FROM devices WHERE fingerprint = ?1",
            params![fingerprint],
        )?;
        Ok(removed > 0)
    }

    // ----- partial uploads -----

    pub fn upsert_partial(&self, partial: &PartialUpload) -> Result<(), StoreError> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO partial_uploads
             (part_path, final_path, sender_fingerprint, file_name, size, sha256, received, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                partial.part_path.to_string_lossy().into_owned(),
                partial.final_path.to_string_lossy().into_owned(),
                partial.sender_fingerprint,
                partial.file_name,
                to_i64(partial.size),
                partial.sha256,
                to_i64(partial.received),
                partial.updated_at,
            ],
        )?;
        Ok(())
    }

    /// A resumable upload of the same content from the same sender.
    pub fn find_partial(
        &self,
        sender_fingerprint: &str,
        sha256: &str,
        size: u64,
    ) -> Result<Option<PartialUpload>, StoreError> {
        let conn = self.conn.lock();
        let mut statement = conn.prepare(&format!(
            "{PARTIAL_SELECT} WHERE sender_fingerprint = ?1 AND sha256 = ?2 COLLATE NOCASE
                              AND size = ?3 ORDER BY updated_at DESC LIMIT 1"
        ))?;
        statement
            .query_row(
                params![sender_fingerprint, sha256, to_i64(size)],
                partial_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn partial_by_path(&self, part_path: &Path) -> Result<Option<PartialUpload>, StoreError> {
        let conn = self.conn.lock();
        let mut statement = conn.prepare(&format!("{PARTIAL_SELECT} WHERE part_path = ?1"))?;
        statement
            .query_row(
                params![part_path.to_string_lossy().into_owned()],
                partial_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn remove_partial(&self, part_path: &Path) -> Result<bool, StoreError> {
        let removed = self.conn.lock().execute(
            "DELETE FROM partial_uploads WHERE part_path = ?1",
            params![part_path.to_string_lossy().into_owned()],
        )?;
        Ok(removed > 0)
    }

    /// Removes records older than `max_age` and returns them so the caller
    /// can delete the files.
    pub fn expire_partials(&self, max_age: Duration) -> Result<Vec<PartialUpload>, StoreError> {
        let cutoff = unix_now() - to_i64(max_age.as_secs());
        let conn = self.conn.lock();
        let expired = {
            let mut statement = conn.prepare(&format!("{PARTIAL_SELECT} WHERE updated_at < ?1"))?;
            let rows = statement.query_map(params![cutoff], partial_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        conn.execute(
            "DELETE FROM partial_uploads WHERE updated_at < ?1",
            params![cutoff],
        )?;
        Ok(expired)
    }
}

const DEVICE_SELECT: &str = "SELECT fingerprint, alias, custom_alias, device_type, device_model,
    version, host, port, protocol, favorite, paired, first_seen, last_seen FROM devices";

fn device_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnownDevice> {
    Ok(KnownDevice {
        fingerprint: row.get(0)?,
        alias: row.get(1)?,
        custom_alias: row.get(2)?,
        device_type: row.get(3)?,
        device_model: row.get(4)?,
        version: row.get(5)?,
        host: row.get(6)?,
        port: row
            .get::<_, Option<i64>>(7)?
            .and_then(|port| u16::try_from(port).ok()),
        protocol: row.get(8)?,
        favorite: row.get(9)?,
        paired: row.get(10)?,
        first_seen: row.get(11)?,
        last_seen: row.get(12)?,
    })
}

const PARTIAL_SELECT: &str = "SELECT part_path, final_path, sender_fingerprint, file_name, size,
    sha256, received, updated_at FROM partial_uploads";

fn partial_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PartialUpload> {
    Ok(PartialUpload {
        part_path: PathBuf::from(row.get::<_, String>(0)?),
        final_path: PathBuf::from(row.get::<_, String>(1)?),
        sender_fingerprint: row.get(2)?,
        file_name: row.get(3)?,
        size: to_u64(row.get(4)?),
        sha256: row.get(5)?,
        received: to_u64(row.get(6)?),
        updated_at: row.get(7)?,
    })
}

fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, started_at: i64) -> TransferRecord {
        TransferRecord {
            id: id.into(),
            session_id: "s".into(),
            direction: Direction::Receive,
            peer_fingerprint: "FP".into(),
            peer_alias: "Peer".into(),
            file_name: format!("{id}.bin"),
            path: Some(PathBuf::from("/tmp/x")),
            size: 42,
            mime: "application/octet-stream".into(),
            status: TransferStatus::Finished,
            error: None,
            started_at,
            finished_at: Some(started_at + 1),
        }
    }

    #[test]
    fn transfers_round_trip_and_prune() {
        let db = Database::open_in_memory().unwrap();
        for i in 0..5 {
            db.record_transfer(&record(&format!("t{i}"), 100 + i))
                .unwrap();
        }
        let listed = db.list_transfers(10).unwrap();
        assert_eq!(listed.len(), 5);
        assert_eq!(listed[0], record("t4", 104));
        assert_eq!(db.prune_transfers(2).unwrap(), 3);
        let ids: Vec<String> = db
            .list_transfers(10)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["t4", "t3"]);
        assert!(db.delete_transfer("t4").unwrap());
        assert_eq!(db.clear_transfers().unwrap(), 1);
    }

    #[test]
    fn devices_keep_user_columns() {
        let db = Database::open_in_memory().unwrap();
        let mut device = KnownDevice {
            fingerprint: "FP".into(),
            alias: "Old".into(),
            custom_alias: None,
            device_type: Some("desktop".into()),
            device_model: None,
            version: Some("2.2".into()),
            host: Some("10.0.0.1".into()),
            port: Some(53317),
            protocol: Some("https".into()),
            favorite: false,
            paired: false,
            first_seen: 1,
            last_seen: 1,
        };
        db.upsert_device(&device).unwrap();
        assert!(db.set_favorite("FP", true).unwrap());
        assert!(db.set_custom_alias("FP", Some("Mine")).unwrap());
        device.alias = "New".into();
        device.last_seen = 2;
        device.first_seen = 99;
        db.upsert_device(&device).unwrap();
        let stored = db.device("FP").unwrap().unwrap();
        assert_eq!(stored.alias, "New");
        assert_eq!(stored.display_name(), "Mine");
        assert!(stored.favorite);
        assert_eq!(stored.first_seen, 1);
        assert_eq!(stored.last_seen, 2);
        assert_eq!(
            db.devices_to_probe(Duration::from_secs(0)).unwrap().len(),
            1
        );
        assert!(db.remove_device("FP").unwrap());
        assert!(db.device("FP").unwrap().is_none());
    }

    #[test]
    fn partial_uploads_match_and_expire() {
        let db = Database::open_in_memory().unwrap();
        let partial = PartialUpload {
            part_path: PathBuf::from("/tmp/a.bin.lan-send.part"),
            final_path: PathBuf::from("/tmp/a.bin"),
            sender_fingerprint: "FP".into(),
            file_name: "a.bin".into(),
            size: 10,
            sha256: Some("ABC".into()),
            received: 4,
            updated_at: unix_now() - 10,
        };
        db.upsert_partial(&partial).unwrap();
        assert_eq!(
            db.find_partial("FP", "abc", 10).unwrap(),
            Some(partial.clone())
        );
        assert_eq!(db.find_partial("FP", "abc", 11).unwrap(), None);
        assert_eq!(
            db.expire_partials(Duration::from_secs(3600)).unwrap(),
            Vec::new()
        );
        assert_eq!(
            db.expire_partials(Duration::from_secs(1)).unwrap(),
            vec![partial.clone()]
        );
        assert_eq!(db.partial_by_path(&partial.part_path).unwrap(), None);
    }
}
