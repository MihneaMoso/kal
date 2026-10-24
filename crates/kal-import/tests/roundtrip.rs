use std::str::FromStr;

use icalendar::{Calendar as IcsCalendar, Component};
use kal_import::{export_all, export_calendar, import_ics};
use ulid::Ulid;

use kal_core::models::{
    datetime_from_parts, Calendar, CalendarItem, CalendarSource, Color, ItemKind, ItemMetadata,
    Reminder, ReminderOffset,
};

fn ts(y: i32, m: u32, d: u32, h: u32) -> kal_core::models::DateTimeTz {
    datetime_from_parts(y, m, d, h, 30, 0).unwrap()
}

fn cal(id: Ulid) -> Calendar {
    Calendar {
        id,
        name: "Work".into(),
        color: Color("#3366cc".into()),
        source: CalendarSource::Local,
        visible: true,
        deleted: false,
        updated_at: datetime_from_parts(2026, 1, 1, 0, 0, 0).unwrap(),
    }
}

fn sample_event() -> (Calendar, CalendarItem) {
    let id = Ulid::new();
    let mut item = CalendarItem::new(ItemKind::Event, "Standup", id, ts(2026, 8, 24, 9));
    item.end = Some(ts(2026, 8, 24, 10));
    item.location = Some("Room 2".into());
    item.notes = Some("daily sync".into());
    item.rrule = Some("FREQ=DAILY;COUNT=5".into());
    item.exdates = vec![ts(2026, 8, 25, 9)];
    item.reminders = vec![Reminder::minutes_before(10), Reminder::minutes_before(1440)];
    (cal(id), item)
}

#[test]
fn event_round_trip_preserves_fields() {
    let (calendar, item) = sample_event();
    let ics = export_calendar(&calendar, std::slice::from_ref(&item));

    // Sanity: valid VCALENDAR with our properties.
    assert!(IcsCalendar::from_str(&ics).is_ok());
    assert!(ics.contains("RRULE:FREQ=DAILY;COUNT=5"));
    assert!(ics.contains("-PT86400S"));

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 1);
    let got = &imported.items[0];

    assert_eq!(got.title, "Standup");
    assert_eq!(got.location.as_deref(), Some("Room 2"));
    assert_eq!(got.notes.as_deref(), Some("daily sync"));
    assert_eq!(got.rrule.as_deref(), Some("FREQ=DAILY;COUNT=5"));
    assert_eq!(got.kind, ItemKind::Event);
    assert!(!got.all_day);

    // Times come back as UTC instants.
    assert_eq!(
        got.start.with_timezone(&chrono::Utc),
        item.start.with_timezone(&chrono::Utc)
    );

    // EXDATE survives (as UTC instant).
    assert_eq!(got.exdates.len(), 1);
    assert_eq!(
        got.exdates[0].with_timezone(&chrono::Utc),
        item.exdates[0].with_timezone(&chrono::Utc)
    );

    // Both reminders round-trip with their offsets and ids.
    let got_offsets: Vec<i64> = got
        .reminders
        .iter()
        .filter_map(|r| match r.offset {
            ReminderOffset::MinutesBefore { minutes } => Some(minutes),
            _ => None,
        })
        .collect();
    assert!(got_offsets.contains(&10));
    assert!(got_offsets.contains(&1440));
    let orig_ids: Vec<Ulid> = item.reminders.iter().map(|r| r.id).collect();
    assert!(got.reminders.iter().all(|r| orig_ids.contains(&r.id)));
}

#[test]
fn task_round_trip_with_completion() {
    let id = Ulid::new();
    let mut task = CalendarItem::new(ItemKind::Task, "Buy milk", id, ts(2026, 8, 24, 12));
    task.completed = Some(ts(2026, 8, 24, 15));
    task.reminders = vec![Reminder::minutes_before(30)];

    let calendar = cal(id);
    let ics = export_calendar(&calendar, &[task]);
    assert!(ics.contains("VTODO"));
    assert!(ics.contains("STATUS:COMPLETED"));

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 1);
    let got = &imported.items[0];
    assert_eq!(got.kind, ItemKind::Task);
    assert_eq!(got.title, "Buy milk");
    assert!(got.completed.is_some());
}

#[test]
fn all_day_birthday_round_trip() {
    let id = Ulid::new();
    let mut bday = CalendarItem::new(ItemKind::Birthday, "Ada", id, ts(1815, 12, 10, 0));
    bday.all_day = true;
    bday.end = None;
    bday.metadata = ItemMetadata {
        birthday_of: Some("Ada Lovelace".into()),
        ..Default::default()
    };

    let calendar = cal(id);
    let ics = export_calendar(&calendar, &[bday]);
    assert!(ics.contains("VALUE=DATE"));
    assert!(ics.contains("CATEGORIES:BIRTHDAY"));

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 1);
    let got = &imported.items[0];
    assert_eq!(got.kind, ItemKind::Birthday);
    assert!(got.all_day);
    assert_eq!(got.metadata.birthday_of.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        got.start.date_naive(),
        chrono::NaiveDate::from_ymd_opt(1815, 12, 10).unwrap()
    );
}

