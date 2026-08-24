# RULES.md — Environment Quirks & Continuation Guide

> BUILD LAYOUT (IMPORTANT): `.cargo/config.toml` sets
> `target-dir = "../kal-build"` → binaries land in `/home/moso/kal-build/`
> (OUTSIDE the repo) to keep the project directory tiny. Run the app via
> `/home/moso/kal-build/debug/kal` or `cd app && dx serve`. NEVER delete the
> repo thinking builds are inside; to reclaim disk, `rm -rf /home/moso/kal-build`.
> dx serve uses its own target dir under ~/.cache — unaffected.

> NOTE: The app was RENAMED from "Chrono" to "Kal" (user request). Crate names
> are kal-core/kal-storage/kal-sync/kal-notify/kal-import/kal-ffi, binary is
> `kal`, package `kal-app`. ICS extension headers are X-KAL-*. Docs may still
> say Chrono in historical entries. Old DB dir ~/.local/share/Kal is orphaned;
> new DB lives in ~/.local/share/kal/calendar.db.

Read this FIRST when resuming work. It contains hard-won discoveries that will
bite you again if ignored, plus the exact state to resume from.

## Hard-won toolchain facts (do not relearn these)

1. **Kal 0.4.42+**: `TimeZone::with_ymd_and_hms` returns `MappedLocalTime`,
   whose method is `.single()` — NOT `into_option()` (removed). Also
   `LocalResult` is deprecated/renamed; don't import it.
2. **serde**: internally-tagged enums (`#[serde(tag = "type")]`) CANNOT
   serialize primitive newtype variants (`MinutesBefore(i64)` → runtime error
   "cannot serialize tagged newtype variant containing an integer"). Use struct
   variants (`MinutesBefore { minutes: i64 }`).
3. **rusqlite `query_map`** row-mapping functions must return
   `rusqlite::Result<T>`, not a custom error type. Pattern used in
   kal-storage: internal helpers return `StorageError`, converted via a
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

- Workspace crates under `crates/`, app in `app/`. `kal-core` must stay UI-free.
- Migrations: append-only array `MIGRATIONS` in
  `crates/kal-storage/src/lib.rs`; NEVER edit applied migrations.
- All item/calendar writes go through `upsert_*` (sync-ready full-row replace).
- Deletes are soft (tombstones): never `DELETE FROM items`.
- Tests live in `crates/*/tests/*.rs` + inline `#[cfg(test)]`. Run:
  `cargo test --workspace`.
- License headers: MIT OR Apache-2.0 dual (see DECISIONS.md D1).

## Where we left off / resume plan

- ✅ Phase 1 COMPLETE: workspace scaffolded; kal-core models +
  kal-storage (migrations, repository) tested; Dioxus desktop shell builds &
  runs (`cargo run -p kal-app`); 15 tests green.
- ✅ Phase 2 COMPLETE: kal-core `viewmodel` module (month_grid,
  items_on_date, agenda_range) + tests; app has month/week/day/agenda views,
  event/task/birthday editor modal (create/edit/delete), task checkboxes,
  calendar visibility toggles, light/dark theme, single shared
  items/calendars resources via context.
- ✅ **Dioxus upgraded to 0.7.10** (app + `dx` CLI match); `cd app && dx serve`
  works (builds in ~50s cold, launches desktop window). Dioxus.toml added.
  Code needed NO changes for the 0.6→0.7 jump — all APIs used still exist
  (`use_context_provider`, `use_resource().value()`, `Signal::new`,
  `e.stop_propagation()`, `dangerous_inner_html`).
- ✅ Phase 4a/4b COMPLETE: `kal-core::reminders::compute_firings(items,
  from, horizon)` (+tests) and `kal-notify` crate: `Notifier` trait,
  NullNotifier, DesktopNotifier (notify-rust, feature "desktop"),
  ThreadScheduler implementing ReminderScheduler { reschedule/clear/
  pending_count } with cancel-flag threads (+tests with CollectingNotifier).
- ✅ Phase 4 COMPLETE: editor has reminder preset chips (10m/30m/1h/1d/1w) +
  custom minutes field; App provides `SchedulerHandle` context; a use_effect
  reconciles `compute_firings(items, now, 14d)` → `ThreadScheduler.reschedule`
  whenever the items resource length changes.
