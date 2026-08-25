//! SQLite persistence: schema migrations + repository layer.
//!
//! Sync-safe by design: all mutations go through `upsert_*` (full-row replace
//! keyed by ULID) so the future CRDT/sync layer can replay remote states with
//! the same API used locally.

use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use ulid::Ulid;

use kal_core::models::{Calendar, CalendarItem, Color, DateTimeTz};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("invalid stored data: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

impl From<StorageError> for rusqlite::Error {
    fn from(e: StorageError) -> Self {
        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
    }
}

/// Opens (creating if needed) and migrates a database file.
///
/// A `rusqlite::Connection` is `Send` but not `Sync`; the internal mutex
/// makes `Database` safely shareable (`Arc<Database>`) across async tasks
/// while serializing actual SQLite access.
pub struct Database {
    conn: std::sync::Mutex<Connection>,
}

/// Ordered migration list; `PRAGMA user_version` tracks applied count.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE calendars (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL,
    source      TEXT NOT NULL,
    visible     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE items (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL,
    title          TEXT NOT NULL,
    notes          TEXT,
    location       TEXT,
    calendar_id    TEXT NOT NULL REFERENCES calendars(id),
    start_epoch    INTEGER NOT NULL,
    start_rfc3339  TEXT NOT NULL,
    end_epoch      INTEGER,
    end_rfc3339    TEXT,
    all_day        INTEGER NOT NULL DEFAULT 0,
    rrule          TEXT,
    exdates_json   TEXT NOT NULL DEFAULT '[]',
    completed_epoch INTEGER,
    completed_rfc3339 TEXT,
    reminders_json TEXT NOT NULL DEFAULT '[]',
    color_override TEXT,
    created_epoch  INTEGER NOT NULL,
    created_rfc3339 TEXT NOT NULL,
    updated_epoch  INTEGER NOT NULL,
    updated_rfc3339 TEXT NOT NULL,
    deleted        INTEGER NOT NULL DEFAULT 0,
    metadata_json  TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_items_start ON items(start_epoch);
CREATE INDEX idx_items_calendar ON items(calendar_id);
"#,
    // v2: calendar LWW timestamp for sync.
    r#"
ALTER TABLE calendars ADD COLUMN updated_epoch INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendars ADD COLUMN updated_rfc3339 TEXT NOT NULL DEFAULT '';
"#,
    // v3: device-local settings (theme, time format, week start, default view…).
    r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
];

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::with_connection(conn)
    }

    /// In-memory database, useful for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(conn: Connection) -> Result<Self> {
        {
            let c = &conn;
            c.pragma_update(None, "journal_mode", "WAL")?;
            c.pragma_update(None, "foreign_keys", "ON")?;
        }
        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let target = (i + 1) as i64;
            if version < target {
                conn.execute_batch(sql)?;
                conn.pragma_update(None, "user_version", target)?;
            }
        }
        Ok(())
    }

    // ----- calendars -----

    pub fn upsert_calendar(&self, cal: &Calendar) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO calendars (id, name, color, source, visible, updated_epoch, updated_rfc3339)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, color=excluded.color,
                source=excluded.source, visible=excluded.visible,
                updated_epoch=excluded.updated_epoch, updated_rfc3339=excluded.updated_rfc3339",
            params![cal.id.to_string(), cal.name, cal.color.as_str(),
                    serde_json::to_string(&cal.source).unwrap(), cal.visible as i64,
                    epoch(&cal.updated_at), cal.updated_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_calendar(&self, id: Ulid) -> Result<Option<Calendar>> {
        let conn = self.conn.lock().unwrap();
        conn
            .query_row(
                "SELECT id, name, color, source, visible, updated_epoch, updated_rfc3339 FROM calendars WHERE id = ?1",
                params![id.to_string()],
                row_to_calendar,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_calendars(&self) -> Result<Vec<Calendar>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color, source, visible, updated_epoch, updated_rfc3339 FROM calendars ORDER BY name")?;
        let rows = stmt.query_map([], row_to_calendar)?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .collect())
    }

    // ----- items -----

    pub fn upsert_item(&self, item: &CalendarItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        item.validate()
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        conn.execute(
            "INSERT INTO items (
                id, kind, title, notes, location, calendar_id,
                start_epoch, start_rfc3339, end_epoch, end_rfc3339,
                all_day, rrule, exdates_json, completed_epoch, completed_rfc3339,
                reminders_json, color_override,
                created_epoch, created_rfc3339, updated_epoch, updated_rfc3339,
                deleted, metadata_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)
             ON CONFLICT(id) DO UPDATE SET
                kind=excluded.kind, title=excluded.title, notes=excluded.notes,
                location=excluded.location, calendar_id=excluded.calendar_id,
                start_epoch=excluded.start_epoch, start_rfc3339=excluded.start_rfc3339,
                end_epoch=excluded.end_epoch, end_rfc3339=excluded.end_rfc3339,
                all_day=excluded.all_day, rrule=excluded.rrule,
                exdates_json=excluded.exdates_json,
                completed_epoch=excluded.completed_epoch,
                completed_rfc3339=excluded.completed_rfc3339,
                reminders_json=excluded.reminders_json,
                color_override=excluded.color_override,
                created_epoch=excluded.created_epoch, created_rfc3339=excluded.created_rfc3339,
                updated_epoch=excluded.updated_epoch, updated_rfc3339=excluded.updated_rfc3339,
                deleted=excluded.deleted, metadata_json=excluded.metadata_json",
            params![
                item.id.to_string(),
                serde_json::to_string(&item.kind).unwrap(),
                item.title,
                item.notes,
                item.location,
                item.calendar_id.to_string(),
                epoch(&item.start),
                item.start.to_rfc3339(),
                item.end.as_ref().map(epoch),
                item.end.as_ref().map(|d| d.to_rfc3339()),
                item.all_day as i64,
                item.rrule,
                serde_json::to_string(&item.exdates).unwrap(),
                item.completed.as_ref().map(epoch),
                item.completed.as_ref().map(|d| d.to_rfc3339()),
                serde_json::to_string(&item.reminders).unwrap(),
                item.color_override.as_ref().map(|c| c.to_string()),
                epoch(&item.created_at),
                item.created_at.to_rfc3339(),
                epoch(&item.updated_at),
                item.updated_at.to_rfc3339(),
                item.deleted as i64,
                serde_json::to_string(&item.metadata).unwrap(),
            ],
        )?;
        Ok(())
    }

    /// Soft-delete tombstone (CRDT sync requires keeping the row).
    pub fn soft_delete_item(&self, id: Ulid) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().fixed_offset();
        let n = conn.execute(
            "UPDATE items SET deleted=1, updated_epoch=?2, updated_rfc3339=?3 WHERE id=?1",
            params![id.to_string(), epoch(&now), now.to_rfc3339()],
        )?;
        Ok(n > 0)
    }

    pub fn get_item(&self, id: Ulid) -> Result<Option<CalendarItem>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT * FROM items WHERE id = ?1",
            params![id.to_string()],
            row_to_item,
        )
        .optional()
        .map_err(Into::into)
    }

    /// All non-deleted items overlapping `[from, to]` (by start/end epochs).
    pub fn items_in_range(&self, from: DateTimeTz, to: DateTimeTz) -> Result<Vec<CalendarItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT * FROM items
             WHERE deleted=0 AND start_epoch <= ?2 AND COALESCE(end_epoch, start_epoch) >= ?1
             ORDER BY start_epoch",
        )?;
        let rows = stmt.query_map(params![epoch(&from), epoch(&to)], row_to_item)?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .collect())
    }

    // ----- settings -----

    pub fn set_setting(&self, key: &str, value_json: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value_json],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn list_items(&self, include_deleted: bool) -> Result<Vec<CalendarItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(if include_deleted {
            "SELECT * FROM items"
        } else {
            "SELECT * FROM items WHERE deleted=0"
        })?;
        let rows = stmt.query_map([], row_to_item)?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .collect())
    }
}

