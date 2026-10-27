//! Stable C ABI over Kal's core (spec §5.6 / §7 phase 7; the mobile-rewrite
//! bridge).
//!
//! Native widget shims (Android AppWidgetProvider via JNI, iOS WidgetKit via
//! Swift) and — from the Kotlin Android rewrite on — the Kotlin interface layer
//! link this crate and call plain C functions. Strings are UTF-8 JSON allocated
//! by Rust and freed by the caller with [`kal_free`]; the DB handle is an opaque
//! pointer created by [`kal_open`].
//!
//! ## Contract rules (guide §6)
//!
//! - **Versioned.** [`kal_bridge_version`] returns [`KAL_BRIDGE_VERSION`]; the
//!   Kotlin side refuses to proceed on a mismatch instead of running against an
//!   unknown ABI. Within a major version only *additive* changes are allowed.
//! - **Boring types only.** UTF-8 C strings, 64-bit ints and status codes cross
//!   the boundary. Structured data is JSON text built from the core's own
//!   tolerant models (`kal_core::models`), so no lossy mapping exists between
//!   the DB and the interface layer.
//! - **No panics and no undefined behavior across the boundary.** Every export
//!   is wrapped in `catch_unwind`; failures return NULL (query) or a negative
//!   status (command) plus a message readable with [`kal_last_error`].
//! - **Missing data is not an error.** `kal_get_setting`/`kal_item_json` return
//!   the JSON literal `null` for "absent"; only genuine failures set the error
//!   channel.
//!
//! All functions are `extern "C"`, never panic across the boundary, and return
//! NULL on error.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::ptr;

use kal_core::models::{Calendar, CalendarItem, Color, DateTimeTz, ItemKind, Occurrence, Ulid};
use kal_storage::scoped::DeleteScope;
use kal_storage::Database;

/// Monotonic bridge schema version.
///
/// Bumped whenever the Kotlin declaration set changes: the app compares this
/// value at startup and refuses to run against an unknown ABI. Additive JSON
/// fields do not need a bump; a new required entry point does. The compiled
/// Kotlin app reads this at startup through [`kal_bridge_version`].
pub const KAL_BRIDGE_VERSION: u32 = 2;

/// Opaque database handle.
pub struct KalDb {
    db: Database,
    /// The path the handle was opened with. The sync identity, node config and
    /// outbox live next to it — the same files the Dioxus app wrote, so a
    /// device keeps its chain across the rewrite.
    db_path: std::path::PathBuf,
}

thread_local! {
    /// Last error message for *this* thread, if the previous call failed.
    /// Thread-local because core calls may arrive from any JNI-attached thread.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

fn set_error(msg: impl Into<String>) {
    // NUL cannot appear in a C string; sanitize rather than lose the message.
    let sanitized = msg.into().replace('\0', " ");
    let c = CString::new(sanitized).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

/// Return the bridge schema version. Kotlin compares this against its own
/// expected major version before touching any other export.
#[no_mangle]
pub extern "C" fn kal_bridge_version() -> u32 {
    KAL_BRIDGE_VERSION
}

/// Last error message set by a failed call on the calling thread, as a freshly
/// allocated C string (free with [`kal_free`]), or NULL when the last call
/// succeeded. Never returns a Rust panic — the message is a plain copy.
#[no_mangle]
pub extern "C" fn kal_last_error() -> *mut c_char {
    LAST_ERROR.with(|e| match e.borrow().as_ref() {
        Some(msg) => msg.clone().into_raw(),
        None => ptr::null_mut(),
    })
}

/// Run a query and map its outcome onto the C boundary: JSON string on success,
/// NULL + error channel on failure, instead of an unwinding panic.
fn run_json<F>(f: F) -> *mut c_char
where
    F: FnOnce() -> Result<String, String>,
{
    // The channel describes *this* call only: clearing it means a stale failure
    // can never be mistaken for the outcome of the next one.
    clear_error();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(json)) => match CString::new(json) {
            Ok(s) => s.into_raw(),
            Err(_) => {
                set_error("bridge payload contained a NUL byte");
                ptr::null_mut()
            }
        },
        Ok(Err(e)) => {
            set_error(e);
            ptr::null_mut()
        }
        Err(_) => {
            set_error("panic caught at the Kal bridge boundary");
            ptr::null_mut()
        }
    }
}

/// Run a mutating command: 0 on success, -1 + error channel on failure.
fn run_status<F>(f: F) -> i32
where
    F: FnOnce() -> Result<(), String>,
{
    clear_error();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            set_error(e);
            -1
        }
        Err(_) => {
            set_error("panic caught at the Kal bridge boundary");
            -1
        }
    }
}

/// Guard a query against a NULL/foreign handle, then run it.
fn with_db<F>(db: *mut KalDb, f: F) -> *mut c_char
where
    F: FnOnce(&KalDb) -> Result<String, String>,
{
    if db.is_null() {
        clear_error();
        set_error("null database handle");
        return ptr::null_mut();
    }
    let kal = unsafe { &*db };
    run_json(move || f(kal))
}

/// Guard a command against a NULL/foreign handle, then run it.
fn with_db_status<F>(db: *mut KalDb, f: F) -> i32
where
    F: FnOnce(&KalDb) -> Result<(), String>,
{
    if db.is_null() {
        clear_error();
        set_error("null database handle");
        return -1;
    }
    let kal = unsafe { &*db };
    run_status(move || f(kal))
}

/// Read a NUL-terminated UTF-8 argument, reporting NULL pointers as errors.
fn cstr_arg(p: *const c_char, what: &str) -> Result<String, String> {
    if p.is_null() {
        return Err(format!("null {what}"));
    }
    Ok(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// Parse a ULID argument, reporting the offending value on failure.
fn parse_ulid(s: &str) -> Result<ulid::Ulid, String> {
    s.parse::<ulid::Ulid>()
        .map_err(|e| format!("invalid ULID {s:?}: {e}"))
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Open (or create) the calendar database at `path`. Returns NULL on failure.
///
/// # Safety
/// `path` must be a valid NUL-terminated UTF-8 C string pointer.
#[no_mangle]
pub unsafe extern "C" fn kal_open(path: *const c_char) -> *mut KalDb {
    clear_error();
    if path.is_null() {
        set_error("null database path");
        return ptr::null_mut();
    }
    let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        let db = Database::open(&path).map_err(|e| format!("could not open database: {e}"))?;
        Ok::<_, String>((db, std::path::PathBuf::from(path)))
    }));
    match opened {
        Ok(Ok((db, db_path))) => Box::into_raw(Box::new(KalDb { db, db_path })),
        Ok(Err(e)) => {
            set_error(e);
            ptr::null_mut()
        }
        Err(_) => {
            set_error("panic caught at the Kal bridge boundary");
            ptr::null_mut()
        }
    }
}

