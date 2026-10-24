//! .ics (RFC 5545) import/export for Kal (spec §5.4).
//!
//! Export produces standards-compliant VCALENDARs that any other calendar
//! application can read — data portability is a hard requirement.
//! Google Calendar REST import lives in the `google` module (phase 5c).

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use icalendar::{
    Calendar as IcsCalendar, Component, DatePerhapsTime, Event as IcsEvent, EventLike,
    Todo as IcsTodo,
};

use kal_core::models::{
    Calendar, CalendarItem, CalendarSource, Color, DateTimeTz, ItemKind, Reminder, ReminderOffset,
};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("failed to parse ics: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, ImportError>;

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn fmt_utc(dt: &DateTimeTz) -> String {
    dt.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string()
}

fn fmt_date(d: &NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

fn parse_utc(s: &str) -> Option<DateTimeTz> {
    let s = s.trim();
    NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .ok()
        .map(|n| n.and_utc().fixed_offset())
}

/// Trigger strings we emit/accept: `-PT{N}M`, `-PT{N}H`, `-PT{N}D`.
fn trigger_to_minutes(trigger: &str) -> Option<i64> {
    let t = trigger.trim().to_uppercase();
    let t = t.strip_prefix('-')?;
    let t = t.strip_prefix("P")?.strip_prefix('T')?;
    let (num, unit) = t.split_at(t.len() - 1);
    let n: i64 = num.parse().ok()?;
    match unit {
        // icalendar-rs emits relative triggers in seconds (-PT600S).
        "S" => Some((n + 59) / 60),
        "M" => Some(n),
        "H" => Some(n * 60),
        "D" => Some(n * 60 * 24),
        _ => None,
    }
}

#[allow(dead_code)] // used by tests / future VALARM re-emit path
fn minutes_to_trigger(minutes: i64) -> String {
    format!("-PT{minutes}M")
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Serialize one Kal calendar plus its items as an RFC 5545 VCALENDAR.
pub fn export_calendar(calendar: &Calendar, items: &[CalendarItem]) -> String {
    let mut out = IcsCalendar::new();
    out.name(&calendar.name);

    let shown: Vec<&CalendarItem> = items
        .iter()
        .filter(|i| i.calendar_id == calendar.id && !i.deleted)
        .collect();
    // A linked occurrence can only be an override if its series is in the same
    // export; otherwise its RECURRENCE-ID would dangle for every other client.
    let exported: HashSet<ulid::Ulid> = shown.iter().map(|i| i.id).collect();
    for item in shown {
        push_item(&mut out, item, &exported);
    }
    out.to_string()
}

/// Export every visible calendar into a single combined VCALENDAR.
pub fn export_all(calendars: &[Calendar], items: &[CalendarItem]) -> String {
    let mut out = IcsCalendar::new();
    out.name("Kal");
    let visible: Vec<_> = calendars
        .iter()
        .filter(|c| c.visible && !c.deleted)
        .map(|c| c.id)
        .collect();
    let shown: Vec<&CalendarItem> = items
        .iter()
        .filter(|i| !i.deleted && visible.contains(&i.calendar_id))
        .collect();
    let exported: HashSet<ulid::Ulid> = shown.iter().map(|i| i.id).collect();
    for item in shown {
        push_item(&mut out, item, &exported);
    }
    out.to_string()
}

fn push_item(out: &mut IcsCalendar, item: &CalendarItem, exported: &HashSet<ulid::Ulid>) {
    match item.kind {
        ItemKind::Task => {
            let mut todo = IcsTodo::new();
            todo.summary(&item.title);
            apply_common(&mut todo, item, &item.id.to_string());
            todo.add_property(
                "STATUS",
                if item.completed.is_some() {
                    "COMPLETED"
                } else {
                    "NEEDS-ACTION"
                },
            );
            if let Some(done) = item.completed {
                todo.append_property(icalendar::Property::new("COMPLETED", fmt_utc(&done)));
            }
            out.push(todo);
        }
        ItemKind::Event | ItemKind::Birthday => {
            // A linked occurrence is an *override* of its series under RFC 5545:
            // same UID as the series plus a RECURRENCE-ID naming the series
            // instant it replaced. That keeps the exception linked when the
            // file is imported anywhere (and when Kal re-imports its own
            // export); a standalone event at the new time would silently lose
            // the link. The series' EXDATE stays as well, so a client that
            // ignores overrides still shows one event, not two.
            let override_of = item
                .metadata
                .series_id
                .filter(|series| exported.contains(series))
                .zip(item.metadata.recurrence_id.as_ref());
            let uid = match override_of {
                Some((series, _)) => series.to_string(),
                None => item.id.to_string(),
            };

            let mut event = IcsEvent::new();
            event.summary(&item.title);
            if item.all_day {
                event.append_property(
                    icalendar::Property::new("DTSTART", fmt_date(&item.start.date_naive()))
                        .add_parameter("VALUE", "DATE"),
                );
                if let Some(end) = &item.end {
                    // DTEND is exclusive in RFC 5545 for DATE values.
                    event.append_property(
                        icalendar::Property::new(
                            "DTEND",
                            fmt_date(&(end.date_naive() + chrono::Duration::days(1))),
                        )
                        .add_parameter("VALUE", "DATE"),
                    );
                }
            } else {
                event.starts(item.start.with_timezone(&Utc));
                if let Some(end) = &item.end {
                    event.ends(end.with_timezone(&Utc));
                }
            }
            if let Some((_, recurrence_id)) = override_of {
                // RECURRENCE-ID has the same value type as the series' DTSTART:
                // DATE for an all-day series, UTC otherwise.
                if item.all_day {
                    event.append_property(
                        icalendar::Property::new(
                            "RECURRENCE-ID",
                            fmt_date(&recurrence_id.date_naive()),
                        )
                        .add_parameter("VALUE", "DATE"),
                    );
                } else {
                    event.append_property(icalendar::Property::new(
                        "RECURRENCE-ID",
                        fmt_utc(recurrence_id),
                    ));
                }
            }
            apply_common(&mut event, item, &uid);
            for reminder in &item.reminders {
                if let ReminderOffset::MinutesBefore { minutes } = reminder.offset {
                    let mut alarm =
                        icalendar::Alarm::display(&item.title, -chrono::Duration::minutes(minutes))
                            .done();
                    // Keep our reminder identity so imports round-trip 1:1.
                    alarm.append_property(icalendar::Property::new(
                        "X-KAL-REMINDER-ID",
                        reminder.id.to_string(),
                    ));
                    event.append_component(alarm);
                }
            }
            out.push(event);
        }
    }
}

fn apply_common<C: Component>(comp: &mut C, item: &CalendarItem, uid: &str) {
    comp.append_property(icalendar::Property::new("UID", uid));
    if let Some(notes) = &item.notes {
        comp.add_property("DESCRIPTION", notes.clone());
    }
    if let Some(loc) = &item.location {
        comp.add_property("LOCATION", loc.clone());
    }
    if let Some(rule) = &item.rrule {
        comp.add_property("RRULE", rule.clone());
    }
    if !item.exdates.is_empty() {
        // EXDATE is a multi-value property per RFC 5545.
        let list: Vec<String> = item.exdates.iter().map(fmt_utc).collect();
        comp.append_multi_property(icalendar::Property::new("EXDATE", list.join(",")));
    }
    if item.kind == ItemKind::Birthday {
        comp.append_multi_property(icalendar::Property::new("CATEGORIES", "BIRTHDAY"));
        if let Some(person) = &item.metadata.birthday_of {
            comp.add_property("X-KAL-BIRTHDAY-OF", person.clone());
        }
    }
    comp.add_property(
        "DTSTAMP",
        item.updated_at
            .with_timezone(&Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string(),
    );
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Everything parsed out of an .ics payload, ready to be persisted under one
/// newly-created `IcsImport` calendar.
pub struct ImportedCalendar {
    pub calendar: Calendar,
    pub items: Vec<CalendarItem>,
}

pub fn import_ics(ics: &str, calendar_name: &str) -> Result<ImportedCalendar> {
    let parsed = IcsCalendar::from_str(ics).map_err(|e| ImportError::Parse(e.to_string()))?;

    let calendar = Calendar {
        id: ulid::Ulid::new(),
        name: parsed.get_name().unwrap_or(calendar_name).to_string(),
        color: Color("#7a6ff0".into()),
        source: CalendarSource::IcsImport,
        visible: true,
        deleted: false,
        updated_at: Utc::now().fixed_offset(),
    };

    let mut items = Vec::new();

    // Two passes: masters first, then overrides. A `RECURRENCE-ID` component
    // names its series only by UID, so the series has to be imported before
    // the exception can be linked to it.
    let events: Vec<&IcsEvent> = parsed.events().collect();
    let mut series_ids: HashMap<String, ulid::Ulid> = HashMap::new();
    for event in events
        .iter()
        .copied()
        .filter(|event| event.get_recurrence_id().is_none())
    {
        let Some(item) = event_item(&calendar, event) else {
            continue;
        };
        if let Some(uid) = event.property_value("UID") {
            series_ids.insert(uid.to_string(), item.id);
        }
        items.push(item);
    }

    for event in events
        .iter()
        .copied()
        .filter(|event| event.get_recurrence_id().is_some())
    {
        let Some(mut item) = event_item(&calendar, event) else {
            continue;
        };
        // The UID belongs to the series, so the override gets its own id (a
        // shared id would make the two items overwrite each other on upsert).
        item.id = ulid::Ulid::new();
        // An override never carries the series' rule.
        item.rrule = None;
        let series = event
            .property_value("UID")
            .and_then(|uid| series_ids.get(uid))
            .copied();
        let recurrence_id = event.get_recurrence_id().and_then(recurrence_instant);
        if let (Some(series_id), Some(recurrence_id)) = (series, recurrence_id) {
            // Same shape as "This event" in the editor: the occurrence keeps
            // the series' id + the instant it replaced, and the series hides
            // that instant with an EXDATE so it is not generated twice.
            item.metadata.series_id = Some(series_id);
            item.metadata.recurrence_id = Some(recurrence_id);
            if let Some(master) = items.iter_mut().find(|master| master.id == series_id) {
                // All-day EXDATEs are instants at local midnight, but they are
                // written as UTC, so compare whole dates for those (a plain
                // instant compare would add a second EXDATE on every
                // round-trip). Timed items compare exactly.
                let already = if item.all_day {
                    master.exdates.iter().any(|exdate| {
                        exdate.with_timezone(&chrono::Local).date_naive()
                            == recurrence_id.date_naive()
                    })
                } else {
                    master.exdates.contains(&recurrence_id)
                };
                if !already {
                    master.exdates.push(recurrence_id);
                }
            }
        }
        items.push(item);
    }

    for todo in parsed.todos() {
        let mut item = base_item(&calendar, todo);
        item.kind = ItemKind::Task;
        item.all_day = false;
        item.completed = todo.property_value("COMPLETED").and_then(parse_utc);
        if let Some(DatePerhapsTime::DateTime(dt)) = todo.get_start() {
            if let Some(utc) = dt.try_into_utc() {
                item.start = utc.fixed_offset();
            }
        }
        read_exdates_and_reminders(&mut item, todo);
        items.push(item);
    }

    Ok(ImportedCalendar { calendar, items })
}

/// One VEVENT → the Kal item it describes (kind, time, EXDATEs, alarms).
/// `None` when the component has no usable start.
fn event_item(calendar: &Calendar, event: &IcsEvent) -> Option<CalendarItem> {
    let mut item = base_item(calendar, event);
    if item.kind != ItemKind::Birthday {
        item.kind = ItemKind::Event;
    }
    let tz = chrono::Local::now().fixed_offset();

    match event.get_start() {
        Some(DatePerhapsTime::Date(d)) => {
            item.all_day = true;
            item.start = midnight(d, tz);
        }
        Some(DatePerhapsTime::DateTime(dt)) => {
            item.start = dt.try_into_utc()?.fixed_offset();
        }
        None => return None,
    }

    item.end = match event.get_end() {
        Some(DatePerhapsTime::Date(d)) => {
            // Exclusive DATE end → last covered instant.
            Some(midnight(d - chrono::Duration::days(1), tz))
        }
        Some(DatePerhapsTime::DateTime(dt)) => dt.try_into_utc().map(|u| u.fixed_offset()),
        None => None,
    };

    read_exdates_and_reminders(&mut item, event);
    Some(item)
}

/// `RECURRENCE-ID` names the series instant an override replaced. DATE keeps
/// the all-day convention (midnight local, as the master's DTSTART was read),
/// DATE-TIME is an absolute instant.
fn recurrence_instant(value: DatePerhapsTime) -> Option<DateTimeTz> {
    match value {
        DatePerhapsTime::Date(d) => Some(midnight(d, chrono::Local::now().fixed_offset())),
        DatePerhapsTime::DateTime(dt) => dt.try_into_utc().map(|utc| utc.fixed_offset()),
    }
}

fn midnight(d: NaiveDate, tz: DateTimeTz) -> DateTimeTz {
    let naive = d.and_time(NaiveTime::MIN);
    tz.timezone()
        .from_local_datetime(&naive)
        .single()
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive).fixed_offset())
        .fixed_offset()
}

/// Read a possibly-multi-valued property's values.
fn multi_values<'a, C: Component>(comp: &'a C, key: &str) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = comp
        .multi_properties()
        .get(key)
        .map(|props| props.iter().map(|p| p.value()).collect())
        .unwrap_or_default();
    if out.is_empty() {
        if let Some(v) = comp.property_value(key) {
            out.push(v);
        }
    }
    out
}

fn base_item<C: Component>(calendar: &Calendar, comp: &C) -> CalendarItem {
    let tz = chrono::Local::now().fixed_offset();
    let fallback_start = chrono::Utc::now().with_timezone(tz.offset());
    let mut item = CalendarItem::new(ItemKind::Event, "", calendar.id, fallback_start);
    item.id = comp
        .property_value("UID")
        .and_then(|uid| ulid::Ulid::from_string(uid).ok())
        .unwrap_or(item.id);
    item.title = comp.property_value("SUMMARY").unwrap_or("").to_string();
    item.notes = comp.property_value("DESCRIPTION").map(str::to_string);
    item.location = comp.property_value("LOCATION").map(str::to_string);
    item.rrule = comp
        .property_value("RRULE")
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    item.created_at = tz;
    item.updated_at = tz;
    if multi_values(comp, "CATEGORIES").contains(&"BIRTHDAY") {
        item.kind = ItemKind::Birthday;
        item.all_day = true;
        item.start = midnight(fallback_start.date_naive(), tz);
        item.end = None;
        item.metadata.birthday_of = comp
            .property_value("X-KAL-BIRTHDAY-OF")
            .map(str::to_string)
            .or_else(|| Some(item.title.clone()));
    }
    item
}

fn read_exdates_and_reminders<C: Component>(item: &mut CalendarItem, comp: &C) {
    for value in multi_values(comp, "EXDATE") {
        for part in value.split(',') {
            if let Some(dt) = parse_utc(part.trim()) {
                item.exdates.push(dt);
            }
        }
    }
    for child in comp.components() {
        if child.component_kind() != "VALARM" {
            continue;
        }
        let Some(trigger) = child.property_value("TRIGGER") else {
            continue;
        };
        if let Some(minutes) = trigger_to_minutes(trigger) {
            let id = child
                .property_value("X-KAL-REMINDER-ID")
                .and_then(|s| ulid::Ulid::from_string(s).ok())
                .unwrap_or_else(ulid::Ulid::new);
            item.reminders.push(Reminder {
                id,
                offset: ReminderOffset::MinutesBefore { minutes },
                method: kal_core::models::NotifyMethod::Push,
            });
        }
    }
}

pub mod google;