#[test]
fn deleted_items_are_not_exported_and_hidden_calendars_skipped_by_export_all() {
    let id = Ulid::new();
    let mut item = CalendarItem::new(ItemKind::Event, "Gone", id, ts(2026, 8, 24, 9));
    item.deleted = true;

    let calendar = cal(id);
    let ics = export_calendar(&calendar, &[item]);
    assert!(!ics.contains("Gone"));

    let visible_id = Ulid::new();
    let hidden = Calendar {
        visible: false,
        ..cal(visible_id)
    };
    let shown = CalendarItem::new(ItemKind::Event, "Shown", visible_id, ts(2026, 8, 24, 9));
    let ics = export_all(std::slice::from_ref(&hidden), &[shown]);
    assert!(!ics.contains("Shown"));
}

#[test]
fn external_gcal_style_ics_imports() {
    // A Google-Calendar-shaped payload with TZID datetimes and no ULIDs.
    let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Google Inc//Google Calendar 70.9054//EN\r\n\
CALSCALE:GREGORIAN\r\n\
X-WR-CALNAME:External\r\n\
BEGIN:VEVENT\r\n\
DTSTART;TZID=America/New_York:20260901T140000\r\n\
DTEND;TZID=America/New_York:20260901T150000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=TU;COUNT=3\r\n\
DTSTAMP:20260820T000000Z\r\n\
UID:abc123@google.com\r\n\
CREATED:20260819T000000Z\r\n\
DESCRIPTION:Quarterly review\r\n\
LAST-MODIFIED:20260819T000000Z\r\n\
LOCATION:Big room\r\n\
SEQUENCE:0\r\n\
STATUS:CONFIRMED\r\n\
SUMMARY:Board meeting\r\n\
TRANSP:OPAQUE\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    let imported = import_ics(ics, "Imported").unwrap();
    assert_eq!(imported.calendar.name, "External");
    assert_eq!(imported.calendar.source, CalendarSource::IcsImport);
    assert_eq!(imported.items.len(), 1);

    let it = &imported.items[0];
    assert_eq!(it.title, "Board meeting");
    assert_eq!(it.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=TU;COUNT=3"));
    // UID is not a ULID → a fresh one was minted.
    assert_ne!(it.id.to_string(), "abc123@google.com");
    // Recurrence expands against the imported rule.
    let occs = kal_core::viewmodel::expand_occurrences(
        it,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
    );
    assert_eq!(occs.len(), 3); // three Tuesdays
}

/// A linked occurrence ("This event") and the series it belongs to.
fn linked_pair() -> (Calendar, CalendarItem, CalendarItem) {
    let series_id = Ulid::new();
    let mut master = CalendarItem::new(ItemKind::Event, "Standup", series_id, ts(2026, 8, 24, 9));
    master.id = series_id;
    master.end = Some(ts(2026, 8, 24, 10));
    master.rrule = Some("FREQ=DAILY;COUNT=5".into());
    master.exdates = vec![ts(2026, 8, 25, 9)];

    let mut exception = CalendarItem::new(
        ItemKind::Event,
        "Standup (moved)",
        series_id,
        ts(2026, 8, 25, 11),
    );
    exception.end = Some(ts(2026, 8, 25, 12));
    exception.metadata.series_id = Some(series_id);
    exception.metadata.recurrence_id = Some(ts(2026, 8, 25, 9));
    (cal(series_id), master, exception)
}

#[test]
fn linked_occurrence_exports_as_recurrence_id_override() {
    let (calendar, master, exception) = linked_pair();
    let ics = export_all(&[calendar], &[master, exception]);
    let parsed = IcsCalendar::from_str(&ics).unwrap();
    let events: Vec<_> = parsed.events().collect();
    assert_eq!(events.len(), 2);

    let master_event = events
        .iter()
        .find(|e| e.get_recurrence_id().is_none())
        .unwrap();
    let override_event = events
        .iter()
        .find(|e| e.get_recurrence_id().is_some())
        .unwrap();

    // One series, one override: same UID, the override names the replaced slot
    // and carries no rule of its own.
    let uid = master_event.property_value("UID").unwrap().to_string();
    assert_eq!(override_event.property_value("UID"), Some(uid.as_str()));
    assert_eq!(override_event.property_value("RRULE"), None);
    let rid = match override_event.get_recurrence_id().unwrap() {
        icalendar::DatePerhapsTime::DateTime(dt) => dt.try_into_utc().unwrap(),
        other => panic!("expected a DATE-TIME recurrence id, got {other:?}"),
    };
    assert_eq!(rid, ts(2026, 8, 25, 9).with_timezone(&chrono::Utc));
    // The series keeps its EXDATE, so an override-unaware reader still shows a
    // single event at that slot.
    assert!(ics.contains("EXDATE"));
}

#[test]
fn recurrence_id_override_links_back_on_import() {
    let (calendar, master, exception) = linked_pair();
    let ics = export_all(&[calendar], &[master, exception]);

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 2);
    let series = imported
        .items
        .iter()
        .find(|item| item.metadata.series_id.is_none())
        .unwrap();
    let occurrence = imported
        .items
        .iter()
        .find(|item| item.metadata.series_id.is_some())
        .unwrap();

    assert_eq!(occurrence.metadata.series_id, Some(series.id));
    assert_eq!(occurrence.metadata.recurrence_id, Some(ts(2026, 8, 25, 9)));
    assert_eq!(occurrence.title, "Standup (moved)");
    assert_eq!(occurrence.start, ts(2026, 8, 25, 11));
    assert!(occurrence.rrule.is_none());
    assert_ne!(occurrence.id, series.id);
    assert!(series.exdates.contains(&ts(2026, 8, 25, 9)));
}

