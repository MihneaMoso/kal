//! Phase-0 bridge-surface tests: the versioned, JSON-over-C-ABI slice the
//! Kotlin interface layer will call (guide §6). Exercises success, malformed
//! input, absent data and NULL-argument paths the way JNI will.

use std::ffi::{c_char, CStr, CString};

use kal_core::models::{datetime_from_parts, Calendar, CalendarItem, Color, ItemKind};
use kal_storage::Database;

unsafe fn take_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { kal_ffi::kal_free(ptr) };
    Some(s)
}

/// Last error for this thread, consumed and freed. None = success path.
fn last_error() -> Option<String> {
    unsafe { take_string(kal_ffi::kal_last_error()) }
}

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

struct Fixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    cal_id: String,
    item_id: String,
}

/// Seed a DB on disk through the core, then hand the path to the bridge.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kal.db");
    let db = Database::open(&path).unwrap();
    let cal = Calendar::local("Work", Color("#3366cc".into()));
    db.upsert_calendar(&cal).unwrap();
    let mut item = CalendarItem::new(
        ItemKind::Event,
        "Standup",
        cal.id,
        datetime_from_parts(2026, 8, 24, 9, 0, 0).unwrap(),
    );
    item.reminders = vec![kal_core::models::Reminder::minutes_before(10)];
    item.location = Some("Room 1".into());
    db.upsert_item(&item).unwrap();
    Fixture {
        _dir: dir,
        path,
        cal_id: cal.id.to_string(),
        item_id: item.id.to_string(),
    }
}

impl Fixture {
    unsafe fn open(&self) -> *mut kal_ffi::KalDb {
        let db = unsafe { kal_ffi::kal_open(c(self.path.to_str().unwrap()).as_ptr()) };
        assert!(!db.is_null(), "open failed: {:?}", last_error());
        db
    }
}

#[test]
fn version_handshake_is_available() {
    assert_eq!(kal_ffi::kal_bridge_version(), kal_ffi::KAL_BRIDGE_VERSION);
    assert_eq!(kal_ffi::KAL_BRIDGE_VERSION, 1);
}

#[test]
fn settings_round_trip_and_absent_is_json_null() {
    let f = fixture();
    unsafe {
        let db = f.open();

        // Absent setting: a JSON `null`, and NOT an error (missing ≠ failure).
        let missing = take_string(kal_ffi::kal_get_setting(db, c("theme").as_ptr())).unwrap();
        assert_eq!(missing, "null");
        assert!(last_error().is_none(), "absent setting is not an error");

        // Plain-string write (RULES.md: never pre-JSON-encode) …
        assert_eq!(
            kal_ffi::kal_set_setting(db, c("theme").as_ptr(), c("dark").as_ptr()),
            0
        );
        // … reads back as a JSON string, unquoted at the storage level.
        let theme = take_string(kal_ffi::kal_get_setting(db, c("theme").as_ptr())).unwrap();
        assert_eq!(theme, "\"dark\"");

        // Whole-store snapshot is one JSON object.
        assert_eq!(
            kal_ffi::kal_set_setting(
                db,
                c("preferences").as_ptr(),
                c("{\"time_24h\":true}").as_ptr()
            ),
            0
        );
        let all = take_string(kal_ffi::kal_settings_json(db)).unwrap();
        assert!(all.starts_with('{'), "{all}");
        assert!(all.contains("\"theme\":\"dark\""), "{all}");
        assert!(all.contains("preferences"), "{all}");

        kal_ffi::kal_close(&mut { db });
    }
}

#[test]
fn calendars_and_items_cross_as_core_model_json() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let cals = take_string(kal_ffi::kal_calendars_json(db)).unwrap();
        let parsed: Vec<Calendar> = serde_json::from_str(&cals).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Work");
        assert_eq!(parsed[0].color.as_str(), "#3366cc");

        let items = take_string(kal_ffi::kal_items_json(db, 0)).unwrap();
        let parsed_items: Vec<CalendarItem> = serde_json::from_str(&items).unwrap();
        assert_eq!(parsed_items.len(), 1);
        // Full fidelity: reminders, location and offsets survive the crossing.
        assert_eq!(parsed_items[0].title, "Standup");
        assert_eq!(parsed_items[0].location.as_deref(), Some("Room 1"));
        assert_eq!(parsed_items[0].reminders.len(), 1);
        assert_eq!(
            parsed_items[0].start.offset().local_minus_utc(),
            0,
            "fixed offset preserved"
        );

        let one = take_string(kal_ffi::kal_item_json(db, c(&f.item_id).as_ptr())).unwrap();
        let item: CalendarItem = serde_json::from_str(&one).unwrap();
        assert_eq!(item.id.to_string(), f.item_id);

        // Absent item is JSON `null`, not an error.
        let ghost = take_string(kal_ffi::kal_item_json(
            db,
            c("01ARZ3NDEKTSV4RRFFQ69G5FAV").as_ptr(),
        ))
        .unwrap();
        assert_eq!(ghost, "null");
        assert!(last_error().is_none());

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn upsert_item_from_json_round_trips_edits() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // Read → mutate → write back, exactly the editor's flow.
        let one = take_string(kal_ffi::kal_item_json(db, c(&f.item_id).as_ptr())).unwrap();
        let mut item: CalendarItem = serde_json::from_str(&one).unwrap();
        item.title = "Standup (renamed)".into();
        item.all_day = true;
        item.updated_at = datetime_from_parts(2026, 8, 25, 12, 0, 0).unwrap();
        let edited = serde_json::to_string(&item).unwrap();
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&edited).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let back = take_string(kal_ffi::kal_item_json(db, c(&f.item_id).as_ptr())).unwrap();
        let reread: CalendarItem = serde_json::from_str(&back).unwrap();
        assert_eq!(reread.title, "Standup (renamed)");
        assert!(reread.all_day);

        // A calendar can be written the same way.
        let mut cal = Calendar::local("Personal", Color("#e91e63".into()));
        cal.id = f.cal_id.parse().unwrap();
        assert_eq!(
            kal_ffi::kal_upsert_calendar_json(
                db,
                c(&serde_json::to_string(&cal).unwrap()).as_ptr()
            ),
            0
        );
        let cals: Vec<Calendar> =
            serde_json::from_str(&take_string(kal_ffi::kal_calendars_json(db)).unwrap()).unwrap();
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0].name, "Personal");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn soft_delete_writes_a_tombstone_without_removing_the_row() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // Unknown id: nothing to tombstone, and not an error.
        assert_eq!(
            kal_ffi::kal_soft_delete_item(db, c("01ARZ3NDEKTSV4RRFFQ69G5FAV").as_ptr()),
            0
        );
        assert!(last_error().is_none(), "absent id is not an error");

        assert_eq!(kal_ffi::kal_soft_delete_item(db, c(&f.item_id).as_ptr()), 1);
        // Core semantics: re-deleting rewrites the tombstone and bumps its LWW
        // timestamp (that is what keeps the deletion propagating over sync);
        // the row is never removed.
        assert_eq!(kal_ffi::kal_soft_delete_item(db, c(&f.item_id).as_ptr()), 1);
        assert!(last_error().is_none());

        // Live view excludes it; include_deleted shows the tombstone.
        let live: Vec<CalendarItem> =
            serde_json::from_str(&take_string(kal_ffi::kal_items_json(db, 0)).unwrap()).unwrap();
        assert!(live.is_empty());
        let all: Vec<CalendarItem> =
            serde_json::from_str(&take_string(kal_ffi::kal_items_json(db, 1)).unwrap()).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].deleted);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn failures_report_through_the_error_channel_not_panics() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // Malformed JSON: status -1 plus an actionable message.
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c("not json").as_ptr()),
            -1
        );
        let err = last_error().expect("failure must set the error channel");
        assert!(err.contains("invalid item json"), "{err}");

        // Well-formed JSON, wrong shape: still an error, still no panic.
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c("{\"id\":5}").as_ptr()),
            -1
        );
        assert!(last_error().is_some());

        // Bad ULID for a lookup: error, not `null` (absent ≠ malformed).
        assert!(take_string(kal_ffi::kal_item_json(db, c("nope").as_ptr())).is_none());
        let err = last_error().expect("bad ULID must set the error channel");
        assert!(err.contains("invalid ULID"), "{err}");

        // A successful call clears the channel for the next caller.
        let _ = take_string(kal_ffi::kal_items_json(db, 0));
        assert!(last_error().is_none());

        // NULL pointers are reported, never dereferenced.
        assert!(take_string(kal_ffi::kal_calendars_json(std::ptr::null_mut())).is_none());
        assert!(last_error().is_some());
        assert_eq!(
            kal_ffi::kal_set_setting(db, std::ptr::null(), c("dark").as_ptr()),
            -1
        );
        assert!(last_error().is_some());

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn existing_widget_surface_still_works_unchanged() {
    // Regression guard for the ABI the Android widgets already ship against:
    // the same handle drives both the widget DTOs and the new model JSON.
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let upcoming = take_string(kal_ffi::kal_upcoming_json(db, 1785000000, 1787700000)).unwrap();
        assert!(upcoming.contains("Standup"), "{upcoming}");

        // The widget DTO shape is frozen — key sets are asserted exactly, so a
        // refactor here cannot silently rename or drop a field the Kotlin/Swift
        // shims parse.
        let upcoming: Vec<serde_json::Value> = serde_json::from_str(&upcoming).unwrap();
        let entry = upcoming
            .iter()
            .find(|e| e["title"] == "Standup")
            .expect("Standup entry");
        assert_eq!(
            sorted_keys(entry),
            ["age", "allDay", "color", "end", "id", "kind", "start", "title"]
        );
        assert_eq!(entry["kind"], "event");
        assert_eq!(entry["allDay"], false);

        let grid = take_string(kal_ffi::kal_month_grid_json(db, 2026, 8, 0)).unwrap();
        let grid: Vec<Vec<serde_json::Value>> = serde_json::from_str(&grid).unwrap();
        assert_eq!(grid.len(), kal_core::viewmodel::MONTH_GRID_WEEKS);
        assert!(grid.iter().all(|week| week.len() == 7));

        let cell = grid
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-24")
            .expect("2026-08-24 cell");
        assert_eq!(sorted_keys(cell), ["date", "inMonth", "items"]);
        assert_eq!(cell["inMonth"], true);
        let item = &cell["items"][0];
        assert_eq!(item["title"], "Standup");
        assert_eq!(item["time"], "09:00");
        assert_eq!(
            sorted_keys(item),
            ["allDay", "color", "id", "time", "title"]
        );

        kal_ffi::kal_close(&mut db);
    }
}

