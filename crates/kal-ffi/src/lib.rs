//! Stable C ABI over Kal's core (spec §5.6 / §7 phase 7).
//!
//! Native widget shims (Android Glance via JNI, iOS WidgetKit via Swift)
//! link this crate and call plain C functions. Strings are UTF-8 JSON
//! allocated by Rust and freed by the caller with [`kal_free`]; the DB handle
//! is an opaque pointer created by [`kal_open`].
//!
//! All functions are `extern "C"`, never panic across the boundary, and
//! return NULL on error.

use std::ffi::{c_char, CStr, CString};
use std::ptr;

use kal_storage::Database;

/// Opaque database handle.
pub struct KalDb {
    db: Database,
}

/// Open (or create) the calendar database at `path`. Returns NULL on failure.
///
/// # Safety
/// `path` must be a valid NUL-terminated UTF-8 C string pointer.
#[no_mangle]
pub unsafe extern "C" fn kal_open(path: *const c_char) -> *mut KalDb {
    catch_null(|| {
        let path = CStr::from_ptr(path).to_string_lossy().into_owned();
        let db = Database::open(path).map_err(|_| ())?;
        Ok(Box::into_raw(Box::new(KalDb { db })))
    })
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
    let handle = *db;
    *db = ptr::null_mut();
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Free a string returned by any `kal_*` function.
///
/// # Safety
/// `s` must be NULL or a pointer from this crate.
#[no_mangle]
pub unsafe extern "C" fn kal_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

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
        let items = kal.db.list_items(false).map_err(|_| ())?;

        let calendars = kal.db.list_calendars().map_err(|_| ())?;
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
        serde_json::to_string(&entries).map_err(|_| ())
    })
}

/// Month grid as JSON rows × 7 days:
/// `[{date:"YYYY-MM-DD", inMonth:bool, items:[{id,title,time,color}]}, ...]`.
/// Always 6 weeks; `first_dow` 0 = Monday … 6 = Sunday.
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
        use chrono::{Datelike, Weekday};
        let first_day = match first_dow % 7 {
            0 => Weekday::Mon,
            1 => Weekday::Tue,
            2 => Weekday::Wed,
            3 => Weekday::Thu,
            4 => Weekday::Fri,
            5 => Weekday::Sat,
            _ => Weekday::Sun,
        };
        let grid = kal_core::viewmodel::month_grid(year, month, first_day);
        let items = kal.db.list_items(false).map_err(|_| ())?;
        let first = grid[0][0];
        let last = grid[kal_core::viewmodel::MONTH_GRID_WEEKS - 1][6];
        let occ_map = kal_core::viewmodel::occurrences_by_date(&items, first, last);

        let mut rows = Vec::with_capacity(grid.len());
        for week in &grid {
            let mut row = Vec::with_capacity(7);
            for date in week {
                let items_json: Vec<serde_json::Value> = occ_map
                    .get(date)
                    .map(|occs| {
                        occs.iter()
                            .filter_map(|o| {
                                items.iter().find(|i| i.id == o.item_id).map(|i| {
                                    serde_json::json!({
                                        "id": i.id.to_string(),
                                        "title": i.title,
                                        "time": if i.all_day { "".into() } else {
                                            o.start.format("%H:%M").to_string()
                                        },
                                    })
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                row.push(serde_json::json!({
                    "date": date.format("%Y-%m-%d").to_string(),
                    "inMonth": date.month() == month,
                    "items": items_json,
                }));
            }
            rows.push(row);
        }
        serde_json::to_string(&rows).map_err(|_| ())
    })
}

// ---------------------------------------------------------------------------
// helpers — no panics may escape into C
// ---------------------------------------------------------------------------

fn epoch_to_tz(epoch: i64, offset: chrono::FixedOffset) -> kal_core::models::DateTimeTz {
    use chrono::TimeZone;
    offset
        .timestamp_opt(epoch, 0)
        .single()
        .unwrap_or_else(|| chrono::Utc::now().fixed_offset())
}

fn catch_null<F>(f: F) -> *mut KalDb
where
    F: FnOnce() -> Result<*mut KalDb, ()>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

fn with_db<F>(db: *mut KalDb, f: F) -> *mut c_char
where
    F: FnOnce(&KalDb) -> Result<String, ()>,
{
    if db.is_null() {
        return ptr::null_mut();
    }
    let kal = unsafe { &*db };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(kal)));
    match result {
        Ok(Ok(json)) => match CString::new(json) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        _ => ptr::null_mut(),
    }
}
