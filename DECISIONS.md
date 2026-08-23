# Decision Log

Judgment calls made while implementing ambiguous parts of the Kal spec.
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
| D9 | **Desktop data dir via `dirs-next` → `<data>/Kal/calendar.db`** | Standard per-platform location; falls back to in-memory DB when no data dir exists (e.g. sandboxes/CI). |
| D10 | **Default calendars auto-created on first launch**: "Personal" (Local) + "Birthdays" (source=Birthdays) per §4. | Matches spec's dedicated auto-created birthdays calendar. |
| D11 | **Dioxus 0.6 entry point uses `dioxus::launch(App)`** | 0.6 idiom replacing `dioxus_desktop::launch`. |

## Phase 3 — Recurrence

| # | Decision | Rationale |
|---|----------|-----------|
| D12 | **`rrule` crate v0.14** | Pure Rust, actively maintained, full RFC 5545 RRULE incl. validation. |
| D13 | **rrule works in its own `Tz` enum** (Local | Kal-tz), not arbitrary TimeZone — we convert DateTime<FixedOffset> ↔ rrule::Tz via UTC at the boundary (`viewmodel::expand_occurrences`). | API constraint discovered by reading crate source. |
| D14 | **Editor exposes simplified repeat presets** (none/daily/weekly/monthly/yearly), not full RRULE editing | Simplest option preserving architecture; full BYDAY/interval editing deferred. Presets expand to plain `FREQ=…`. |
| D15 | **"This event only"** = EXDATE original occurrence on base + new standalone item; **"…and following"** = append `UNTIL` (occurrence −1min) to base + new series from edited occurrence; COUNT-based rules are left untouched for "and following" (falls back to base edit semantics). | Matches Google Calendar scope UX; documented limitation for COUNT rules. |
| D16 | **Multi-day non-recurring events render one occurrence per covered day.** | Keeps month-grid cells self-contained without cross-cell span logic yet. |
| D17 | **Week/day/agenda/month views consume one shared `occurrences_by_date` map** instead of querying per cell | Single expansion pass per render; O(items×window). |

## Phase 4 — Reminders

| # | Decision | Rationale |
|---|----------|-----------|
| D18 | **Firing computation in kal-core** (`reminders::compute_firings`), platform scheduling in kal-notify | Keeps logic headless-testable & reusable by widget FFI; notify crate stays thin. |
| D19 | **Desktop scheduling = thread-per-firing with cancel flags** (ThreadScheduler), only next N firings armed | No daemon needed locally; simple, dependency-free; Android/iOS will use AlarmManager/UNUserNotificationCenter instead. |
| D20 | **Missed firings are dropped on reconcile** (no catch-up of past reminders) | Matches local-first semantics without background guarantees; avoids notification storms after long offline periods. |
| D21 | **Reconcile trigger = items resource version change** (len check) in a use_effect | Cheap approximation of "on foreground/mutation/sync"; revisit when sync lands. |
| D22 | **notify-rust behind `desktop` feature** | Keeps mobile/web builds compiling before their native FFI backends exist. |

## Phase 5 — Import/Export

| # | Decision | Rationale |
|---|----------|-----------|
| D23 | **`icalendar` crate 0.17** (parser + Kal-tz features) over `ics` | Typed builders AND parser in one crate; actively maintained. |
| D24 | **Datetimes exported as UTC**, not TZID form | We store fixed offsets without IANA names; UTC is standards-compliant, lossless-in-instant, and simplest. TZID payloads from Google still import correctly via Kal-tz conversion. |
| D25 | **All-day DTEND emitted exclusive (+1 day)** per RFC 5545 §3.6.1; on import converted back to inclusive last-day instant. | Interop correctness with Google/Apple/etc. |
| D26 | **Reminders round-trip as VALARM DISPLAY with TRIGGER=-PT{n}S plus X-KAL-REMINDER-ID** preserving reminder ULIDs; trigger parser accepts S/M/H/D. | Unlimited reminders survive export→import 1:1 while staying readable by other apps. |
| D27 | **Tasks as VTODO** with STATUS COMPLETED/NEEDS-ACTION + COMPLETED timestamp. | Standard mapping; other clients understand it. |
| D28 | **Birthdays marked CATEGORIES:BIRTHDAY + X-KAL-BIRTHDAY-OF person field.** | Round-trips our metadata through standard containers. |
| D29 | **Non-ULID UIDs (e.g. Google) get fresh ULIDs on import.** | Keeps internal id space consistent; provenance lives on the IcsImport calendar. |

## Rename

| # | Decision | Rationale |
|---|----------|-----------|
| D30 | **Full rename Chrono → Kal** including crate names (kal-*), binary `kal`, package `kal-app`, ICS extensions `X-KAL-*`, notification appname, and data dir `~/.local/share/kal`. | User request; done wholesale so no mixed branding remains. The external `chrono` Rust date crate keeps its name (it's a dependency, not our product). |

## Pending decisions for later phases

- Sync CRDT engine: automerge vs yrs (phase 8).
- Transport: iroh vs libp2p (phase 8).
- ICS crate choice: `icalendar` vs `ics` (phase 5).
- Fluent-rs for i18n (phase 9).
