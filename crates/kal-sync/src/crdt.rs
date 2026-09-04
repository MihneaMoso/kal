//! Conflict-free merge for Kal data (spec §5.4 step 4).
//!
//! Model: state-based CRDT using **last-writer-wins registers at item and
//! calendar granularity**, ordered by `(updated_at, deleted, tie_break)`.
//! Tombstones (`deleted = true`) always outrank older live edits at equal
//! timestamps so deletes propagate. This converges under arbitrary message
//! reordering/duplication, matching the gossip model where peers exchange
//! full snapshots of what the other is missing.
//!
//! Upgrade path documented in DECISIONS.md: field-level LWW or Automerge/Yrs
//! can replace `merge_item` without changing the wire envelope.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use kal_core::models::{Calendar, CalendarItem};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// One peer's complete shareable state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    pub calendars: BTreeMap<Ulid, Calendar>,
    pub items: BTreeMap<Ulid, CalendarItem>,
}

/// Envelope gossiped between paired devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEnvelope {
    /// Sender's stable device id (random ULID generated on chain join).
    pub device_id: Ulid,
    pub state: SyncState,
}

impl SyncState {
    pub fn from_parts(calendars: Vec<Calendar>, items: Vec<CalendarItem>) -> Self {
        Self {
            calendars: calendars.into_iter().map(|c| (c.id, c)).collect(),
            items: items.into_iter().map(|i| (i.id, i)).collect(),
        }
    }

