//! Cross-platform reminder scheduling for Kal (spec §5.3).
//!
//! Reminders are computed locally from the calendar database and materialized
//! as platform-native local notifications — no push server, no network.
//!
//! Layers:
//! - [`Notifier`]: shows a notification *now* (platform backend).
//! - [`ReminderScheduler`]: given [`ReminderFiring`]s, arranges future
//!   firings; re-calling `reschedule` reconciles (cancels + re-arms), matching
//!   the "reschedule on every foreground / after sync merge" requirement.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use kal_core::reminders::ReminderFiring;

/// Platform notification backend.
pub trait Notifier: Send + Sync + 'static {
    fn show(&self, title: &str, body: &str);
}

/// No-op notifier (tests, CI, platforms without a backend yet).
#[derive(Default, Clone, Copy)]
pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn show(&self, _title: &str, _body: &str) {}
}

/// Desktop notifications via notify-rust (DBus/Windows/macOS).
#[cfg(feature = "desktop")]
#[derive(Default, Clone, Copy)]
pub struct DesktopNotifier;

#[cfg(feature = "desktop")]
impl Notifier for DesktopNotifier {
    fn show(&self, title: &str, body: &str) {
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .appname("Kal")
            .show();
    }
}

/// Schedules reminder firings on background threads.
///
/// Each firing gets a thread that sleeps until its fire time; a cancellation
/// flag is checked before showing so `reschedule` can supersede stale plans.
/// Thread-per-firing is acceptable because only the next N firings are armed.
pub struct ThreadScheduler<N: Notifier> {
    notifier: Arc<N>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    now: Box<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>,
}

impl<N: Notifier> ThreadScheduler<N> {
    pub fn new(notifier: N) -> Self {
        Self {
            notifier: Arc::new(notifier),
            cancels: Mutex::new(HashMap::new()),
            now: Box::new(chrono::Utc::now),
        }
    }

    /// Override the clock (tests).
    pub fn with_clock(
        notifier: N,
        now: impl Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync + 'static,
    ) -> Self {
        Self {
            notifier: Arc::new(notifier),
            cancels: Mutex::new(HashMap::new()),
            now: Box::new(now),
        }
    }

    fn firing_key(f: &ReminderFiring) -> String {
        format!("{}:{}:{}", f.item_id, f.reminder_id, f.fire_at.to_rfc3339())
    }
}

impl<N: Notifier> ReminderScheduler for ThreadScheduler<N> {
    fn reschedule(&self, firings: &[ReminderFiring]) {
        self.clear();

        let mut cancels = self.cancels.lock().unwrap();
        for firing in firings {
            let key = Self::firing_key(firing);
            let cancel = Arc::new(AtomicBool::new(false));
            let cancelled = cancel.clone();
            let notifier = self.notifier.clone();
            let fire_at_utc = firing.fire_at.with_timezone(&chrono::Utc);
            let title = format!("Kal — {}", firing.title);
            let body = firing.title.clone();
            let now_fn = &self.now;

            // Compute sleep duration up-front against the injected clock so
            // tests don't need real wall-clock alignment.
            let delay = (fire_at_utc - (now_fn)()).to_std().unwrap_or(Duration::ZERO);

            cancels.insert(key, cancel);
            thread::spawn(move || {
                // Sleep in small slices so cancellation is responsive.
                let mut remaining = delay;
                while remaining > Duration::ZERO && !cancelled.load(Ordering::Relaxed) {
                    let slice = remaining.min(Duration::from_millis(500));
                    thread::sleep(slice);
                    remaining -= slice;
                }
                if !cancelled.load(Ordering::Relaxed) {
                    notifier.show(&title, &body);
                }
            });
        }
    }

    fn clear(&self) {
        let mut cancels = self.cancels.lock().unwrap();
        for flag in cancels.values() {
            flag.store(true, Ordering::Relaxed);
        }
        cancels.clear();
    }

    fn pending_count(&self) -> usize {
        self.cancels.lock().unwrap().len()
    }
}

/// Abstraction over the platform scheduling surface. Implementations exist
/// for desktop threads here; Android/iOS FFI implementations arrive with the
/// mobile phases and delegate to AlarmManager / UNUserNotificationCenter.
pub trait ReminderScheduler: Send + Sync {
    /// Replace all scheduled reminders with `firings`.
    fn reschedule(&self, firings: &[ReminderFiring]);
    /// Cancel everything scheduled.
    fn clear(&self);
    /// Number of currently armed firings (diagnostics/tests).
    fn pending_count(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    #[derive(Clone)]
    struct CollectingNotifier(Arc<Mutex<Vec<String>>>);

    impl Notifier for CollectingNotifier {
        fn show(&self, title: &str, _body: &str) {
            self.0.lock().unwrap().push(title.to_string());
        }
    }

    #[test]
    fn scheduler_fires_after_delay() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let notifier = CollectingNotifier(shown.clone());
        let sched = ThreadScheduler::new(notifier);

        let when = Utc::now() + chrono::Duration::milliseconds(100);
        sched.reschedule(&[firing("direct", when)]);

        thread::sleep(Duration::from_millis(500));
        assert!(!shown.lock().unwrap().is_empty());
    }

    #[test]
    fn clear_cancels_pending() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let notifier = CollectingNotifier(shown.clone());
        let sched = ThreadScheduler::new(notifier);

        let when = Utc::now() + chrono::Duration::milliseconds(150);
        sched.reschedule(&[ReminderFiring {
            item_id: ulid::Ulid::new(),
            reminder_id: ulid::Ulid::new(),
            fire_at: when.fixed_offset(),
            title: "cancelled".into(),
        }]);
        assert_eq!(sched.pending_count(), 1);
        sched.clear();
        assert_eq!(sched.pending_count(), 0);

        thread::sleep(Duration::from_millis(400));
        assert!(shown.lock().unwrap().is_empty());
    }

    #[test]
    fn reschedule_supersedes_previous_plan() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let notifier = CollectingNotifier(shown.clone());
        let sched = ThreadScheduler::new(notifier);

        let soon = Utc::now() + chrono::Duration::milliseconds(120);
        let later = Utc::now() + chrono::Duration::seconds(60);
        sched.reschedule(&[firing("stale-one", later)]);
        sched.reschedule(&[firing("fresh-two", soon)]);

        thread::sleep(Duration::from_millis(500));
        let titles = shown.lock().unwrap();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].contains("fresh-two"));
        assert!(!titles.iter().any(|t| t.contains("stale-one")));
    }

    fn firing(name: &str, at: DateTime<Utc>) -> ReminderFiring {
        ReminderFiring {
            item_id: ulid::Ulid::new(),
            reminder_id: ulid::Ulid::new(),
            fire_at: at.fixed_offset(),
            title: name.into(),
        }
    }
}