/// Sorted key names of a JSON object, for exact shape assertions.
fn sorted_keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

// ---------------------------------------------------------------------------
// App-facing view payloads (the Kotlin shell renders these)
// ---------------------------------------------------------------------------

impl Fixture {
    /// Seed another item through the core (the bridge opens its own handle).
    fn seed(&self, item: &CalendarItem) {
        let db = Database::open(&self.path).unwrap();
        db.upsert_item(item).unwrap();
    }

    /// The fixture calendar's id as a core `Ulid`.
    fn cal_ulid(&self) -> kal_core::models::Ulid {
        kal_core::models::Ulid::from_string(&self.cal_id).unwrap()
    }
}

/// The display entry is the one shape every Kotlin view parses. Pinning it
/// exactly means a change here is a deliberate bridge change; `calendarId` is
/// what lets the interface layer apply the calendar-visibility filter where the
/// Dioxus shell did, and `start` is what lets it format times from the user's
/// own 12/24-hour preference.
#[test]
fn app_month_payload_carries_the_shared_display_entry() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_month_json(db, 2026, 8, 0)).unwrap();
        let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap();
        assert_eq!(rows.len(), 6, "always 6 weeks");
        assert!(rows.iter().all(|row| row.len() == 7), "7 days per row");

        let cell = rows
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-24")
            .expect("2026-08-24 cell");
        assert_eq!(sorted_keys(cell), ["date", "inMonth", "items"]);
        assert_eq!(cell["inMonth"], true, "August cell");

        let entry = &cell["items"][0];
        assert_eq!(
            sorted_keys(entry),
            [
                "age",
                "allDay",
                "calendarId",
                "color",
                "completed",
                "end",
                "id",
                "kind",
                "overrideColor",
                "start",
                "title"
            ]
        );
        assert_eq!(entry["title"], "Standup");
        assert_eq!(entry["calendarId"], f.cal_id);
        assert_eq!(entry["kind"], "event");
        assert_eq!(entry["allDay"], false);
        assert_eq!(entry["completed"], false, "open item");
        assert!(
            entry["start"].as_str().unwrap().contains("09:00"),
            "raw start instant: {}",
            entry["start"]
        );
        assert_eq!(
            entry["age"],
            serde_json::Value::Null,
            "age is birthdays-only"
        );

        // The grid spills out of the month: the first cell of a Mon-start
        // August 2026 grid is in July.
        assert_eq!(rows[0][0]["date"], "2026-07-27");
        assert_eq!(rows[0][0]["inMonth"], false);

        kal_ffi::kal_close(&mut db);
    }
}

/// The day payload must split all-day items into their own strip and give timed
/// ones canvas fractions — the day view positions blocks from these numbers
/// instead of recomputing the layout in Kotlin.
#[test]
fn app_day_payload_splits_all_day_and_positions_timed() {
    let f = fixture();
    let mut holiday = CalendarItem::new(
        ItemKind::Event,
        "Holiday",
        f.cal_ulid(),
        datetime_from_parts(2026, 8, 24, 0, 0, 0).unwrap(),
    );
    holiday.all_day = true;
    f.seed(&holiday);

    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_day_json(db, c("2026-08-24").as_ptr())).unwrap();
        let day: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(sorted_keys(&day), ["allDay", "date", "timed"]);
        assert_eq!(day["date"], "2026-08-24");

        let all_day = day["allDay"].as_array().unwrap();
        assert_eq!(all_day.len(), 1);
        assert_eq!(all_day[0]["title"], "Holiday");
        assert_eq!(all_day[0]["allDay"], true);

        let timed = day["timed"].as_array().unwrap();
        assert_eq!(timed.len(), 1, "the all-day item must not be positioned");
        let block = &timed[0];
        assert_eq!(
            sorted_keys(block),
            ["heightFrac", "item", "lane", "lanes", "topFrac"]
        );
        assert_eq!(block["item"]["title"], "Standup");
        // 09:00 is 540/1440 of the canvas.
        let top = block["topFrac"].as_f64().unwrap();
        assert!((top - 540.0 / 1440.0).abs() < 1e-9, "topFrac {top}");
        assert!(block["heightFrac"].as_f64().unwrap() >= 0.02);
        assert_eq!(block["lane"], 0);
        assert_eq!(block["lanes"], 1);

        // A day with nothing on it is empty, not an error.
        let empty = take_string(kal_ffi::kal_day_json(db, c("2026-08-25").as_ptr())).unwrap();
        let day: serde_json::Value = serde_json::from_str(&empty).unwrap();
        assert_eq!(day["allDay"].as_array().unwrap().len(), 0);
        assert_eq!(day["timed"].as_array().unwrap().len(), 0);
        assert!(last_error().is_none());

        kal_ffi::kal_close(&mut db);
    }
}

/// The week view is always Monday-anchored (the Dioxus one ignored the
/// first-day preference too) and returns exactly 7 days from the given start.
#[test]
fn app_week_payload_is_seven_days_from_the_given_start() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_week_json(db, c("2026-08-24").as_ptr())).unwrap();
        let days: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(days.len(), 7);
        assert_eq!(days[0]["date"], "2026-08-24");
        assert_eq!(days[6]["date"], "2026-08-30");
        assert_eq!(sorted_keys(&days[0]), ["date", "items"]);
        assert_eq!(days[0]["items"].as_array().unwrap().len(), 1);
        assert_eq!(days[1]["items"].as_array().unwrap().len(), 0, "empty day");

        kal_ffi::kal_close(&mut db);
    }
}

/// Agenda rows are chronological (date, then start time) and the cap keeps the
/// earliest rows, matching the view's `truncate(100)` after a full window scan.
#[test]
fn app_agenda_orders_chronologically_and_honours_the_cap() {
    let f = fixture();
    for (title, day, hour) in [("Later", 26, 15), ("Sooner", 25, 8)] {
        let item = CalendarItem::new(
            ItemKind::Event,
            title,
            f.cal_ulid(),
            datetime_from_parts(2026, 8, day, hour, 0, 0).unwrap(),
        );
        f.seed(&item);
    }

    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_agenda_json(
            db,
            c("2026-08-24").as_ptr(),
            30,
            100,
        ))
        .unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        let titles: Vec<&str> = rows
            .iter()
            .map(|r| r["item"]["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["Standup", "Sooner", "Later"]);
        assert_eq!(sorted_keys(&rows[0]), ["date", "item"]);
        assert_eq!(rows[0]["date"], "2026-08-24");

        let capped = take_string(kal_ffi::kal_agenda_json(
            db,
            c("2026-08-24").as_ptr(),
            30,
            2,
        ))
        .unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&capped).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["item"]["title"], "Sooner", "cap keeps the earliest");

        // The window is inclusive of `from + days`; a window with nothing in it
        // is an empty array, not an error.
        let empty = take_string(kal_ffi::kal_agenda_json(
            db,
            c("2026-09-30").as_ptr(),
            0,
            100,
        ))
        .unwrap();
        assert_eq!(empty, "[]");
        assert!(last_error().is_none());

        kal_ffi::kal_close(&mut db);
    }
}

