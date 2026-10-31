# Architecture

Kal is a local-first calendar built as a Rust workspace. One core, many
shells: the Dioxus desktop/mobile app, native widgets, and (future) sync
peers all speak to the same domain logic.

```
crates/
├── kal-core/       Pure domain: models, recurrence expansion, view-models,
│                   reminder firing computation. ZERO UI deps — reused by the
│                   app AND linked into widget shims via kal-ffi.
├── kal-storage/    Storage backend, gated by platform: SQLite (rusqlite,
│                   bundled) on native; in-memory IndexedDB-backed on wasm.
│                   Same synchronous `Database` API on both.
│                   Append-only PRAGMA-versioned migrations. Repository exposes
│                   upsert_* only (sync-ready) plus soft-delete tombstones.
│                   scoped.rs holds the scoped-recurrence-edit algorithm once,
│                   with each backend implementing its two-row seam. It also
│                   re-anchors an "all events" edit on the series' own start
│                   date (the editor form is anchored on the clicked
│                   occurrence), shared by the Dioxus editor and the bridge.
├── kal-sync/       Account-free P2P sync: LWW CRDT merge, BIP39 sync-chain
│                   identity, XChaCha20-Poly1305 encrypted envelopes,
│                   pluggable Transport trait (live iroh gossip + DHT discovery,
│                   folder-gossip fallback; LAN mDNS later).
├── kal-notify/     Reminder scheduling: pure firing computation lives in
│                   kal-core; this crate materializes firings as platform
│                   local notifications on desktop (notify-rust, or cancellable
│                   sleeps in tests). The Android app arms the same computed
│                   firings with AlarmManager/NotificationManager from Kotlin
│                   (kal_reminder_firings_json), so nothing here links JNI.
├── kal-import/     .ics export/import (icalendar crate) with round-trip
│                   fidelity incl. VALARM reminders; Google Calendar read-only
│                   import behind a Transport abstraction (RFC 8628 device
│                   flow + REST v3 mapping). The Google path is library-only
│                   scaffolding: no UI, token storage or app call site wires it
│                   up yet (STEP0 decision 15).
├── kal-update/     Release checking/staging shared by the desktop app and the
│                   Android bridge: version arithmetic, GitHub latest-release
│                   query and asset resolution, size-capped SHA-256-verified
│                   download, all behind a Transport trait (ureq in production,
│                   a fake in tests). Only the install step is platform code.
├── kal-ffi/        Stable C ABI (JSON strings) over kal-storage + kal-core
│                   (plus kal-import for the ICS import/export, kal-update
│                   for the release check and kal-sync for chain pairing),
│                   consumed by native widget shims and the Kotlin Android
│                   interface layer. Versioned via kal_bridge_version();
│                   panic-guarded exports with a per-thread error channel
│                   (kal_last_error). Builds rlib + cdylib + staticlib.
└── kal-jni/        JNI layer for the Android app: translates JVM↔C types and
                    statically links kal-ffi, so one library (libkal_jni.so)
                    carries both the Java_* entry points and the kal_* C ABI the
                    Kotlin app and widgets load. Thirty-two
                    catch_unwind-guarded exports; all logic stays in kal-ffi.

app/                Dioxus 0.7 desktop shell (views, editor modal, settings,
                    pairing UI). Single shared use_resource per dataset;
                    mutations restart resources; a global signal re-renders
                    after sync merges. app/src/updater.rs = in-app updates
                    (release logic now in kal-update; desktop stage+restart swap;
                    Android APK self-install; compiled out on wasm).
android/app/         Owned Kotlin/Gradle Android app (the Dioxus → Kotlin rewrite).
                    Single-activity Compose shell talking to the native core
                    through crates/kal-jni (libkal_jni.so), which it loads after
                    the kal_bridge_version() handshake. Phase 2 gives it the top
                    bar + drawer + calendar sidebar, the ‹ Today › view nav and
                    the Month/Week/Day/Agenda views, all rendered from the
                    app-facing payloads (kal_month_json/kal_week_json/
                    kal_day_json/kal_agenda_json) whose tokens, strings and
                    geometry are ported 1:1 from app/assets/main.css and app/src.
                    Payload parsing, visibility filtering, formatting and period
                    arithmetic are pure Kotlin with JVM unit tests. Phase 3 adds
                    the editor modal (native save, scoped-recurring UX, task
                    checkbox). Phase 4 arms reminders: Rust computes the firings
                    (kal_reminder_firings_json), Kotlin owns AlarmManager,
                    the notification channel/receiver and re-arming after a
                    reboot. Phase 5 completes the sidebar's Import/Export
                    section: kal_import_ics_json turns an .ics into a new
                    visible calendar plus items, kal_export_ics_file writes
                    files/kal/Kal-export.ics, and Kotlin owns the SAF picker,
                    the KalFileProvider entry and the share chooser. Phase 6
                    lands Settings: the sync-chain profile is a plain `profile`
                    settings row (display name + base64 avatar, validated in
                    Kotlin), and Software & updates calls the native
                    kal_check_updates_json (one blocking check+download pass
                    from kal-update) before handing the staged APK to the
                    PackageInstaller through KalUpdater. The staged APK is
                    checked against the running app first (applicationId,
                    versionCode, signing certificate): a release this build
                    cannot replace is reported instead of opening an installer
                    that fails with Android's opaque message. Release builds
                    take their signing config from `android/keystore.properties`
                    or the KAL_* CI env vars (debug key as the fallback) — see
                    SIGNING.md. Phase 7 closes the last placeholder: the Sync
                    panel (start/join a chain, one-time 24-word phrase modal,
                    fingerprint, reveal, Sync now, device list with Remove,
                    two-step leave) over additive `kal_sync_*` exports. Android
                    syncs live P2P only,
                    reuses the identity files the Dioxus build wrote, and runs a
                    round immediately plus every 10 s from an in-process
                    coroutine while paired. The Rust
                    half is built and staged by
                    scripts/build-kotlin-android.sh; the
                    APK is built here, not by dx. Replaces the dx-generated
                    project that lives under target/dx/… and is wiped on every
                    build.
android/app/…       The home-screen widgets (Phase 8) live in the app module
                    (`com.kal.calendar.widgets`): the two AppWidgetProviders,
                    their RemoteViews res/ and the BOOT_COMPLETED refresher are
                    part of :app, and their JNI entry points
                    (`Java_com_kal_calendar_widgets_…`) sit in `crates/kal-jni`
                    beside the app's, reusing the frozen widget payloads in
                    `kal-ffi`. No staging step: the project is the app.
                    KalUpdater.kt/KalFileProvider.kt/KalShare.kt are likewise
                    part of :app (Phases 5–6); the Dioxus staging trees were
                    removed with the dx Android target.
install.sh          Cross-platform installer: downloads the matching release
                    asset (linux/macos/windows/android + Termux) and verifies
                    it against the release's sha256 via the GitHub API.
web/site/           Landing page for mihneamoso.github.io/kal (Pico template-
                    only, relative URLs), deployed by .github/workflows/pages.yml.
web/app/            Dioxus wasm web shell output for mihneamoso.github.io/kal/app
                    (built from source in CI by scripts/build-web.sh — never
                    committed; gitignored). base_path "kal/app"; deployed by
                    pages.yml (master) and release.yml (tags).
widgets/            (future) Swift WidgetKit shim calling kal-ffi.
```

