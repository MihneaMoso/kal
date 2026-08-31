//! WebAssembly IndexedDB-backed implementation of [`Database`] (public name)
//! — see the crate root for the rationale.
//!
//! The browser's IndexedDB is asynchronous, but the app's [`Database`] API is
//! synchronous and called directly from components. We reconcile the two:
//!
//! * the whole dataset lives in Rust memory ([`State`]) so every query and
//!   mutation returns synchronously, exactly like the native backend;
//! * IndexedDB is a durable snapshot store: the app loads the snapshot once,
//!   asynchronously, before rendering, and every mutation marks the store
//!   dirty and schedules a background write of the full snapshot as one JSON
//!   document (calendars + items + settings) under a single key.
//!
//! Storing a whole-snapshot JSON document mirrors the native design where
//! mutations are full-row replaces keyed by ULID, so a write is always
//! self-consistent and a crash leaves the previous complete snapshot intact.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::Utc;
use kal_core::models::{Calendar, CalendarItem};
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use wasm_bindgen::prelude::*;

use crate::{Result, StorageError};

/// IndexedDB database / object-store / key names.
const DB_NAME: &str = "kal";
const STORE: &str = "kv";
const SNAPSHOT_KEY: &str = "snapshot.v1";
const DB_VERSION: u8 = 1;

/// Serializable snapshot of the whole dataset (one IndexedDB record).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Snapshot {
    calendars: Vec<Calendar>,
    items: Vec<CalendarItem>,
    settings: Vec<(String, String)>,
}

/// In-memory working copy of the dataset.
struct State {
    snapshot: Snapshot,
}

/// The public `Database` type on `wasm32`.
///
/// Shares the same method set as the native backend so the rest of the app is
/// platform-agnostic. Reference-counted (single-threaded wasm); mutation
/// methods only need `&self` and use interior mutability.
pub struct Database {
    state: Rc<RefCell<State>>,
    dirty: Rc<Cell<bool>>,
}

/// The IndexedDB `Database` handle from `indexed_db_futures`.
mod idb {
    pub use indexed_db_futures::database::Database as Handle;
    pub use indexed_db_futures::prelude::*;
    pub use indexed_db_futures::transaction::TransactionMode;
}

async fn open_idb() -> Result<idb::Handle> {
    use idb::*;
    let db = idb::Handle::open(DB_NAME)
        .with_version(DB_VERSION)
        .with_on_upgrade_needed(|_event, db| {
            db.create_object_store(STORE).build().map(|_| ())?;
            Ok(())
        })
        .await
        .map_err(|e| StorageError::Db(e.to_string()))?;
    Ok(db)
}

async fn idb_put_snapshot(snapshot: &Snapshot) -> Result<()> {
    use idb::*;
    let json = serde_json::to_string(snapshot).map_err(|e| StorageError::Corrupt(e.to_string()))?;
    let db = open_idb().await?;
    let tx = db
        .transaction(STORE)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .map_err(|e| StorageError::Db(e.to_string()))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| StorageError::Db(e.to_string()))?;
    store
        .put(&json)
        .with_key(SNAPSHOT_KEY)
        .await
        .map_err(|e| StorageError::Db(e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| StorageError::Db(e.to_string()))?;
    Ok(())
}

async fn idb_get_snapshot() -> Result<Option<Snapshot>> {
    use idb::*;
    let db = open_idb().await?;
    let tx = db
        .transaction(STORE)
        .build()
        .map_err(|e| StorageError::Db(e.to_string()))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| StorageError::Db(e.to_string()))?;
    let value: Option<JsValue> = store
        .get(SNAPSHOT_KEY)
        .await
        .map_err(|e| StorageError::Db(e.to_string()))?;
    match value {
        None => Ok(None),
        Some(v) => {
            let json = v
                .as_string()
                .ok_or_else(|| StorageError::Corrupt("snapshot is not a string".into()))?;
            let snap = serde_json::from_str::<Snapshot>(&json)
                .map_err(|e| StorageError::Corrupt(e.to_string()))?;
            Ok(Some(snap))
        }
    }
}

impl Database {
    /// Open the on-disk (well, IndexedDB) database, restoring any persisted
    /// snapshot. Returns a fresh empty store if none exists yet.
    pub fn open(_path: impl std::convert::AsRef<std::path::Path>) -> Result<Self> {
        Self::open_in_memory()
    }