/// Close a handle returned by `kal_open` and NULL out the caller's slot,
/// making double-close safe.
///
/// # Safety
/// `db` must point at a slot holding either NULL or a live `kal_open` handle.
#[no_mangle]
pub unsafe extern "C" fn kal_close(db: *mut *mut KalDb) {
    if db.is_null() {
        return;
    }
    let handle = unsafe { *db };
    unsafe { *db = ptr::null_mut() };
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Free a string returned by any `kal_*` function (including
/// [`kal_last_error`]).
///
/// # Safety
/// `s` must be NULL or a pointer from this crate.
#[no_mangle]
pub unsafe extern "C" fn kal_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// ---------------------------------------------------------------------------
// Calendar data (JSON, the core's own model shape)
// ---------------------------------------------------------------------------

/// All calendars as a JSON array of the core `Calendar` model.
/// Returns NULL on error; free with kal_free().
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_calendars_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let cals = kal.db.list_calendars().map_err(|e| e.to_string())?;
        serde_json::to_string(&cals).map_err(|e| e.to_string())
    })
}

/// All items as a JSON array of the core `CalendarItem` model.
/// `include_deleted` non-zero includes tombstoned rows. Returns NULL on error.
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_items_json(db: *mut KalDb, include_deleted: i32) -> *mut c_char {
    with_db(db, |kal| {
        let items = kal
            .db
            .list_items(include_deleted != 0)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&items).map_err(|e| e.to_string())
    })
}

/// One item as a JSON object, or the literal `null` when it does not exist.
/// Returns NULL only on error; free with kal_free().
///
/// # Safety
/// `db` must be a live handle and `id` a NUL-terminated ULID string.
#[no_mangle]
pub unsafe extern "C" fn kal_item_json(db: *mut KalDb, id: *const c_char) -> *mut c_char {
    with_db(db, |kal| {
        let id = parse_ulid(&cstr_arg(id, "item id")?)?;
        let item = kal.db.get_item(id).map_err(|e| e.to_string())?;
        serde_json::to_string(&item).map_err(|e| e.to_string())
    })
}

/// Insert or replace an item from its JSON model (full-row upsert, the
/// sync-ready write path). Returns 0 on success, -1 on error.
///
/// # Safety
/// `db` must be a live handle and `json` a NUL-terminated JSON `CalendarItem`.
#[no_mangle]
pub unsafe extern "C" fn kal_upsert_item_json(db: *mut KalDb, json: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let text = cstr_arg(json, "item json")?;
        let item: CalendarItem =
            serde_json::from_str(&text).map_err(|e| format!("invalid item json: {e}"))?;
        kal.db.upsert_item(&item).map_err(|e| e.to_string())
    })
}

/// Tombstone an item (the row is never physically removed): 1 when a row was
/// written, 0 when no such id exists, -1 on error. Re-deleting refreshes the
/// tombstone's LWW timestamp — which is exactly what keeps the deletion
/// propagating through sync — so deleting a known id twice also returns 1.
///
/// Deleting a recurring series also tombstones the occurrences detached from it
/// by "This event", so a series delete cannot leave copies behind; deleting a
/// detached occurrence deletes only that occurrence.
///
/// # Safety
/// `db` must be a live handle and `id` a NUL-terminated ULID string.
#[no_mangle]
pub unsafe extern "C" fn kal_soft_delete_item(db: *mut KalDb, id: *const c_char) -> i32 {
    clear_error();
    if db.is_null() {
        set_error("null database handle");
        return -1;
    }
    let kal = unsafe { &*db };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let id = parse_ulid(&cstr_arg(id, "item id")?)?;
        // Deleting a series also tombstones the occurrences detached from it;
        // deleting one occurrence leaves the series alone.
        match kal.db.delete_with_exceptions(id) {
            Ok(true) => Ok(1),
            Ok(false) => Ok(0),
            Err(e) => Err(e.to_string()),
        }
    }));
    match outcome {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            set_error(e);
            -1
        }
        Err(_) => {
            set_error("panic caught at the Kal bridge boundary");
            -1
        }
    }
}

/// Delete one item or a scoped part of a recurring series. The JSON form is
/// `{id, occurrenceStart?, scope?}`; scope is `all` (default), `this_only` or
/// `this_and_following`.
///
/// Unlike [`kal_soft_delete_item`] this is scope-aware: `this_only` EXDATEs one
/// occurrence (or tombstones one detached copy), `this_and_following` truncates
/// the series before the clicked occurrence and removes later detached copies,
/// and `all` from a detached copy deletes its whole series. Returns 0 on success
/// (including an already-absent id) and -1 with the reason on the error channel.
///
/// # Safety
/// `db` must be a live handle and `json` a NUL-terminated [`DeleteItemForm`].
#[no_mangle]
pub unsafe extern "C" fn kal_delete_item_json(db: *mut KalDb, json: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let text = cstr_arg(json, "delete form json")?;
        let form: DeleteItemForm =
            serde_json::from_str(&text).map_err(|e| format!("invalid delete form: {e}"))?;
        let id = parse_ulid(&form.id)?;
        let occurrence = match form.occurrence_start.as_deref().filter(|s| !s.is_empty()) {
            Some(text) => Some(
                chrono::DateTime::parse_from_rfc3339(text)
                    .map_err(|e| format!("invalid occurrence start {text:?}: {e}"))?,
            ),
            None => None,
        };
        let scope = match form.scope.as_deref() {
            None | Some("") | Some("all") => DeleteScope::All,
            Some("this_only") => DeleteScope::ThisOnly,
            Some("this_and_following") => DeleteScope::ThisAndFollowing,
            Some(other) => return Err(format!("unknown delete scope {other:?}")),
        };
        // A missing id is idempotent success: the editor only needs to know the
        // item is gone.
        kal.db
            .delete_scoped(id, occurrence, scope)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Insert or replace a calendar from its JSON model. Returns 0 / -1.
///
/// # Safety
/// `db` must be a live handle and `json` a NUL-terminated JSON `Calendar`.
#[no_mangle]
pub unsafe extern "C" fn kal_upsert_calendar_json(db: *mut KalDb, json: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let text = cstr_arg(json, "calendar json")?;
        let cal: Calendar =
            serde_json::from_str(&text).map_err(|e| format!("invalid calendar json: {e}"))?;
        kal.db.upsert_calendar(&cal).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// Settings (device-local key/value store)
// ---------------------------------------------------------------------------

/// Read a setting as JSON: the literal `null` when absent, else a JSON string.
/// Returns NULL only on error; free with kal_free().
///
/// # Safety
/// `db` must be a live handle and `key` a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn kal_get_setting(db: *mut KalDb, key: *const c_char) -> *mut c_char {
    with_db(db, |kal| {
        let key = cstr_arg(key, "setting key")?;
        let value = kal.db.get_setting(&key).map_err(|e| e.to_string())?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    })
}

/// Write a setting (plain string value, exactly as `set_setting` stores it —
/// callers must NOT pre-JSON-encode, per the `""val""` corruption gotcha in
/// RULES.md). Returns 0 / -1.
///
/// # Safety
/// `db` must be a live handle; `key`/`value` NUL-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn kal_set_setting(
    db: *mut KalDb,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    with_db_status(db, |kal| {
        let key = cstr_arg(key, "setting key")?;
        let value = cstr_arg(value, "setting value")?;
        kal.db.set_setting(&key, &value).map_err(|e| e.to_string())
    })
}

