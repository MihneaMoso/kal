//! .ics (RFC 5545) import/export for Chrono (spec §5.4).
//!
//! Export produces standards-compliant VCALENDARs that any other calendar
//! application can read — data portability is a hard requirement.
//! Google Calendar REST import lives in the `google` module (phase 5c).

use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use icalendar::{
    Calendar as IcsCalendar, Component, DatePerhapsTime, Event as IcsEvent, EventLike,
    Todo as IcsTodo,
};

use chrono_core::models::{
    Calendar, CalendarItem, CalendarSource, Color, DateTimeTz, ItemKind, Reminder,
    ReminderOffset,
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

/// Serialize one Chrono calendar plus its items as an RFC 5545 VCALENDAR.
pub fn export_calendar(calendar: &Calendar, items: &[CalendarItem]) -> String {
    let mut out = IcsCalendar::new();
    out.name(&calendar.name);

    for item in items.iter().filter(|i| i.calendar_id == calendar.id && !i.deleted) {
        push_item(&mut out, item);
    }
    out.to_string()
}

/// Export every visible calendar into a single combined VCALENDAR.
pub fn export_all(calendars: &[Calendar], items: &[CalendarItem]) -> String {
    let mut out = IcsCalendar::new();
    out.name("Chrono");
    let visible: Vec<_> = calendars.iter().filter(|c| c.visible).map(|c| c.id).collect();
    for item in items.iter().filter(|i| !i.deleted && visible.contains(&i.calendar_id)) {
        push_item(&mut out, item);
    }
    out.to_string()
}

fn push_item(out: &mut IcsCalendar, item: &CalendarItem) {
    match item.kind {
        ItemKind::Task => {
            let mut todo = IcsTodo::new();
            todo.summary(&item.title);
            apply_common(&mut todo, item);
            todo.add_property(
                "STATUS",
                if item.completed.is_some() { "COMPLETED" } else { "NEEDS-ACTION" },
            );
            if let Some(done) = item.completed {
                todo.append_property(
                    icalendar::Property::new("COMPLETED", fmt_utc(&done)),
                );
            }
            out.push(todo);
        }
        ItemKind::Event | ItemKind::Birthday => {
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
                        icalendar::Property::new("DTEND", fmt_date(&(end.date_naive() + chrono::Duration::days(1))))
                            .add_parameter("VALUE", "DATE"),
                    );
                }
            } else {
                event.starts(item.start.with_timezone(&Utc));
                if let Some(end) = &item.end {
                    event.ends(end.with_timezone(&Utc));
                }
            }
            apply_common(&mut event, item);
            for reminder in &item.reminders {
                if let ReminderOffset::MinutesBefore { minutes } = reminder.offset {
                    let mut alarm = icalendar::Alarm::display(
                &item.title,
                -chrono::Duration::minutes(minutes),
            )
            .done();
                    // Keep our reminder identity so imports round-trip 1:1.
                    alarm.append_property(icalendar::Property::new(
                        "X-CHRONO-REMINDER-ID",
                        reminder.id.to_string(),
                    ));
                    event.append_component(alarm);
                }
            }
            out.push(event);
        }
    }
}

fn apply_common<C: Component>(comp: &mut C, item: &CalendarItem) {
    comp.append_property(icalendar::Property::new("UID", item.id.to_string()));
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
            comp.add_property("X-CHRONO-BIRTHDAY-OF", person.clone());
        }
    }
    comp.add_property(
        "DTSTAMP",
        item.updated_at.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string(),
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
    let parsed =
        IcsCalendar::from_str(ics).map_err(|e| ImportError::Parse(e.to_string()))?;

    let calendar = Calendar {
        id: ulid::Ulid::new(),
        name: parsed.get_name().unwrap_or(calendar_name).to_string(),
        color: Color("#7a6ff0".into()),
        source: CalendarSource::IcsImport,
        visible: true,
    };

    let mut items = Vec::new();

    for event in parsed.events() {
        let mut item = base_item(&calendar, event);
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
                if let Some(utc) = dt.try_into_utc() {
                    item.start = utc.fixed_offset();
                } else {
                    continue;
                }
            }
            None => continue,
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

fn base_item<'a, C: Component>(calendar: &Calendar, comp: &'a C) -> CalendarItem {
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
            .property_value("X-CHRONO-BIRTHDAY-OF")
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
                .property_value("X-CHRONO-REMINDER-ID")
                .and_then(|s| ulid::Ulid::from_string(s).ok())
                .unwrap_or_else(ulid::Ulid::new);
            item.reminders.push(Reminder {
                id,
                offset: ReminderOffset::MinutesBefore { minutes },
                method: chrono_core::models::NotifyMethod::Push,
            });
        }
    }
}