#[test]
fn override_before_master_still_links_and_adds_the_exdate() {
    const SERIES: &str = "01J8ZQ0000000000000000000A";
    let ics = format!(
        "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:{SERIES}\r\n\
RECURRENCE-ID:20260902T090000Z\r\n\
DTSTART:20260902T110000Z\r\n\
DTEND:20260902T120000Z\r\n\
SUMMARY:Moved\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
UID:{SERIES}\r\n\
DTSTART:20260901T090000Z\r\n\
DTEND:20260901T100000Z\r\n\
RRULE:FREQ=DAILY;COUNT=3\r\n\
SUMMARY:Series\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n"
    );

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 2);
    let series_id = Ulid::from_str(SERIES).unwrap();
    let series = imported.items.iter().find(|i| i.id == series_id).unwrap();
    let occurrence = imported
        .items
        .iter()
        .find(|i| i.metadata.series_id.is_some())
        .unwrap();

    assert_eq!(occurrence.metadata.series_id, Some(series_id));
    assert_eq!(
        occurrence.metadata.recurrence_id,
        Some(datetime_from_parts(2026, 9, 2, 9, 0, 0).unwrap())
    );
    // Kal hides the replaced slot on the series row.
    assert_eq!(
        series.exdates,
        vec![datetime_from_parts(2026, 9, 2, 9, 0, 0).unwrap()]
    );
}

#[test]
fn orphan_recurrence_id_imports_standalone() {
    const ORPHAN: &str = "01J8ZQ0000000000000000000B";
    let ics = format!(
        "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:{ORPHAN}\r\n\
RECURRENCE-ID:20260902T090000Z\r\n\
DTSTART:20260902T110000Z\r\n\
SUMMARY:Moved\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n"
    );

    let imported = import_ics(&ics, "Imported").unwrap();
    assert_eq!(imported.items.len(), 1);
    let item = &imported.items[0];
    assert_eq!(item.title, "Moved");
    // No master in the file → nothing to link; it must not reuse the series id.
    assert!(item.metadata.series_id.is_none());
    assert!(item.metadata.recurrence_id.is_none());
    assert_ne!(item.id.to_string(), ORPHAN);
}

#[test]
fn all_day_override_uses_a_date_recurrence_id() {
    let series_id = Ulid::new();
    let mut master = CalendarItem::new(ItemKind::Event, "Trip", series_id, ts(2026, 9, 1, 0));
    master.id = series_id;
    master.all_day = true;
    master.rrule = Some("FREQ=DAILY;COUNT=3".into());

    let mut exception = CalendarItem::new(
        ItemKind::Event,
        "Trip (free day)",
        series_id,
        ts(2026, 9, 2, 0),
    );
    exception.all_day = true;
    exception.metadata.series_id = Some(series_id);
    exception.metadata.recurrence_id = Some(ts(2026, 9, 2, 0));

    let ics = export_all(&[cal(series_id)], &[master, exception]);
    let parsed = IcsCalendar::from_str(&ics).unwrap();
    let override_event = parsed
        .events()
        .find(|e| e.get_recurrence_id().is_some())
        .unwrap();
    let rid = override_event.get_recurrence_id().unwrap();
    assert!(matches!(
        rid,
        icalendar::DatePerhapsTime::Date(d)
            if d == chrono::NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
    ));

    let imported = import_ics(&ics, "Imported").unwrap();
    let occurrence = imported
        .items
        .iter()
        .find(|i| i.metadata.series_id.is_some())
        .unwrap();
    let series = imported
        .items
        .iter()
        .find(|i| i.metadata.series_id.is_none())
        .unwrap();
    assert_eq!(
        occurrence.metadata.recurrence_id.unwrap().date_naive(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
    );
    // The all-day EXDATE is stored as the local midnight of that date.
    assert!(series
        .exdates
        .iter()
        .any(|e| e.with_timezone(&chrono::Local).date_naive()
            == chrono::NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()));
}