/// Every setting as one JSON object of `key -> string`. NULL on error.
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_settings_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let pairs = kal.db.all_settings().map_err(|e| e.to_string())?;
        let map: serde_json::Map<String, serde_json::Value> = pairs
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        serde_json::to_string(&map).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// Expanded views (widget shims use these today; the app reuses them)
// ---------------------------------------------------------------------------

/// Upcoming visible items overlapping `[from_epoch, to_epoch]` as a JSON
/// array: `[{id,title,start,end,allDay,kind,color,age}]` sorted by start.
/// `kind` ∈ event|task|birthday; `age` is set for birthdays only.
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_upcoming_json(
    db: *mut KalDb,
    from_epoch: i64,
    to_epoch: i64,
) -> *mut c_char {
    with_db(db, |kal| {
        let offset = *chrono::Local::now().offset();
        let from = epoch_to_tz(from_epoch, offset);
        let to = epoch_to_tz(to_epoch, offset);
        // Expand occurrences (recurring rules included) rather than querying
        // raw rows so yearly birthdays etc. surface correctly.
        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;

        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cal_by_id: std::collections::HashMap<_, _> =
            calendars.iter().map(|c| (c.id, c)).collect();

        let occs =
            kal_core::viewmodel::occurrences_by_date(&items, from.date_naive(), to.date_naive());
        // Flatten grouped occurrences back into display entries.
        let mut entries: Vec<serde_json::Value> = Vec::new();
        for day_occs in occs.values() {
            for occ in day_occs {
                if let Some(item) = items.iter().find(|i| i.id == occ.item_id) {
                    let color = item
                        .effective_color(cal_by_id.get(&item.calendar_id).copied())
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    entries.push(serde_json::json!({
                        "id": item.id.to_string(),
                        "title": item.title,
                        "start": occ.start.to_rfc3339(),
                        "end": occ.end.map(|e| e.to_rfc3339()),
                        "allDay": item.all_day,
                        "kind": format!("{:?}", item.kind).to_lowercase(),
                        "color": color,
                        "age": item.birthday_age_at(occ.start),
                    }));
                }
            }
        }
        entries.sort_by_key(|e| e["start"].as_str().map(str::to_string));
        serde_json::to_string(&entries).map_err(|e| e.to_string())
    })
}