/// Birthday ages are computed against the occurrence's own date (a yearly
/// birthday ages on the day it lands), and only for birthdays.
#[test]
fn app_entries_report_birthday_age_only() {
    let f = fixture();
    let mut birthday = CalendarItem::new(
        ItemKind::Birthday,
        "Ada",
        f.cal_ulid(),
        datetime_from_parts(1990, 8, 24, 0, 0, 0).unwrap(),
    );
    birthday.all_day = true;
    birthday.metadata.birthday_of = Some("Ada".into());
    // Birthdays recur yearly (vCard import sets this), so the 1990 start date
    // has an occurrence on 2026-08-24 to compute an age for.
    birthday.rrule = Some("FREQ=YEARLY".into());
    f.seed(&birthday);

    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_month_json(db, 2026, 8, 0)).unwrap();
        let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap();
        let cell = rows
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-24")
            .expect("2026-08-24 cell");
        let birthdays: Vec<&serde_json::Value> = cell["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "birthday")
            .collect();
        assert_eq!(birthdays.len(), 1);
        assert_eq!(birthdays[0]["age"], 36, "1990 → 2026");
        assert_eq!(birthdays[0]["allDay"], true);

        kal_ffi::kal_close(&mut db);
    }
}

/// A task's completion state crosses in the payload so the read-only views can
/// show the checked box and the struck-through title.
#[test]
fn app_entries_report_task_completion() {
    let f = fixture();
    let mut done = CalendarItem::new(
        ItemKind::Task,
        "Ship it",
        f.cal_ulid(),
        datetime_from_parts(2026, 8, 24, 10, 0, 0).unwrap(),
    );
    done.completed = Some(datetime_from_parts(2026, 8, 24, 11, 0, 0).unwrap());
    f.seed(&done);
    let open = CalendarItem::new(
        ItemKind::Task,
        "Draft it",
        f.cal_ulid(),
        datetime_from_parts(2026, 8, 24, 11, 0, 0).unwrap(),
    );
    f.seed(&open);

    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_month_json(db, 2026, 8, 0)).unwrap();
        let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap();
        let cell = rows
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-24")
            .expect("2026-08-24 cell");
        let tasks: Vec<&serde_json::Value> = cell["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "task")
            .collect();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["title"], "Ship it", "10:00 sorts first");
        assert_eq!(tasks[0]["completed"], true);
        assert_eq!(tasks[1]["title"], "Draft it");
        assert_eq!(tasks[1]["completed"], false);

        kal_ffi::kal_close(&mut db);
    }
}

/// Colour resolution: the effective colour (item dots, day blocks) falls back to
/// the calendar's, while the chip colour uses only the item's own override —
/// the two different rules the Dioxus views had.
#[test]
fn app_entries_expose_override_and_effective_colour() {
    let f = fixture();
    let mut tinted = CalendarItem::new(
        ItemKind::Event,
        "Tinted",
        f.cal_ulid(),
        datetime_from_parts(2026, 8, 24, 10, 0, 0).unwrap(),
    );
    tinted.color_override = Some(Color("#112233".into()));
    f.seed(&tinted);

    unsafe {
        let mut db = f.open();

        let json = take_string(kal_ffi::kal_month_json(db, 2026, 8, 0)).unwrap();
        let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap();
        let cell = rows
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-24")
            .expect("2026-08-24 cell");
        let items = cell["items"].as_array().unwrap();

        let plain = items.iter().find(|e| e["title"] == "Standup").unwrap();
        assert_eq!(
            plain["color"], "#3366cc",
            "effective colour falls back to the calendar"
        );
        assert_eq!(plain["overrideColor"], serde_json::Value::Null);

        let tinted = items.iter().find(|e| e["title"] == "Tinted").unwrap();
        assert_eq!(tinted["color"], "#112233");
        assert_eq!(tinted["overrideColor"], "#112233");

        kal_ffi::kal_close(&mut db);
    }
}

/// The sidebar's visibility toggle sends the payload's own calendar JSON back
/// with one field flipped, so that payload must be valid `upsert` input — and
/// its `source` must keep the snake_case spelling the model deserialises.
#[test]
fn app_calendar_payload_round_trips_through_upsert() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let payload = take_string(kal_ffi::kal_calendars_json(db)).unwrap();
        let mut cals: Vec<serde_json::Value> = serde_json::from_str(&payload).unwrap();
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0]["name"], "Work");
        assert_eq!(cals[0]["visible"], true);
        assert_eq!(cals[0]["deleted"], false);
        assert_eq!(cals[0]["source"], "local", "snake_case on the wire");

        // Exactly what `CalendarModel.patchedVisible` does.
        cals[0]["visible"] = serde_json::Value::Bool(false);
        let patched = serde_json::to_string(&cals[0]).unwrap();
        assert_eq!(
            kal_ffi::kal_upsert_calendar_json(db, c(&patched).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let after: Vec<serde_json::Value> =
            serde_json::from_str(&take_string(kal_ffi::kal_calendars_json(db)).unwrap()).unwrap();
        assert_eq!(
            after.len(),
            1,
            "the round-trip must not duplicate the calendar"
        );
        assert_eq!(after[0]["visible"], false);
        assert_eq!(after[0]["name"], "Work");
        assert_eq!(after[0]["color"], "#3366cc");
        assert_eq!(after[0]["id"], f.cal_id);

        kal_ffi::kal_close(&mut db);
    }
}

/// A malformed date/time argument is an error with a readable message, never a
/// panic and never a silently-empty payload.
#[test]
fn app_views_reject_malformed_dates() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        assert!(kal_ffi::kal_day_json(db, c("2026-13-99").as_ptr()).is_null());
        let err = last_error().expect("error message");
        assert!(err.contains("2026-13-99"), "{err}");

        assert!(kal_ffi::kal_week_json(db, c("nonsense").as_ptr()).is_null());
        assert!(last_error().unwrap().contains("nonsense"));

        assert!(kal_ffi::kal_agenda_json(db, std::ptr::null(), 30, 100).is_null());
        assert!(last_error().unwrap().contains("null"));

        kal_ffi::kal_close(&mut db);
    }
}

// ---------------------------------------------------------------------------
// editor writes (phase 3)
// ---------------------------------------------------------------------------

/// One item as parsed core-model JSON (snake_case, as `kal_item_json` emits).
unsafe fn read_item(db: *mut kal_ffi::KalDb, id: &str) -> serde_json::Value {
    let raw = unsafe { take_string(kal_ffi::kal_item_json(db, c(id).as_ptr())) }.unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// Every live item, as parsed core-model JSON.
unsafe fn read_items(db: *mut kal_ffi::KalDb) -> Vec<serde_json::Value> {
    let raw = unsafe { take_string(kal_ffi::kal_items_json(db, 0)) }.unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// The live item with that exact title (panics when absent).
unsafe fn find_item(db: *mut kal_ffi::KalDb, title: &str) -> serde_json::Value {
    unsafe { read_items(db) }
        .into_iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("no item titled {title:?}"))
}

/// Save one form, asserting it succeeded.
unsafe fn save(db: *mut kal_ffi::KalDb, form: &serde_json::Value) {
    let status = unsafe { kal_ffi::kal_save_item_json(db, c(&form.to_string()).as_ptr()) };
    assert_eq!(status, 0, "save failed: {:?}", last_error());
}

/// An instant on the device's local clock — the offset items written by the
/// editor carry, so date/time form fields round-trip without shifting.
fn local_dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<chrono::FixedOffset> {
    use chrono::TimeZone;
    chrono::Local
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .unwrap()
        .fixed_offset()
}

/// Seed a weekly series through the bridge; returns its ULID.
unsafe fn seed_weekly_series(db: *mut kal_ffi::KalDb, cal_id: &str, title: &str) -> String {
    let start = local_dt(2026, 8, 3, 9, 0);
    let mut item = CalendarItem::new(ItemKind::Event, title, cal_id.parse().unwrap(), start);
    item.end = Some(start + chrono::Duration::hours(1));
    item.rrule = Some("FREQ=WEEKLY".into());
    assert_eq!(
        unsafe {
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&item).unwrap()).as_ptr())
        },
        0,
        "{:?}",
        last_error()
    );
    item.id.to_string()
}