## Sync-chain design (Brave-Sync model)

**Pairing.** Device A generates a 24-word BIP39 phrase. From the phrase seed,
SHA-256 domain-separated stretching derives two keys deterministically:

- an X25519 identity keypair → device *fingerprint* = first 8 bytes of
  SHA-256(pubkey), shown in the UI;
- an XChaCha20-Poly1305 payload key for envelope encryption.

Device B types the same phrase → derives identical keys → is now authorized.
No server account exists anywhere.

**Transport.** `kal_sync::Transport` moves opaque blobs between devices. The
primary implementation is *live P2P* (`kal_sync::live`, native only): each
device derives the same gossip topic id from the chain fingerprint, publishes
its iroh node id to a distributed hash table under that topic, and joins the
shared gossip swarm via the public n0 relay — so two devices that hold the same
phrase find each other from anywhere with zero configuration. The *file gossip*
implementation (sealed `.kalblob` files in a user-chosen outbox folder movable
by Syncthing/Dropbox/USB) remains the fallback for offline/air-gap setups (the
desktop app only: on Android the outbox is app-private storage no peer can
read). Because payloads are AEAD-encrypted under the chain key, both carriers
are untrusted; relays only ever see ciphertext. LAN mDNS plugs into the same
trait later.

**Orchestration (shared).** The round logic — seal local state, broadcast,
drain for a bounded window, merge, persist, retire shadowed placeholder
calendars — and the identity-file contract live in `kal_sync::client` and
`kal_sync::identity_file`, not in an app shell. The Dioxus app (`app/src/
sync_live.rs` is a re-export shim) and the Android bridge (`kal_sync_round_json`)
run the same implementation, so a phone and a desktop cannot drift. Identity
files sit next to the calendar database (`<files>/kal/sync-identity.json`,
`sync-node.json`, `sync-outbox/`) and are shared with the pre-rewrite app.

