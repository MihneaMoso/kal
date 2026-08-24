//! Google Calendar import (spec §5.4): read-only OAuth2 + REST v3.
//!
//! The HTTP transport is abstracted (`Transport`) so all parsing/mapping is
//! unit-testable without network. Tokens are held in memory / local config
//! only — Kal runs no backend, nothing else ever receives them.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone as _, Utc};
use serde::Deserialize;

use kal_core::models::{Calendar, CalendarItem, CalendarSource, Color, DateTimeTz, ItemKind};

use crate::ImportError;

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
/// Read-only scope: we never write back to Google.
pub const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

// ---------------------------------------------------------------------------
// Wire types (subset used by Kal)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleCalendarList {
    pub items: Vec<GoogleCalendar>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleCalendar {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub background_color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleEvents {
    /// Incremental sync token for later refreshes (stored locally only).
    pub next_sync_token: Option<String>,
    #[serde(default)]
    pub items: Vec<GoogleEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleEvent {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub status: Option<String>,
    pub start: Option<GoogleTime>,
    pub end: Option<GoogleTime>,
    /// RFC 5545 RRULE lines (usually 0 or 1).
    #[serde(default, rename = "recurrence")]
    pub recurrence_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleTime {
    pub date: Option<String>,
    pub date_time: Option<String>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

impl GoogleEvent {
    pub fn is_cancelled(&self) -> bool {
        self.status.as_deref() == Some("cancelled")
    }

    pub fn start_dtz(&self) -> Option<DateTimeTz> {
        let t = self.start.as_ref()?;
        if let Some(date) = &t.date {
            return midnight(NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?);
        }
        let dt = DateTime::parse_from_rfc3339(t.date_time.as_deref()?).ok()?;
        Some(dt)
    }

    pub fn end_dtz(&self) -> Option<DateTimeTz> {
        let t = self.end.as_ref()?;
        if let Some(date) = &t.date {
            // Google's DATE end is exclusive already.
            return midnight(NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?);
        }
        DateTime::parse_from_rfc3339(t.date_time.as_deref()?).ok()
    }
}

fn midnight(d: NaiveDate) -> Option<DateTimeTz> {
    let offset = *chrono::Local::now().offset();
    offset
        .from_local_datetime(&d.and_time(NaiveTime::MIN))
        .single()
}

fn ulid_from_google_id(google_id: &str) -> ulid::Ulid {
    // Deterministic-ish: hash into a ULID-shaped value; uniqueness matters,
    // not ordering. Stable per event id via simple FNV-1a over bytes.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in google_id.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ulid::Ulid::from_bytes({
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&hash.to_be_bytes());
        bytes
    })
}

/// Map one Google event onto a Kal item destined for `calendar_id`.
pub fn map_event(event: &GoogleEvent, calendar_id: ulid::Ulid) -> Option<CalendarItem> {
    if event.is_cancelled() {
        return None;
    }
    let mut item = CalendarItem::new(
        ItemKind::Event,
        event.summary.clone().unwrap_or_default(),
        calendar_id,
        event.start_dtz()?,
    );
    item.id = ulid_from_google_id(&event.id);
    item.end = event.end_dtz();
    item.all_day = event.start.as_ref().and_then(|s| s.date.clone()).is_some();
    item.notes = event.description.clone().filter(|s| !s.is_empty());
    item.location = event.location.clone().filter(|s| !s.is_empty());
    item.rrule = event.recurrence_rules.first().map(|r| {
        r.split_once(':')
            .map(|(_, body)| body)
            .unwrap_or(r)
            .to_string()
    });
    Some(item)
}

/// Map a calendar-list response into Kal calendars tagged `GoogleImport`.
pub fn map_calendars(list: &[GoogleCalendar]) -> Vec<Calendar> {
    list.iter()
        .map(|g| Calendar {
            id: ulid_from_google_id(&format!("cal:{}", g.id)),
            name: g.summary.clone(),
            color: Color(
                g.background_color
                    .clone()
                    .unwrap_or_else(|| "#1a73e8".into()),
            ),
            source: CalendarSource::GoogleImport,
            visible: true,
            updated_at: Utc::now().fixed_offset(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Transport abstraction
// ---------------------------------------------------------------------------

/// HTTP abstraction so parsing/mapping is testable without network.
/// `bearer` carries the OAuth token for authorized calls.
pub trait Transport {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<String, ImportError>;
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, ImportError>;
}

/// Live transport over ureq (feature = "google").
#[cfg(feature = "google")]
pub struct UreqTransport;

#[cfg(feature = "google")]
impl Transport for UreqTransport {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<String, ImportError> {
        let mut req = ureq::get(url);
        if let Some(tok) = bearer {
            req = req.set("Authorization", &format!("Bearer {tok}"));
        }
        req.call()
            .map_err(|e| ImportError::Parse(e.to_string()))?
            .into_string()
            .map_err(|e| ImportError::Parse(e.to_string()))
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, ImportError> {
        ureq::post(url)
            .send_form(form)
            .map_err(|e| ImportError::Parse(e.to_string()))?
            .into_string()
            .map_err(|e| ImportError::Parse(e.to_string()))
    }
}

/// Fetch a calendar's upcoming events (single page; sync-token refresh uses
/// the same endpoint with `syncToken` appended by the caller).
pub fn events_url(calendar_id: &str) -> String {
    format!(
        "https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events         ?maxResults=2500&singleEvents=false"
    )
    .replace("calendars/", "calendars/")
    .replace("{calendar_id}", &urlencode(calendar_id))
}

pub fn fetch_events<T: Transport>(
    http: &T,
    access_token: &str,
    calendar_id: &str,
) -> Result<GoogleEvents, ImportError> {
    let body = http.get(&events_url(calendar_id), Some(access_token))?;
    parse_events(&body)
}

/// RFC 8628 device-flow step 1: request a device/user code pair.
pub fn start_device_flow<T: Transport>(
    http: &T,
    client_id: &str,
) -> Result<serde_json::Value, ImportError> {
    let body = http.post_form(
        GOOGLE_DEVICE_CODE_URL,
        &[("client_id", client_id), ("scope", GOOGLE_SCOPE)],
    )?;
    serde_json::from_str(&body).map_err(|e| ImportError::Parse(e.to_string()))
}

/// RFC 8628 device-flow polling step.
pub fn poll_device_token<T: Transport>(
    http: &T,
    client_id: &str,
    client_secret: &str,
    device_code: &str,
) -> Result<serde_json::Value, ImportError> {
    let body = http.post_form(
        GOOGLE_TOKEN_URL,
        &[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ],
    )?;
    serde_json::from_str(&body).map_err(|e| ImportError::Parse(e.to_string()))
}

pub fn parse_events(json: &str) -> Result<GoogleEvents, ImportError> {
    serde_json::from_str(json).map_err(|e| ImportError::Parse(e.to_string()))
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_timed_recurring_event() {
        let json = r#"{
            "id": "evt_abc123",
            "summary": "Board meeting",
            "location": "Big room",
            "start": { "dateTime": "2026-09-01T14:00:00-04:00" },
            "end": { "dateTime": "2026-09-01T15:00:00-04:00" },
            "recurrence": ["RRULE:FREQ=WEEKLY;BYDAY=TU;COUNT=3"]
        }"#;
        let ev: GoogleEvent = serde_json::from_str(json).unwrap();
        assert!(!ev.is_cancelled());
        let item = map_event(&ev, ulid::Ulid::nil()).unwrap();

        assert_eq!(item.title, "Board meeting");
        assert_eq!(item.location.as_deref(), Some("Big room"));
        assert_eq!(item.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=TU;COUNT=3"));
        assert!(!item.all_day);
        assert_eq!(item.start.format("%H:%M").to_string(), "14:00");

        // Deterministic id: same google id → same ULID.
        let again = map_event(&ev, ulid::Ulid::nil()).unwrap();
        assert_eq!(item.id, again.id);
    }

    #[test]
    fn maps_all_day_event_and_cancelled_filtered() {
        let json = r#"{
            "items": [
                { "id": "e1", "summary": "Holiday", "status": "cancelled",
                  "start": { "date": "2026-12-25" }, "end": { "date": "2026-12-26" } },
                { "id": "e2", "summary": "Christmas",
                  "start": { "date": "2026-12-25" }, "end": { "date": "2026-12-26" },
                  "description": "day off" }
            ],
            "nextSyncToken": "tok123"
        }"#;
        let events = parse_events(json).unwrap();
        assert_eq!(events.next_sync_token.as_deref(), Some("tok123"));

        let mapped: Vec<_> = events
            .items
            .iter()
            .filter_map(|e| map_event(e, ulid::Ulid::nil()))
            .collect();
        assert_eq!(mapped.len(), 1); // cancelled dropped
        let it = &mapped[0];
        assert!(it.all_day);
        assert_eq!(it.notes.as_deref(), Some("day off"));
        assert_eq!(
            it.start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 12, 25).unwrap()
        );
    }

    #[test]
    fn calendars_map_to_google_import_source() {
        let json = r##"{ "items": [
            { "id": "primary", "summary": "Me", "backgroundColor": "#3366cc" },
            { "id": "holidays", "summary": "Holidays" }
        ]}"##;
        let list: GoogleCalendarList = serde_json::from_str(json).unwrap();
        let cals = map_calendars(&list.items);
        assert_eq!(cals.len(), 2);
        assert!(cals
            .iter()
            .all(|c| c.source == CalendarSource::GoogleImport));
        assert_eq!(cals[0].name, "Me");
    }

    #[test]
    fn urlencoding_of_calendar_ids() {
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
    }

    #[allow(dead_code)] // silence unused warnings on helper kept for live path
    fn _refs() {
        let _: Option<&str> = None::<&str>.into();
        let _ = (GOOGLE_AUTH_URL, GOOGLE_TOKEN_URL, GOOGLE_DEVICE_CODE_URL);
    }
}