/// Reminder minutes, sorted, as the model stores them.
fn reminder_minutes(item: &serde_json::Value) -> Vec<i64> {
    item["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["offset"]["minutes"].as_i64().unwrap())
        .collect()
}

#[test]
fn save_form_creates_an_event_from_the_form() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        save(
            db,
            &serde_json::json!({
                "kind": "event",
                "title": "Lunch",
                "date": "2026-08-24",
                "startTime": "12:30",
                "endTime": "13:30",
                "calendarId": f.cal_id,
                "location": "Cafeteria",
            }),
        );

        let saved = find_item(db, "Lunch");
        assert_eq!(saved["kind"], "event");
        assert_eq!(saved["all_day"], false);
        assert_eq!(saved["location"], "Cafeteria");
        assert_eq!(saved["calendar_id"], f.cal_id);
        assert_eq!(saved["color_override"], serde_json::Value::Null);
        assert_eq!(saved["rrule"], serde_json::Value::Null);
        assert!(saved["notes"].is_null());
        assert!(reminder_minutes(&saved).is_empty());
        assert!(
            saved["start"]
                .as_str()
                .unwrap()
                .starts_with("2026-08-24T12:30:00"),
            "{}",
            saved["start"]
        );
        assert!(
            saved["end"]
                .as_str()
                .unwrap()
                .starts_with("2026-08-24T13:30:00"),
            "{}",
            saved["end"]
        );
        assert_ne!(saved["id"], f.item_id, "a new item gets its own ULID");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_all_day_colour_and_reminder_minutes() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // All-day (no times) + an explicit override + a duplicated preset and a
        // positive custom reminder, which must come out sorted and deduped.
        save(
            db,
            &serde_json::json!({
                "kind": "event",
                "title": "Holiday",
                "date": "2026-08-24",
                "allDay": true,
                "calendarId": f.cal_id,
                "color": "#e91e63",
                "reminders": [30, 10, 10],
                "customReminder": "45",
            }),
        );

        let saved = find_item(db, "Holiday");
        assert_eq!(saved["all_day"], true);
        assert!(saved["end"].is_null(), "all-day item has no end");
        assert_eq!(saved["color_override"], "#e91e63");
        assert_eq!(reminder_minutes(&saved), vec![10, 30, 45]);
        // Every reminder is a push reminder (the only method today).
        assert_eq!(saved["reminders"][0]["method"], "push");
        assert_eq!(saved["reminders"][0]["offset"]["type"], "minutes_before");

        // A zero/negative custom value is ignored, not stored.
        save(
            db,
            &serde_json::json!({
                "kind": "event", "title": "No reminders", "date": "2026-08-24",
                "calendarId": f.cal_id, "customReminder": "0",
            }),
        );
        assert!(reminder_minutes(&find_item(db, "No reminders")).is_empty());

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_birthday_links_the_person() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        save(
            db,
            &serde_json::json!({
                "kind": "birthday", "title": "Ada", "date": "1990-12-10",
                "allDay": true, "calendarId": f.cal_id, "person": "Ada Lovelace",
                "rrule": "FREQ=YEARLY",
            }),
        );
        let saved = find_item(db, "Ada");
        assert_eq!(saved["kind"], "birthday");
        assert_eq!(saved["all_day"], true);
        assert!(saved["end"].is_null());
        assert_eq!(saved["metadata"]["birthday_of"], "Ada Lovelace");
        assert_eq!(saved["rrule"], "FREQ=YEARLY");

        // Documented parity: the Dioxus editor wrote `Some(person)` even for an
        // empty field, and `validate()` only rejects `None`, so a birthday with
        // no person is accepted — with an empty linkage. (Rendered as a plain
        // birth-date entry; the name comes from the title.)
        save(
            db,
            &serde_json::json!({
                "kind": "birthday", "title": "Anonymous", "date": "1990-12-10",
                "allDay": true, "calendarId": f.cal_id, "rrule": "FREQ=YEARLY",
            }),
        );
        assert_eq!(find_item(db, "Anonymous")["metadata"]["birthday_of"], "");

        kal_ffi::kal_close(&mut db);
    }
}

/// A birthday is a recurring item, so its editor save takes the "All events"
/// path: the person the dialog carries must reach the row there too.
#[test]
fn save_form_all_scope_keeps_a_birthdays_person() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let start = local_dt(1990, 12, 10, 0, 0);
        let mut birthday =
            CalendarItem::new(ItemKind::Birthday, "Ada", f.cal_id.parse().unwrap(), start);
        birthday.all_day = true;
        birthday.end = None;
        birthday.rrule = Some("FREQ=YEARLY".into());
        birthday.metadata.birthday_of = Some("Ada".into());
        assert_eq!(
            kal_ffi::kal_upsert_item_json(
                db,
                c(&serde_json::to_string(&birthday).unwrap()).as_ptr()
            ),
            0,
            "{:?}",
            last_error()
        );
        let id = birthday.id.to_string();

        save(
            db,
            &serde_json::json!({
                "id": id, "kind": "birthday", "title": "Ada Lovelace",
                "date": "1990-12-10", "allDay": true, "calendarId": f.cal_id,
                "person": "Ada Lovelace", "rrule": "FREQ=YEARLY",
                "scope": "all",
            }),
        );

        let after = read_item(db, &id);
        assert_eq!(after["title"], "Ada Lovelace");
        assert_eq!(after["metadata"]["birthday_of"], "Ada Lovelace");
        assert_eq!(after["rrule"], "FREQ=YEARLY");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_preserves_fields_the_form_does_not_show() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // Give the seeded item a note, an old creation time and a completion
        // stamp, none of which the editor exposes.
        let mut item: CalendarItem = serde_json::from_value(read_item(db, &f.item_id)).unwrap();
        item.notes = Some("hidden note".into());
        item.created_at = datetime_from_parts(2020, 1, 1, 0, 0, 0).unwrap();
        item.completed = Some(datetime_from_parts(2026, 1, 2, 0, 0, 0).unwrap());
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&item).unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        // Edit through the form: only the title and (emptied) location change.
        save(
            db,
            &serde_json::json!({
                "id": f.item_id, "kind": "event", "title": "Standup (renamed)",
                "date": "2026-08-24", "startTime": "09:00", "endTime": "10:00",
                "calendarId": f.cal_id, "location": "",
            }),
        );

        let saved = read_item(db, &f.item_id);
        assert_eq!(saved["title"], "Standup (renamed)");
        assert_eq!(saved["notes"], "hidden note");
        assert!(
            saved["location"].is_null(),
            "empty location means no location"
        );
        assert!(
            saved["created_at"]
                .as_str()
                .unwrap()
                .starts_with("2020-01-01"),
            "creation time survives an edit: {}",
            saved["created_at"]
        );
        assert!(
            saved["completed"]
                .as_str()
                .unwrap()
                .starts_with("2026-01-02"),
            "the form has no completion field, so it must be preserved: {}",
            saved["completed"]
        );
        assert_eq!(
            read_items(db).len(),
            1,
            "editing the seeded item must not duplicate the row"
        );
        // The form's reminder list replaces the item's (the seed had 10 min).
        assert!(reminder_minutes(&saved).is_empty());

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_this_only_exdates_the_base_and_keeps_the_edited_copy() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Standup");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();
        let occ = start + chrono::Duration::days(14);

        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Standup (moved)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "scope": "this_only",
            }),
        );

        // The base keeps its rule but excludes the clicked occurrence…
        let after = read_item(db, &base_id);
        assert_eq!(after["rrule"], "FREQ=WEEKLY");
        let excluded: Vec<chrono::DateTime<chrono::FixedOffset>> = after["exdates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| chrono::DateTime::parse_from_rfc3339(d.as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(excluded, vec![occ]);

        // …and exactly one new, rule-free item carries the edit at that instant.
        let copy = find_item(db, "Standup (moved)");
        assert!(copy["rrule"].is_null());
        assert_ne!(copy["id"], base_id);
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(copy["start"].as_str().unwrap()).unwrap(),
            occ
        );
        assert_eq!(read_items(db).len(), 3, "seed + base + edited copy");

        kal_ffi::kal_close(&mut db);
    }
}

/// "All events" rewrites the series row, but the form is anchored on the
/// *clicked occurrence*: writing that date as the new series start moved the
/// series forward and stranded every earlier occurrence outside it, so changing
/// a colour made the past events vanish. The series must keep its own start.
#[test]
fn save_form_all_scope_keeps_the_series_on_its_own_start() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Standup");
        let before = read_item(db, &base_id);
        let start =
            chrono::DateTime::parse_from_rfc3339(before["start"].as_str().unwrap()).unwrap();
        // An occurrence well after the series start: the worst case for the bug.
        let occ = start + chrono::Duration::days(77);

        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Standup",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "rrule": "FREQ=WEEKLY",
                "color": "#e67c73",
                "scope": "all",
            }),
        );

        // One row, same id, colour applied, series start untouched.
        let after = read_item(db, &base_id);
        let after_start =
            chrono::DateTime::parse_from_rfc3339(after["start"].as_str().unwrap()).unwrap();
        let after_end =
            chrono::DateTime::parse_from_rfc3339(after["end"].as_str().unwrap()).unwrap();
        assert_eq!(after_start.date_naive(), start.date_naive());
        assert_eq!(after_start.time(), start.time());
        assert_eq!(after_end.date_naive(), start.date_naive());
        assert_eq!(after_end.time(), start.time() + chrono::Duration::hours(1));
        assert_eq!(after["color_override"], "#e67c73");
        assert_eq!(after["rrule"], "FREQ=WEEKLY");
        assert!(after["exdates"].as_array().unwrap().is_empty());
        assert_eq!(read_items(db).len(), 2, "seed + the series, no copies");

        // The user-visible half: the first occurrence still renders in its month.
        let json = take_string(kal_ffi::kal_month_json(db, 2026, 8, 0)).unwrap();
        let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap();
        let first_day = rows
            .iter()
            .flatten()
            .find(|c| c["date"] == "2026-08-03")
            .expect("the series' own first day still renders");
        assert!(
            first_day["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["title"] == "Standup"),
            "{first_day}"
        );

        kal_ffi::kal_close(&mut db);
    }
}