**Merge.** State-based CRDT with whole-record LWW registers. Every record
carries `updated_at`; winners are chosen by

1. newer `updated_at`,
2. tombstone flag at equal timestamps (deletes propagate),
3. lexicographic whole-record tie-break (clock-skew safety).

This total order makes convergence independent of delivery order/duplication,
which is all a gossip network guarantees. Peers exchange full snapshots of
what the other is missing (`SyncEnvelope { device_id, state }`), so long
offline periods need no version-vector bookkeeping. Upgrade path: swap
whole-record registers for field-level or Automerge/Yrs documents inside
`SyncState::merge` without touching the wire envelope.

**Revocation.** The sync panel lists the device ids whose envelopes have
reached this install. Remove writes the id to a device-local blocklist; every
sync round rejects that device's envelopes before merge, so the block survives
restarts and applies to both the live and shared-folder transports. Full
key-rotation-with-repair UX is tracked as future work.

**Leaving the chain.** In the UI, "Leave sync chain" (below "Sync now", with a
two-step inline confirm) deletes the local `sync-identity.json` and the local
`sync-outbox/` folder and drops the cached live transport immediately. The
gossip chain has no central membership, so this is purely local: other devices
simply stop receiving snapshots from this one. The per-device `sync-node.json`
(iroh identity) is kept — it belongs to the device, not the chain.

**Sync on Android.** The sidebar's Sync panel is a thin view over eight bridge
exports (`kal_sync_status_json`, `_start_chain_json`, `_join_chain_json`,
`_phrase_json`, `_leave`, `_round_json`, `_devices_json`, `_revoke_device`);
no new permission is needed because
`INTERNET` already ships for the updater. Reading status never mints an
identity — a fresh install stays unpaired until the user starts or joins a
chain. A paired app runs one round immediately and then every 10 s from an
in-process coroutine (the Dioxus app's background thread had the same cadence
and the same lifecycle: nothing runs once the process dies), reloading the
views when a round merges. The round is blocking (peer discovery up to ~15 s,
then a bounded drain), so it runs on `Dispatchers.IO`.

## Reminders

Reminders are computed, not pushed: `kal_core::reminders::compute_firings`
expands items (RRULE included) over a horizon and yields absolute fire times.
A recurring series fires once per occurrence; a non-recurring item fires once
from its own start — the grid expansion's one-occurrence-per-covered-day rule
(so a multi-day event shows on every cell) must not arm the same reminder on
each of those days. On desktop `kal_notify::ThreadScheduler` arms the next N
firings as cancellable
sleeps → OS notifications. On Android the firings cross the bridge
(`kal_reminder_firings_json`) and Kotlin arms the nearest 64 as
`setExactAndAllowWhileIdle` alarms (inexact when exact access is denied),
delivering from a `BroadcastReceiver` — the OS holds them, so they survive the
process; a boot receiver re-arms after a restart. The schedule is reconciled on
app start, after every data mutation, on foreground and after sync merges — no
background daemon, no network, works airplane-mode.