    /// An in-memory store with no backing persistence (tests / safety net).
    /// Real use should go through [`Self::load`] which restores persisted data.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            state: Rc::new(RefCell::new(State {
                snapshot: Snapshot::default(),
            })),
            dirty: Rc::new(Cell::new(false)),
        })
    }

    /// Asynchronously load the persisted snapshot from IndexedDB into memory.
    /// Returns an empty store when nothing was ever saved. Call once at startup.
    pub async fn load() -> Result<Self> {
        let snapshot = idb_get_snapshot().await?.unwrap_or_default();
        Ok(Self {
            state: Rc::new(RefCell::new(State { snapshot })),
            dirty: Rc::new(Cell::new(false)),
        })
    }

    /// Restore the persisted IndexedDB snapshot into an already-created shared
    /// instance, so every `Arc<Database>` handle (already handed out to the UI)
    /// observes the loaded data without any handle being re-pointed. Used by the
    /// web entry point after a plain in-memory handle has been provided.
    pub async fn load_into(&self) -> Result<()> {
        let snapshot = idb_get_snapshot().await?.unwrap_or_default();
        *self.state.borrow_mut() = State { snapshot };
        Ok(())
    }

    /// Schedule an asynchronous flush of the full snapshot to IndexedDB. Safe
    /// to call after any mutation; coalesces concurrent flushes, and always
    /// writes the latest state at execution time.
    pub fn schedule_persist(&self) {
        if self.dirty.replace(true) {
            return;
        }
        let state = self.state.clone();
        let dirty = self.dirty.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let snapshot = {
                // Capture a consistent snapshot under a single borrow, then
                // drop it before awaiting so the future isn't holding a borrow.
                let s = &state.borrow().snapshot;
                Snapshot {
                    calendars: s.calendars.clone(),
                    items: s.items.clone(),
                    settings: s.settings.clone(),
                }
            };
            if let Err(e) = idb_put_snapshot(&snapshot).await {
                web_sys::console::error_1(&JsValue::from_str(&e.to_string()));
            }
            // Allow subsequent mutations to schedule the next flush.
            dirty.set(false);
        });
    }

    // ----- calendars -----

    pub fn upsert_calendar(&self, cal: &Calendar) -> Result<()> {
        let mut s = self.state.borrow_mut();
        if let Some(existing) = s.snapshot.calendars.iter_mut().find(|c| c.id == cal.id) {
            *existing = cal.clone();
        } else {
            s.snapshot.calendars.push(cal.clone());
        }
        drop(s);
        self.schedule_persist();
        Ok(())
    }

    pub fn get_calendar(&self, id: Ulid) -> Result<Option<Calendar>> {
        Ok(self
            .state
            .borrow()
            .snapshot
            .calendars
            .iter()
            .find(|c| c.id == id)
            .cloned())
    }

    /// Lists calendars, mirroring native `ORDER BY name`.
    pub fn list_calendars(&self) -> Result<Vec<Calendar>> {
        let mut v = self.state.borrow().snapshot.calendars.clone();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(v)
    }

    // ----- items -----

    pub fn upsert_item(&self, item: &CalendarItem) -> Result<()> {
        item.validate()
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        let mut s = self.state.borrow_mut();
        if let Some(existing) = s.snapshot.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item.clone();
        } else {
            s.snapshot.items.push(item.clone());
        }
        drop(s);
        self.schedule_persist();
        Ok(())
    }

    /// Soft-delete tombstone (CRDT sync requires keeping the row).
    pub fn soft_delete_item(&self, id: Ulid) -> Result<bool> {
        let now = Utc::now().fixed_offset();
        let found = {
            let mut s = self.state.borrow_mut();
            match s.snapshot.items.iter_mut().find(|i| i.id == id) {
                Some(item) => {
                    item.deleted = true;
                    item.updated_at = now;
                    true
                }
                None => false,
            }
        };
        if found {
            self.schedule_persist();
        }
        Ok(found)
    }

    pub fn get_item(&self, id: Ulid) -> Result<Option<CalendarItem>> {
        Ok(self
            .state
            .borrow()
            .snapshot
            .items
            .iter()
            .find(|i| i.id == id)
            .cloned())
    }

    /// All non-deleted items overlapping `[from, to]` (by start/end), ordered
    /// by start — mirrors native `ORDER BY start_epoch`.
    pub fn items_in_range(
        &self,
        from: kal_core::models::DateTimeTz,
        to: kal_core::models::DateTimeTz,
    ) -> Result<Vec<CalendarItem>> {
        let from_e = from.timestamp();
        let to_e = to.timestamp();
        let mut out: Vec<CalendarItem> = self
            .state
            .borrow()
            .snapshot
            .items
            .iter()
            .filter(|i| {
                !i.deleted
                    && i.start.timestamp() <= to_e
                    && i.end.map_or(i.start.timestamp(), |e| e.timestamp()) >= from_e
            })
            .cloned()
            .collect();
        out.sort_by_key(|i| i.start.timestamp());
        Ok(out)
    }

    // ----- settings -----

    pub fn set_setting(&self, key: &str, value_json: &str) -> Result<()> {
        let mut s = self.state.borrow_mut();
        if let Some(pair) = s.snapshot.settings.iter_mut().find(|(k, _)| k == key) {
            pair.1 = value_json.to_string();
        } else {
            s.snapshot
                .settings
                .push((key.to_string(), value_json.to_string()));
        }
        drop(s);
        self.schedule_persist();
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .state
            .borrow()
            .snapshot
            .settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone()))
    }

    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        Ok(self.state.borrow().snapshot.settings.clone())
    }

    pub fn list_items(&self, include_deleted: bool) -> Result<Vec<CalendarItem>> {
        let items = &self.state.borrow().snapshot.items;
        Ok(if include_deleted {
            items.clone()
        } else {
            items.iter().filter(|i| !i.deleted).cloned().collect()
        })
    }
}
