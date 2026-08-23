//! Core data model for Kal (spec §4).
//!
//! Every persisted entity is CRDT-friendly: plain serde structs keyed by ULID,
//! with `updated_at` + `deleted` (tombstone) so a later Automerge/Yrs layer can
//! wrap fields as registers without changing these shapes.

use chrono::{DateTime, Datelike, FixedOffset, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

pub type Ulid = ulid::Ulid;

/// Timezone-aware timestamp. `FixedOffset` keeps the original UTC offset on
/// serialization so events survive round-trips without tz database lookups.
pub type DateTimeTz = DateTime<FixedOffset>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Event,
    Task,
    Birthday,
}

/// Full-palette color stored as `#RRGGBB` hex — not limited to presets (§5.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub String);

impl Color {
    pub fn new(hex: impl Into<String>) -> crate::Result<Self> {
        let hex = hex.into();
        let valid = hex.len() == 7
            && hex.starts_with('#')
            && hex[1..].chars().all(|c| c.is_ascii_hexdigit());
        if valid {
            Ok(Color(hex))
        } else {
            Err(crate::Error::InvalidItem(format!(
                "color must be #RRGGBB, got {hex:?}"
            )))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyMethod {
    /// Locally-scheduled system notification (the only method today; §5.3).
    Push,
    Banner,
    /// Reserved for future use.
    Email,
}

/// Reminder trigger: relative offset from item start, or an absolute instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReminderOffset {
    /// Minutes before the item start (may exceed 24h, e.g. 10080 = 1 week).
    MinutesBefore { minutes: i64 },
    Absolute(DateTimeTz),
}

impl ReminderOffset {
    pub fn resolve(&self, start: DateTimeTz) -> DateTimeTz {
        match self {
            ReminderOffset::MinutesBefore { minutes } => start - chrono::Duration::minutes(*minutes),
            ReminderOffset::Absolute(t) => *t,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: Ulid,
    pub offset: ReminderOffset,
    pub method: NotifyMethod,
}

impl Reminder {
    pub fn minutes_before(minutes: i64) -> Self {
        Self {
            id: Ulid::new(),
            offset: ReminderOffset::MinutesBefore { minutes },
            method: NotifyMethod::Push,
        }
    }
}

/// Provenance of a calendar (§4 / §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSource {
    Local,
    GoogleImport,
    IcsImport,
    Birthdays,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: Ulid,
    pub name: String,
    pub color: Color,
    pub source: CalendarSource,
    pub visible: bool,
}

impl Calendar {
    pub fn local(name: impl Into<String>, color: Color) -> Self {
        Self {
            id: Ulid::new(),
            name: name.into(),
            color,
            source: CalendarSource::Local,
            visible: true,
        }
    }
}

/// Extra metadata attached to items. Currently only birthday linkage (§4);
/// kept in one struct to avoid schema churn when more metadata arrives.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMetadata {
    /// Set when `kind == Birthday`: free-form contact identifier from vCard import.
    pub birthday_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarItem {
    pub id: Ulid,
    pub kind: ItemKind,
    pub title: String,
    pub notes: Option<String>,
    pub location: Option<String>,
    pub calendar_id: Ulid,
    pub start: DateTimeTz,
    pub end: Option<DateTimeTz>,
    pub all_day: bool,
    /// RFC 5545 RRULE string, e.g. "FREQ=WEEKLY;BYDAY=MO,WE".
    pub rrule: Option<String>,
    /// EXDATE-style exception instants (ISO-8601 with offset) skipped by the series.
    pub exdates: Vec<DateTimeTz>,
    /// Tasks only: completion timestamp.
    pub completed: Option<DateTimeTz>,
    pub reminders: Vec<Reminder>,
    pub color_override: Option<Color>,
    pub created_at: DateTimeTz,
    pub updated_at: DateTimeTz,
    pub deleted: bool,
    pub metadata: ItemMetadata,
}

impl CalendarItem {
    /// Create a new event-like item with sane defaults.
    pub fn new(kind: ItemKind, title: impl Into<String>, calendar_id: Ulid, start: DateTimeTz) -> Self {
        let now = Utc::now().fixed_offset();
        Self {
            id: Ulid::new(),
            kind,
            title: title.into(),
            notes: None,
            location: None,
            calendar_id,
            start,
            end: None,
            all_day: false,
            rrule: None,
            exdates: Vec::new(),
            completed: None,
            reminders: Vec::new(),
            color_override: None,
            created_at: now,
            updated_at: now,
            deleted: false,
            metadata: ItemMetadata::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.deleted
    }

    /// Effective display color: override wins over calendar color.
    pub fn effective_color(&self, calendar: Option<&Calendar>) -> Option<Color> {
        self.color_override.clone().or_else(|| calendar.map(|c| c.color.clone()))
    }

    /// Validate structural invariants before persisting/syncing.
    pub fn validate(&self) -> crate::Result<()> {
        if self.title.trim().is_empty() {
            return Err(crate::Error::InvalidItem("title must not be empty".into()));
        }
        if let Some(end) = self.end {
            if end < self.start {
                return Err(crate::Error::InvalidItem(
                    "end must not be earlier than start".into(),
                ));
            }
        }
        if self.kind == ItemKind::Birthday && self.metadata.birthday_of.is_none() {
            return Err(crate::Error::InvalidItem(
                "birthday items require metadata.birthday_of".into(),
            ));
        }
        Ok(())
    }

    /// Age in whole years at `on` for birthday items (§5.1 age badges).
    pub fn birthday_age_at(&self, on: DateTimeTz) -> Option<u32> {
        if self.kind != ItemKind::Birthday {
            return None;
        }
        let birth = self.start;
        let mut age = on.year() - birth.year();
        if (on.month(), on.day()) < (birth.month(), birth.day()) {
            age -= 1;
        }
        Some(age.max(0) as u32)
    }
}

/// An occurrence of an item on the calendar grid: either the base instance or
/// one expanded from an RRULE (phase 3 will replace this naive expansion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub item_id: Ulid,
    pub start: DateTimeTz,
    pub end: Option<DateTimeTz>,
}

impl Occurrence {
    pub fn single(item: &CalendarItem) -> Occurrence {
        Occurrence {
            item_id: item.id,
            start: item.start,
            end: item.end,
        }
    }
}

/// Parse an ISO-8601 datetime with offset (`2026-08-23T14:30:00+02:00`).
pub fn parse_datetime_tz(s: &str) -> Option<DateTimeTz> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// Build a timezone-aware datetime at a fixed offset from civil components.
pub fn datetime_from_parts(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    min: u32,
    offset_hours: i32,
) -> Option<DateTimeTz> {
    let offset = FixedOffset::east_opt(offset_hours * 3600)?;
    offset.with_ymd_and_hms(year, month, day, hour, min, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(y: i32, m: u32, d: u32, h: u32) -> DateTimeTz {
        datetime_from_parts(y, m, d, h, 0, 0).unwrap()
    }

    fn sample(kind: ItemKind, start: DateTimeTz) -> CalendarItem {
        let cal = Calendar::local("Test", Color("#3366cc".into()));
        CalendarItem::new(kind, "Sample", cal.id, start)
    }

    #[test]
    fn color_validation() {
        assert!(Color::new("#aabb01").is_ok());
        assert!(Color::new("red").is_err());
        assert!(Color::new("#12345").is_err());
    }

    #[test]
    fn validate_rejects_bad_items() {
        let mut it = sample(ItemKind::Event, ts(2026, 8, 23, 9));
        assert!(it.validate().is_ok());

        it.end = Some(ts(2026, 8, 22, 9));
        assert!(it.validate().is_err());

        let bday = sample(ItemKind::Birthday, ts(1990, 5, 1, 0));
        assert!(bday.validate().is_err());
    }

    #[test]
    fn birthday_age() {
        let cal = Calendar::local("Birthdays", Color("#e91e63".into()));
        let mut it = CalendarItem::new(ItemKind::Birthday, "Ada", cal.id, ts(1990, 12, 10, 0));
        it.metadata.birthday_of = Some("contact-1".into());
        assert_eq!(it.birthday_age_at(ts(2026, 12, 10, 12)), Some(36));
        // Not yet reached in 2026.
        assert_eq!(it.birthday_age_at(ts(2026, 6, 1, 12)), Some(35));
        // Non-birthdays have no age.
        assert_eq!(sample(ItemKind::Event, it.start).birthday_age_at(it.start), None);
    }

    #[test]
    fn reminder_offset_resolution() {
        let start = ts(2026, 8, 23, 9);
        let off = ReminderOffset::MinutesBefore { minutes: 1440 };
        assert_eq!(off.resolve(start), ts(2026, 8, 22, 9));
    }

    #[test]
    fn serde_round_trip() {
        let cal = Calendar::local("Work", Color("#00ff00".into()));
        let json = serde_json::to_string(&cal).unwrap();
        assert_eq!(serde_json::from_str::<Calendar>(&json).unwrap(), cal);
    }

    #[test]
    fn parse_rfc3339_with_offset_preserved() {
        let dt = parse_datetime_tz("2026-08-23T14:30:00+02:00").unwrap();
        assert_eq!(dt.offset().local_minus_utc(), 2 * 3600);
    }

    #[test]
    fn local_result_single_helper() {
        let off = FixedOffset::east_opt(0).unwrap();
        let r = off.with_ymd_and_hms(2026, 2, 30, 0, 0, 0);
        assert!(r.single().is_none());
    }
}