## ICS import/export

`kal-import` owns the format (`.ics` via the `icalendar` crate; VALARM reminders
round-trip). The Android sidebar crosses the ABI twice:
`kal_import_ics_json` parses a payload into one new visible `ics_import`
calendar plus its items (an item the store rejects is skipped), and
`kal_export_ics_file` writes every calendar and live item to a given path as one
VCALENDAR. Rust writes the file; Kotlin picks the input document over SAF
(`*/*`, read off the main thread) and hands the output to the share sheet
through `KalFileProvider` (`content://com.kal.calendar.updates/ics` → the same
`files/kal/Kal-export.ics`). A rejection surfaces the native reason in the
sidebar instead of only `eprintln!`.

## Settings, profile and updates

The `profile` setting row holds the sync-chain identity's display name and an
optional avatar (`username`, `avatar_mime`, `avatar_b64`); there is no native
profile code — the interface reads and writes that row as JSON, and the avatar's
4 MB cap and magic-byte check live in Kotlin, so only the final string crosses
the bridge. Software & updates calls `kal_check_updates_json(current_version,
dest_path)`: `kal-update` resolves the platform's asset from the GitHub latest
release, and when it is strictly newer downloads it (size-capped, SHA-256
verified) into `files/kal/updates/kal-update.apk`. Kotlin owns the last step —
the `PackageInstaller` intent for `content://com.kal.calendar.updates/apk` — but
first compares the staged APK's applicationId, versionCode and signing
certificate with the running app and reports a mismatch instead of opening an
installer that can only fail. The desktop app uses the same crate and stages a
`tar.gz` swap instead.

## Scoped recurring edits

`kal_storage::scoped` owns the series surgery for both front ends.

- *This event* EXDATEs the clicked occurrence out of the series and writes a
  copy. The copy is not an unrelated event: it carries
  `metadata.series_id`/`recurrence_id` naming its series and the excluded
  instant, so the editor keeps offering the scope choice, *This event* edits that
  row in place, and *All events* reaches it. (The link lives in the existing
  `metadata_json` column — no schema change — and rides along with the row
  through sync; it mirrors iCalendar's `RECURRENCE-ID` exception.)
- *…and following* truncates the base with an `UNTIL` and starts a new series
  (the new series carries no link). It is not offered from a linked occurrence:
  splitting a series from a modified instance would also have to relink the later
  exceptions, which is not implemented.
- *All events* rewrites the **series** row — so it re-anchors the edited
  start/end on the series' own date, keeping its UTC offset and taking the form's
  wall-clock times — and copies the dialog's shared fields (title, location,
  calendar, all-day, reminders, colour) to every linked occurrence. Each
  occurrence keeps its own date; a series-wide time change moves its clock by the
  same amount. The series' rule and EXDATEs always come from the series row, never
  from the form row (the dialog shows the series' rule while an occurrence is
  open). A date edit under *All events* is not a way to move the series start.
- Deleting a series also tombstones its linked occurrences; deleting one
  occurrence deletes only that occurrence (the series keeps its EXDATE).

Both backends expose `Database::{apply_scoped_edit, apply_all_edit,
save_exception, delete_with_exceptions}` over the shared algorithm; the Dioxus
editor and the JNI bridge call the same functions, so there is no second
implementation to drift.

## Data flow on mutation

```
editor save ──► db.upsert_item ──► restart(items resource) ──► views re-render
                                             └──► reminder reconcile effect
sync merge ──► db.upsert_* (merged rows) ──► RESOURCES_DIRTY signal ──► same
```