- ✅ Phase 5a/5b COMPLETE: kal-import implements export_calendar/
  export_all/import_ics with VALARM reminders, RRULE/EXDATE, VTODO tasks,
  birthday CATEGORIES/X-CHRONO extensions; 5 round-trip tests incl. a
  Google-shaped TZID payload. App sidebar has Import .ics / Export all buttons
  via rfd file dialogs. `icalendar` v0.17 with "Kal-tz" feature enabled.
- ✅ Phase 5c COMPLETE: kal-import::google — wire types with camelCase serde
  renames, map_event/map_calendars into GoogleImport calendars, deterministic
  ULIDs from google ids (FNV-1a), events_url/start_device_flow/
  poll_device_token behind `Transport` trait (UreqTransport under feature
  "google"); 4 offline tests. NOTE: raw strings containing `"#` need r##"..."##.
- ✅ Phase 7a COMPLETE: kal-ffi C ABI (kal_open/kal_close(out-param,
  double-close-safe)/kal_free/kal_upcoming_json/kal_month_grid_json), all
  panic-guarded returning NULL; JSON contract documented in widgets/kal_ffi.h;
  ABI tests incl. NULL-safety. Upcoming-json expands occurrences (yearly
  birthdays surface).
- ✅ Phase 7b: widgets/kal_ffi.h + android Glance + iOS WidgetKit shim sources
  (not compiled here — need SDKs).
- ✅ Phase 6 partial: birthday age badges ("Name · 36") in chips and rows.
- ✅ Phase 8a–8c COMPLETE (crates/kal-sync, 18 tests):
  - crdt.rs: SyncState/SyncEnvelope, whole-record LWW merge ordered by
    (updated_at, tombstone-flag, content) → convergent under gossip.
  - keys.rs: ChainIdentity — 24-word BIP39 phrase (feature "rand" needed!) →
    seed → XChaCha20Poly1305 payload key + X25519 identity/fingerprint.
    encrypt→nonce‖ct; decrypt errors as KeyError::Decrypt.
  - session.rs: SyncSession { seal_state/accept_blob/revoked list } +
    Transport trait; loopback tests prove two-replica convergence,
    intruder rejection, revocation, corruption handling.
  - GOTCHA: Calendar model gained updated_at (+ storage migration v2);
    any new Calendar literal must set it.
  - bip39 gotcha: phrases must be valid word counts (12/15/18/21/24); the
    classic test vector is 11×"abandon"+"about".
- ✅ Phase 8d COMPLETE: app/src/sync_ui.rs SyncPanel in sidebar — start chain
  (24-word phrase shown once in modal grid), join by pasting phrase,
  fingerprint display; identity persisted to <dbdir>/sync-identity.json with
  0600 perms.
- ✅ Phase 8e COMPLETE: transport_file.rs FileTransport (folder-gossip of
  .kalblob blobs; works with Syncthing/Dropbox/USB) + app "Sync now" button:
  seal→publish→drain→accept→upsert merged state→RESOURCES_DIRTY global signal
  restarts calendars/items resources in App. Sender-self filtering done in
  caller (recv returns all). 20 kal-sync tests total.
- ✅ Phase 9a COMPLETE: `settings` table (migration v3) + get/set/all_settings
  repo API; ui::Settings struct (theme/time_24h/first_day_monday/default_view)
  persisted as "preferences" JSON; TopBar has selects for time format, week
  start, default view + theme toggle, all saving immediately. Views format
  times via Settings::fmt_time; month grid honors week start.
  GOTCHA: Calendar/Settings literals need updated_at/full fields; run
  `cargo build` from REPO ROOT only (workdir matters for sed/python edits).
- ✅ Phase 9b COMPLETE: app/src/i18n.rs — lightweight FTL parser (flat
  key = value, comments skipped) + tr(key) with visible `<key>` fallback;
  strings in app/i18n/en-US/main.ftl. ViewMode::label now returns String.
  NOTE: fluent crate intentionally deferred until a second locale lands — the
  parser covers current needs and keeps deps light; swap-in documented.
