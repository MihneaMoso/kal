//! A "This event" occurrence stays a member of its series.
//!
//! The copy is still a single row (the series EXDATEs the original instant), but
//! it carries `metadata.series_id`, so the editor offers the scope choice again
//! and an "All events" save reaches it. These tests pin the semantics the owner
//! asked for:
//!
//! - "All events" recolours/renames every occurrence, including ones edited
//!   individually, without re-dating them;
//! - the series keeps its own rule and EXDATEs when the dialog is opened on an
//!   exception (it shows the series' rule there);
//! - "This event" on an exception updates that row only;
//! - deleting a series takes its detached occurrences with it.

use chrono::{Datelike, TimeZone};
use kal_core::models::{Calendar, CalendarItem, Color, DateTimeTz, ItemKind};
use kal_storage::Database;
use ulid::Ulid;

fn ts(value: &str) -> DateTimeTz {
    chrono::DateTime::parse_from_rfc3339(value).unwrap()
}

fn setup() -> (Database, Ulid) {
    let db = Database::open_in_memory().unwrap();
    let cal = Calendar::local("Work", Color("#3366cc".into()));
    db.upsert_calendar(&cal).unwrap();
    (db, cal.id)
}

/// A weekly series row starting 2026-08-03 09:00+03:00, hour long.
fn series(cal: Ulid) -> CalendarItem {
    let start = ts("2026-08-03T09:00:00+03:00");
    let mut item = CalendarItem::new(ItemKind::Event, "Standup", cal, start);
    item.end = Some(start + chrono::Duration::hours(1));
    item.rrule = Some("FREQ=WEEKLY".into());
    item
}

/// The form the dialog sends for `row`: the row's own date/time with `edits`.
fn form(row: &CalendarItem, title: &str, color: &str) -> CalendarItem {
    let mut item = row.clone();
    item.title = title.into();
    item.color_override = Some(Color(color.into()));
    item
}

fn by_id(items: &[CalendarItem], id: Ulid) -> &CalendarItem {
    items.iter().find(|i| i.id == id).expect("row exists")
}

/// Two occurrences detached from `base`, each with its own colour.
fn two_exceptions(db: &Database, base: &CalendarItem) -> (CalendarItem, CalendarItem) {
    let first_occ = ts("2026-08-17T09:00:00+03:00");
    let second_occ = ts("2026-08-24T09:00:00+03:00");

    // The dialog is anchored on the clicked occurrence (as in the app).
    let mut first = series(base.calendar_id);
    first.id = base.id;
    first.title = "Standup (first)".into();
    first.start = first_occ;
    first.end = Some(first_occ + chrono::Duration::hours(1));
    first.color_override = Some(Color("#27ae60".into()));
    db.apply_scoped_edit(base.id, &first, first_occ, false)
        .unwrap();

    let mut second = series(base.calendar_id);
    second.id = base.id;
    second.title = "Standup (second)".into();
    second.start = second_occ;
    second.end = Some(second_occ + chrono::Duration::hours(1));
    second.color_override = Some(Color("#e91e63".into()));
    db.apply_scoped_edit(base.id, &second, second_occ, false)
        .unwrap();

    let all = db.list_items(false).unwrap();
    let copies: Vec<CalendarItem> = all.iter().filter(|i| i.id != base.id).cloned().collect();
    assert_eq!(copies.len(), 2, "{all:#?}");
    (copies[0].clone(), copies[1].clone())
}

