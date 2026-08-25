use chrono::Timelike;
use kal_core::models::{Calendar, CalendarItem, CalendarSource, Color, ItemKind, Reminder};
use kal_storage::Database;

fn ts(y: i32, m: u32, d: u32, h: u32) -> kal_core::models::DateTimeTz {
    kal_core::models::datetime_from_parts(y, m, d, h, 0, 0).unwrap()
}

fn setup() -> (Database, Calendar) {
    let db = Database::open_in_memory().unwrap();
    let cal = Calendar::local("Work", Color("#3366cc".into()));
    db.upsert_calendar(&cal).unwrap();
    (db, cal)
}

#[test]
fn migration_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    // Open twice; second open must not fail or duplicate schema.
    drop(Database::open(&path).unwrap());
    Database::open(&path).unwrap();
}

#[test]
fn calendar_round_trip() {
    let (db, cal) = setup();
    assert_eq!(db.get_calendar(cal.id).unwrap().unwrap(), cal);
    let mut all = db.list_calendars().unwrap();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(all.iter().any(|c| c.id == cal.id));

    let mut renamed = cal.clone();
    renamed.name = "Renamed".into();
    renamed.visible = false;
    db.upsert_calendar(&renamed).unwrap();
    assert_eq!(db.list_calendars().unwrap().len(), 1);
    assert_eq!(db.get_calendar(cal.id).unwrap().unwrap().name, "Renamed");
}

#[test]
fn item_round_trip_preserves_everything() {
    let (db, cal) = setup();

    let mut item = CalendarItem::new(ItemKind::Event, "Standup", cal.id, ts(2026, 8, 24, 9));
    item.end = Some(ts(2026, 8, 24, 9).with_minute(30).unwrap());
    item.location = Some("Room 2".into());
    item.notes = Some("daily sync".into());
    item.rrule = Some("FREQ=DAILY".into());
    item.exdates = vec![ts(2026, 8, 25, 9)];
    item.reminders = vec![Reminder::minutes_before(10), Reminder::minutes_before(1440)];
    item.color_override = Some(Color("#ff8800".into()));
    item.metadata.birthday_of = None;

    db.upsert_item(&item).unwrap();
    assert_eq!(db.get_item(item.id).unwrap().unwrap(), item);

    // Range query containing the event.
    let hits = db
        .items_in_range(ts(2026, 8, 23, 0), ts(2026, 8, 25, 0))
        .unwrap();
    assert_eq!(hits.len(), 1);

    // Range query missing it.
    let miss = db
        .items_in_range(ts(2030, 1, 1, 0), ts(2030, 1, 2, 0))
        .unwrap();
    assert!(miss.is_empty());
}

#[test]
fn soft_delete_keeps_tombstone_hidden_from_queries() {
    let (db, cal) = setup();
    let item = CalendarItem::new(ItemKind::Task, "Buy milk", cal.id, ts(2026, 8, 24, 12));
    db.upsert_item(&item).unwrap();

    assert!(db.soft_delete_item(item.id).unwrap());
    assert!(!db.soft_delete_item(item.id).is_err());

    assert!(db.get_item(item.id).unwrap().unwrap().deleted);
    assert!(db
        .items_in_range(ts(2026, 8, 24, 0), ts(2026, 8, 25, 0))
        .unwrap()
        .is_empty());

    let all = db.list_items(true).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].deleted);
}

#[test]
fn update_via_upsert_replaces_row() {
    let (db, cal) = setup();
    let mut item = CalendarItem::new(ItemKind::Event, "Old", cal.id, ts(2026, 8, 24, 9));
    db.upsert_item(&item).unwrap();

    item.title = "New".into();
    item.start = ts(2026, 9, 1, 15);
    db.upsert_item(&item).unwrap();

    let stored = db.get_item(item.id).unwrap().unwrap();
    assert_eq!(stored.title, "New");
    assert_eq!(stored.start, ts(2026, 9, 1, 15));
    assert_eq!(db.list_items(false).unwrap().len(), 1);
}