/// A "This event" copy is one occurrence of the series, not an unrelated item:
/// it names its series and the excluded instant. The editor uses the link to
/// keep offering the scope choice; "All events" uses it to reach the copy.
#[test]
fn save_form_this_only_links_the_copy_to_the_series() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Yoga");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();
        let occ = start + chrono::Duration::days(7);

        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Yoga (this one)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "scope": "this_only",
            }),
        );

        let copy = find_item(db, "Yoga (this one)");
        assert!(copy["rrule"].is_null(), "the copy stays a single item");
        assert_eq!(copy["metadata"]["series_id"], base_id);
        let linked = chrono::DateTime::parse_from_rfc3339(
            copy["metadata"]["recurrence_id"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(linked, occ, "the excluded instant is remembered");

        kal_ffi::kal_close(&mut db);
    }
}

/// Reopening an individually edited occurrence and choosing "This event" edits
/// that row in place: no second copy, the link survives, and the series' rule
/// shown by the dialog is not written onto the occurrence.
#[test]
fn save_form_this_only_on_a_linked_occurrence_updates_it_in_place() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Yoga");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();
        let occ = start + chrono::Duration::days(7);

        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Yoga (this one)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "scope": "this_only",
            }),
        );
        let copy = find_item(db, "Yoga (this one)");
        let copy_id = copy["id"].as_str().unwrap().to_string();
        let rows_before = read_items(db).len();

        // The dialog shows the series' rule while editing the occurrence, so the
        // form carries it.
        save(
            db,
            &serde_json::json!({
                "id": copy_id,
                "occurrenceStart": copy["start"].as_str().unwrap(),
                "kind": "event",
                "title": "Yoga (renamed)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "rrule": "FREQ=WEEKLY",
                "color": "#16a085",
                "scope": "this_only",
            }),
        );

        let after = read_item(db, &copy_id);
        assert_eq!(after["title"], "Yoga (renamed)");
        assert_eq!(after["color_override"], "#16a085");
        assert!(
            after["rrule"].is_null(),
            "the series' rule must not turn the occurrence into a series: {after}"
        );
        assert_eq!(after["metadata"]["series_id"], base_id);
        assert_eq!(
            read_items(db).len(),
            rows_before,
            "editing the occurrence must not create another copy"
        );
        let base_after = read_item(db, &base_id);
        assert_eq!(
            base_after["exdates"].as_array().unwrap().len(),
            1,
            "the series is untouched"
        );
        assert!(base_after["color_override"].is_null());

        kal_ffi::kal_close(&mut db);
    }
}

/// The owner's scenario: two occurrences were edited individually (different
/// colours), then the series is recoloured/renamed with "All events". Both
/// copies must change too — they keep their own dates — and the series must keep
/// its rule and exclusions even though the dialog was opened on a copy.
#[test]
fn save_form_all_scope_from_a_linked_occurrence_reaches_every_sibling() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Yoga");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();

        let mut copies = Vec::new();
        for (days, title, colour) in [
            (7, "Yoga (edited one)", "#27ae60"),
            (14, "Yoga (edited two)", "#e91e63"),
        ] {
            let occ = start + chrono::Duration::days(days);
            save(
                db,
                &serde_json::json!({
                    "id": base_id,
                    "occurrenceStart": occ.to_rfc3339(),
                    "kind": "event",
                    "title": title,
                    "date": occ.format("%Y-%m-%d").to_string(),
                    "startTime": occ.format("%H:%M").to_string(),
                    "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                    "calendarId": f.cal_id,
                    "color": colour,
                    "scope": "this_only",
                }),
            );
            let copy = find_item(db, title);
            copies.push((
                copy["id"].as_str().unwrap().to_string(),
                copy["start"].as_str().unwrap().to_string(),
            ));
        }

        // Open the first copy and pick "All events" with a new colour and title.
        let (copy_id, copy_start) = copies[0].clone();
        let at = chrono::DateTime::parse_from_rfc3339(&copy_start).unwrap();
        save(
            db,
            &serde_json::json!({
                "id": copy_id,
                "occurrenceStart": copy_start,
                "kind": "event",
                "title": "Yoga (team)",
                "date": at.format("%Y-%m-%d").to_string(),
                "startTime": at.format("%H:%M").to_string(),
                "endTime": (at + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "rrule": "FREQ=WEEKLY",
                "color": "#8e44ad",
                "scope": "all",
            }),
        );

        let base_after = read_item(db, &base_id);
        assert_eq!(base_after["title"], "Yoga (team)");
        assert_eq!(base_after["color_override"], "#8e44ad");
        assert_eq!(
            base_after["rrule"], "FREQ=WEEKLY",
            "the form's rule belongs to the series, and it survives"
        );
        assert_eq!(
            base_after["exdates"].as_array().unwrap().len(),
            2,
            "the exclusions survive an All-events save from a copy"
        );

        for (id, before) in copies {
            let after = read_item(db, &id);
            assert_eq!(after["color_override"], "#8e44ad", "{after}");
            assert_eq!(after["title"], "Yoga (team)");
            assert_eq!(after["start"], before, "each copy keeps its own date");
            assert!(after["rrule"].is_null(), "{after}");
            assert_eq!(after["metadata"]["series_id"], base_id);
        }

        kal_ffi::kal_close(&mut db);
    }
}

