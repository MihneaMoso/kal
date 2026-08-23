# RULES.md — Environment Quirks & Continuation Guide

Read this FIRST when resuming work. It contains hard-won discoveries that will
bite you again if ignored, plus the exact state to resume from.

## Hard-won toolchain facts (do not relearn these)

1. **chrono 0.4.42+**: `TimeZone::with_ymd_and_hms` returns `MappedLocalTime`,
   whose method is `.single()` — NOT `into_option()` (removed). Also
   `LocalResult` is deprecated/renamed; don't import it.
2. **serde**: internally-tagged enums (`#[serde(tag = "type")]`) CANNOT
   serialize primitive newtype variants (`MinutesBefore(i64)` → runtime error
   "cannot serialize tagged newtype variant containing an integer"). Use struct
   variants (`MinutesBefore { minutes: i64 }`).
3. **rusqlite `query_map`** row-mapping functions must return
   `rusqlite::Result<T>`, not a custom error type. Pattern used in
   chrono-storage: internal helpers return `StorageError`, converted via a
   blanket `impl From<StorageError> for rusqlite::Error`
   (`Error::ToSqlConversionFailure`). And collect with
   `rows.collect::<rusqlite::Result<Vec<_>>>()?` then wrap in `Ok(...)` —
   collecting directly into `Result<Vec<_>, StorageError>` does NOT compile.
4. **dioxus 0.6 desktop**: entry is `dioxus::launch(App)`. In RSX,
   `{expr.format(...)}` with escaped quotes fails to parse ("Expected Ident or
   Expression") — precompute strings into locals before rsx.
   - **`let` statements are NOT allowed inside rsx `for` loop bodies** (only
     top-level in the fn before `rsx!`, or inside `{ ... }` expression blocks).
   - Every closure inside rsx must own its captures: clone `Arc<Database>`
     handles into locals per use BEFORE rsx; a variable moved into one
     handler can't be moved again by another.
   - A component's `for x in list { <Component/> }` body is itself an FnMut
     closure — per-iteration state must live in a child COMPONENT (props are
     moved per iteration legally), not in inline lets.
   - Component props require `Clone + PartialEq` on custom prop structs.
   - Match a signal in rsx via `match *view.read() { ... }` (deref!);
     string interpolation `"{c.year}"` is FIELD access only — methods fail.
   - Shared state pattern that works: App creates ONE `use_resource` per
     dataset, provides via `use_context_provider`; child components read it
     from context and `.restart()` it after mutations.
5. **crates.io reality check**: `dirs-next` latest major is **2**, not 5.
6. Build takes ~3–5 min cold (dioxus-desktop pulls GTK/webkit deps on Linux);
   incremental after that. Use `cargo build 2>&1 | grep -E "^error" -A10` to
   keep output small.

## Project conventions

- Workspace crates under `crates/`, app in `app/`. `chrono-core` must stay UI-free.
- Migrations: append-only array `MIGRATIONS` in
  `crates/chrono-storage/src/lib.rs`; NEVER edit applied migrations.
- All item/calendar writes go through `upsert_*` (sync-ready full-row replace).
- Deletes are soft (tombstones): never `DELETE FROM items`.
- Tests live in `crates/*/tests/*.rs` + inline `#[cfg(test)]`. Run:
  `cargo test --workspace`.
- License headers: MIT OR Apache-2.0 dual (see DECISIONS.md D1).

## Where we left off / resume plan

- ✅ Phase 1 COMPLETE: workspace scaffolded; chrono-core models +
  chrono-storage (migrations, repository) tested; Dioxus desktop shell builds &
  runs (`cargo run -p chrono-app`); 15 tests green.
- ✅ Phase 2 COMPLETE: chrono-core `viewmodel` module (month_grid,
  items_on_date, agenda_range) + tests; app has month/week/day/agenda views,
  event/task/birthday editor modal (create/edit/delete), task checkboxes,
  calendar visibility toggles, light/dark theme, single shared
  items/calendars resources via context.
- ✅ **Dioxus upgraded to 0.7.10** (app + `dx` CLI match); `cd app && dx serve`
  works (builds in ~50s cold, launches desktop window). Dioxus.toml added.
  Code needed NO changes for the 0.6→0.7 jump — all APIs used still exist
  (`use_context_provider`, `use_resource().value()`, `Signal::new`,
  `e.stop_propagation()`, `dangerous_inner_html`).
- ✅ Phase 4a/4b COMPLETE: `chrono-core::reminders::compute_firings(items,
  from, horizon)` (+tests) and `chrono-notify` crate: `Notifier` trait,
  NullNotifier, DesktopNotifier (notify-rust, feature "desktop"),
  ThreadScheduler implementing ReminderScheduler { reschedule/clear/
  pending_count } with cancel-flag threads (+tests with CollectingNotifier).
- ▶ NEXT: Phase 4c — wire into app:
  1. Editor modal: reminder preset chips (10m/30m/1h/1d/1w/custom minutes)
     stored via EditorState → Vec<Reminder> on item; save maps minutes to
     Reminder::minutes_before.
  2. App startup/use_effect on items change: compute_firings(all visible
     items, now, horizon=14d) → ThreadScheduler.reschedule. Provide scheduler
     as context from App (Arc<ThreadScheduler<DesktopNotifier>>).
  3. Notification tap deep-linking deferred to mobile phase.
- THEN Phase 5 (.ics import/export), etc. `rrule` crate (v0.14) in chrono-core;
  `viewmodel::expand_occurrences` / `occurrences_by_date` (rrule + exdate
  aware) + tests; views render recurring instances; editor has Repeat selector
  (none/daily/weekly/monthly/yearly); per-instance edit scoping implemented in
  `app/src/ui.rs::db_apply_scoped_edit` (This-only → EXDATE+new single item,
  …and-following → UNTIL-truncate + new series, All-events → edit base).
- ▶ NEXT: Phase 4 — reminders & notifications:
  1. Implement `crates/chrono-notify`: trait abstraction over platform
     notification scheduling; desktop impl via `notify-rust`; a pure
     `compute_next_firings(items, from, n)` helper lives in chrono-core or
     notify crate.
  2. App: on startup/foreground + after mutations, schedule next N firings of
     all visible items' reminders; notification click deep-links (desktop:
     just focus; deep-link routing later).
  3. Reminder presets UI in editor (10m/30m/1h/1d/1w/custom minutes).
- Then Phase 5 (.ics import/export via `icalendar` crate + Google OAuth),
  Phase 6 mobile, etc. Full plan: README.md §Roadmap and master prompt §7.

## Verification checklist before declaring a phase done

- `cargo test --workspace` green
- `cargo build --workspace` warning-free (fix or justify warnings)
- Desktop app still launches: `timeout 8 ./target/debug/chrono; echo $?`
- Update DECISIONS.md with any judgment calls made during the phase