#[test]
fn invalid_items_are_rejected() {
    let (db, cal) = setup();
    let mut item = CalendarItem::new(ItemKind::Event, "   ", cal.id, ts(2026, 8, 24, 9));
    assert!(db.upsert_item(&item).is_err());

    item.title = "ok".into();
    item.end = Some(ts(2026, 8, 23, 9)); // before start
    assert!(db.upsert_item(&item).is_err());
}

#[test]
fn foreign_key_enforced() {
    let db = Database::open_in_memory().unwrap();
    let orphan = CalendarItem::new(
        ItemKind::Event,
        "no calendar",
        ulid::Ulid::new(),
        ts(2026, 8, 24, 9),
    );
    // validate() passes but FK constraint must catch the dangling reference.
    assert!(orphan.validate().is_ok());
    assert!(db.upsert_item(&orphan).is_err());
}

#[test]
fn birthday_item_round_trip_with_metadata() {
    let (db, _cal) = setup();
    let bd_cal = Calendar {
        id: ulid::Ulid::new(),
        name: "Birthdays".into(),
        color: Color("#e91e63".into()),
        source: CalendarSource::Birthdays,
        visible: true,
        updated_at: ts(2020, 1, 1, 0),
    };
    db.upsert_calendar(&bd_cal).unwrap();

    let mut bday = CalendarItem::new(
        ItemKind::Birthday,
        "Ada Lovelace",
        bd_cal.id,
        ts(1815, 12, 10, 0),
    );
    bday.all_day = true;
    bday.metadata.birthday_of = Some("vcf:ada-1".into());
    db.upsert_item(&bday).unwrap();

    let got = db.get_item(bday.id).unwrap().unwrap();
    assert_eq!(got.kind, ItemKind::Birthday);
    assert_eq!(got.metadata.birthday_of.as_deref(), Some("vcf:ada-1"));
}

#[test]
fn settings_round_trip_and_upsert() {
    let db = Database::open_in_memory().unwrap();
    assert_eq!(db.get_setting("theme").unwrap(), None);
    db.set_setting("theme", r#""dark""#).unwrap();
    db.set_setting("time_format", r#""24h""#).unwrap();
    assert_eq!(
        db.get_setting("theme").unwrap().as_deref(),
        Some(r#""dark""#)
    );
    db.set_setting("theme", r#""light""#).unwrap(); // upsert replaces
    assert_eq!(
        db.get_setting("theme").unwrap().as_deref(),
        Some(r#""light""#)
    );

    let mut all = db.all_settings().unwrap();
    all.sort();
    assert_eq!(all.len(), 2);

    // Survives reopen (migrations idempotent).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.db");
    drop(Database::open(&path).unwrap());
}

#[test]
fn legacy_pre_v2_calendar_rows_with_empty_timestamp_read_cleanly() {
    // Simulate a database written before migration v2: calendars lack any
    // updated_at (the ALTER added DEFAULT ''). list_calendars() must not
    // fail with "premature end of input" — regression for the sync/editor
    // breakage on real user databases.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE calendars (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL,
                source TEXT NOT NULL, visible INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO calendars VALUES
             ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'Personal', '#3366cc', '\"local\"', 1);
            PRAGMA user_version = 1;",
        )
        .unwrap();
    }

    let db = Database::open(&path).unwrap(); // applies v2 + v3 migrations
    let cals = db.list_calendars().unwrap();
    assert_eq!(cals.len(), 1);
    assert_eq!(cals[0].name, "Personal");

    // Writing back stamps whatever the caller provides (sync LWW contract):
    // callers set updated_at = now on mutation.
    let mut repaired = cals[0].clone();
    repaired.updated_at = chrono::Utc::now().fixed_offset();
    db.upsert_calendar(&repaired).unwrap();
    let reread = db.list_calendars().unwrap();
    assert!(reread[0].updated_at.timestamp() > 1_700_000_000);
}