/// Deleting a series also tombstones its detached occurrences; deleting one
/// occurrence leaves the series alone.
#[test]
fn deleting_a_series_removes_its_linked_occurrences() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Yoga");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();
        let occ = start + chrono::Duration::days(7);
        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Yoga (this one)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "scope": "this_only",
            }),
        );
        assert!(read_items(db)
            .iter()
            .any(|i| i["title"] == "Yoga (this one)"));

        assert_eq!(
            kal_ffi::kal_soft_delete_item(db, c(&base_id).as_ptr()),
            1,
            "{:?}",
            last_error()
        );

        let live = read_items(db);
        assert!(
            live.iter()
                .all(|i| i["title"] != "Yoga" && i["title"] != "Yoga (this one)"),
            "the series and its copy are both gone: {live:?}"
        );

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_this_and_following_truncates_the_base_with_until() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let base_id = seed_weekly_series(db, &f.cal_id, "Standup");
        let base = read_item(db, &base_id);
        let start = chrono::DateTime::parse_from_rfc3339(base["start"].as_str().unwrap()).unwrap();
        let occ = start + chrono::Duration::days(7);

        save(
            db,
            &serde_json::json!({
                "id": base_id,
                "occurrenceStart": occ.to_rfc3339(),
                "kind": "event",
                "title": "Standup (new series)",
                "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": occ.format("%H:%M").to_string(),
                "endTime": (occ + chrono::Duration::hours(1)).format("%H:%M").to_string(),
                "calendarId": f.cal_id,
                "rrule": "FREQ=WEEKLY",
                "scope": "this_and_following",
            }),
        );

        // UNTIL is one minute before the occurrence, in UTC.
        let until = (occ - chrono::Duration::minutes(1))
            .with_timezone(&chrono::Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string();
        let after = read_item(db, &base_id);
        assert_eq!(after["rrule"], format!("FREQ=WEEKLY;UNTIL={until}"));
        assert!(after["exdates"].as_array().unwrap().is_empty());

        // The new series keeps recurring from the edited occurrence.
        let copy = find_item(db, "Standup (new series)");
        assert_eq!(copy["rrule"], "FREQ=WEEKLY");
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(copy["start"].as_str().unwrap()).unwrap(),
            occ
        );
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(copy["end"].as_str().unwrap()).unwrap(),
            occ + chrono::Duration::hours(1),
            "the split-off series keeps the form's end"
        );

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_this_and_following_leaves_an_existing_until_alone() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // A series that already bounds itself must not gain a second UNTIL.
        let start = local_dt(2026, 8, 3, 9, 0);
        let mut item =
            CalendarItem::new(ItemKind::Event, "Bounded", f.cal_id.parse().unwrap(), start);
        item.rrule = Some("FREQ=WEEKLY;UNTIL=20261231T090000Z".into());
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&item).unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );
        let base_id = item.id.to_string();
        let occ = start + chrono::Duration::days(7);

        save(
            db,
            &serde_json::json!({
                "id": base_id, "occurrenceStart": occ.to_rfc3339(),
                "kind": "event", "title": "Bounded (new)", "date": occ.format("%Y-%m-%d").to_string(),
                "startTime": "09:00", "calendarId": f.cal_id,
                "rrule": "FREQ=WEEKLY", "scope": "this_and_following",
            }),
        );

        assert_eq!(
            read_item(db, &base_id)["rrule"],
            "FREQ=WEEKLY;UNTIL=20261231T090000Z"
        );

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_reports_bad_input_instead_of_panicking() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let before = read_items(db).len();

        let cases: [(serde_json::Value, &str); 5] = [
            (
                serde_json::json!({"kind": "event", "title": "X", "date": "not-a-date"}),
                "invalid start",
            ),
            (
                serde_json::json!({"kind": "event", "title": "", "date": "2026-08-24"}),
                "title",
            ),
            (
                serde_json::json!({
                    "kind": "event", "title": "Backwards", "date": "2026-08-24",
                    "startTime": "10:00", "endTime": "09:00", "calendarId": f.cal_id,
                }),
                "end",
            ),
            (
                serde_json::json!({
                    "kind": "event", "title": "Bad colour", "date": "2026-08-24",
                    "calendarId": f.cal_id, "color": "#zzz",
                }),
                "color",
            ),
            (
                serde_json::json!({"kind": "chore", "title": "X", "date": "2026-08-24"}),
                "kind",
            ),
        ];
        for (form, expected) in cases {
            let status = kal_ffi::kal_save_item_json(db, c(&form.to_string()).as_ptr());
            assert_eq!(status, -1, "expected a rejection for {form}");
            let err = last_error().expect("an error message");
            assert!(
                err.contains(expected),
                "{err:?} should mention {expected:?}"
            );
        }

        // Malformed JSON is reported too, and nothing above wrote a row.
        assert_eq!(kal_ffi::kal_save_item_json(db, c("{not json").as_ptr()), -1);
        assert!(last_error().unwrap().contains("invalid save form"));
        assert_eq!(read_items(db).len(), before);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_resolves_the_default_calendar() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        // No calendarId: the one visible live calendar is used.
        save(
            db,
            &serde_json::json!({"kind": "event", "title": "Defaulted", "date": "2026-08-24"}),
        );
        assert_eq!(find_item(db, "Defaulted")["calendar_id"], f.cal_id);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn save_form_creates_a_personal_calendar_when_none_exist() {
    // A database with no calendars at all: the editor must still be able to
    // save, so the bridge creates the same "Personal" the app seeds.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kal.db");
    Database::open(&path).unwrap();
    unsafe {
        let mut db = kal_ffi::kal_open(c(path.to_str().unwrap()).as_ptr());
        assert!(!db.is_null(), "{:?}", last_error());

        save(
            db,
            &serde_json::json!({"kind": "event", "title": "First ever", "date": "2026-08-24"}),
        );

        let cals: Vec<serde_json::Value> =
            serde_json::from_str(&take_string(kal_ffi::kal_calendars_json(db)).unwrap()).unwrap();
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0]["name"], "Personal");
        assert_eq!(cals[0]["color"], "#3366cc");
        assert_eq!(cals[0]["visible"], true);
        assert_eq!(find_item(db, "First ever")["calendar_id"], cals[0]["id"]);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn toggle_item_completion_flips_and_refreshes_the_timestamp() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let start = datetime_from_parts(2026, 8, 24, 9, 0, 0).unwrap();
        let mut task =
            CalendarItem::new(ItemKind::Task, "Buy milk", f.cal_id.parse().unwrap(), start);
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&task).unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );
        task.updated_at = datetime_from_parts(2020, 1, 1, 0, 0, 0).unwrap();
        // Re-write with the old timestamp so the refresh is observable.
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&task).unwrap()).as_ptr()),
            0
        );
        let id = task.id.to_string();
        let before = read_item(db, &id)["updated_at"].clone();

        assert_eq!(
            kal_ffi::kal_toggle_item_completed_json(db, c(&id).as_ptr()),
            0,
            "{:?}",
            last_error()
        );
        let done = read_item(db, &id);
        assert!(!done["completed"].is_null(), "first toggle completes");
        assert_ne!(
            done["updated_at"], before,
            "the LWW timestamp is refreshed so the change propagates"
        );

        assert_eq!(
            kal_ffi::kal_toggle_item_completed_json(db, c(&id).as_ptr()),
            0
        );
        assert!(
            read_item(db, &id)["completed"].is_null(),
            "second toggle un-completes"
        );

        // Unknown id: a reported error, not a panic.
        assert_eq!(
            kal_ffi::kal_toggle_item_completed_json(db, c("01ARZ3NDEKTSV4RRFFQ69G5FAV").as_ptr()),
            -1
        );
        assert!(last_error().unwrap().contains("no item"));
        assert_eq!(
            kal_ffi::kal_toggle_item_completed_json(db, std::ptr::null()),
            -1
        );

        kal_ffi::kal_close(&mut db);
    }
}

/// An empty end time means "no end", not midnight. Reading it as midnight made
/// `end < start` for every end-less item, which the Dioxus editor then swallowed
/// — so a new task could never be created at all.
#[test]
fn save_form_task_has_no_end() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        save(
            db,
            &serde_json::json!({
                "kind": "task", "title": "Buy milk", "date": "2026-08-24",
                "startTime": "09:00", "calendarId": f.cal_id,
            }),
        );

        let saved = find_item(db, "Buy milk");
        assert_eq!(saved["kind"], "task");
        assert_eq!(saved["all_day"], false);
        assert!(saved["end"].is_null(), "a task has no end");
        assert!(saved["completed"].is_null());
        assert!(
            saved["start"]
                .as_str()
                .unwrap()
                .starts_with("2026-08-24T09:00:00"),
            "{}",
            saved["start"]
        );

        kal_ffi::kal_close(&mut db);
    }
}

// ---------------------------------------------------------------------------
// Reminder firings (the Kotlin AlarmManager backend)
// ---------------------------------------------------------------------------

/// Every firing the Android scheduler would arm, as parsed JSON.
unsafe fn firings(db: *mut kal_ffi::KalDb, horizon_days: i64, cap: i64) -> Vec<serde_json::Value> {
    let ptr = unsafe { kal_ffi::kal_reminder_firings_json(db, horizon_days, cap) };
    let raw = take_string(ptr).expect("the firings query failed");
    serde_json::from_str(&raw).unwrap()
}

/// Seed one event with `minutes_before` reminders; returns its ULID.
unsafe fn seed_reminder_item(
    db: *mut kal_ffi::KalDb,
    cal_id: &str,
    title: &str,
    start: chrono::DateTime<chrono::FixedOffset>,
    minutes_before: &[i64],
) -> String {
    let mut item = CalendarItem::new(ItemKind::Event, title, cal_id.parse().unwrap(), start);
    item.end = Some(start + chrono::Duration::hours(1));
    item.reminders = minutes_before
        .iter()
        .map(|m| kal_core::models::Reminder::minutes_before(*m))
        .collect();
    assert_eq!(
        unsafe {
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&item).unwrap()).as_ptr())
        },
        0,
        "{:?}",
        last_error()
    );
    item.id.to_string()
}

/// The instant a firing's `fireAt` names.
fn fire_at(firing: &serde_json::Value) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339(firing["fireAt"].as_str().unwrap()).unwrap()
}