/// Month grid as JSON rows × 7 days:
/// `[{date:"YYYY-MM-DD", inMonth:bool, items:[{id,title,time,color}]}, ...]`.
/// Always 6 weeks; `first_dow` 0 = Monday … 6 = Sunday.
///
/// This is the **widget** shape and is frozen by the key-set assertions in
/// `tests/bridge.rs`; the app shell calls [`kal_month_json`], which returns the
/// richer shared entry (with `calendarId`, `kind` and a raw start instant).
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_month_grid_json(
    db: *mut KalDb,
    year: i32,
    month: u32,
    first_dow: u8,
) -> *mut c_char {
    with_db(db, |kal| {
        use chrono::Datelike;

        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cal_by_id = calendar_index(&calendars);
        let cells = month_occurrences(&items, year, month, first_weekday(first_dow));

        let mut rows = Vec::with_capacity(cells.len());
        for week in &cells {
            let mut row = Vec::with_capacity(7);
            for (date, occs) in week {
                let items_json: Vec<serde_json::Value> = occs
                    .iter()
                    .filter_map(|o| {
                        items.iter().find(|i| i.id == o.item_id).map(|i| {
                            serde_json::json!({
                                "id": i.id.to_string(),
                                "title": i.title,
                                "time": if i.all_day { "".into() } else {
                                    o.start.format("%H:%M").to_string()
                                },
                                "allDay": i.all_day,
                                "color": i
                                    .effective_color(cal_by_id.get(&i.calendar_id).copied())
                                    .map(|c| c.to_string())
                                    .unwrap_or_default(),
                            })
                        })
                    })
                    .collect();
                row.push(serde_json::json!({
                    "date": date.format("%Y-%m-%d").to_string(),
                    "inMonth": date.month() == month,
                    "items": items_json,
                }));
            }
            rows.push(row);
        }
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// App-facing expanded views (the Kotlin shell renders these directly)
//
// The widget exports above stay frozen; these carry the shared display entry
// below, which adds what the interface layer needs and the widgets never did:
// the owning calendar (so the client can apply the calendar-visibility filter
// exactly where the Dioxus shell did), the item kind (birthday chips) and the
// raw start instant (the client formats times from the user's preference).
// ---------------------------------------------------------------------------

/// Index calendars by id so an entry can resolve its effective colour.
fn calendar_index(calendars: &[Calendar]) -> std::collections::HashMap<Ulid, &Calendar> {
    calendars.iter().map(|c| (c.id, c)).collect()
}

/// Map the wire convention `first_dow` (0 = Monday … 6 = Sunday) to a weekday.
fn first_weekday(first_dow: u8) -> chrono::Weekday {
    use chrono::Weekday;
    match first_dow % 7 {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// A month's cells as `(date, occurrences)` in display order. Shared by the
/// widget and app month payloads, which differ only in how each entry is shaped.
fn month_occurrences(
    items: &[CalendarItem],
    year: i32,
    month: u32,
    first_day: chrono::Weekday,
) -> Vec<Vec<(chrono::NaiveDate, Vec<Occurrence>)>> {
    let grid = kal_core::viewmodel::month_grid(year, month, first_day);
    let first = grid[0][0];
    let last = grid[kal_core::viewmodel::MONTH_GRID_WEEKS - 1][6];
    let occ_map = kal_core::viewmodel::occurrences_by_date(items, first, last);
    grid.into_iter()
        .map(|week| {
            week.into_iter()
                .map(|date| {
                    let occs = occ_map.get(&date).cloned().unwrap_or_default();
                    (date, occs)
                })
                .collect()
        })
        .collect()
}

/// The display entry every app view renders: one expanded occurrence joined
/// with its item and effective colour.
fn display_entry(
    item: &CalendarItem,
    occ: &Occurrence,
    cals: &std::collections::HashMap<Ulid, &Calendar>,
) -> serde_json::Value {
    serde_json::json!({
        "id": item.id.to_string(),
        "calendarId": item.calendar_id.to_string(),
        "title": item.title,
        "start": occ.start.to_rfc3339(),
        "end": occ.end.map(|e| e.to_rfc3339()),
        "allDay": item.all_day,
        "kind": format!("{:?}", item.kind).to_lowercase(),
        "completed": item.completed.is_some(),
        "color": item
            .effective_color(cals.get(&item.calendar_id).copied())
            .map(|c| c.to_string())
            .unwrap_or_default(),
        "overrideColor": item.color_override.as_ref().map(ToString::to_string),
        "age": item.birthday_age_at(occ.start),
    })
}

/// Shape occurrences into display entries, skipping ids with no live item.
fn display_entries(
    items: &[CalendarItem],
    occs: &[Occurrence],
    cals: &std::collections::HashMap<Ulid, &Calendar>,
) -> Vec<serde_json::Value> {
    occs.iter()
        .filter_map(|o| {
            items
                .iter()
                .find(|i| i.id == o.item_id)
                .map(|i| display_entry(i, o, cals))
        })
        .collect()
}

/// Parse a `YYYY-MM-DD` argument, reporting the offending value on failure.
fn parse_date_arg(value: &str, what: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| format!("invalid {what} {value:?} (want YYYY-MM-DD): {e}"))
}

/// Month grid for the app shell: 6 rows × 7 days of
/// `{date, inMonth, items:[entry]}`. `first_dow` 0 = Monday … 6 = Sunday.
/// Returns NULL on error.
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_month_json(
    db: *mut KalDb,
    year: i32,
    month: u32,
    first_dow: u8,
) -> *mut c_char {
    with_db(db, |kal| {
        use chrono::Datelike;

        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cals = calendar_index(&calendars);
        let cells = month_occurrences(&items, year, month, first_weekday(first_dow));

        let rows: Vec<Vec<serde_json::Value>> = cells
            .iter()
            .map(|week| {
                week.iter()
                    .map(|(date, occs)| {
                        serde_json::json!({
                            "date": date.format("%Y-%m-%d").to_string(),
                            "inMonth": date.month() == month,
                            "items": display_entries(&items, occs, &cals),
                        })
                    })
                    .collect()
            })
            .collect();
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    })
}

/// One week as 7 days of `{date, items:[entry]}` starting at `week_start`
/// (`YYYY-MM-DD`). Returns NULL on error.
///
/// # Safety
/// `db` must be a live handle and `week_start` a NUL-terminated date.
#[no_mangle]
pub unsafe extern "C" fn kal_week_json(db: *mut KalDb, week_start: *const c_char) -> *mut c_char {
    with_db(db, |kal| {
        let start = parse_date_arg(&cstr_arg(week_start, "week start")?, "week start")?;

        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cals = calendar_index(&calendars);

        let map = kal_core::viewmodel::occurrences_by_date(
            &items,
            start,
            start + chrono::Duration::days(6),
        );
        let days: Vec<serde_json::Value> = (0..7)
            .map(|i| {
                let date = start + chrono::Duration::days(i);
                let occs = map.get(&date).cloned().unwrap_or_default();
                serde_json::json!({
                    "date": date.format("%Y-%m-%d").to_string(),
                    "items": display_entries(&items, &occs, &cals),
                })
            })
            .collect();
        serde_json::to_string(&days).map_err(|e| e.to_string())
    })
}

/// One day for the day view: `{date, allDay:[entry], timed:[positioned]}` where
/// each positioned entry is `{item:entry, topFrac, heightFrac, lane, lanes}`
/// (fractions of a 24 h canvas, same layout the Dioxus day view used).
/// Returns NULL on error.
///
/// # Safety
/// `db` must be a live handle and `date` a NUL-terminated `YYYY-MM-DD`.
#[no_mangle]
pub unsafe extern "C" fn kal_day_json(db: *mut KalDb, date: *const c_char) -> *mut c_char {
    with_db(db, |kal| {
        let day = parse_date_arg(&cstr_arg(date, "day")?, "day")?;

        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cals = calendar_index(&calendars);

        let occs = kal_core::viewmodel::occurrences_by_date(&items, day, day)
            .get(&day)
            .cloned()
            .unwrap_or_default();

        // All-day items render in their own strip; timed ones get grid
        // positions (minimum height 0.02 of the canvas, as before).
        let is_all_day = |o: &Occurrence| {
            items
                .iter()
                .find(|i| i.id == o.item_id)
                .map(|i| i.all_day)
                .unwrap_or(false)
        };
        let (all_day, timed): (Vec<Occurrence>, Vec<Occurrence>) =
            occs.iter().cloned().partition(is_all_day);

        let day_start = day
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(*chrono::Local::now().offset())
            .single()
            .unwrap_or_else(|| chrono::Utc::now().fixed_offset());
        let positioned = kal_core::viewmodel::layout_day(timed, day_start, 0.02);

        let timed_json: Vec<serde_json::Value> = positioned
            .iter()
            .filter_map(|p| items.iter().find(|i| i.id == p.occ.item_id).map(|i| (i, p)))
            .map(|(item, p)| {
                serde_json::json!({
                    "item": display_entry(item, &p.occ, &cals),
                    "topFrac": p.top_frac,
                    "heightFrac": p.height_frac,
                    "lane": p.lane,
                    "lanes": p.lanes,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "date": day.format("%Y-%m-%d").to_string(),
            "allDay": display_entries(&items, &all_day, &cals),
            "timed": timed_json,
        });
        serde_json::to_string(&payload).map_err(|e| e.to_string())
    })
}

/// Agenda rows for `[from, from + days]` (inclusive, as the agenda view's
/// "next 30 days" window is), capped at `cap` rows, in date-then-time order:
/// `[{date, item:entry}, ...]`. Returns NULL on error.
///
/// # Safety
/// `db` must be a live handle and `from` a NUL-terminated `YYYY-MM-DD`.
#[no_mangle]
pub unsafe extern "C" fn kal_agenda_json(
    db: *mut KalDb,
    from: *const c_char,
    days: i64,
    cap: i64,
) -> *mut c_char {
    with_db(db, |kal| {
        let from_date = parse_date_arg(&cstr_arg(from, "agenda start")?, "agenda start")?;
        let to_date = from_date + chrono::Duration::days(days.max(0));
        let cap = cap.max(0) as usize;

        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let cals = calendar_index(&calendars);

        // A BTreeMap gives date order; each day's list is already time-sorted.
        let map = kal_core::viewmodel::occurrences_by_date(&items, from_date, to_date);
        let mut rows: Vec<serde_json::Value> = Vec::new();
        'row: for (date, occs) in &map {
            for occ in occs {
                let Some(item) = items.iter().find(|i| i.id == occ.item_id) else {
                    continue;
                };
                if rows.len() == cap {
                    break 'row;
                }
                rows.push(serde_json::json!({
                    "date": date.format("%Y-%m-%d").to_string(),
                    "item": display_entry(item, occ, &cals),
                }));
            }
        }
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// editor writes (app-facing)
// ---------------------------------------------------------------------------

/// The editor form, exactly as the Kotlin editor collects it. It mirrors the
/// Dioxus `ui::EditorState`, so the write semantics — default-calendar
/// resolution, load-or-create, recurring scope — stay native while the form
/// widgets themselves live in Kotlin (STEP0 §0.19.1/§0.19.2).
///
/// Absent fields fall back to sensible defaults, so a caller only sends what
/// its form shows.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SaveItemForm {
    /// `None`/absent creates a new item.
    id: Option<String>,
    /// Start instant of the *clicked occurrence* (RFC 3339) — drives scoped
    /// edits, and differs from the series base start for recurring items.
    occurrence_start: Option<String>,
    /// `event` | `task` | `birthday`.
    kind: String,
    title: String,
    /// `YYYY-MM-DD`; times are read in the device's current local offset.
    date: String,
    /// `HH:MM`, or empty for "no time" (all-day items and tasks).
    start_time: String,
    /// `HH:MM`, or empty for "no end" (tasks and all-day items).
    end_time: String,
    all_day: bool,
    /// Absent/empty/`null` resolves to the first visible live calendar.
    calendar_id: Option<String>,
    /// `None` for other kinds.
    location: String,
    /// Birthday linkage (`metadata.birthday_of`); ignored for other kinds.
    person: String,
    /// Final RFC 5545 rule; absent/empty/`null` means "doesn't repeat".
    rrule: Option<String>,
    /// Selected reminder presets, in minutes before the start.
    reminders: Vec<i64>,
    /// Raw "Custom reminder (minutes before)" text; used when positive.
    custom_reminder: String,
    /// `#RRGGBB` override; absent/empty/`null` means "use the calendar colour".
    color: Option<String>,
    /// `all` | `this_only` | `this_and_following`.
    scope: Option<String>,
}

/// One scoped delete request. `scope` is `all` (default), `this_only` or
/// `this_and_following`; `occurrenceStart` is the clicked occurrence for a
/// recurring item.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DeleteItemForm {
    id: String,
    occurrence_start: Option<String>,
    scope: Option<String>,
}

/// Read a form date/time the way the Dioxus editor did: an empty time means
/// midnight, and the instant resolves in the device's current local offset
/// (DST gaps/overlaps have no single answer, so they read as invalid).
fn parse_form_when(date: &str, time: &str) -> Option<DateTimeTz> {
    use chrono::{NaiveDate, NaiveTime};
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let t = if time.is_empty() {
        NaiveTime::from_hms_opt(0, 0, 0)?
    } else {
        NaiveTime::parse_from_str(time, "%H:%M").ok()?
    };
    d.and_time(t)
        .and_local_timezone(*chrono::Local::now().offset())
        .single()
        .map(|dt| dt.fixed_offset())
}

/// The first visible live calendar, else any live one, else a freshly created
/// "Personal" — never a tombstoned row and never `Ulid::nil()`, which would
/// fail the items' calendar foreign key.
fn default_calendar_id(kal: &KalDb) -> Result<Ulid, String> {
    let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
    let found = calendars
        .iter()
        .filter(|c| !c.deleted)
        .find(|c| c.visible)
        .or_else(|| calendars.iter().find(|c| !c.deleted));
    if let Some(c) = found {
        return Ok(c.id);
    }
    let cal = Calendar::local("Personal", Color("#3366cc".into()));
    let id = cal.id;
    kal.db.upsert_calendar(&cal).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Turn a form into the item to persist. When editing, the row is loaded first
/// so the fields the form does not show (notes, completion, creation time,
/// EXDATEs…) survive the save.
fn item_from_form(
    kal: &KalDb,
    form: &SaveItemForm,
    start: DateTimeTz,
    end: Option<DateTimeTz>,
) -> Result<CalendarItem, String> {
    let kind: ItemKind = serde_json::from_value(serde_json::Value::String(form.kind.clone()))
        .map_err(|_| format!("unknown item kind {:?}", form.kind))?;
    let calendar_id = match form.calendar_id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => parse_ulid(id)?,
        None => default_calendar_id(kal)?,
    };

    let mut item = match form.id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => kal
            .db
            .get_item(parse_ulid(id)?)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| CalendarItem::new(kind, "", calendar_id, start)),
        None => CalendarItem::new(kind, "", calendar_id, start),
    };

    item.kind = kind;
    item.title = form.title.clone();
    item.location = if form.location.is_empty() {
        None
    } else {
        Some(form.location.clone())
    };
    item.calendar_id = calendar_id;
    item.start = start;
    item.end = end;
    item.all_day = form.all_day;
    if kind == ItemKind::Birthday {
        item.metadata.birthday_of = Some(form.person.clone());
    }
    item.rrule = match form.rrule.as_deref() {
        Some(rule) if !rule.is_empty() => Some(rule.to_string()),
        _ => None,
    };
    item.reminders = {
        let mut mins = form.reminders.clone();
        if let Ok(m) = form.custom_reminder.trim().parse::<i64>() {
            if m > 0 && !mins.contains(&m) {
                mins.push(m);
            }
        }
        mins.sort_unstable();
        mins.dedup();
        mins.into_iter()
            .map(kal_core::models::Reminder::minutes_before)
            .collect()
    };
    item.color_override = match form.color.as_deref() {
        Some(hex) if !hex.is_empty() => Some(Color::new(hex).map_err(|e| e.to_string())?),
        _ => None,
    };
    item.updated_at = chrono::Utc::now().fixed_offset();
    Ok(item)
}

/// Save the editor form: create or update one item, applying the requested
/// scope for a recurring series. Returns 0 on success, -1 on failure with the
/// message on [`kal_last_error`] — the Kotlin editor keeps its dialog open and
/// shows it, rather than the Dioxus editor's silent no-op.
///
/// # Safety
/// `db` must be a live handle and `json` a NUL-terminated [`SaveItemForm`].
#[no_mangle]
pub unsafe extern "C" fn kal_save_item_json(db: *mut KalDb, json: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let text = cstr_arg(json, "save form json")?;
        let form: SaveItemForm =
            serde_json::from_str(&text).map_err(|e| format!("invalid save form: {e}"))?;

        let start = parse_form_when(&form.date, &form.start_time).ok_or_else(|| {
            format!(
                "invalid start {:?} {:?} (want YYYY-MM-DD and HH:MM)",
                form.date, form.start_time
            )
        })?;
        // An empty end time means "no end" (tasks and all-day items), not
        // midnight: the Dioxus editor read it as midnight, which made
        // `end < start` and so rejected every end-less item — silently, since
        // it swallowed the error, so a new task could never be created at all.
        // `CalendarItem.end` is optional and the views already render an absent
        // end as "09:00 – ", so "no end" is the intended reading.
        let end = if form.end_time.is_empty() {
            None
        } else {
            parse_form_when(&form.date, &form.end_time)
        };
        let item = item_from_form(kal, &form, start, end)?;

        let occurrence = match form.occurrence_start.as_deref().filter(|s| !s.is_empty()) {
            Some(text) => Some(
                chrono::DateTime::parse_from_rfc3339(text)
                    .map_err(|e| format!("invalid occurrence start {text:?}: {e}"))?,
            ),
            None => None,
        };

        // A scope without a clicked occurrence falls back to a plain save,
        // exactly as the Dioxus editor's catch-all arm did.
        match (form.scope.as_deref(), form.id.as_deref(), occurrence) {
            // "All events" rewrites the series row itself and carries the
            // dialog's shared fields to the occurrences detached from it (see
            // `kal_storage::scoped::apply_all_edit`). The form is anchored on the
            // clicked occurrence, so the series keeps its own date; saving the
            // occurrence's date as the new start moved the series forward and
            // deleted everything before it. A missing row (raced with a delete,
            // or a plain item) saves plainly, as before.
            (Some("all"), Some(_), _) => {
                if !kal.db.apply_all_edit(&item).map_err(|e| e.to_string())? {
                    kal.db.upsert_item(&item).map_err(|e| e.to_string())?;
                }
            }
            // "This event" on an occurrence that is already detached from its
            // series: the row is the occurrence, so update it in place — running
            // the EXDATE-and-copy path again would detach it a second time.
            (Some("this_only"), Some(_), _) if item.metadata.series_id.is_some() => {
                kal.db.save_exception(&item).map_err(|e| e.to_string())?;
            }
            (Some("this_only"), Some(base), Some(occ_start)) => {
                if !kal
                    .db
                    .apply_scoped_edit(parse_ulid(base)?, &item, occ_start, false)
                    .map_err(|e| e.to_string())?
                {
                    return Err(format!("no series {base} to split"));
                }
            }
            // Splitting a series from a modified occurrence is not supported
            // (the editor hides the scope); refuse rather than detach it again.
            (Some("this_and_following"), Some(_), _) if item.metadata.series_id.is_some() => {
                return Err("…and following is not available on an edited occurrence".into());
            }
            (Some("this_and_following"), Some(base), Some(occ_start)) => {
                if !kal
                    .db
                    .apply_scoped_edit(parse_ulid(base)?, &item, occ_start, true)
                    .map_err(|e| e.to_string())?
                {
                    return Err(format!("no series {base} to truncate"));
                }
            }
            _ => kal.db.upsert_item(&item).map_err(|e| e.to_string())?,
        }
        Ok(())
    })
}

/// Flip a task's completion and refresh its LWW timestamp (the row's checkbox).
/// Returns 0 on success, -1 on failure with the message on [`kal_last_error`].
///
/// # Safety
/// `db` must be a live handle and `id` a NUL-terminated ULID string.
#[no_mangle]
pub unsafe extern "C" fn kal_toggle_item_completed_json(db: *mut KalDb, id: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let id = parse_ulid(&cstr_arg(id, "item id")?)?;
        let mut item = kal
            .db
            .get_item(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no item {id}"))?;
        let now = chrono::Utc::now().fixed_offset();
        item.completed = if item.completed.is_some() {
            None
        } else {
            Some(now)
        };
        item.updated_at = now;
        kal.db.upsert_item(&item).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// Reminder firings (the Kotlin AlarmManager backend)
// ---------------------------------------------------------------------------

/// One reminder firing, in the camelCase shape the app-facing payloads use.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderFiringPayload {
    item_id: String,
    reminder_id: String,
    /// RFC 3339 with a numeric offset (the instant the alarm must fire).
    fire_at: String,
    title: String,
}

/// Every reminder firing in `[now, now + horizon_days]`, nearest first, capped
/// at `cap` (a non-positive `cap` means "no cap"). This is
/// [`kal_core::reminders::compute_firings`] over every live item.
///
/// Deliberately **not** filtered by calendar visibility: the Dioxus reconcile
/// effect passed the unfiltered items resource, so a reminder on a hidden
/// calendar still fires. Past firings are dropped (D20: missed reminders are
/// not caught up), completed tasks and tombstoned items stay silent, and the
/// core clamps `horizon_days` to its own recurrence horizon.
///
/// # Safety
/// `db` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn kal_reminder_firings_json(
    db: *mut KalDb,
    horizon_days: i64,
    cap: i64,
) -> *mut c_char {
    with_db(db, |kal| {
        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let now = chrono::Local::now().fixed_offset();
        let firings = kal_core::reminders::compute_firings(&items, now, horizon_days.max(0));

        let mut payload = Vec::new();
        for firing in firings {
            if cap > 0 && payload.len() >= cap as usize {
                break;
            }
            payload.push(ReminderFiringPayload {
                item_id: firing.item_id.to_string(),
                reminder_id: firing.reminder_id.to_string(),
                fire_at: firing.fire_at.to_rfc3339(),
                title: firing.title,
            });
        }
        serde_json::to_string(&payload).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// ICS import / export (the sidebar's Import/Export section)
// ---------------------------------------------------------------------------

/// Parse an .ics payload and store it as one new calendar plus its items.
///
/// Semantics are the Dioxus sidebar's `import_ics_file`: one `IcsImport`
/// calendar (named from the file's `X-WR-CALNAME`, else `calendar_name`),
/// visible, and every parsed item upserted. An item the store rejects is
/// skipped and the rest still land — the original's `eprintln!`-and-continue.
/// 0 on success, -1 on failure with the reason on [`kal_last_error`]
/// (a parse failure, a dead handle, a NULL argument).
///
/// # Safety
/// `db` must be a live handle and both strings NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn kal_import_ics_json(
    db: *mut KalDb,
    text: *const c_char,
    calendar_name: *const c_char,
) -> i32 {
    with_db_status(db, |kal| {
        let text = cstr_arg(text, "ics text")?;
        let name = cstr_arg(calendar_name, "calendar name")?;
        let imported = kal_import::import_ics(&text, &name).map_err(|e| e.to_string())?;
        kal.db
            .upsert_calendar(&imported.calendar)
            .map_err(|e| e.to_string())?;
        for item in &imported.items {
            if let Err(e) = kal.db.upsert_item(item) {
                eprintln!("kal-ffi: skipped imported item {:?}: {e}", item.title);
            }
        }
        Ok(())
    })
}

/// Write every calendar and live item to `path` as one VCALENDAR, the text the
/// system share sheet hands out. The Rust side owns the file (as in the Dioxus
/// build); the interface layer only chooses where and shares it.
/// 0 on success, -1 on failure with the reason on [`kal_last_error`].
///
/// # Safety
/// `db` must be a live handle and `path` NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn kal_export_ics_file(db: *mut KalDb, path: *const c_char) -> i32 {
    with_db_status(db, |kal| {
        let path = cstr_arg(path, "export path")?;
        let calendars = kal.db.list_calendars().map_err(|e| e.to_string())?;
        let items = kal.db.list_items(false).map_err(|e| e.to_string())?;
        let ics = kal_import::export_all(&calendars, &items);
        std::fs::write(&path, ics).map_err(|e| format!("write {path} failed: {e}"))
    })
}

// ---------------------------------------------------------------------------
// Update check (the settings screen's Software & updates section)
// ---------------------------------------------------------------------------

/// The update check's JSON, built over an injected transport:
///
///   {"status": "up_to_date"}
///   {"status": "update_ready", "version": "0.3.0"}
///   {"status": "error", "message": "update check failed: …"}
///
/// Public so the bridge tests can feed a canned release instead of the network;
/// [`kal_check_updates_json`] calls it with the real transport. The download is
/// written to `dest` only when the release is strictly newer and its digest
/// verifies.
pub fn check_updates_json_with(
    transport: &dyn kal_update::Transport,
    token: &str,
    current: &str,
    dest: &std::path::Path,
) -> String {
    let payload = match kal_update::check_with(transport, token, current, dest) {
        kal_update::CheckOutcome::UpToDate { .. } => serde_json::json!({ "status": "up_to_date" }),
        kal_update::CheckOutcome::Ready { version } => {
            serde_json::json!({ "status": "update_ready", "version": version })
        }
        kal_update::CheckOutcome::Failed { message } => {
            serde_json::json!({ "status": "error", "message": message })
        }
    };
    payload.to_string()
}

/// Check GitHub for a newer release and stage it into `dest_path`, one blocking
/// pass (the Dioxus button downloaded before it reported too). `current_version`
/// comes from the interface layer's own build, so an APK versioned differently
/// from the Rust crate still compares correctly.
///
/// The status is always a JSON object; NULL is reserved for a NULL argument or
/// an unsupported platform, with the reason on [`kal_last_error`].
///
/// # Safety
/// Both arguments must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn kal_check_updates_json(
    current_version: *const c_char,
    dest_path: *const c_char,
) -> *mut c_char {
    run_json(|| {
        let current = cstr_arg(current_version, "current version")?;
        let dest = cstr_arg(dest_path, "download path")?;
        let token = kal_update::platform_token()?;
        Ok(check_updates_json_with(
            &kal_update::Http,
            token,
            &current,
            std::path::Path::new(&dest),
        ))
    })
}

// ---------------------------------------------------------------------------
// Sync chain (the sidebar's Sync panel)
// ---------------------------------------------------------------------------

/// Load this handle's chain identity, if any. Never creates one.
fn sync_identity(kal: &KalDb) -> Option<kal_sync::ChainIdentity> {
    kal_sync::identity_file::load_identity_at(&kal.db_path)
}

/// `{"phrase": "…", "fingerprint": "…"}` — the recovery phrase plus the
/// chain's stable fingerprint.
fn sync_identity_json(identity: &kal_sync::ChainIdentity) -> String {
    serde_json::json!({
        "phrase": identity.phrase(),
        "fingerprint": identity.fingerprint().fingerprint_hex,
    })
    .to_string()
}

/// Pairing status as JSON:
///
///   {"paired": false, "fingerprint": null}
///   {"paired": true,  "fingerprint": "…"}
///
/// Reading status never mints an identity: a fresh install stays unpaired
/// until the user starts or joins a chain.
///
/// # Safety
/// `db` must be a handle returned by [`kal_open`] (or NULL, which is refused).
#[no_mangle]
pub unsafe extern "C" fn kal_sync_status_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let identity = sync_identity(kal);
        Ok(serde_json::json!({
            "paired": identity.is_some(),
            "fingerprint": identity.map(|i| i.fingerprint().fingerprint_hex),
        })
        .to_string())
    })
}