    /// Merge `remote` into `self`; both sides end up identical afterwards.
    pub fn merge(&mut self, remote: &SyncState) {
        for (id, cal) in &remote.calendars {
            let winner = match self.calendars.get(id) {
                // A deleted calendar outranks a live one at equal timestamps
                // so deletions propagate; a merely hidden one keeps its
                // historical behavior (hide wins ties, then syncs visibility).
                Some(mine) => pick_winner(
                    mine.updated_at,
                    mine.deleted || !mine.visible,
                    &mine.name,
                    cal.updated_at,
                    cal.deleted || !cal.visible,
                    &cal.name,
                ),
                None => Winner::Remote,
            };
            if winner == Winner::Remote {
                self.calendars.insert(*id, cal.clone());
            }
        }
        for (id, item) in &remote.items {
            let winner = match self.items.get(id) {
                Some(mine) => pick_winner(
                    mine.updated_at,
                    mine.deleted,
                    &mine.title,
                    item.updated_at,
                    item.deleted,
                    &item.title,
                ),
                None => Winner::Remote,
            };
            if winner == Winner::Remote {
                self.items.insert(*id, item.clone());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Winner {
    Local,
    Remote,
}

/// Total, deterministic ordering used by every register:
/// 1. newer `updated_at` wins;
/// 2. tombstone-ish flag beats live value at the exact same timestamp;
/// 3. otherwise lexicographic content compare breaks final ties.
fn pick_winner(
    local_ts: DateTime<FixedOffset>,
    local_tombstone: bool,
    local_content: &str,
    remote_ts: DateTime<FixedOffset>,
    remote_tombstone: bool,
    remote_content: &str,
) -> Winner {
    match local_ts.cmp(&remote_ts) {
        std::cmp::Ordering::Greater => return Winner::Local,
        std::cmp::Ordering::Less => return Winner::Remote,
        std::cmp::Ordering::Equal => {}
    }
    match (local_tombstone, remote_tombstone) {
        (true, false) => return Winner::Local,
        (false, true) => return Winner::Remote,
        _ => {}
    }
    if local_content >= remote_content {
        Winner::Local
    } else {
        Winner::Remote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use kal_core::models::{Color, ItemKind};

    fn ts(secs: u64) -> DateTime<FixedOffset> {
        Utc.timestamp_opt(secs as i64, 0)
            .single()
            .unwrap()
            .fixed_offset()
    }

    fn item(title: &str, ts_val: u64, deleted: bool) -> CalendarItem {
        let start = Utc
            .timestamp_opt(1_785_000_000, 0)
            .single()
            .unwrap()
            .fixed_offset();
        let mut it = CalendarItem::new(ItemKind::Event, title, Ulid::new(), start);
        it.updated_at = ts(ts_val);
        it.created_at = ts(0);
        it.deleted = deleted;
        it
    }

    fn calendar(name: &str, ts_val: u64) -> Calendar {
        let mut c = Calendar::local(name, Color("#3366cc".into()));
        c.updated_at = ts(ts_val);
        c
    }

    #[test]
    fn newer_edit_wins() {
        let a = item("old", 100, false);
        let mut b = a.clone();
        b.title = "new".into();
        b.updated_at = ts(200);
        assert_eq!(merge_choice(&a, &b), Winner::Remote);
        assert_eq!(merge_choice(&b, &a), Winner::Local);
    }

    fn merge_choice(local: &CalendarItem, remote: &CalendarItem) -> Winner {
        pick_winner(
            local.updated_at,
            local.deleted,
            &local.title,
            remote.updated_at,
            remote.deleted,
            &remote.title,
        )
    }

    #[test]
    fn tombstone_beats_concurrent_live_edit_at_same_time() {
        let dead = item("gone", 100, true);
        let alive = item("renamed", 100, false);
        assert_eq!(merge_choice(&dead, &alive), Winner::Local);
        assert_eq!(merge_choice(&alive, &dead), Winner::Remote);
    }

    #[test]
    fn merge_is_symmetric_and_idempotent() {
        let i1 = item("shared", 100, false);

        let mut s1 = SyncState::default();
        s1.items.insert(i1.id, i1.clone());

        let mut s2 = SyncState::default();
        let mut edited = i1.clone();
        edited.title = "edited-on-2".into();
        edited.updated_at = ts(300);
        s2.items.insert(edited.id, edited.clone());
        let extra = item("only-on-2", 150, false);
        s2.items.insert(extra.id, extra.clone());

        s1.merge(&s2);
        assert_eq!(s1.items[&i1.id].title, "edited-on-2");
        assert_eq!(s1.items[&extra.id].title, "only-on-2");

        // Idempotent.
        let snapshot = s1.clone();
        s1.merge(&s2);
        assert_eq!(s1, snapshot);

        // Symmetric: both directions converge to identical states.
        let mut back = s2.clone();
        back.merge(&snapshot);
        let mut forward = snapshot.clone();
        forward.merge(&s2);
        assert_eq!(back, forward);
    }

    #[test]
    fn delete_propagates_over_older_edits_and_newer_live_wins_back() {
        let orig = item("target", 100, false);

        // Delete happens later → wins.
        let mut tomb = orig.clone();
        tomb.deleted = true;
        tomb.title = String::new();
        tomb.updated_at = ts(500);

        // A live edit even later resurrects (user re-created/edited after).
        let mut revived = orig.clone();
        revived.title = "revived".into();
        revived.updated_at = ts(600);

        let mut a = SyncState::default();
        a.items.insert(orig.id, tomb);
        let mut b = SyncState::default();
        b.items.insert(orig.id, revived);

        a.merge(&b);
        assert_eq!(a.items[&orig.id].title, "revived");
        assert!(!a.items[&orig.id].deleted);
    }

    #[test]
    fn three_replicas_converge_after_pairwise_gossip() {
        // Simulates arbitrary offline editing + out-of-order gossip rounds.
        let base = item("base", 100, false);

        let variants: Vec<CalendarItem> = (0..3)
            .map(|n| {
                let mut v = base.clone();
                v.title = format!("variant-{n}");
                v.updated_at = ts(200 + n * 10);
                v
            })
            .collect();

        let mut replicas = [
            SyncState::default(),
            SyncState::default(),
            SyncState::default(),
        ];
        for (r, v) in replicas.iter_mut().zip(variants.iter()) {
            r.items.insert(base.id, v.clone());
        }
        // Also give replica 0 an extra item nobody else has.
        let solo = item("solo", 50, false);
        replicas[0].items.insert(solo.id, solo.clone());

        // Gossip in a ring twice (worst-case propagation), then verify.
        for _ in 0..2 {
            for i in 0..3 {
                let j = (i + 1) % 3;
                let snapshot_i = replicas[i].clone();
                replicas[j].merge(&snapshot_i);
            }
        }
        assert_eq!(replicas[0], replicas[1]);
        assert_eq!(replicas[1], replicas[2]);
        assert_eq!(replicas[0].items[&base.id].title, "variant-2");
        assert_eq!(replicas[0].items.len(), 2);
    }

    #[test]
    fn calendars_merge_lww_and_union() {
        let shared = calendar("Shared", 100);
        let mut a = SyncState::default();
        let mut b = SyncState::default();

        let mut renamed = shared.clone();
        renamed.name = "Renamed".into();
        renamed.visible = false;
        renamed.updated_at = ts(400);
        b.calendars.insert(shared.id, renamed);

        let other = calendar("Other", 120);
        a.calendars.insert(other.id, other.clone());

        a.merge(&b);
        assert_eq!(a.calendars[&shared.id].name, "Renamed");
        assert!(!a.calendars[&shared.id].visible);
        assert!(a.calendars.contains_key(&other.id));
    }

    #[test]
    fn envelope_serializes() {
        let env = SyncEnvelope {
            device_id: Ulid::new(),
            state: SyncState::default(),
        };
        let json = serde_json::to_string(&env).unwrap();
        assert_eq!(serde_json::from_str::<SyncEnvelope>(&json).unwrap(), env);
    }
}