/// A birthday is a recurring item too, so its dialog save goes through the
/// "All events" path: the person the dialog carries must survive there.
#[test]
fn all_events_keeps_a_birthdays_person() {
    let (db, cal) = setup();
    let start = ts("1990-12-10T00:00:00+02:00");
    let mut birthday = CalendarItem::new(ItemKind::Birthday, "Ada", cal, start);
    birthday.all_day = true;
    birthday.end = None;
    birthday.rrule = Some("FREQ=YEARLY".into());
    birthday.metadata.birthday_of = Some("Ada".into());
    db.upsert_item(&birthday).unwrap();

    let mut edited = birthday.clone();
    edited.title = "Ada Lovelace".into();
    edited.metadata.birthday_of = Some("Ada Lovelace".into());
    assert!(db.apply_all_edit(&edited).unwrap());

    let after = db.get_item(birthday.id).unwrap().unwrap();
    assert_eq!(after.title, "Ada Lovelace");
    assert_eq!(after.metadata.birthday_of.as_deref(), Some("Ada Lovelace"));
    assert_eq!(after.rrule.as_deref(), Some("FREQ=YEARLY"));
}

#[test]
fn this_event_links_the_copy_to_its_series() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let occ = ts("2026-08-17T09:00:00+03:00");

    let mut edited = base.clone();
    edited.title = "Standup (moved)".into();
    assert!(db.apply_scoped_edit(base.id, &edited, occ, false).unwrap());

    let items = db.list_items(false).unwrap();
    let copy = items.iter().find(|i| i.id != base.id).expect("the copy");
    assert_eq!(copy.metadata.series_id, Some(base.id));
    assert_eq!(copy.metadata.recurrence_id, Some(occ));
    assert_eq!(copy.rrule, None, "the copy is a single item");

    // And the series excluded exactly that instant.
    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(after.exdates, vec![occ]);
}

#[test]
fn all_events_reaches_every_detached_occurrence_without_re_dating_it() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let (first, second) = two_exceptions(&db, &base);
    let first_start = first.start;
    let second_start = second.start;

    // The dialog is opened on the series row (or any occurrence) and recoloured.
    let edited = form(&base, "Standup (team)", "#8e44ad");
    assert!(db.apply_all_edit(&edited).unwrap());

    let items = db.list_items(false).unwrap();
    let after_base = by_id(&items, base.id);
    assert_eq!(after_base.color_override, Some(Color("#8e44ad".into())));
    assert_eq!(after_base.title, "Standup (team)");
    assert_eq!(after_base.rrule.as_deref(), Some("FREQ=WEEKLY"));
    assert_eq!(after_base.exdates.len(), 2, "the exclusions survive");

    for (copy, start) in [(&first, first_start), (&second, second_start)] {
        let after = by_id(&items, copy.id);
        assert_eq!(
            after.color_override,
            Some(Color("#8e44ad".into())),
            "the individually edited occurrence follows the series colour"
        );
        assert_eq!(after.title, "Standup (team)");
        assert_eq!(after.start, start, "its own date is untouched");
        assert_eq!(after.rrule, None, "it is still one occurrence");
        assert_eq!(after.metadata.series_id, Some(base.id), "still linked");
    }
}

#[test]
fn all_events_from_an_occurrence_keeps_the_series_rule_and_shifts_the_clock() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let (first, second) = two_exceptions(&db, &base);

    // Open the *first* exception, move its clock to 14:00, and pick "All events".
    // The dialog shows the series' rule there, so the form carries it.
    let mut edited = first.clone();
    edited.start = at(first.start, 14, 0);
    edited.end = Some(edited.start + chrono::Duration::hours(1));
    edited.color_override = Some(Color("#e67e22".into()));
    edited.title = "Standup (late)".into();
    edited.rrule = Some("FREQ=WEEKLY".into());

    assert!(db.apply_all_edit(&edited).unwrap());

    let items = db.list_items(false).unwrap();
    let after_base = by_id(&items, base.id);
    assert_eq!(
        after_base.rrule.as_deref(),
        Some("FREQ=WEEKLY"),
        "the form's rule belongs to the series, never to the occurrence"
    );
    assert_eq!(after_base.exdates.len(), 2, "the exclusions survive");
    assert_eq!(after_base.title, "Standup (late)");
    assert_eq!(
        after_base.start,
        ts("2026-08-03T14:00:00+03:00"),
        "the series keeps its date and takes the dialog's time"
    );

    let after_first = by_id(&items, first.id);
    assert_eq!(
        after_first.start, edited.start,
        "the row being edited keeps the dialog's date and time, not shifted twice"
    );
    assert_eq!(after_first.rrule, None);
    assert_eq!(after_first.metadata.series_id, Some(base.id));

    let after_second = by_id(&items, second.id);
    assert_eq!(
        after_second.start,
        ts("2026-08-24T14:00:00+03:00"),
        "the other occurrence moves with the series' clock, on its own date"
    );
    assert_eq!(after_second.end, Some(ts("2026-08-24T15:00:00+03:00")));
    assert_eq!(after_second.color_override, Some(Color("#e67e22".into())));
}

