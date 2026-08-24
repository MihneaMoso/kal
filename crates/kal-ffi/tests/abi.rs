//! Exercises the C ABI the way Swift/Kotlin shims will call it.

use std::ffi::{c_char, CStr, CString};

use kal_storage::Database;

use kal_core::models::{datetime_from_parts, Calendar, CalendarItem, Color, ItemKind};

unsafe fn take_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    kal_ffi::kal_free(ptr);
    Some(s)
}

fn setup_db(path: &std::path::Path) -> ulid::Ulid {
    let db = Database::open(path).unwrap();
    let cal = Calendar::local("Work", Color("#3366cc".into()));
    db.upsert_calendar(&cal).unwrap();

    let mut ev = CalendarItem::new(
        ItemKind::Event,
        "Standup",
        cal.id,
        datetime_from_parts(2026, 8, 24, 9, 0, 0).unwrap(),
    );
    ev.end = Some(datetime_from_parts(2026, 8, 24, 10, 0, 0).unwrap());
    db.upsert_item(&ev).unwrap();

    let mut bday = CalendarItem::new(
        ItemKind::Birthday,
        "Ada",
        cal.id,
        datetime_from_parts(1990, 8, 24, 0, 0, 0).unwrap(),
    );
    bday.all_day = true;
    // Birthdays are yearly-recurring by spec (§4); the editor enforces this.
    bday.rrule = Some("FREQ=YEARLY".into());
    bday.metadata.birthday_of = Some("Ada".into());
    db.upsert_item(&bday).unwrap();

    cal.id
}

#[test]
fn open_query_close_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kal.db");
    setup_db(&path);

    let c_path = CString::new(path.to_str().unwrap()).unwrap();
    unsafe {
        let mut db = kal_ffi::kal_open(c_path.as_ptr());
        assert!(!db.is_null());

        // Window containing Aug 24 2026 (epoch bounds generous).
        let from = 1785000000; // ~2026-08-01
        let to = 1787700000; // ~2026-09-01
        let json = take_string(kal_ffi::kal_upcoming_json(db, from, to)).unwrap();
        assert!(json.contains("Standup"), "{json}");
        assert!(json.contains("\"kind\":\"birthday\""), "{json}");
        assert!(
            json.contains("\"age\":36") || json.contains("\"age\":35"),
            "{json}"
        );
        assert!(json.contains("#3366cc"));

        // Month grid for August 2026.
        let grid = take_string(kal_ffi::kal_month_grid_json(db, 2026, 8, 0)).unwrap();
        assert!(grid.contains("2026-08-24"));
        assert!(grid.contains("Standup"));
        assert!(grid.contains("\"inMonth\":false")); // spillover days exist

        // Invalid inputs return NULL, not panics.
        assert!(take_string(kal_ffi::kal_month_grid_json(db, -5, 99, 0)).is_none());
        assert!(take_string(kal_ffi::kal_upcoming_json(std::ptr::null_mut(), 0, 1)).is_none());

        kal_ffi::kal_close(&mut db);
        assert!(db.is_null()); // nulled after close
        kal_ffi::kal_close(&mut db); // double-close is safe
        kal_ffi::kal_free(std::ptr::null_mut()); // NULL free is safe
    }
}

#[test]
fn open_with_garbage_path_returns_null() {
    let bad = CString::new("/nonexistent-dir-that-cannot-exist/x/y.db").unwrap();
    // Parent dirs don't exist → open must fail cleanly with NULL.
    unsafe {
        assert!(kal_ffi::kal_open(bad.as_ptr()).is_null());
    }
}