/// Start a brand-new chain: generate a 24-word phrase, persist it next to the
/// database and return it (shown once for the user to write down).
///
/// # Safety
/// `db` must be a handle returned by [`kal_open`] (or NULL, which is refused).
#[no_mangle]
pub unsafe extern "C" fn kal_sync_start_chain_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let identity = kal_sync::ChainIdentity::generate().map_err(|e| e.to_string())?;
        kal_sync::identity_file::save_identity_at(&kal.db_path, &identity)?;
        Ok(sync_identity_json(&identity))
    })
}

/// Join an existing chain by its 24-word recovery phrase.
///
/// # Safety
/// `phrase` must be a valid NUL-terminated UTF-8 C string pointer.
#[no_mangle]
pub unsafe extern "C" fn kal_sync_join_chain_json(
    db: *mut KalDb,
    phrase: *const c_char,
) -> *mut c_char {
    with_db(db, move |kal| {
        let phrase = cstr_arg(phrase, "recovery phrase")?;
        let identity = kal_sync::ChainIdentity::from_phrase(phrase.trim())
            .map_err(|_| "Invalid recovery phrase".to_string())?;
        kal_sync::identity_file::save_identity_at(&kal.db_path, &identity)?;
        Ok(sync_identity_json(&identity))
    })
}