#[test]
fn this_event_on_a_detached_occurrence_updates_it_in_place() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let occ = ts("2026-08-17T09:00:00+03:00");
    let mut edited = base.clone();
    db.apply_scoped_edit(base.id, &edited, occ, false).unwrap();
    let copy = db
        .list_items(false)
        .unwrap()
        .into_iter()
        .find(|i| i.id != base.id)
        .unwrap();
    edited.id = copy.id;

    let mut renamed = copy.clone();
    renamed.title = "Standup (just this one)".into();
    renamed.color_override = Some(Color("#16a085".into()));
    // The dialog shows the series' rule; it must not turn the copy into a series.
    renamed.rrule = Some("FREQ=WEEKLY".into());
    assert!(db.save_exception(&renamed).unwrap());

    let items = db.list_items(false).unwrap();
    assert_eq!(items.len(), 2, "no second copy was created");
    let after = by_id(&items, copy.id);
    assert_eq!(after.title, "Standup (just this one)");
    assert_eq!(after.color_override, Some(Color("#16a085".into())));
    assert_eq!(after.rrule, None);
    assert_eq!(after.metadata.series_id, Some(base.id));
    assert_eq!(after.metadata.recurrence_id, Some(occ));
    assert_eq!(
        db.get_item(base.id).unwrap().unwrap().exdates,
        vec![occ],
        "the series is untouched"
    );
}

#[test]
fn deleting_a_series_tombstones_its_detached_occurrences() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let (_first, _second) = two_exceptions(&db, &base);

    assert!(db.delete_with_exceptions(base.id).unwrap());
    assert!(db.list_items(false).unwrap().is_empty());
    let all = db.list_items(true).unwrap();
    assert!(all.iter().all(|i| i.deleted), "{all:#?}");

    // Deleting one occurrence on its own leaves the series and its siblings.
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let (first, second) = two_exceptions(&db, &base);
    assert!(db.delete_with_exceptions(first.id).unwrap());
    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 2, "series + the other occurrence");
    assert!(live.iter().any(|i| i.id == base.id));
    assert!(live.iter().any(|i| i.id == second.id));
}

#[test]
fn editing_an_occurrence_after_its_series_is_deleted_detaches_it() {
    let (db, cal) = setup();
    let base = series(cal);
    db.upsert_item(&base).unwrap();
    let (first, _second) = two_exceptions(&db, &base);

    db.delete_with_exceptions(base.id).unwrap();
    let mut revived = first.clone();
    revived.title = "Still here".into();
    assert!(db.save_exception(&revived).unwrap());

    let after = db.get_item(first.id).unwrap().unwrap();
    assert_eq!(after.title, "Still here");
    assert_eq!(after.metadata.series_id, None);
    assert_eq!(after.metadata.recurrence_id, None);
}

/// `day`'s date at another wall-clock time, in its own offset.
fn at(day: DateTimeTz, hour: u32, minute: u32) -> DateTimeTz {
    day.offset()
        .with_ymd_and_hms(day.year(), day.month(), day.day(), hour, minute, 0)
        .single()
        .expect("fixed offsets are never ambiguous")
}
