//! Reminder firing computation (spec §5.3).
//!
//! Pure logic: given items and a time window, produce the reminder firings
//! the platform scheduler should materialize. The platform layer
//! (`kal-notify`) turns these into OS-native local notifications.

use chrono::NaiveDate;

use crate::models::{CalendarItem, ItemKind, Occurrence};
use crate::viewmodel::expand_occurrences;

/// A single upcoming reminder firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderFiring {
    pub item_id: crate::models::Ulid,
    pub reminder_id: crate::models::Ulid,
    /// When the notification should fire.
    pub fire_at: crate::models::DateTimeTz,
    /// Item title for display.
    pub title: String,
}

/// How far ahead recurring series are expanded when computing firings.
/// Bounds work per item regardless of unbounded rules.
const RECURRENCE_HORIZON_DAYS: i64 = 366;

/// Compute all reminder firings in `[from, from + horizon_days]` for active
/// items, sorted by fire time.
///
/// Completed tasks are skipped entirely. Reminders whose resolved instant is
/// already in the past relative to `from` are not fired (the scheduler
/// reconciles on foreground; missed firings are dropped, matching local-first
/// semantics without a background daemon guarantee).
pub fn compute_firings(
    items: &[CalendarItem],
    from: crate::models::DateTimeTz,
    horizon_days: i64,
) -> Vec<ReminderFiring> {
    let to_date =
        from.date_naive() + chrono::Duration::days(horizon_days.min(RECURRENCE_HORIZON_DAYS));
    let mut out = Vec::new();

    for item in items {
        if item.deleted || item.reminders.is_empty() {
            continue;
        }
        if item.kind == ItemKind::Task && item.completed.is_some() {
            continue;
        }
        for occ in occurrences_window(item, from.date_naive(), to_date) {
            for reminder in &item.reminders {
                let fire_at = reminder.offset.resolve(occ.start);
                if fire_at < from {
                    continue;
                }
                out.push(ReminderFiring {
                    item_id: item.id,
                    reminder_id: reminder.id,
                    fire_at,
                    title: item.title.clone(),
                });
            }
        }
    }
    out.sort_by_key(|f| f.fire_at);
    out.dedup_by(|a, b| {
        a.item_id == b.item_id && a.reminder_id == b.reminder_id && a.fire_at == b.fire_at
    });
    out
}

fn occurrences_window(item: &CalendarItem, from: NaiveDate, to: NaiveDate) -> Vec<Occurrence> {
    expand_occurrences(item, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{datetime_from_parts, Calendar, ItemKind, Reminder, ReminderOffset};
    use chrono::{DateTime, FixedOffset, Timelike};

    fn cal() -> Calendar {
        Calendar::local("Test", crate::models::Color("#3366cc".into()))
    }

    fn ts(y: i32, m: u32, d: u32, h: u32) -> DateTime<FixedOffset> {
        datetime_from_parts(y, m, d, h, 0, 0).unwrap()
    }

    fn event(start: DateTime<FixedOffset>, minutes_before: &[i64]) -> CalendarItem {
        let mut it = CalendarItem::new(ItemKind::Event, "Standup", cal().id, start);
        it.end = Some(start + chrono::Duration::hours(1));
        it.reminders = minutes_before
            .iter()
            .map(|m| Reminder::minutes_before(*m))
            .collect();
        it
    }

    #[test]
    fn single_event_offsets() {
        let it = event(ts(2026, 8, 24, 9), &[30, 1440]);
        let firings = compute_firings(&[it.clone()], ts(2026, 8, 20, 0), 30);
        let times: Vec<_> = firings.iter().map(|f| f.fire_at).collect();
        assert_eq!(times.len(), 2);
        assert_eq!(times[0], ts(2026, 8, 23, 9)); // 24h before
        assert_eq!(times[1], ts(2026, 8, 24, 8).with_minute(30).unwrap()); // 30m before

        // Firing ids map back to reminders.
        let rem_ids: Vec<_> = it.reminders.iter().map(|r| r.id).collect();
        assert!(firings.iter().all(|f| rem_ids.contains(&f.reminder_id)));
    }

    #[test]
    fn past_and_out_of_horizon_firings_dropped() {
        let it = event(ts(2026, 8, 24, 9), &[1440]);
        // Window starts after the 24h-before instant (Aug 23 09:00).
        let firings = compute_firings(&[it.clone()], ts(2026, 8, 24, 0), 7);
        assert!(firings.is_empty());

        // Horizon ends before the reminder would fire.
        let firings = compute_firings(&[it], ts(2026, 8, 1, 0), 2);
        assert!(firings.is_empty());
    }

    #[test]
    fn completed_tasks_are_silent() {
        let mut it = event(ts(2026, 8, 24, 9), &[60]);
        it.kind = ItemKind::Task;
        it.completed = Some(ts(2026, 8, 23, 12));
        assert!(compute_firings(&[it], ts(2026, 8, 20, 0), 30).is_empty());
    }

    #[test]
    fn deleted_items_are_silent() {
        let mut it = event(ts(2026, 8, 24, 9), &[60]);
        it.deleted = true;
        assert!(compute_firings(&[it], ts(2026, 8, 20, 0), 30).is_empty());
    }

    #[test]
    fn recurring_series_fire_each_instance() {
        let mut it = event(ts(2026, 9, 1, 9), &[60]);
        it.rrule = Some("FREQ=DAILY;COUNT=5".into());
        let firings = compute_firings(&[it], ts(2026, 8, 31, 0), 30);
        assert_eq!(firings.len(), 5);
        assert_eq!(firings[0].fire_at, ts(2026, 9, 1, 8));
        assert_eq!(firings[4].fire_at, ts(2026, 9, 5, 8));
    }

    #[test]
    fn absolute_offset_resolves_independently_of_start() {
        let abs = datetime_from_parts(2026, 12, 24, 18, 0, 0).unwrap();
        let mut it = CalendarItem::new(ItemKind::Event, "Party", cal().id, ts(2026, 12, 24, 20));
        it.reminders = vec![Reminder {
            id: ulid::Ulid::new(),
            offset: ReminderOffset::Absolute(abs),
            method: crate::models::NotifyMethod::Push,
        }];
        let firings = compute_firings(&[it], ts(2026, 12, 1, 0), 40);
        assert_eq!(firings.len(), 1);
        assert_eq!(firings[0].fire_at, abs);
    }

    #[test]
    fn sorted_by_fire_time_across_items() {
        let early = event(ts(2026, 8, 25, 9), &[120]);
        let late = event(ts(2026, 8, 24, 22), &[60]);
        let firings = compute_firings(&[early, late], ts(2026, 8, 20, 0), 30);
        let times: Vec<_> = firings.iter().map(|f| f.fire_at).collect();
        let mut sorted = times.clone();
        sorted.sort();
        assert_eq!(times, sorted);
    }
}