/// Reveal the stored recovery phrase (for pairing another device).
///
/// # Safety
/// `db` must be a handle returned by [`kal_open`] (or NULL, which is refused).
#[no_mangle]
pub unsafe extern "C" fn kal_sync_phrase_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let identity = sync_identity(kal).ok_or("not paired")?;
        Ok(sync_identity_json(&identity))
    })
}

/// Leave the chain: delete the local identity + outbox and drop the cached
/// transport. Local-only — the gossip chain has no central membership.
///
/// # Safety
/// `db` must be a handle returned by [`kal_open`] (or NULL, which is refused).
#[no_mangle]
pub unsafe extern "C" fn kal_sync_leave(db: *mut KalDb) -> i32 {
    with_db_status(db, |kal| {
        kal_sync::identity_file::clear_identity_at(&kal.db_path)?;
        kal_sync::client::reset_transport();
        Ok(())
    })
}

/// Run one blocking sync round: waits up to ~15 s for a gossip peer, then
/// drains and merges for up to ~12 s. Call from a background thread.
///
///   {"merged": N}   — N peer snapshots merged (0 = nothing new)
///
/// Returns NULL + the reason on failure (e.g. "not paired", "no peers yet").
///
/// # Safety
/// `db` must be a handle returned by [`kal_open`] (or NULL, which is refused).
#[no_mangle]
pub unsafe extern "C" fn kal_sync_round_json(db: *mut KalDb) -> *mut c_char {
    with_db(db, |kal| {
        let identity = sync_identity(kal).ok_or("not paired")?;
        let merged = kal_sync::client::sync_round(&identity, &kal.db_path, &kal.db)?;
        Ok(serde_json::json!({ "merged": merged }).to_string())
    })
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn epoch_to_tz(epoch: i64, offset: chrono::FixedOffset) -> kal_core::models::DateTimeTz {
    use chrono::TimeZone;
    offset
        .timestamp_opt(epoch, 0)
        .single()
        .unwrap_or_else(|| chrono::Utc::now().fixed_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panic inside any guarded query/command must become NULL/-1 plus an
    /// error message, never an abort or an unwinding panic across the boundary.
    #[test]
    fn panics_are_caught_at_the_boundary() {
        let ptr = run_json(|| panic!("boom inside the core"));
        assert!(ptr.is_null(), "panic must degrade to NULL");
        let err = kal_last_error();
        assert!(!err.is_null());
        let msg = unsafe { CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned();
        unsafe { kal_free(err) };
        assert!(msg.contains("panic"), "{msg}");

        let status = run_status(|| panic!("boom inside the core"));
        assert_eq!(status, -1);
        let err = kal_last_error();
        assert!(!err.is_null());
        unsafe { kal_free(err) };
    }

    /// The error channel is per-thread and cleared by the next call, so a
    /// failure on one theme/JNI thread cannot leak into another.
    #[test]
    fn error_channel_is_thread_local_and_clears() {
        clear_error();
        assert!(kal_last_error().is_null());
        set_error("first failure");
        assert!(run_status(|| Ok(())).eq(&0));
        assert!(kal_last_error().is_null(), "success clears the error");

        set_error("thread A failure");
        let other = std::thread::spawn(|| kal_last_error().is_null())
            .join()
            .unwrap();
        assert!(other, "other threads see their own empty channel");
        let err = kal_last_error();
        assert!(!err.is_null());
        unsafe { kal_free(err) };
    }

    /// Facility commands (no DB) must still be panic-guarded and report the
    /// version the Kotlin side handshakes against.
    #[test]
    fn version_surface_is_stable() {
        assert_eq!(kal_bridge_version(), KAL_BRIDGE_VERSION);
        assert_eq!(KAL_BRIDGE_VERSION, 2);
    }

    /// NULL handles must set the error channel rather than dereferencing.
    #[test]
    fn null_handle_reports_error() {
        let ptr = with_db(ptr::null_mut(), |_| Ok("never".into()));
        assert!(ptr.is_null());
        let err = kal_last_error();
        assert!(!err.is_null(), "null handle must explain itself");
        unsafe { kal_free(err) };
    }

    /// Every export the Kotlin side declares must exist with the expected
    /// signature; taking its address fails compilation if one is renamed or
    /// removed. (The symbol-vs-Kotlin-declaration check lands with the Kotlin
    /// project in phase 1; this is the Rust half of that gate.)
    #[test]
    fn exported_symbols_are_present() {
        let exports: [*const (); 33] = [
            kal_bridge_version as *const (),
            kal_last_error as *const (),
            kal_open as *const (),
            kal_close as *const (),
            kal_free as *const (),
            kal_calendars_json as *const (),
            kal_items_json as *const (),
            kal_item_json as *const (),
            kal_upsert_item_json as *const (),
            kal_soft_delete_item as *const (),
            kal_delete_item_json as *const (),
            kal_upsert_calendar_json as *const (),
            kal_get_setting as *const (),
            kal_set_setting as *const (),
            kal_settings_json as *const (),
            kal_upcoming_json as *const (),
            kal_month_grid_json as *const (),
            kal_month_json as *const (),
            kal_week_json as *const (),
            kal_day_json as *const (),
            kal_agenda_json as *const (),
            kal_save_item_json as *const (),
            kal_toggle_item_completed_json as *const (),
            kal_reminder_firings_json as *const (),
            kal_import_ics_json as *const (),
            kal_export_ics_file as *const (),
            kal_check_updates_json as *const (),
            kal_sync_status_json as *const (),
            kal_sync_start_chain_json as *const (),
            kal_sync_join_chain_json as *const (),
            kal_sync_phrase_json as *const (),
            kal_sync_leave as *const (),
            kal_sync_round_json as *const (),
        ];
        assert!(exports.iter().all(|p| !p.is_null()));
    }
}