fn epoch(dt: &DateTimeTz) -> i64 {
    dt.timestamp()
}

fn from_epoch(secs: i64, rfc3339: &str) -> Result<DateTimeTz> {
    // Prefer the stored RFC3339 form so the original UTC offset survives;
    // fall back to the epoch if parsing fails.
    match DateTime::parse_from_rfc3339(rfc3339) {
        Ok(dt) => Ok(dt),
        Err(_) => Utc
            .timestamp_opt(secs, 0)
            .single()
            .map(|dt| dt.fixed_offset())
            .ok_or_else(|| StorageError::Corrupt(format!("bad timestamp {rfc3339}"))),
    }
}

fn row_to_calendar(row: &rusqlite::Row<'_>) -> rusqlite::Result<Calendar> {
    let id: String = row.get(0)?;
    // v2 columns may be absent in legacy rows, and rows written before v2
    // carry the migration DEFAULT '' here. Both must fall back to epoch 0
    // instead of failing the whole read ("premature end of input").
    let _upd_e: Option<i64> = row.get(5).ok();
    let upd_s: Option<String> = row.get(6).ok();
    let updated_at = upd_s
        .as_deref()
        .filter(|r| !r.is_empty())
        .and_then(|r| DateTime::parse_from_rfc3339(r).ok())
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap().fixed_offset());
    Ok(Calendar {
        id: Ulid::from_str(&id).map_err(|e| StorageError::Corrupt(e.to_string()))?,
        name: row.get(1)?,
        color: Color(row.get(2)?),
        source: serde_json::from_str(&row.get::<_, String>(3)?)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?,
        visible: row.get::<_, i64>(4)? != 0,
        updated_at,
    })
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarItem> {
    let get_dt = |sec_idx: usize, str_idx: usize| -> rusqlite::Result<DateTimeTz> {
        let secs: Option<i64> = row.get(sec_idx)?;
        let s: Option<String> = row.get(str_idx)?;
        match (secs, s) {
            (Some(e), Some(r)) => from_epoch(e, &r).map_err(rusqlite::Error::from),
            _ => Err(StorageError::Corrupt("missing required datetime".into()).into()),
        }
    };
    let opt_dt = |sec_idx: usize, str_idx: usize| -> rusqlite::Result<Option<DateTimeTz>> {
        let secs: Option<i64> = row.get(sec_idx)?;
        let s: Option<String> = row.get(str_idx)?;
        match (secs, s) {
            (None, None) => Ok(None),
            (Some(e), Some(r)) => Ok(Some(from_epoch(e, &r)?)),
            _ => Err(StorageError::Corrupt("inconsistent nullable datetime".into()).into()),
        }
    };

    let ulid_of = |idx: usize| -> rusqlite::Result<Ulid> {
        let s: String = row.get(idx)?;
        let id = Ulid::from_str(&s)
            .map_err(|e| StorageError::Corrupt(e.to_string()))
            .map_err(rusqlite::Error::from)?;
        Ok(id)
    };

    Ok(CalendarItem {
        id: ulid_of(0)?,
        kind: serde_json::from_str(&row.get::<_, String>(1)?)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?,
        title: row.get(2)?,
        notes: row.get(3)?,
        location: row.get(4)?,
        calendar_id: ulid_of(5)?,
        start: get_dt(6, 7)?,
        end: opt_dt(8, 9)?,
        all_day: row.get::<_, i64>(10)? != 0,
        rrule: row.get(11)?,
        exdates: serde_json::from_str(&row.get::<_, String>(12)?)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?,
        completed: opt_dt(13, 14)?,
        reminders: serde_json::from_str(&row.get::<_, String>(15)?)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?,
        color_override: row.get::<_, Option<String>>(16)?.map(Color),
        created_at: get_dt(17, 18)?,
        updated_at: get_dt(19, 20)?,
        deleted: row.get::<_, i64>(21)? != 0,
        metadata: serde_json::from_str(&row.get::<_, String>(22)?)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?,
    })
}