#[test]
fn reminder_firings_are_nearest_first_and_carry_their_item() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let now = chrono::Local::now().fixed_offset();

        let far = seed_reminder_item(
            db,
            &f.cal_id,
            "Far",
            now + chrono::Duration::hours(2),
            &[60],
        );
        let near = seed_reminder_item(
            db,
            &f.cal_id,
            "Near",
            now + chrono::Duration::hours(1),
            &[10],
        );

        let all = firings(db, 366, 64);
        // The fixture's own item is dated 2026-08-24, so its firing is past.
        assert_eq!(all.len(), 2, "{all:?}");

        assert_eq!(all[0]["title"], "Near");
        assert_eq!(all[0]["itemId"], near);
        assert_eq!(fire_at(&all[0]), now + chrono::Duration::minutes(50));
        // The reminder identity comes from the stored item, so a notification
        // can be traced back to the reminder that asked for it.
        assert_eq!(
            all[0]["reminderId"],
            read_item(db, &near)["reminders"][0]["id"]
        );

        assert_eq!(all[1]["title"], "Far");
        assert_eq!(all[1]["itemId"], far);
        assert_eq!(fire_at(&all[1]), now + chrono::Duration::hours(1));

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn reminder_firings_drop_the_past_and_honour_the_cap() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let now = chrono::Local::now().fixed_offset();

        // A 60-minute warning for an event 30 minutes away already happened:
        // missed firings are dropped, never caught up (D20).
        seed_reminder_item(
            db,
            &f.cal_id,
            "Too late",
            now + chrono::Duration::minutes(30),
            &[60],
        );
        assert!(firings(db, 366, 64).is_empty());

        // Two future firings; the cap keeps only the nearest, and 0 means no cap.
        seed_reminder_item(
            db,
            &f.cal_id,
            "Far",
            now + chrono::Duration::hours(2),
            &[60],
        );
        seed_reminder_item(
            db,
            &f.cal_id,
            "Near",
            now + chrono::Duration::hours(1),
            &[10],
        );

        let capped = firings(db, 366, 1);
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0]["title"], "Near");

        let uncapped = firings(db, 366, 0);
        assert_eq!(uncapped.len(), 2);
        assert_eq!(uncapped[0]["title"], "Near");
        assert_eq!(uncapped[1]["title"], "Far");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn completed_tasks_and_deleted_items_are_silent() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let now = chrono::Local::now().fixed_offset();

        let mut task = CalendarItem::new(
            ItemKind::Task,
            "Done",
            f.cal_id.parse().unwrap(),
            now + chrono::Duration::hours(1),
        );
        task.reminders = vec![kal_core::models::Reminder::minutes_before(10)];
        task.completed = Some(now);
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&task).unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let gone = seed_reminder_item(
            db,
            &f.cal_id,
            "Gone",
            now + chrono::Duration::hours(1),
            &[10],
        );
        // Count-shaped, like the core: 1 = a tombstone was written.
        assert_eq!(
            kal_ffi::kal_soft_delete_item(db, c(&gone).as_ptr()),
            1,
            "{:?}",
            last_error()
        );

        assert!(firings(db, 366, 64).is_empty());

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn a_reminder_on_a_hidden_calendar_still_fires() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let now = chrono::Local::now().fixed_offset();

        // Visibility filters the *views*, never the reminders: the Dioxus
        // reconcile effect read the unfiltered items resource.
        let mut hidden = Calendar::local("Hidden", Color("#e91e63".into()));
        hidden.visible = false;
        assert_eq!(
            kal_ffi::kal_upsert_calendar_json(
                db,
                c(&serde_json::to_string(&hidden).unwrap()).as_ptr()
            ),
            0,
            "{:?}",
            last_error()
        );

        seed_reminder_item(
            db,
            &hidden.id.to_string(),
            "Secret",
            now + chrono::Duration::hours(1),
            &[10],
        );

        let all = firings(db, 366, 64);
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[0]["title"], "Secret");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn a_recurring_series_fires_once_per_instance() {
    let f = fixture();
    unsafe {
        use chrono::Timelike;

        let mut db = f.open();
        // Whole seconds: the RRULE expansion works at second granularity, while
        // `now()` carries nanoseconds.
        let start = (chrono::Local::now().fixed_offset() + chrono::Duration::hours(2))
            .with_nanosecond(0)
            .unwrap();

        let mut item =
            CalendarItem::new(ItemKind::Event, "Daily", f.cal_id.parse().unwrap(), start);
        item.end = Some(start + chrono::Duration::hours(1));
        item.rrule = Some("FREQ=DAILY;COUNT=3".into());
        item.reminders = vec![kal_core::models::Reminder::minutes_before(0)];
        assert_eq!(
            kal_ffi::kal_upsert_item_json(db, c(&serde_json::to_string(&item).unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let all = firings(db, 366, 64);
        assert_eq!(all.len(), 3, "{all:?}");
        assert_eq!(fire_at(&all[0]), start);
        assert_eq!(fire_at(&all[1]), start + chrono::Duration::days(1));
        assert_eq!(fire_at(&all[2]), start + chrono::Duration::days(2));

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn firings_report_a_failure_instead_of_panicking() {
    let ptr = unsafe { kal_ffi::kal_reminder_firings_json(std::ptr::null_mut(), 366, 64) };
    assert!(ptr.is_null());
    assert!(
        last_error().is_some_and(|e| !e.is_empty()),
        "a dead handle must explain itself"
    );
}

// ---------------------------------------------------------------------------
// ICS import / export (the sidebar's Import/Export section)
// ---------------------------------------------------------------------------

/// A foreign .ics with a calendar name, one event, a location and a VALARM.
const SAMPLE_ICS: &str = concat!(
    "BEGIN:VCALENDAR\r\n",
    "VERSION:2.0\r\n",
    "PRODID:-//Kal test//EN\r\n",
    "X-WR-CALNAME:Team\r\n",
    "BEGIN:VEVENT\r\n",
    "UID:evt-1\r\n",
    "SUMMARY:Standup\r\n",
    "DTSTART:20260824T090000Z\r\n",
    "DTEND:20260824T093000Z\r\n",
    "LOCATION:Room 2\r\n",
    "BEGIN:VALARM\r\n",
    "ACTION:DISPLAY\r\n",
    "DESCRIPTION:Reminder\r\n",
    "TRIGGER:-PT10M\r\n",
    "END:VALARM\r\n",
    "END:VEVENT\r\n",
    "END:VCALENDAR\r\n",
);

unsafe fn read_calendars(db: *mut kal_ffi::KalDb) -> Vec<serde_json::Value> {
    let raw = unsafe { take_string(kal_ffi::kal_calendars_json(db)) }.unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn import_creates_a_visible_ics_calendar_with_its_items() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        assert_eq!(
            kal_ffi::kal_import_ics_json(db, c(SAMPLE_ICS).as_ptr(), c("Imported").as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        // One new calendar, named from the file (not the fallback), visible and
        // tagged as an .ics import so the sidebar can label its origin.
        let calendars = read_calendars(db);
        let imported = calendars
            .iter()
            .find(|c| c["name"] == "Team")
            .expect("the .ics calendar");
        assert_eq!(imported["source"], "ics_import");
        assert_eq!(imported["visible"], true);

        // The file's event landed on that calendar, with its details.
        let items = read_items(db);
        let event = items
            .iter()
            .find(|i| i["title"] == "Standup" && i["location"] == "Room 2")
            .expect("the imported event");
        assert_eq!(event["kind"], "event");
        assert_eq!(event["calendar_id"], imported["id"]);
        assert_eq!(event["all_day"], false);
        let start = chrono::DateTime::parse_from_rfc3339(event["start"].as_str().unwrap()).unwrap();
        assert_eq!(
            start,
            chrono::DateTime::parse_from_rfc3339("2026-08-24T09:00:00Z").unwrap()
        );
        // The VALARM survived the crossing.
        assert_eq!(reminder_minutes(event), vec![10]);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn import_falls_back_to_the_given_calendar_name() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let unnamed = SAMPLE_ICS.replace("X-WR-CALNAME:Team\r\n", "");
        assert_eq!(
            kal_ffi::kal_import_ics_json(db, c(&unnamed).as_ptr(), c("Imported").as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let named = SAMPLE_ICS.replace("X-WR-CALNAME:Team", "X-WR-CALNAME:Holidays");
        assert_eq!(
            kal_ffi::kal_import_ics_json(db, c(&named).as_ptr(), c("Ignored").as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let names: Vec<String> = read_calendars(db)
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect();
        // Every import makes its own calendar, exactly like the Dioxus sidebar
        // (no deduplication by name).
        assert!(names.contains(&"Imported".to_string()), "{names:?}");
        assert!(names.contains(&"Holidays".to_string()), "{names:?}");
        assert!(names.contains(&"Work".to_string()), "{names:?}");
        assert_eq!(names.len(), 3, "{names:?}");

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn import_reports_a_parse_failure_instead_of_panicking() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        assert_eq!(
            kal_ffi::kal_import_ics_json(
                db,
                c("this is not an ics").as_ptr(),
                c("Imported").as_ptr()
            ),
            -1
        );
        let reason = last_error().expect("a parse failure must explain itself");
        assert!(reason.contains("ics"), "{reason}");

        // A failed parse writes nothing at all.
        assert_eq!(read_calendars(db).len(), 1);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn import_rejects_null_arguments() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        assert_eq!(
            kal_ffi::kal_import_ics_json(db, std::ptr::null(), c("Imported").as_ptr()),
            -1
        );
        assert_eq!(last_error().as_deref(), Some("null ics text"));

        assert_eq!(
            kal_ffi::kal_import_ics_json(db, c(SAMPLE_ICS).as_ptr(), std::ptr::null()),
            -1
        );
        assert_eq!(last_error().as_deref(), Some("null calendar name"));

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn export_writes_one_vcalendar_that_imports_back() {
    let f = fixture();
    unsafe {
        let mut db = f.open();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Kal-export.ics");
        assert_eq!(
            kal_ffi::kal_export_ics_file(db, c(path.to_str().unwrap()).as_ptr()),
            0,
            "{:?}",
            last_error()
        );

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("BEGIN:VCALENDAR"), "{text}");
        assert!(text.contains("Standup"), "{text}");
        assert!(text.contains("Room 1"), "{text}");
        assert!(text.trim_end().ends_with("END:VCALENDAR"), "{text}");

        // Read it into a fresh database: the whole point of the export.
        let fresh_dir = tempfile::tempdir().unwrap();
        let fresh_path = fresh_dir.path().join("kal.db");
        let mut fresh = kal_ffi::kal_open(c(fresh_path.to_str().unwrap()).as_ptr());
        assert!(!fresh.is_null(), "{:?}", last_error());
        assert_eq!(
            kal_ffi::kal_import_ics_json(fresh, c(&text).as_ptr(), c("Imported").as_ptr()),
            0,
            "{:?}",
            last_error()
        );
        let titles: Vec<String> = read_items(fresh)
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(titles, vec!["Standup".to_string()]);
        kal_ffi::kal_close(&mut fresh);

        kal_ffi::kal_close(&mut db);
    }
}

#[test]
fn export_reports_an_unwritable_path() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-dir").join("Kal-export.ics");

        assert_eq!(
            kal_ffi::kal_export_ics_file(db, c(missing.to_str().unwrap()).as_ptr()),
            -1
        );
        let reason = last_error().expect("a failed write must explain itself");
        assert!(reason.contains("write"), "{reason}");

        kal_ffi::kal_close(&mut db);
    }
}

// ---------------------------------------------------------------------------
// Update check (the settings screen's Software & updates section)
// ---------------------------------------------------------------------------

const UPDATE_RELEASE_URL: &str = "https://api.github.com/repos/MihneaMoso/kal/releases/latest";
const UPDATE_ASSET_URL: &str = "https://example/android.apk";

/// Serves canned bodies per URL: the release contract is pinned without a socket.
struct FakeRelease {
    release: String,
    artifact: Vec<u8>,
    serve_artifact: bool,
}

impl kal_update::Transport for FakeRelease {
    fn get(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        if url == UPDATE_RELEASE_URL {
            return Ok(self.release.clone().into_bytes());
        }
        if url == UPDATE_ASSET_URL && self.serve_artifact {
            return Ok(self.artifact.clone());
        }
        Err(format!("unexpected url {url}"))
    }
}

fn release_for(version: &str, digest: &str) -> String {
    format!(
        r#"{{"tag_name": "v{version}", "assets": [
            {{"name": "kal-{version}-android.apk",
             "browser_download_url": "{UPDATE_ASSET_URL}",
             "digest": "sha256:{digest}"}}]}}"#
    )
}

#[test]
fn update_check_stages_a_newer_release_and_verifies_it() {
    let payload = b"apk bytes".to_vec();
    let fake = FakeRelease {
        release: release_for("0.3.0", &kal_update::hex_digest(&payload)),
        artifact: payload.clone(),
        serve_artifact: true,
    };
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("kal/updates/kal-update.apk");

    let json = kal_ffi::check_updates_json_with(&fake, kal_update::ANDROID_TOKEN, "0.2.0", &dest);

    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["status"], "update_ready");
    assert_eq!(v["version"], "0.3.0");
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
}

#[test]
fn update_check_reports_up_to_date_without_downloading() {
    let fake = FakeRelease {
        release: release_for("0.2.0", "whatever"),
        artifact: Vec::new(),
        serve_artifact: false,
    };
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("kal-update.apk");

    let json = kal_ffi::check_updates_json_with(&fake, kal_update::ANDROID_TOKEN, "0.2.0", &dest);

    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["status"], "up_to_date");
    assert!(v.get("version").is_none(), "{json}");
    assert!(v.get("message").is_none(), "{json}");
    assert!(!dest.exists());
}

#[test]
fn update_check_reports_a_failure_as_a_message() {
    let fake = FakeRelease {
        // Promises a digest the payload cannot match.
        release: release_for("0.3.0", "deadbeef"),
        artifact: b"tampered".to_vec(),
        serve_artifact: true,
    };
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("kal-update.apk");

    let json = kal_ffi::check_updates_json_with(&fake, kal_update::ANDROID_TOKEN, "0.2.0", &dest);

    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["status"], "error");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .starts_with("SHA-256 mismatch"),
        "{json}"
    );
    assert!(!dest.exists(), "a rejected download must not be staged");
}

#[test]
fn update_check_rejects_null_arguments() {
    unsafe {
        assert!(
            kal_ffi::kal_check_updates_json(std::ptr::null(), c("/tmp/x.apk").as_ptr()).is_null()
        );
        assert_eq!(last_error().as_deref(), Some("null current version"));

        assert!(kal_ffi::kal_check_updates_json(c("0.2.0").as_ptr(), std::ptr::null()).is_null());
        assert_eq!(last_error().as_deref(), Some("null download path"));
    }
}

// ---------------------------------------------------------------------------
// Sync chain: the sidebar's Sync panel calls exactly this surface.
// ---------------------------------------------------------------------------

/// A fresh install is unpaired, and reading status must not mint an identity
/// (Step 0 Q3: no silent random-chain join). The file lives next to the DB so
/// the Dioxus app's chain carries over.
#[test]
fn sync_status_is_unpaired_on_a_fresh_install_without_creating_files() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let status = take_string(kal_ffi::kal_sync_status_json(db)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(v["paired"], false, "{status}");
        assert!(v["fingerprint"].is_null(), "{status}");
        assert!(last_error().is_none());

        let identity_file = f.path.with_file_name("sync-identity.json");
        assert!(
            !identity_file.exists(),
            "status must not create an identity"
        );
        assert!(!identity_file.with_file_name("sync-node.json").exists());

        kal_ffi::kal_close(&mut db);
    }
}

/// Starting a chain returns the 24-word phrase + fingerprint, persists it next
/// to the DB, and a second handle (an app restart) reads the same chain.
#[test]
fn sync_start_chain_persists_the_phrase_for_the_next_launch() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        let started = take_string(kal_ffi::kal_sync_start_chain_json(db)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&started).unwrap();
        let phrase = v["phrase"].as_str().unwrap();
        let fingerprint = v["fingerprint"].as_str().unwrap();
        assert_eq!(phrase.split_whitespace().count(), 24, "{started}");
        assert_eq!(fingerprint.len(), 16, "{started}");

        assert!(f.path.with_file_name("sync-identity.json").exists());

        let status = take_string(kal_ffi::kal_sync_status_json(db)).unwrap();
        let s: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(s["paired"], true);
        assert_eq!(s["fingerprint"], fingerprint);

        let revealed = take_string(kal_ffi::kal_sync_phrase_json(db)).unwrap();
        let r: serde_json::Value = serde_json::from_str(&revealed).unwrap();
        assert_eq!(r["phrase"], phrase);
        assert_eq!(r["fingerprint"], fingerprint);

        kal_ffi::kal_close(&mut db);

        // "Restart": a fresh handle on the same DB is still paired.
        let mut db2 = f.open();
        let status2 = take_string(kal_ffi::kal_sync_status_json(db2)).unwrap();
        let s2: serde_json::Value = serde_json::from_str(&status2).unwrap();
        assert_eq!(s2["fingerprint"], fingerprint);
        kal_ffi::kal_close(&mut db2);
    }
}

