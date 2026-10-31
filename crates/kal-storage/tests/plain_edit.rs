//! "All events" on a row that is not a recurring series.
//!
//! Both clients send `scope=all` for a normal save, so a plain item reaches
//! [`kal_storage::scoped::apply_all_edit`] too. The dialog's date must win
//! there: re-anchoring it on the stored start made date edits silently do
//! nothing. A recurring series is different — its form is anchored on the
//! clicked occurrence, so it must keep its own start date.

use kal_core::models::{Calendar, CalendarItem, Color, DateTimeTz, ItemKind};
use kal_storage::Database;
use ulid::Ulid;

fn ts(value: &str) -> DateTimeTz {
    chrono::DateTime::parse_from_rfc3339(value).unwrap()
}

fn setup() -> (Database, Ulid) {
    let db = Database::open_in_memory().unwrap();
    let cal = Calendar::local("Personal", Color("#3366cc".into()));
    db.upsert_calendar(&cal).unwrap();
    (db, cal.id)
}

fn item(cal: Ulid, title: &str, start: &str, end: Option<&str>) -> CalendarItem {
    let start = ts(start);
    let mut item = CalendarItem::new(ItemKind::Event, title, cal, start);
    item.end = end.map(ts);
    item
}

/// What the dialog sends after the user moves the date: one date field, so the
/// end lands on the same date even when the stored item spans several days.
fn form(row: &CalendarItem, date: &str, start_time: &str, end_time: &str) -> CalendarItem {
    let mut item = row.clone();
    let offset = row.start.format("%:z").to_string();
    item.start = ts(&format!("{date}T{start_time}:00{offset}"));
    item.end = Some(ts(&format!("{date}T{end_time}:00{offset}")));
    item
}

#[test]
fn all_scope_moves_a_plain_item_to_the_dialogs_date() {
    let (db, cal) = setup();
    let base = item(
        cal,
        "Dentist",
        "2026-10-30T09:00:00+02:00",
        Some("2026-10-30T10:00:00+02:00"),
    );
    db.upsert_item(&base).unwrap();

    let mut edited = form(&base, "2026-11-05", "09:00", "10:00");
    edited.title = "Dentist (moved)".into();

    assert!(db.apply_all_edit(&edited).unwrap());

    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(after.start, ts("2026-11-05T09:00:00+02:00"));
    assert_eq!(after.end, Some(ts("2026-11-05T10:00:00+02:00")));
    assert_eq!(after.title, "Dentist (moved)");
    assert_eq!(after.rrule, None);
}

#[test]
fn all_scope_moves_a_multiday_item_without_collapsing_its_span() {
    let (db, cal) = setup();
    let base = item(
        cal,
        "Conference",
        "2026-10-30T09:00:00+02:00",
        Some("2026-11-02T12:00:00+02:00"),
    );
    db.upsert_item(&base).unwrap();

    // The dialog only knows the end *time*, so it sends 12:00 on the new start
    // date. The stored three-day span must survive the move.
    let edited = form(&base, "2026-11-05", "09:00", "12:00");

    assert!(db.apply_all_edit(&edited).unwrap());

    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(after.start, ts("2026-11-05T09:00:00+02:00"));
    assert_eq!(
        after.end,
        Some(ts("2026-11-08T12:00:00+02:00")),
        "the end keeps its original distance from the start"
    );
}

#[test]
fn all_scope_still_anchors_a_recurring_series_on_its_own_date() {
    let (db, cal) = setup();
    let mut base = item(
        cal,
        "Standup",
        "2026-08-03T09:00:00+03:00",
        Some("2026-08-03T10:00:00+03:00"),
    );
    base.rrule = Some("FREQ=WEEKLY".into());
    db.upsert_item(&base).unwrap();

    // The dialog was opened on the 2026-10-19 occurrence, so its date is not
    // the series' start; only the wall-clock time may move.
    let edited = form(&base, "2026-10-19", "11:30", "12:15");

    assert!(db.apply_all_edit(&edited).unwrap());

    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(after.start, ts("2026-08-03T11:30:00+03:00"));
    assert_eq!(after.end, Some(ts("2026-08-03T12:15:00+03:00")));
    assert_eq!(after.rrule.as_deref(), Some("FREQ=WEEKLY"));
}

#[test]
fn all_scope_keeps_a_multiday_recurring_series_span() {
    let (db, cal) = setup();
    let mut base = item(
        cal,
        "Retreat",
        "2026-08-03T09:00:00+03:00",
        Some("2026-08-05T12:00:00+03:00"),
    );
    base.rrule = Some("FREQ=WEEKLY".into());
    db.upsert_item(&base).unwrap();

    let edited = form(&base, "2026-10-19", "09:00", "12:00");

    assert!(db.apply_all_edit(&edited).unwrap());

    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(after.start, ts("2026-08-03T09:00:00+03:00"));
    assert_eq!(after.end, Some(ts("2026-08-05T12:00:00+03:00")));
}
