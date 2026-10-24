//! The "All events" half of a scoped edit: re-anchoring an edited occurrence on
//! the series' own start (see `kal_storage::scoped::anchored_on_series_start`).
//!
//! The regression it pins: the editor form is anchored on the clicked
//! occurrence, so writing those times verbatim as the series' new start moved
//! the series forward and every earlier occurrence fell outside it — an "All
//! events" colour change deleted the past.

use kal_core::models::{CalendarItem, ItemKind};
use kal_storage::scoped::anchored_on_series_start;
use ulid::Ulid;

/// A weekly series row starting 2026-08-03 09:00+03:00, hour long.
fn series() -> CalendarItem {
    let start = chrono::DateTime::parse_from_rfc3339("2026-08-03T09:00:00+03:00").unwrap();
    let mut item = CalendarItem::new(ItemKind::Event, "Standup", Ulid::new(), start);
    item.end = Some(start + chrono::Duration::hours(1));
    item.rrule = Some("FREQ=WEEKLY".into());
    item
}

/// A form save as the editor sends it: the clicked occurrence's date and time,
/// carrying the edits. The offset is the device's, not the series'.
fn form(start: &str, end: Option<&str>) -> CalendarItem {
    let mut item = series();
    item.start = chrono::DateTime::parse_from_rfc3339(start).unwrap();
    item.end = end.map(|e| chrono::DateTime::parse_from_rfc3339(e).unwrap());
    item.title = "Standup (recoloured)".into();
    item
}

#[test]
fn all_scope_keeps_the_series_date_and_takes_the_form_time() {
    let base = series();
    // Clicked in October (a different offset), with the time edited to 11:30.
    let edited = form(
        "2026-10-19T11:30:00+02:00",
        Some("2026-10-19T12:15:00+02:00"),
    );

    let item = anchored_on_series_start(base.start, &edited);

    assert_eq!(
        item.start.to_rfc3339(),
        "2026-08-03T11:30:00+03:00",
        "the series keeps its own date and offset"
    );
    assert_eq!(item.end.unwrap().to_rfc3339(), "2026-08-03T12:15:00+03:00");
    // Everything else is the edit.
    assert_eq!(item.title, "Standup (recoloured)");
    assert_eq!(item.rrule.as_deref(), Some("FREQ=WEEKLY"));
    assert_eq!(item.id, edited.id);
}

#[test]
fn all_scope_leaves_an_edit_on_the_series_start_day_alone() {
    let base = series();
    let edited = form(
        "2026-08-03T09:00:00+03:00",
        Some("2026-08-03T10:00:00+03:00"),
    );

    let item = anchored_on_series_start(base.start, &edited);

    assert_eq!(item.start, edited.start);
    assert_eq!(item.end, edited.end);
}

#[test]
fn all_scope_keeps_a_missing_end_missing() {
    let base = series();
    let edited = form("2026-10-19T11:30:00+02:00", None);

    let item = anchored_on_series_start(base.start, &edited);

    assert_eq!(item.start.to_rfc3339(), "2026-08-03T11:30:00+03:00");
    assert_eq!(item.end, None);
}
