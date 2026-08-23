# Decision Log

Judgment calls made while implementing ambiguous parts of the Chrono spec.
Newest decisions may reference later phases; each entry notes the phase.

## Phase 1 — Foundation

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **License: MIT OR Apache-2.0 dual** (over AGPL-3.0) | Maximizes FOSS adoption; spec allowed either. Applied workspace-wide. |
| D2 | **Storage: synchronous `rusqlite` (bundled SQLite), not async `sqlx`** | Simplest option preserving architecture. Queries are fast (<1ms typical); the Dioxus app wraps calls in `use_resource` futures so the render thread is never blocked. Revisit if profiling demands it. |
| D3 | **Datetimes stored twice** (epoch seconds for indexed range queries + RFC3339 string to preserve original UTC offset) | SQLite has no tz-aware type; range queries need a numeric index, but round-tripping through UTC would silently rewrite event offsets. |
| D4 | **Reminders/exdates/metadata stored as JSON columns**, not normalized tables | They are always loaded/written together with their item (aggregate root); simplifies CRDT upsert replay. Schema can normalize later behind the same repository API. |
| D5 | **`ReminderOffset::MinutesBefore { minutes }` as struct variant** | serde internally-tagged enums cannot serialize primitive newtype variants; struct variant gives forward-compatible JSON too. |
| D6 | **`Color` is a validated `#RRGGBB` string** | Full-palette custom colors required by §5.7; avoids pulling a palette crate into core. |
| D7 | **Birthday contact link is free-form `metadata.birthday_of: Option<String>`** | Contacts/vCard module arrives in phase 6; a loose ID avoids coupling core to a contact model now. |
| D8 | **Migrations tracked via `PRAGMA user_version` with an ordered const array** | Zero-dep, adequate for single-file DBs; append-only rule documented in RULES.md. |
| D9 | **Desktop data dir via `dirs-next` → `<data>/chrono/calendar.db`** | Standard per-platform location; falls back to in-memory DB when no data dir exists (e.g. sandboxes/CI). |
| D10 | **Default calendars auto-created on first launch**: "Personal" (Local) + "Birthdays" (source=Birthdays) per §4. | Matches spec's dedicated auto-created birthdays calendar. |
| D11 | **Dioxus 0.6 entry point uses `dioxus::launch(App)`** | 0.6 idiom replacing `dioxus_desktop::launch`. |

## Phase 3 — Recurrence

| # | Decision | Rationale |
|---|----------|-----------|
| D12 | **`rrule` crate v0.14** | Pure Rust, actively maintained, full RFC 5545 RRULE incl. validation. |
| D13 | **rrule works in its own `Tz` enum** (Local \| chrono-tz), not arbitrary TimeZone — we convert DateTime<FixedOffset> ↔ rrule::Tz via UTC at the boundary (`viewmodel::expand_occurrences`). | API constraint discovered by reading crate source. |
| D14 | **Editor exposes simplified repeat presets** (none/daily/weekly/monthly/yearly), not full RRULE editing | Simplest option preserving architecture; full BYDAY/interval editing deferred. Presets expand to plain `FREQ=…`. |
| D15 | **"This event only"** = EXDATE original occurrence on base + new standalone item; **"…and following"** = append `UNTIL` (occurrence −1min) to base + new series from edited occurrence; COUNT-based rules are left untouched for "and following" (falls back to base edit semantics). | Matches Google Calendar scope UX; documented limitation for COUNT rules. |
| D16 | **Multi-day non-recurring events render one occurrence per covered day.** | Keeps month-grid cells self-contained without cross-cell span logic yet. |
| D17 | **Week/day/agenda/month views consume one shared `occurrences_by_date` map** instead of querying per cell | Single expansion pass per render; O(items×window). |

## Pending decisions for later phases

- Sync CRDT engine: automerge vs yrs (phase 8).
- Transport: iroh vs libp2p (phase 8).
- ICS crate choice: `icalendar` vs `ics` (phase 5).
- Fluent-rs for i18n (phase 9).
