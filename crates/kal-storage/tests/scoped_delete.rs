//! Scoped deletes for recurring items, plus the split end-bound regression.
//!
//! The bug: pressing Delete on a recurring event and then "…and following" ran
//! an *edit* split. The base already had an `UNTIL`, so it was left untouched,
//! and the new tail inherited no end at all. The following occurrences therefore
//! existed twice (and the tail ran forever). A scoped delete must never create a
//! replacement row, and a split must carry the original end onto the tail.

use chrono::{Duration, NaiveDate};
use kal_core::models::{Calendar, CalendarItem, Color, DateTimeTz, ItemKind};
use kal_core::viewmodel::expand_occurrences;
use kal_storage::scoped::DeleteScope;
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

fn series(cal: Ulid, start: &str, rule: &str) -> CalendarItem {
    let start = ts(start);
    let mut item = CalendarItem::new(ItemKind::Event, "Standup", cal, start);
    item.end = Some(start + Duration::hours(1));
    item.rrule = Some(rule.into());
    item
}

/// The live occurrence instants of one row in `[from, to]`, as RFC 3339.
fn starts(db: &Database, id: Ulid, from: &str, to: &str) -> Vec<String> {
    let item = db.get_item(id).unwrap().unwrap();
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap();
    let to = NaiveDate::parse_from_str(to, "%Y-%m-%d").unwrap();
    expand_occurrences(&item, from, to)
        .into_iter()
        .map(|o| o.start.to_rfc3339())
        .collect()
}

/// Two "This event" copies of a weekly series, at the given occurrences.
fn two_exceptions(
    db: &Database,
    base: &CalendarItem,
    first_occ: DateTimeTz,
    second_occ: DateTimeTz,
) -> (CalendarItem, CalendarItem) {
    let mut first = base.clone();
    first.title = "Standup (first)".into();
    first.start = first_occ;
    first.end = Some(first_occ + Duration::hours(1));
    db.apply_scoped_edit(base.id, &first, first_occ, false)
        .unwrap();

    let mut second = base.clone();
    second.title = "Standup (second)".into();
    second.start = second_occ;
    second.end = Some(second_occ + Duration::hours(1));
    db.apply_scoped_edit(base.id, &second, second_occ, false)
        .unwrap();

    let items = db.list_items(false).unwrap();
    let copies: Vec<CalendarItem> = items.into_iter().filter(|i| i.id != base.id).collect();
    assert_eq!(copies.len(), 2);
    (copies[0].clone(), copies[1].clone())
}

#[test]
fn this_only_on_a_series_excludes_the_occurrence_without_a_copy() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();

    let occ = ts("2026-08-17T09:00:00+03:00");
    assert!(db
        .delete_scoped(base.id, Some(occ), DeleteScope::ThisOnly)
        .unwrap());

    // No replacement row, unlike the "This event" edit.
    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, base.id);
    let after = db.get_item(base.id).unwrap().unwrap();
    assert!(after
        .exdates
        .iter()
        .any(|d| d.timestamp() == occ.timestamp()));

    assert_eq!(
        starts(&db, base.id, "2026-08-01", "2026-08-31"),
        [
            "2026-08-03T09:00:00+03:00",
            "2026-08-10T09:00:00+03:00",
            "2026-08-24T09:00:00+03:00",
            "2026-08-31T09:00:00+03:00",
        ]
    );
}

#[test]
fn this_only_without_a_clicked_occurrence_removes_the_series_start() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();

    assert!(db
        .delete_scoped(base.id, None, DeleteScope::ThisOnly)
        .unwrap());
    let after = db.get_item(base.id).unwrap().unwrap();
    assert!(after
        .exdates
        .iter()
        .any(|d| d.timestamp() == base.start.timestamp()));
}

#[test]
fn this_and_following_truncates_a_bounded_series_without_a_copy() {
    let (db, cal) = setup();
    let base = series(
        cal,
        "2026-10-01T09:00:00+03:00",
        "FREQ=DAILY;UNTIL=20261020T060000Z",
    );
    db.upsert_item(&base).unwrap();

    let split = ts("2026-10-10T09:00:00+03:00");
    assert!(db
        .delete_scoped(base.id, Some(split), DeleteScope::ThisAndFollowing)
        .unwrap());

    // One row, truncated — not the base plus a duplicate tail.
    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 1);
    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(
        after.rrule.as_deref(),
        Some("FREQ=DAILY;UNTIL=20261010T055900Z"),
        "the existing UNTIL is shortened to just before the split"
    );
    let list = starts(&db, base.id, "2026-10-01", "2026-10-31");
    assert_eq!(list.len(), 9, "{list:?}");
    assert!(list.iter().all(|d| ts(d).timestamp() < split.timestamp()));
}

#[test]
fn this_and_following_removes_later_detached_occurrences() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();
    let first_occ = ts("2026-08-17T09:00:00+03:00");
    let second_occ = ts("2026-08-24T09:00:00+03:00");
    let (first, _second) = two_exceptions(&db, &base, first_occ, second_occ);

    // Open the first detached occurrence and delete "this and following".
    assert!(db
        .delete_scoped(first.id, Some(first_occ), DeleteScope::ThisAndFollowing)
        .unwrap());

    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, base.id);
    let after = db.get_item(base.id).unwrap().unwrap();
    assert_eq!(
        after.rrule.as_deref(),
        Some("FREQ=WEEKLY;UNTIL=20260817T055900Z")
    );
    assert!(
        after.exdates.is_empty(),
        "the tail's EXDATEs move out with it"
    );

    assert_eq!(
        starts(&db, base.id, "2026-08-01", "2026-08-31"),
        ["2026-08-03T09:00:00+03:00", "2026-08-10T09:00:00+03:00",]
    );
    // Both detached rows are tombstoned, not physically dropped.
    let all = db.list_items(true).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().filter(|i| i.id != base.id).all(|i| i.deleted));
}