- ✅ Phase 9c COMPLETE: a11y (aria-labels on nav/theme, dialog role +
  aria-modal + label on editor modal, :focus-visible outlines,
  prefers-reduced-motion); desktop mini-widget window via
  `dioxus::desktop::window().new_window(dom, cfg)` — always-on-top 280x320,
  self-contained component opening its own DB handle, refreshes every 5min,
  button in sidebar. dioxus-desktop dep added for Config/WindowBuilder/tao
  re-exports. Component fns used with VirtualDom::new need
  #[allow(non_snake_case)].
- ✅ Phase 10 COMPLETE: .github/workflows/ci.yml (fmt+clippy -D warnings+tests
  on 3 OSes + release artifact upload); ARCHITECTURE.md; CONTRIBUTING.md;
  README rewritten w/ status table, mobile/widget/packaging sections;
  LICENSE-MIT + LICENSE-APACHE files. Codebase is fmt/clippy-clean.
- ✅ CLIPPY GATE GREEN: `cargo clippy --workspace --all-targets -- -D warnings`
  passes. Fixes: Database Send+Sync (Mutex<Connection>), removed
  NotifyMethod import, from_ref(&x) instead of &[x.clone()], array instead of
  vec! for fixed replicas, contains_key instead of get().is_none(),
  constants test replaced dead _refs helper.
- ✅ kal-storage Database now wraps Connection in std Mutex → Send+Sync,
  fixing a real thread-safety hole under the async runtime (clippy caught it).
- ▶ PROJECT WRAP-UP: all spec phases either complete or explicitly deferred
  with owners listed in RULES "Future work" below. + desktop mini-widget window, then Phase 10. (fluent-rs), 9c a11y/perf + desktop
  mini-widget window, then Phase 10 release eng. (12/24h, first day of week, default
  view), i18n scaffolding (fluent-rs), a11y pass, desktop mini-widget window.
  Then Phase 10 release eng. Real iroh/mDNS transports remain future work
  behind the same Transport trait.: after merges re-run reminder
  reconcile + restart items resource; add "Sync now" using a LoopbackTransport
  placeholder until iroh lands (real transport = phase 8e). Then Phase 9. in app (generate phrase screen,
  join-by-phrase input, device fingerprint display), then wire a periodic
  reschedule of reminders after merges. Then Phase 9 polish.: LWW-CRDT merge + sync-chain
  key derivation (bip39 phrase → x25519/chacha20poly1305) + Transport trait
  with loopback tests. UI pairing screen after lib is solid.
- THEN Phase 9 polish, Phase 10 release. (Android/iOS via dioxus-mobile / xtask),
  birthdays module polish, then Phase 7 widgets (kal-ffi C ABI).
  Context budget: consider committing + summarizing RULES.md before starting
  each new phase. `rrule` crate (v0.14) in kal-core;
  `viewmodel::expand_occurrences` / `occurrences_by_date` (rrule + exdate
  aware) + tests; views render recurring instances; editor has Repeat selector
  (none/daily/weekly/monthly/yearly); per-instance edit scoping implemented in
  `app/src/ui.rs::db_apply_scoped_edit` (This-only → EXDATE+new single item,
  …and-following → UNTIL-truncate + new series, All-events → edit base).
- ▶ NEXT: Phase 4 — reminders & notifications:
  1. Implement `crates/kal-notify`: trait abstraction over platform
     notification scheduling; desktop impl via `notify-rust`; a pure
     `compute_next_firings(items, from, n)` helper lives in kal-core or
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
- Desktop app still launches: `timeout 8 ./target/debug/Kal; echo $?`
- Update DECISIONS.md with any judgment calls made during the phase

## Future work (deferred deliberately, with entry points)

- Live P2P transports: implement `kal_sync::Transport` for iroh and/or mDNS
  (crates/kal-sync/src/session.rs defines the trait; FileTransport is the
  reference impl). Feature-gate deps.
- Mobile app shells (phase 6): dioxus mobile renderer + AlarmManager /
  UNUserNotificationCenter notification FFI. kal-ffi already cross-compiles.
- Widget shims need Xcode/Android SDK builds (sources in widgets/).
- Sync-chain key rotation & re-pairing UX (revocation blocklist exists).
- fluent-rs runtime when locale #2 lands (parser swap documented in D43).