/// Joining with a valid phrase yields the same chain fingerprint on both
/// devices; garbage is refused with the phrase error, leaving the device
/// unpaired.
#[test]
fn sync_join_chain_matches_the_fingerprint_and_rejects_garbage() {
    let f = fixture();
    let identity = kal_sync::ChainIdentity::generate().unwrap();
    unsafe {
        let mut db = f.open();

        let joined = take_string(kal_ffi::kal_sync_join_chain_json(
            db,
            c(&identity.phrase()).as_ptr(),
        ))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&joined).unwrap();
        assert_eq!(
            v["fingerprint"],
            identity.fingerprint().fingerprint_hex.as_str()
        );

        // A fresh fixture that failed to join stays unpaired.
        let g = fixture();
        let mut other = g.open();
        let refused = take_string(kal_ffi::kal_sync_join_chain_json(
            other,
            c("not a phrase").as_ptr(),
        ));
        assert!(refused.is_none());
        assert_eq!(last_error().as_deref(), Some("Invalid recovery phrase"));
        let status = take_string(kal_ffi::kal_sync_status_json(other)).unwrap();
        let s: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(s["paired"], false);
        kal_ffi::kal_close(&mut other);

        kal_ffi::kal_close(&mut db);
    }
}

/// Leaving removes the chain identity (and outbox) but keeps the per-device
/// node identity; status goes back to unpaired.
#[test]
fn sync_leave_clears_the_chain_but_keeps_the_node_identity() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        take_string(kal_ffi::kal_sync_start_chain_json(db)).unwrap();
        let node = f.path.with_file_name("sync-node.json");
        std::fs::write(&node, "{}").unwrap();

        assert_eq!(kal_ffi::kal_sync_leave(db), 0);
        assert!(last_error().is_none());
        assert!(!f.path.with_file_name("sync-identity.json").exists());
        assert!(node.exists(), "the node identity is per-device");

        let status = take_string(kal_ffi::kal_sync_status_json(db)).unwrap();
        let s: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(s["paired"], false);

        // Leaving again is a clean success, not an error.
        assert_eq!(kal_ffi::kal_sync_leave(db), 0);

        kal_ffi::kal_close(&mut db);
    }
}

/// A round without a chain reports "not paired" instead of touching the
/// network.
#[test]
fn sync_round_without_a_chain_reports_not_paired() {
    let f = fixture();
    unsafe {
        let mut db = f.open();
        assert!(kal_ffi::kal_sync_round_json(db).is_null());
        assert_eq!(last_error().as_deref(), Some("not paired"));
        kal_ffi::kal_close(&mut db);
    }
}

/// NULL handles are refused on every sync export.
#[test]
fn sync_exports_reject_a_null_handle() {
    unsafe {
        assert!(kal_ffi::kal_sync_status_json(std::ptr::null_mut()).is_null());
        assert_eq!(last_error().as_deref(), Some("null database handle"));
        assert!(kal_ffi::kal_sync_start_chain_json(std::ptr::null_mut()).is_null());
        assert!(kal_ffi::kal_sync_phrase_json(std::ptr::null_mut()).is_null());
        assert!(kal_ffi::kal_sync_round_json(std::ptr::null_mut()).is_null());
        assert_eq!(kal_ffi::kal_sync_leave(std::ptr::null_mut()), -1);
        assert_eq!(last_error().as_deref(), Some("null database handle"));
    }
}