#[test]
fn this_only_on_a_detached_occurrence_keeps_the_series_and_sibling() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();
    let first_occ = ts("2026-08-17T09:00:00+03:00");
    let second_occ = ts("2026-08-24T09:00:00+03:00");
    let (first, second) = two_exceptions(&db, &base, first_occ, second_occ);

    assert!(db
        .delete_scoped(first.id, Some(first_occ), DeleteScope::ThisOnly)
        .unwrap());

    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 2);
    assert!(live.iter().any(|i| i.id == base.id));
    assert!(live.iter().any(|i| i.id == second.id));
    assert!(live.iter().all(|i| i.id != first.id));
    let after = db.get_item(base.id).unwrap().unwrap();
    assert!(after
        .exdates
        .iter()
        .any(|d| d.timestamp() == first_occ.timestamp()));
}

#[test]
fn all_from_a_detached_occurrence_deletes_the_whole_series() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();
    let first_occ = ts("2026-08-17T09:00:00+03:00");
    let second_occ = ts("2026-08-24T09:00:00+03:00");
    let (first, _second) = two_exceptions(&db, &base, first_occ, second_occ);

    assert!(db
        .delete_scoped(first.id, Some(first_occ), DeleteScope::All)
        .unwrap());

    assert!(db.list_items(false).unwrap().is_empty());
    let all = db.list_items(true).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|i| i.deleted));
}

#[test]
fn this_and_following_from_the_first_occurrence_deletes_the_series() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=WEEKLY");
    db.upsert_item(&base).unwrap();

    assert!(db
        .delete_scoped(base.id, Some(base.start), DeleteScope::ThisAndFollowing)
        .unwrap());
    assert!(db.list_items(false).unwrap().is_empty());
}

#[test]
fn split_shortens_an_existing_until_and_carries_it_to_the_tail() {
    let (db, cal) = setup();
    let base = series(
        cal,
        "2026-08-03T09:00:00+03:00",
        "FREQ=DAILY;UNTIL=20260815T060000Z",
    );
    db.upsert_item(&base).unwrap();

    let split = ts("2026-08-10T09:00:00+03:00");
    let mut edited = base.clone();
    edited.start = split;
    edited.end = Some(split + Duration::hours(1));
    edited.title = "Standup (tail)".into();
    assert!(db.apply_scoped_edit(base.id, &edited, split, true).unwrap());

    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 2, "the original + exactly one tail");
    let after_base = db.get_item(base.id).unwrap().unwrap();
    let tail = live.iter().find(|i| i.id != base.id).unwrap();
    assert_eq!(
        after_base.rrule.as_deref(),
        Some("FREQ=DAILY;UNTIL=20260810T055900Z"),
        "the base is shortened even though it already had an UNTIL"
    );
    assert_eq!(
        tail.rrule.as_deref(),
        Some("FREQ=DAILY;UNTIL=20260815T060000Z"),
        "the tail inherits the original end instead of running forever"
    );

    let base_starts = starts(&db, base.id, "2026-08-01", "2026-08-31");
    let tail_starts = starts(&db, tail.id, "2026-08-01", "2026-08-31");
    assert_eq!(base_starts.len(), 7, "{base_starts:?}");
    assert_eq!(tail_starts.len(), 6, "{tail_starts:?}");
    assert!(
        base_starts.iter().all(|d| !tail_starts.contains(d)),
        "no occurrence appears in both halves"
    );
}

#[test]
fn split_preserves_a_count_end() {
    let (db, cal) = setup();
    let base = series(cal, "2026-08-03T09:00:00+03:00", "FREQ=DAILY;COUNT=10");
    db.upsert_item(&base).unwrap();

    let split = ts("2026-08-06T09:00:00+03:00");
    let mut edited = base.clone();
    edited.start = split;
    edited.end = Some(split + Duration::hours(1));
    edited.title = "Standup (tail)".into();
    assert!(db.apply_scoped_edit(base.id, &edited, split, true).unwrap());

    let live = db.list_items(false).unwrap();
    assert_eq!(live.len(), 2);
    let after_base = db.get_item(base.id).unwrap().unwrap();
    let tail = live.iter().find(|i| i.id != base.id).unwrap();
    assert_eq!(
        after_base.rrule.as_deref(),
        Some("FREQ=DAILY;UNTIL=20260806T055900Z"),
        "COUNT is converted to the equivalent split cutoff"
    );
    assert_eq!(
        tail.rrule.as_deref(),
        Some("FREQ=DAILY;UNTIL=20260812T060000Z"),
        "the tail keeps the count's last occurrence as its end"
    );

    assert_eq!(
        starts(&db, base.id, "2026-08-01", "2026-08-31").len(),
        3,
        "Aug 3–5 stay on the base"
    );
    assert_eq!(
        starts(&db, tail.id, "2026-08-01", "2026-08-31").len(),
        7,
        "Aug 6–12 move to the tail"
    );
}
