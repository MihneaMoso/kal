# Step 0 — Discovery inventory for the Dioxus→Kotlin Android rewrite

Filled per the rewrite guide. Status: complete as a **desk inventory**; the
on-device baseline measurements are still **pending** (see "Baseline" at the
bottom). Read together with `ARCHITECTURE.md` (already a behavioral spec),
`DECISIONS.md`, `RULES.md`, and `info.md`.

## 0.1 Filled-in placeholders

| Placeholder | Value | Evidence |
|---|---|---|
| `<APP_NAME>` | **Kal** | `RULES.md` (rename Chrono→Kal), `app/Dioxus.toml` |
| `<CORE_CRATE>` | **`kal-core` + `kal-storage` + `kal-sync` + `kal-notify` + `kal-import`** (the core is already a workspace of libraries); the bridge lives in **`kal-ffi`** (`crates/kal-ffi`) | workspace `Cargo.toml`, `ARCHITECTURE.md` |
| `<ANDROID_APP_ID>` | **`com.kal.calendar`** | `app/Dioxus.toml` `[bundle] identifier`; all JNI crossings (`com/kal/calendar/…`, `Java_com_kal_calendar_widgets_…`) |
| `<MIN_SDK>` | **30** | `app/Dioxus.toml` `[android.app] min_sdk_version = 30` (target/compile 34) |

## 0.2 The guide's fit check (section 2 of the guide, answered for Kal)

- **Pain is in the interface (2.1) — holds, strongly.** The whole Android UI is
  one Wry `WebView` (`Android/MainActivity.kt` subclasses `WryActivity`).
  Live examples of platform-view workarounds already in the tree:
  - `app/src/android_picker.rs` (304 lines) bypasses the WebView entirely
    because `<input type="file">` "fires onchange but bytes never reach Rust";
  - avatar upload needs a native picker (`KalFilePicker.kt` +
    `MainActivity.onActivityResult`) with a 30 s blocking JNI wait;
  - one `document::eval` hack for the day-view auto-scroll
    (`app/src/ui.rs:421-428`);
  - CSS injected via `dangerous_inner_html` (`app/src/main.rs:463,498`).
- **Value is in services (2.2) — holds.** SQLite repository with migrations and
  poison-row policy (`crates/kal-storage/src/native.rs`), RFC 5545 recurrence +
  scoped editing (`crates/kal-core/src/viewmodel.rs`, `app/src/ui.rs::db_apply_scoped_edit`),
  BIP39+XChaCha20 sync chain (`crates/kal-sync`), iroh gossip transport
  (`crates/kal-sync/src/live.rs`), ics round-trip (`crates/kal-import/src/lib.rs`).
- **A JNI precedent exists (2.3) — holds, three of them.**
  1. `app/src/widget_ffi.rs` — Kotlin widgets call
     `Java_com_kal_calendar_widgets_KalWidgets_nativeSchedule/nativeMonth`
     which call the stable C ABI of `kal-ffi` (`System.loadLibrary("main")` in
     `android/widget/src/main/kotlin/com/kal/calendar/widgets/KalWidgets.kt`).
  2. `app/src/android_picker.rs` + `android/updater/src/main/kotlin/com/kal/calendar/KalFilePicker.kt` —
     blocking-pick JNI with pending-exception clearing.
  3. `app/src/updater.rs::request_android_install` → `KalUpdater.installApk(Context,String)`
     (`android/updater/src/main/kotlin/com/kal/calendar/KalUpdater.kt`).
- **Platform features would be first-class (2.4) — holds.** Home-screen
  widgets, system share sheet, package installer are all already Kotlin-side.

## 0.3 Platforms and features built

| Target | State | Where |
|---|---|---|
| Desktop (GTK/Webkit / Win / macOS) | ships | `main()` desktop branch `app/src/main.rs:197`, `target.'cfg(all(not(android), not(wasm32)))'` deps `app/Cargo.toml:38` |
| Android (Dioxus mobile / wry) | ships (CI `android-build` + tag builds; APK named `kal-<ver>-android.apk`) | `.github/workflows/ci.yml:72-130`, `.github/workflows/release.yml:72-133`, `dx bundle --android` |
| Web (wasm, IndexedDB store) | ships at `mihneamoso.github.io/kal/app/` | `crates/kal-storage/src/wasm.rs`, `app/Dioxus.toml` base_path, `scripts/build-web.sh` |
| iOS | sources only (WidgetKit shim `widgets/`, not built) | `ARCHITECTURE.md` |

Feature gates per target: desktop `dioxus/desktop + dioxus-desktop + kal-notify["desktop"] + rfd + tar + flate2`;
Android `dioxus/mobile + kal-notify (NullNotifier) + kal-ffi + ndk-context + jni`;
wasm compiles out `kal-sync`, updater, `sync_live/sync_ui`, mini widget
(`app/src/main.rs:1-16` module gates).

## 0.4 Routes and screens

There is **no** Dioxus `Routable` router — one static component tree with
modal overlays and a view.Mode switch. Full screen list (parity target = the
current APK, v0.1.9):

| # | Current screen | Implementation | Notes |
|---|---|---|---|
| 1 | Calendar shell (TopBar + Sidebar + Content) | `App/TopBar/Sidebar/Content` `app/src/main.rs:282-851` | top bar = hamburger, title "Kal", subtitle "local-first calendar — <today line>", inline prefs selects (time format, week start) or gear drawer <900px CSS, theme toggle button, settings gear |
| 2 | Month view | `ui::MonthView` `app/src/ui.rs:160-258` | 6-week grid from `viewmodel::month_grid`; today highlight; chips with time+title (+"· age" for birthdays); tap empty cell → new event on date; tap chip → editor |
| 3 | Week view | `ui::WeekView` `app/src/ui.rs:543-588` | 7-day list, per-day items (empty day shows "—") |
| 4 | Day view | `ui::DayView` `app/src/ui.rs:387-504` | all-day strip; 48px/hour absolute grid from `viewmodel::layout_day`; one-shot auto-scroll to 08:00 via `document::eval` |
| 5 | Agenda view | `ui::AgendaView` `app/src/ui.rs:590-615` | next 30 days, rows truncated at 100, empty state "Nothing in the next 30 days." |
| 6 | View navigation strip | `ViewNav` `app/src/main.rs:853-917` | ‹ / Today / › plus Month|Week|Day|Agenda buttons; period label per mode; cursor by ±1 day/week/month |
| 7 | Sidebar: calendars | `Sidebar`/`CalendarRow` `app/src/main.rs:769-835, 934-966` | checkbox toggles visibility (persists, restarts both resources), color dot, name, source label ("google" / "ics" / "auto" / "") |
| 8 | Sidebar: New item | `app/src/main.rs:817-832` | "+ Event / + Task / + Birthday" buttons → editor preset for kind |
| 9 | Editor modal (create/edit event/task/birthday) | `ui::EditorModal` `app/src/ui.rs:888-1217` | fields: Title, Person (birthdays), Date, All-day, Start/End, reminder preset chips (10m/30m/1h/1d/1w) + custom minutes, Location, Calendar select, Color swatches (8 presets + "Calendar"), Repeat presets (none/daily/weekly/monthly/yearly), scope-aware Delete; Save; for recurring edits: "All events / …and following / This event" |
| 10 | Settings modal (profile + software) | `profile::SettingsScreen` `app/src/profile.rs:169-278` | username, avatar (native picker on Android / file input elsewhere), Remove avatar, inline error line, Software & updates section |
| 11 | Software & updates | `profile::SoftwareSection` `app/src/profile.rs:306-353` | "Kal vX", "Check for updates", "Check at startup" checkbox, status line, "Apply update now" when ready |
| 12 | Sync panel (sidebar section) | `sync_ui::SyncPanel` `app/src/sync_ui.rs:255-421` | profile header (avatar/initials + name, opens settings); NotPaired: "Start sync chain" + join-by-pasted-phrase; ShowPhrase: 24-word grid modal + "I wrote it down"; Paired: fingerprint, "Show recovery phrase", "Sync now"/"Syncing…" with last-round status line, "Leave sync chain…" two-step confirm |
| 13 | Desktop extras | `mini_widget.rs`, rfd dialogs | desktop-only (`cfg` gated); **not on Android** |

**No auth/onboarding gate exists.** The app opens straight into the calendar.
Sync-chain pairing is the closest analog (an opt-in setup flow inside the
sidebar), and it has no token/session semantics.

## 0.5 Shell and layout

- Shell: `TopBar` (48px, bg-alt), body flex = Sidebar + Content. Desktop sidebar
  pushes with 170–480px drag resize; mobile sidebar is a **fixed overlay drawer**
  (280px, translateX slide 0.22s, scrim at 42% black below topbar), collapsed by
  default (`UiLayout.mobile = cfg!(android||ios)`, `app/src/main.rs:306-315`,
  CSS `app/assets/main.css:37-59`).
- Breakpoints: `@media (max-width: 900px)` hides inline prefs controls +
  subtitle, shows the gear toggling a prefs drawer (`main.css:377-382`).
- Nav layout: Mobile-first; no bottom tabs; hamburger is the single nav entry.
- Modals: shared `.modal-backdrop`/`.modal` look for Editor and Settings
  (`main.css:345-373`), `role=dialog aria-modal` a11y labels.
- No toasts anywhere. Errors surface as inline red text inside modals
  (`#c0392b`, e.g. `profile.rs:264-266`, `sync_ui.rs:411-417`) and as `eprintln!`
  logs elsewhere (import/export failures).
- Desktop-only window dressing (frameless, custom drag) is `cfg`-gated away
  from Android.

## 0.6 Global state (every shared store)

| Store | Type | Writes | Reads | Default |
|---|---|---|---|---|
| `DbHandle` (`Arc<Database>`) context | `use_context_provider` `main.rs:284` | all upsert call sites | everything | `open_db()` once (Android: `<files>/kal/calendar.db` via JNI `getFilesDir`, `main.rs:75-130`) |
| `calendars_res` resource + context | `use_resource` `main.rs:295-299` | restart after mutations/merge/seed | sidebar, editor, item colors | `list_calendars()` |
| `items_res` resource + context | `main.rs:360-364` | restart after mutations/merge/seed | all views | `list_items(false)` |
| `UiLayout` signal (sidebar_open/width/resizing, prefs_drawer_open, theme, mobile) | context signal `main.rs:307-315`, struct `ui.rs:17-29` | TopBar/sidebar handlers, theme toggle | root, TopBar, Sidebar | mobile=true on Android, sidebar closed, width 230, theme from settings |
| `ViewMode` signal | `main.rs:319` | nav buttons | Content | Month (startup always Month; persisted default_view unused) |
| cursor `NaiveDate` signal | `main.rs:322` | ViewNav handlers | all views | today |
| `Settings` signal | `main.rs:323-324`, struct `ui.rs:33-93` | prefs controls + save | all views (fmt_time etc.) | `Settings::load` → theme dark, 24h, Monday-first, auto_check_updates=true |
| `SchedulerHandle` (ThreadScheduler) context | `main.rs:367-373` | reconcile effect `main.rs:442-458` | — | `NullNotifier` on Android |
| `EDITOR_OPEN` GlobalSignal | `ui.rs:623` | chips/cells/new buttons, editor save/cancel | root render | None |
| `SETTINGS_SCREEN_OPEN`, `PROFILE_EDIT`, `PROFILE_VIEW`, `SCREEN_ERROR` globals | `profile.rs:127-133` | settings screen, upload | SyncPanel header, settings | false / default / default / "" |
| `UPDATE_STATUS Optional<String>`, `UPDATE_READY bool` globals | `updater.rs:12-14` | `run_check/apply_now` | SoftwareSection | None / false |
| `RESOURCES_DIRTY` GlobalSignal | `sync_ui.rs:126` | merge completions (manual + poller) | root `use_effect` `main.rs:388-394` | 0 |
| `SYNC_DRIVER_DIRTY` AtomicUsize | `sync_ui.rs:130` | background sync driver thread (OS thread — cannot touch signals) | 250 ms on-runtime poller `main.rs:401-411` | 0 |

Dioxus-runtime-type usage is **entirely inside `app/`**; `grep dioxus crates/`
returns nothing (kal-* crates are runtime-free by construction — the guide's
biggest Phase-0 hazard does not apply).

## 0.7 Async work (every fetch, task, timer, retry)

| Task | Trigger | Notes |
|---|---|---|
| Updater background check | `use_effect` once at startup if `auto_check_updates` (`main.rs:328-333`) + manual button; `spawn`; blocking ureq inside async | GitHub `/releases/latest`, 4 MB cap, sha256 verified; download 200 MB cap; no retries |
| "Sync now" round | button → `spawn_blocking` (`sync_ui.rs:362-397`) | live first (≤15 s peer wait), folder fallback; disabled button while running; result line |
| Background sync driver | daemon thread every 10 s, idempotent `start_background_sync` (`sync_ui.rs:44-92`) | fresh DB connection per round (`open_db()`), sets `SYNC_DRIVER_DIRTY` |
| Live transport build | once per chain on startup effect (`main.rs:345-356`) | dedicated thread because `IrohTransport::connect` does `Runtime::block_on` (`sync_live.rs:95-129`); DHT record publish + gossip join |
| Live round drain window | `sync_live.rs:203-239` | 12 s window, 250 ms poll, 2 s quiesce after burst, 3 s silent early-exit, self-broadcast skipped |
| Reminder reconcile | effect keyed on `items_res.len()` (`main.rs:442-458`) | `compute_firings(items, now, 14d)` → `ThreadScheduler.reschedule` |
| SYNC_DRIVER_DIRTY poller | 250 ms `use_future` loop (`main.rs:402-411`) | converts atomic counter into signal restarts |
| Avatar pick | background thread + channel; 30 s blocking JNI wait (`android_picker.rs`) | exceptions cleared on cancel/timeout |
| Mini-widget 5-min tick | desktop only | `mini_widget.rs:33-39` |
| Web IndexedDB restore | once, guarded by `loaded` signal (`main.rs:420-435`) | web-only |
| Widgets | `updatePeriodMillis` + `BOOT_COMPLETED` receiver | periodic only, no push (`scripts/stage-widgets.sh`, D51) |

**No rate-limit logic, no retry loops, no debounce/dedupe constants anywhere.**

## 0.8 Boot-time work (Android path)

1. `sync_log::init_trace_log()` — tracing subscriber to `<files>/kal/sync-trace.log`,
   truncated per start (`app/src/sync_log.rs`); on-device via
   `adb shell run-as com.kal.calendar cat files/kal/sync-trace.log`.
2. `dioxus::launch(App)` (`main.rs:269-275`). (Desktop additionally does
   `updater::apply_staged_update()` first — not on Android.)
3. `open_db()` → `getFilesDir` JNI lookup cached in `OnceLock` (`main.rs:75-130`).
4. `ensure_default_calendars` resource: seed well-known ULID calendars
   "Personal" (`#3366cc`) + "Birthdays" (`#e91e63`) **only on empty DB** —
   deterministic IDs so sync unions don't duplicate (`main.rs:161-194`).
5. `Settings::load` (invalid theme self-heals) + `load_profile` into globals.
6. `UiLayout`/ViewMode/cursor/Settings providers seeded.
7. Effects: auto update check, live transport + background sync driver, seed-
   then-reload of calendars/items, reminder reconcile.
8. **Watch-item (known bug investigation, `TODO.md` §After convergence §3):**
   a fresh Android install reportedly **auto-created `sync-identity.json` +
   `sync-node.json` without user action**. Intended onboarding or accidental
   auto-join must be settled before the Kotlin equivalent is built.

## 0.9 Networking and auth

- **Endpoints (all native, none in Kotlin today):** GitHub API
  `api.github.com/repos/MihneaMoso/kal/releases/latest` + asset download
  (`app/src/updater.rs:79,150`); Google Calendar API behind
  `kal-import::google` RFC 8628 device flow + REST v3 mapping — **library
  only, no `google` cargo feature enabled by `app/`, no UI surface**
  (`crates/kal-import/Cargo.toml:8` feature opt-in, `RULES.md` Phase 5c note).
- **P2P network:** iroh gossip + distributed-topic-tracker DHT + n0 relay
  (`crates/kal-sync/src/live.rs`); topic derived from chain fingerprint;
  envelopes are AEAD-encrypted; relays see ciphertext only.
- **Token handling:** none (no accounts; the Google device-flow code is inert).
  The **sync chain phrase is the secret**: persisted at
  `<dbdir>/sync-identity.json` 0600 (`sync_ui.rs:94-111`), shown once on
  screen, re-revealable on demand (`sync_ui.rs:340-348`).
- All network stays native; the Kotlin side does zero networking (existing
  narrow exceptions: none — widgets/FS/pickers never touch the network).

## 0.10 Persistence (formats + fallbacks)

| Asset | Path | Format | Notes |
|---|---|---|---|
| Calendar DB | `<files>/kal/calendar.db` (Android; `~/.local/share/kal/…` desktop) | SQLite, bundled rusqlite | migrations v1–v3+ append-only `MIGRATIONS` `native.rs`; WAL files accompany (`TODO.md` pull procedure); soft deletes only |
| Settings | `settings` table keys `preferences` (Settings JSON), `theme` (plain string), `profile` (UserProfile JSON: username, avatar_mime, avatar_b64) | JSON/plain | legacy quoted-value tolerance (`ui.rs:64-76`) |
| Sync chain identity | `<dbdir>/sync-identity.json` 0600 | `{"phrase": "24 words"}` | |
| iroh node identity | `<dbdir>/sync-node.json` 0600 | `{node_key hex, device_id ULID}` | device-owned, never chain-deleted |
| Folder gossip outbox | `<dbdir>/sync-outbox/` | `.kalblob` sealed blobs | fallback transport; deleted only by "Leave sync chain" |
| Staged update | `<files>/kal/updates/kal-update.apk` | APK | served by `KalFileProvider` (`content://com.kal.calendar.updates/apk`); desktop analog `kal.new` + `swap.marker` |
| ICS export | `<files>/kal/Kal-export.ics` | RFC 5545 + X-KAL-* | served via provider as `content://…/ics` for the share sheet |
| Trace log | `<files>/kal/sync-trace.log` | text | truncated each start |
| Keychain/keystore | **none** — no secure storage is used for anything |

## 0.11 Web content and JS interop

- There is **no embedded web page content**: no login/checkout/OAuth webview,
  no hidden helper page. The single WebView is the *whole UI* (the Dioxus
  mobile renderer) and disappears entirely in the rewrite.
- JS interop inventory: exactly one `document::eval` (day-view scroll,
  `ui.rs:421-428`), CSS injected via `style { dangerous_inner_html }`
  (`main.rs:498`), and the WebView file-input workaround listed in 0.2.
- Custom protocols/handlers: none (`KalFileProvider` is Android-level, not web).

## 0.12 Media and background work

- No audio, video, camera, or location use.
- Must-keep-working-when-backgrounded today: nothing on Android beyond what
  the OS allows a plain activity (no foreground service, no alarms, no media
  session). The 10 s sync driver and reminder threads die with the process.
- Home-screen widgets (2) are Out-of-process-from-UI Kotlin:
  `ScheduleWidgetProvider` (95 LOC), `MonthWidgetProvider` (~490),
  `BootReceiver` (25), shared loader `KalWidgets.kt` (loads `libkal_jni.so`) and
  JSON mapper `KalWidgetsJson.kt`, layouts `android/app/src/main/res/layout/*`.
  Tap → `MainActivity` with the widget date extra; the Kotlin app consumes
  `kal_widget_add_date` to open the add editor on the tapped day (§8.9, D53).

## 0.13 Platform-specific code (everything gated on Android)

| Item | File |
|---|---|
| `android_files_dir` + JNI `getFilesDir` | `app/src/main.rs:75-130`, `app/src/updater.rs:240-265` (duplicate copy) |
| Scheduler = `ThreadScheduler<NullNotifier>` | `app/src/main.rs:32-33, 370` — **Android reminders are silent by design** |
| Overlay drawer mobile flag | `main.rs:306, ui.rs:24-29` + CSS |
| Native pickers / share | `app/src/android_picker.rs`, `android/updater/src/main/kotlin/com/kal/calendar/KalFilePicker.kt`, `KalShare.kt` |
| Android install intent | `updater.rs:367-402` + `KalUpdater.kt` |
| Widget JNI | `app/src/widget_ffi.rs` + `android/widget/...` |
| `MainActivity` (WryActivity + onActivityResult) | `android/MainActivity.kt` |
| Deps | `app/Cargo.toml:46-53` |

## 0.14 Native dependencies

- C/C++: **none of our own.** Build scripts: only `app/build.rs` (version
  injection). `rusqlite` bundles SQLite (C, cross-compiles today).
- TLS: `ureq` w/ rustls (cross-compiles); iroh stack (quinn/QUIC).
- No codecs; no `openssl`; icons are ImageMagick-generated PNGs
  (`scripts/apply-icons.sh` from `assets/logo.jpeg`).

## 0.15 Design tokens and assets (port 1:1)

From `app/assets/main.css` (single coherent sheet, 437 lines):

- Light palette: `--bg #ffffff`, `--bg-alt #f5f6f8`, `--fg #1b1f24`,
  `--fg-muted #6a737d`, `--accent #3366cc`, `--accent-soft #e8effc`,
  `--border #d8dee4`, `--today #eef4ff`; dark: `#16191d / #1f2329 / #e6e8eb /
  #9aa4ae / #6c99f0 / #24304a / #33393f / #22304a`.
- Motion: sidebar 0.15–0.22s ease, buttons 0.12s, `prefers-reduced-motion`
  kills transitions; focus-visible outlines = 2px accent.
- Geometry: radii 5/6/8/12px, 48px topbar & hour rows, buttons 13px text.
- Editing-specific HTML inputs (`<input type=date/time>`) map to platform
  pickers in Kotlin.
- Fonts: `system-ui`; no custom fonts. Icons: hamburger/options glyphs
  (`\u{2630}`, `\u{2699}`, `‹ ›`).
- Launcher/adaptive icons: `assets/icons/*` staged by `apply-icons.sh`.

## 0.16 Copy and strings

- Localized keys live in `app/i18n/en-US/main.ftl` (flat FTL, 40+ keys) via
  `i18n::tr` with visible `<key>` fallback (`app/src/i18n.rs`).
- Many strings are still hardcoded in rsx: sidebar headings ("Calendars",
  "Widget", "Import / Export", "New item"), "+ Event/+ Task/+ Birthday",
  editor labels ("Remind me", "Custom reminder (minutes before)", "Location
  (optional)", "Person", …), editor titles ("New event"/"Edit event"/…),
  repeats ("Doesn't repeat"…"Annually"), preset labels ("10 min"…"1 week"),
  modal copy ("Write these 24 words down…", "Leave the sync chain?",
  "Delete this item?", "Export calendar" intent title), all settings-screen
  copy, updater statuses ("Update ready: vX — apply to restart", "You're up to
  date (vX)", "Package installer opened — confirm to finish",
  "Update check failed: …"), agenda empty text, ViewNav labels
  ("Agenda — next 30 days"). The inventory for the Kotlin side must include
  both the FTL keys and these literals.

## 0.17 File/asset paths moved where (summary)

Stays native (as-is): storage crate, models/viewmodel/reminders, sync crate,
import crate, updater check/stage logic, `kal-ffi` C ABI.
Moves to Kotlin: everything in `app/src` UI plus `android/` Kotlin (picker,
share, updater intent, FileProvider, widgets — last three transfer verbatim
per guide §4.1/§11), icons, CSS tokens, strings, manifest.

---

## 0.18 Route → Kotlin screen table (guide §7)

| Current screen | Kotlin screen | Data source (bridge) | Notes |
|---|---|---|---|
| Shell (TopBar/Sidebar/nav) | `MainActivity` + single-activity Compose shell | `kalFfi` session-free; day/month expansions on demand | hamburger drawer same breakpoints; no auth gate exists |
| Month view | `MonthScreen` (+VM) | `kal_upcoming/day JSON` / extended `kal_ffi` month-grid call (existing `kal_month_grid_json` can serve the app too) | grid + chips from occurrence expansion |
| Week view | `WeekScreen` (+VM) | items-in-range + expansion | rows identical |
| Day view | `DayScreen` (+VM) | expansion for the day | auto-scroll to 08:00 = `scrollToItem(480px)` equivalent; no JS |
| Agenda view | `AgendaScreen` (+VM) | expansion 30-day window, cap 100 | |
| Editor modal | `EditorBottomSheet/Dialog` + VM | `get_item`, `upsert_item`, list calendars, scoped-edit command | scoped edit (This/…and following/All) must ride the bridge as one transactional call in `kal-ffi` |
| Settings modal | `SettingsScreen` + VM | get/set settings, get/set profile (+avatar bytes via SAF picker), check/download update, apply intent | native picker replaces `KalFilePicker` JNI (Kotlin owns intents); magic-byte/4 MB validation can move to Kotlin or stay native — see Decision C |
| Sync panel | `SyncPanel` composable + VM | `syncStatus / startChain / joinChain(phrase) / revealPhrase / syncNow / leaveChain` events from a new bridge section | background driver + live transport stay native; progress via §6.3-style events or polling a status counter |
| Software & updates | part of SettingsScreen | `check_updates() -> status`, staged APK path | installer flow = Kotlin-owned `KalUpdater.installApk` verbatim |
| Widgets | unchanged Kotlin (`android/widget/…`) | same `kal-ffi` via a dedicated cdylib | Decision D on library name |

---

## 0.19 §4 boundary tables (filled)

### 0.19.1 Moves to Kotlin

| Current responsibility | Kotlin owner | Notes |
|---|---|---|
| All screens/shell/inline prefs (0.4, 0.5) | Compose screens + VMs + shell | tokens 1:1 from `main.css` |
| Editor modal incl. scoped-recurring edit UX | Editor screen + VM | save logic native; dialog affordances native |
| Avatar/file picking (JNI workaround) | SAF `ActivityResultContracts` | deletes `android_picker.rs`, `KalFilePicker.kt`, `MainActivity.onActivityResult` |
| ICS export share sheet | `KalShare.kt` verbatim | Rust still writes the .ics file |
| APK self-install | `KalUpdater.kt` + `KalFileProvider.kt` verbatim; provider entry into owned manifest | |
| Home-screen widgets | existing `android/widget/…` verbatim | swap `System.loadLibrary("main")` for the core cdylib (Decision D) |
| Launcher icons / manifest / gradle | owned project | kill dx template regeneration (D44 workaround dies) |
| Notifications (Decision A pending) | NotificationManager / AlarmManager | currently Android shows none |
| Theme, a11y attributes | Compose theme + contentDescription | FTL copy + literals |

### 0.19.2 Stays native

| Service | Why | Bridge surface it needs |
|---|---|---|
| SQLite persistence + migrations + poison-row policy | formats in the field | `open(path) → handle` + list/get/upsert/soft-delete/settings CRUD (extend `kal-ffi`, today's 5 exports are widget-only) |
| Models, occurrence expansion (`viewmodel`), scoped-edit application | recurrence correctness | month/day/agenda/upcoming JSON calls |
| Reminder firing computation | shared with desktop | `compute_firings` stays in core; scheduler backend Decision A |
| Sync chain: CRDT merge, BIP39 keys, AEAD, live iroh + folder fallback, background driver | outage-debugged | `startChain / joinPhrase / revealPhrase / syncNow / statusEvents / leaveChain` |
| `.ics` import/export + Google import library | round-trip fidelity | `exportAll(path) / importIcs(bytes)` |
| Updater check/download/hash-verify | release semantics | `checkUpdates → info`, `downloadUpdate → ready` events; install is Kotlin |
| Dirs resolution | JNI once in core instead of two app copies | core opens DB by path; Kotlin supplies `filesDir` once |

### 0.19.3 Section-8 embedded web content (Kal)

**N/A.** No embedded/hosted web pages, no login page, no script interfaces,
no cookie jar. (The single dioxus-hosted WebView renders the UI itself and is
deleted, not migrated.)

---

## Open questions (need the human owner)

**ANSWERED (owner, 2026-10-16):**

1. **Notifications parity target → LAND THEM** during the rewrite:
   AlarmManager/NotificationManager backend in `kal-notify` (matching D19's
   intended design). New manifest permissions limited to what this strictly
   requires (`POST_NOTIFICATIONS`, `SCHEDULE_EXACT_ALARM` or
   `USE_EXACT_ALARM`) — listed for §9.7 approval, per guide.
2. **Signing key → STABLE KEYSTORE NOW** (in CI secrets) plus a monotonic
   `versionCode`, so the first Kotlin release can upgrade-in-place over a
   Dioxus release. Until then the current `assembleDebug` release APKs
   (`ci.yml:122`, `release.yml:116`) have no upgrade path.
3. **Fresh-install auto-created identity → BUG.** Spec for the Kotlin app:
   **no chain identity exists until the user explicitly starts or joins a
   chain** — nothing auto-pairs, no silent random-chain join. The equivalent
   fix for the current Dioxus build should be specified/landed so the parity
   baseline converges.
4. **Background sync → STRICT PARITY NOW**: driver + iroh runtime live exactly
   as long as the process. No foreground service, no WorkManager in this
   migration (post-parity work).

**STILL OPEN (round-1 list — all six items below were answered in round 2;
see "Decisions recorded — round 2" at the end of this document):**
5. **`versionCode` derivation.** Gradle template defaults are in use; define
   the monotonic scheme (e.g. CI run number) for the owned project.
6. **ABI matrix.** CI builds only `aarch64-linux-android`; `check-android.sh`
   supports armv7/i686/x86_64 locally. Ship arm64-only first (matching today)?
7. **minSdk 30 vs the `-android21` linker wrappers** in `check-android.sh`:
   the app is *configured* min 30; confirm floor stays 30 for the Kotlin app.
8. **Terms/upgrade compatibility check** — confirm the app id must remain
   `com.kal.calendar` (file paths, provider authority `com.kal.calendar.updates`,
   release asset naming `kal-<version>-android.apk` all depend on it). Any
   change breaks reading old data (guide §17 makes formats fixed, but the id
   itself is also load-bearing).
9. **Google Calendar import**: library code exists (`kal-import::google`) but
   the app builds without the `google` feature and has no UI. Confirm it stays
   out of the Kotlin migration scope entirely.
10. **Baseline measurement device**: the adb toolchain is present (NDK 25.2,
    TODO.md shows device workflows) but no device is attached right now —
    provide/attach a device or emulator for the cold-start/idle-CPU/RSS
    baseline before Phase 8 comparisons can be planned.

## Decision required (per guide §4.3 / §6 / §9.6)

| ID | Decision | Default recommendation | Where it bites |
|---|---|---|---|
| A | Android notifications land during the rewrite or stay silent | **Land them** (AlarmManager-scheduled `ThreadScheduler` backend in kal-notify; matches D19's intended design, gives desktop-parity reminders). Requires `POST_NOTIFICATIONS` + `SCHEDULE_EXACT_ALARM`/`USE_EXACT_ALARM` review and owner sign-off (guide §9.7 permits exactly the mechanisms' permission needs). | Phase 4 — **owner confirmed 2026-10-16** |
| B | Bridge binding approach (hand-written JNI/extern "C" vs UniFFI) | **Hand-written** — surface is small (~20–30 functions; `kal-ffi` style already exists, D31/D32 precedent), JSON over C strings, panic-guarded | Phase 0/1 |
| C | Where avatar validation (magic bytes, 4 MB cap) lives | Keep validation logic ported **as-is into Kotlin VM** (pure function, native keeps formats), OR keep native behind `setProfileAvatar(bytes) → result`. Default: move the pure check to Kotlin so bytes cross JNI once, mirroring guide §6.4. | Phase 1/6 |
| D | Core shared-library name & loading | Build one cdylib (e.g. `libkal_ffi.so` from `kal-ffi` + a thin app bridge module) that both the app and widgets load, replacing today's ad-hoc reuse of `libmain.so` (`KalWidgets.kt:13`). **Note:** `app` links `kal-ffi` and re-exports in-process today; in the cdylib world widgets can no longer call through the UI crate. | Phase 0 (cdylib target) & Phase 13 (keep rules) |
| E | Media/audio | N/A — Kal has none; drop §9 of the guide. | — |
| F | `Settings/Editor` "date/time" input mapping | `MaterialDatePicker`/`TimePicker` styled to tokens; confirm no exotic custom pickers are wanted | Phase 3 |
| G | Panic strategy | Keep `catch_unwind` per-export (already the `kal-ffi` pattern); verify release profile (`panic` defaults to unwind; `codegen-units=1` `lto=thin` is fine) — no `abort` dependency found in workspace `Cargo.toml`. No decision needed unless owner objects. | Phase 0 |

## Risks (table per §16, Kal-specific rows added)

| Risk | Likelihood / impact | Mitigation |
|---|---|---|
| JNI threading/lifecycle bugs (classic class) | High / fatal | §12 checklist; `KalFilePicker` 30 s blocking-wait pattern must go (Kotlin owns pickers entirely); rotation/backgrounding torture Phase 8 |
| Dioxus runtime types on bridge threads | **Low / fatal** | already structurally impossible (`grep dioxus crates/` = 0); keep CI grep gate when bridge grows |
| Rust panic through the boundary | Low / fatal (already mitigated) | `kal-ffi` `catch_unwind`/NULL pattern is precedent; extend to every new export + panic-at-boundary test |
| **Sync identity/phrase survives only as plaintext JSON 0600** | Ongoing / high-if-broken | Kotlin must keep the same file, same format, same perms (§17); do not move to EncryptedSharedPreferences without a documented migration |
| Sign-in… none; **pairing phrase handling** | Medium / blocking | phrase reveal/new/join flows retested manually each release (same discipline as §6 login matrix) |
| Behavior drift in timing-sensitive sync (10 s driver cadence, 12 s drain, 15 s peer wait, 250 ms poll) | Medium / high | constants carried over verbatim into the (unchanged) native driver — Kotlin never re-times the loop |
| Third-party/provider drift | Ongoing / medium | n/a — no provider pages; GitHub API + iroh stack unchanged in core |
| Scope creep / redesign | Medium / schedule | parity-first gate per screen vs current APK |
| ABI fragmentation | Low / medium | arm64-only first (today's CI reality) |
| R8/ProGuard stripping bridge symbols | Medium / fatal | keep rules for `KalUpdater`, `KalFileProvider`, `KalWidgets`+providers, JNI-callback interfaces; verified in release build |
| **CI releases debug-signed APKs** — an upgrade path back to the old build is currently unilateral | Medium / high | Decision 2 above; stable keystore + `versionCode` before cutover |
| Widget coupling to `libmain.so` | Medium / fatal-if-forgotten | Decision D: cdylib rename must update `KalWidgets.kt` + keep rules; compile widgets against the same library during Phase 0 |
| Disk/build-budget rule (builds must stay in `~/kal`, ≤1.4G target steady-state) | Medium / process | new Kotlin/Gradle build outputs land in the repo sandbox; budget targets summarized in `RULES.md` must be updated when jniLibs land |
| TODO.md's uncommitted background-sync work & index.html rule | Low / process | review/uncommitted-state documented in TODO.md §Commit; do not touch `web/site/index.html` |

## What explicitly does not change (guide §17, Kal commitments from Step 0)

- The **sync-chain security model**: passphrase-derived keys, ciphertext-only
  transport, fingerprint display/revocation semantics, phrase as the only
  secret, 0600 plaintext JSON storage format and file locations.
- The **storage model**: `<db>/<dir>/kal/calendar.db` SQLite schema,
  migrations append-only, `upsert_*`-only writes, tombstone deletes,
  poison-row tolerance, deterministic ULID seed of default calendars.
- The **settings/profile document formats** (legacy quoted-value tolerance
  included): Kotlin must read what the old app wrote and vice versa.
- **CRDT merge semantics and every sync constant** (window/drain/interval).
- **Recurrence and scoped-edit semantics** (D15) and reminder firing rules
  (D20: no missed-firing catch-up).
- **Update semantics** (check-once-per-process, strictly-newer rule, staged
  APK path, provider authority) — minus the Dioxus artifact build path.
- **Release asset naming** (`kal-<version>-android.apk`), ICS `X-KAL-*`
  extensions, widget refresh contract (periodic + boot).
- **Development discipline**: fmt/clippy `-D warnings`/tests on 3 OSes; tests
  for every calibration; decode once, test once (`RULES.md` checklist).
- Commit identity rules (`RULES.md` git-author constraint) and the no-cleaning/
  no-launching personal env constraints.

## Baseline measurements vs. Phase 8 (guide §14 step 0 + §16)

Not yet measured — list explicitly so the numbers can be attached before
Phase 0 planning:

- Cold start to first interactive paint (current APK v0.1.9)
- Idle CPU after 10 min with app foreground/background
- RSS after 10 min of use
- Net request counts for: calendar open (expect 0 — local), update check (1 +
  1 download), sync round.
- **Procedure**: `scripts/build-android-install.sh` builds the release APK;
  device needed (`adb devices` is currently empty). TODO.md §"gotchas" shows
  the pull-the-DB/trace commands that were used last time on a real device.

## Phase 0 — versioned bridge surface (implemented, awaiting gate sign-off)

Per the guide's phase discipline this adds the versioned C ABI surface and
nothing else: no interface logic moves, no existing target changes behavior,
and no Kotlin exists yet.

### What landed

- `crates/kal-ffi` **is** the bridge (already `rlib` + `cdylib` + `staticlib`,
  so no new crate is needed). New, additive surface:
  - `kal_bridge_version()` / `KAL_BRIDGE_VERSION` (= 1) — the handshake Kotlin
    performs before any other call; additive-only within a major version.
  - `kal_last_error()` — per-thread error channel. Queries signal failure with
    NULL, commands with a negative status; both set an actionable message.
  - `kal_calendars_json`, `kal_items_json(include_deleted)`, `kal_item_json`,
    `kal_upsert_item_json`, `kal_soft_delete_item`, `kal_upsert_calendar_json`,
    `kal_get_setting`, `kal_set_setting`, `kal_settings_json`. Structured data
    is JSON text built from the core's own models (`serde`), so nothing is
    mapped lossily at the boundary.
  - Absent data is **not** an error: `kal_item_json` / `kal_get_setting` return
    the JSON literal `null`; only genuine failures touch the error channel.
  - Every export is `catch_unwind`-guarded: a panic becomes NULL/-1 plus a
    message, never an unwinding panic across the JNI boundary.
- The two existing widget view functions (`kal_upcoming_json`,
  `kal_month_grid_json`) are unchanged except for carrying error text instead of
  `()`; their JSON output is byte-identical (verified by diff), and that shape is
  now frozen by exact key-set assertions in `crates/kal-ffi/tests/bridge.rs`.
- `scripts/check-bridge-abi.sh` — the C-side half of the guide §13 signature
  gate: every function declared in `widgets/kal_ffi.h` must be exported by the
  built shared library, and the header's `KAL_BRIDGE_VERSION` must match the
  compiled constant. Wired into CI (Linux) and runnable locally.
- CI: added a `git grep dioxus -- crates/` gate (guide §5.3 / §12.3 — no
  Dioxus runtime may be reachable from bridge/JNI threads).
- Kept in sync: `widgets/kal_ffi.h`, `ARCHITECTURE.md`, and
  `scripts/check-android.sh` (now also cross-builds the bridge cdylib).

### Gate results

| Check | Result |
|---|---|
| `cargo fmt -p kal-ffi -- --check` | pass |
| `cargo clippy -p kal-ffi --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | pass (17 suites, incl. 14 new bridge tests) |
| `git grep dioxus -- crates/` | no matches |
| `scripts/check-bridge-abi.sh` | 16 declared symbols present, version 1 |
| `scripts/check-android.sh --build` (aarch64) | pass — app + `libkal_ffi.so` (ELF ARM aarch64, Android 21, NDK r25c) |
| `cargo check -p kal-app --target wasm32-unknown-unknown` | pass (web unchanged) |
| Desktop | `cargo test --workspace` builds and runs the app host binary (29 tests) |
| `cargo fmt --all -- --check` (whole repo) | **pre-existing failure** — `app/src/{main,sync_ui,ui}.rs` are unformatted on HEAD |
| `cargo clippy --workspace --all-targets -- -D warnings` | **pre-existing failure** — `crates/kal-sync/examples/sync_probe.rs`, 2 unused-assignment lints |

Both failures reproduce on HEAD with `kal-ffi` untouched (`cargo clippy -p
kal-sync` alone fails; restoring the three app files reproduces the fmt diff),
so neither is a Phase 0 regression.

### Deliberately not in Phase 0

- Scoped-edit transaction, sync-panel calls, updater check/download,
  import/export and profile-document handling — the rest of the surface lands
  with the phase that migrates each screen (signatures are designed from this
  inventory, not up front).
- No device verification: `adb devices` is still empty, so on-hardware parity
  and the baseline table above remain unproven.

### Open (owner decision)

1. The two pre-existing gate failures above — fix in a tiny separate commit, or
   leave for the owner?
2. The remaining Step 0 open questions still stand: `versionCode` formula, ABI
   matrix (recommend arm64-only), minSdk 30 confirmation, app-id retention
   confirmation, Google-import scope (recommend out), and a device for the
   baseline numbers.
3. Phase 1 (Gradle/Kotlin project + version handshake + one static screen) is
   gated on an explicit go.

## Decisions recorded — round 2 (owner, 2026-10-16, decisions tool)

These supersede the round-1 "STILL OPEN" list above.

| # | Question | Decision |
|---|---|---|
| 1 | Phase 1 start | **Go — full Phase 1 scope**, held until the baseline device is attached (#11). |
| 2 | Pre-existing red gates | **Fix both** (fmt on `app/src/{main,sync_ui,ui}.rs`; clippy on `kal-sync/examples/sync_probe.rs`) — plus the two `kal-app` lints that only surfaced once `kal-sync` stopped aborting the run (`sync_ui.rs:268`). Landed; CI gate green. |
| 3 | Binding approach | **Hand-written JNI** over the versioned `kal-ffi` exports; `scripts/check-bridge-abi.sh` enforces the C side, the Kotlin-declaration check arrives in Phase 1. |
| 4 | ABI matrix | **arm64-v8a only.** |
| 5 | minSdk | **Stays 30** (the `-android21` triples in `check-android.sh` are just NDK wrapper names). |
| 6 | App id | **Stays `com.kal.calendar`** — upgrade-in-place, file paths and the provider authority depend on it. |
| 7 | versionCode | **`major*1_000_000 + minor*1_000 + patch`**, derived from the release tag (`v0.1.9` → 1009). |
| 8 | Keystore | **Owner generates + uploads; the agent documents and wires.** Runbook: `SIGNING.md`. |
| 9 | Google import | **In scope for the rewrite** — scope note below. |
| 10 | Q3 identity fix | **Land on the Dioxus build** — correction below. |
| 11 | Baselines | **Measure the current build before Phase 1**; blocked on a device. |
| 12 | Notification permissions | **Approved:** `POST_NOTIFICATIONS` + `SCHEDULE_EXACT_ALARM`/`USE_EXACT_ALARM`, nothing else. |

### Correction to Q3 (fresh-install sync identity)

Q3 assumed the Dioxus app mints a chain identity on a fresh install. Tracing
every writer says otherwise:

- `sync_ui::load_identity()` never creates anything, and boot only *reads* it
  (`main.rs:348`).
- The only writer of `sync-identity.json` is `sync_ui::persist()`, reachable
  solely from the explicit **Start sync chain** button and the **Join chain…**
  flow (`sync_ui.rs:270`, `sync_ui.rs:282`).
- `sync-node.json` (per-device iroh identity, not a chain join) is written by
  `NodeCfg::load_or_create()`, called only from `live_transport()`, which boot
  reaches only when a chain identity already exists.

Both files reappearing "without user action" after a reinstall is therefore most
likely **not** application code: no `allowBackup` / `dataExtractionRules` is
declared in-repo (the manifest is dx-generated, so this needs checking in a
built APK), which leaves Android Auto Backup on by default and restoring the app
files dir; and `adb install -r` preserves data (the local helper's `adb
uninstall` is what wipes it). Closing this needs an on-device reproduction.

What was landed instead of a speculative behavior change:

- `app/src/sync_ui.rs` — `load_identity_at(&Path)` (pure refactor; the app still
  calls `load_identity()`), plus two tests:
  `fresh_install_has_no_chain_identity_and_creates_no_files` (a fresh install has
  no identity and *reading* creates no file) and
  `stored_phrase_is_read_back_from_the_documented_path_and_format` (the
  `sync-identity.json` path + JSON shape the Kotlin app must read and write,
  §17). These pin the parity baseline the owner asked for.

### Google Calendar import — scope note (decided #9)

`crates/kal-import/src/google.rs` is **scaffolding only**: DTOs, mapping
(`map_event`, `map_calendars`) and the OAuth endpoint constants. It is **not wired
into the app** (the app only calls `kal_import::import_ics` / `export_all`; the
`ureq` dependency is optional and the feature is off). No UI, no token storage,
no auth flow exists today.

So "in scope" is **not** a parity item — parity has nothing to compare against.
It is a **new feature** needing its own spec and owner input:

- The core already carries the **device-code** endpoints
  (`GOOGLE_DEVICE_CODE_URL`, `GOOGLE_TOKEN_URL`) — the flow that needs no
  embedded WebView and no custom-scheme redirect (guide §8 red flag avoided).
- It needs an OAuth client the owner creates in Google Cloud Console
  ("TVs and Limited Input devices"), read-only `calendar.readonly`.
- It needs a refresh-token storage decision (plain JSON 0600 like
  `sync-identity.json`, D40-style, vs `EncryptedSharedPreferences`).

Recommendation: run it **after** the parity cutover as its own phase with a
dedicated review, so it cannot destabilise the parity gates.

### Gate status after the round-2 fixes

`cargo fmt --all -- --check` clean · `cargo clippy --workspace --all-targets -D
warnings` clean · `cargo test --workspace` pass (all suites; two new identity
tests) · `scripts/check-bridge-abi.sh` pass · aarch64 Android + wasm cross-builds
pass.

Also landed: `.gitignore` now excludes `*.jks` / `*.keystore` / `*.jks.b64` (it
did not, while `SIGNING.md` instructs the owner to create exactly those files).

### Next action

Device attached (motorola edge 30 / Android 14); baselines measured (table
below). All Step 0 questions are now closed, so Phase 1 is ready to start on the
owner's go.

## Decisions recorded — round 3 (owner, 2026-10-16)

| # | Question | Decision |
|---|---|---|
| 15 | Google import timing | **Own phase after the parity cutover** — parity gates stay clean; its own spec and review. |
| 16 | Google auth mechanism | **OAuth device-code flow** (no embedded WebView, no custom-scheme redirect); owner creates a "TVs and Limited Input devices" client in Google Cloud Console for read-only `calendar.readonly`. |
| 17 | Google token storage | **Plain JSON 0600 beside the DB**, matching D40's sync-identity model. |
| 18 | Auto Backup of secrets | **Exclude `sync-identity.json` + `sync-node.json` from Android Auto Backup** in the Kotlin app — today they are implicitly backed up (no `allowBackup` is declared in-repo; the manifest is dx-generated), so the Kotlin app must declare the exclusion explicitly and this is a deliberate divergence. |
| 19 | Signing transition | **Accept the one-time uninstall + ICS export/import** (`SIGNING.md` §5); every release after that upgrades in place. |
| 20 | Local install script | **`adb uninstall` becomes opt-in via `--wipe`**; the default is an in-place install that preserves data — **landed**. |
| 21 | Baseline device | **Provided**: motorola edge 30 / Android 14. Measurements below. |

Also landed this round:

- `scripts/measure-android-baseline.sh` — the reproducible baseline procedure
  (cold start ×3; foreground-idle RSS/PSS/CPU/network samples at a fixed
  interval; `--quick` for a smoke run). The doc previously listed baselines as
  "not yet measured" without a procedure to run them.
- `scripts/build-android-install.sh` — installs in place by default
  (`adb install -r`), so local installs stop destroying app data and stop hiding
  signature mismatches; on failure it now points at `--wipe` and `SIGNING.md`
  instead of silently wiping.

## Baseline — current Dioxus build (measured 2026-10-16)

Device: motorola edge 30, Android 14. Installed build: `versionName=0.1.0`,
`versionCode=1`, `minSdk=24`/`targetSdk=34` (dx defaults, matching the
"`versionCode` is unmanaged" finding in `SIGNING.md` §2).

Measured with `scripts/measure-android-baseline.sh` (full run 2026-10-16 23:04:
3 cold-start runs, then 10 foreground-idle samples every 60 s).

| Metric | Baseline (Dioxus build) |
|---|---|
| Cold start — force-stop, then `am start -W` | 694 / 604 / 633 ms (`TotalTime`, warm page cache) |
| Foreground-idle RSS | 217 MB → 114 MB (settles down across the 10 min) |
| Foreground-idle PSS | 96 MB → 65 MB |
| Foreground-idle CPU | 3.4 % on the first sample, then 0.5–1.2 % |
| Network during foreground idle | 0 bytes, every sample |

These are the numbers the Kotlin app has to match (guide §14/§16). An earlier
smoke run also recorded 1714 ms for a first launch after a long idle; the full
run does not reproduce that because the device is not rooted and the page cache
cannot be dropped.

## Phase 1 — owned Kotlin project, JNI bridge, one static screen (implemented, awaiting gate sign-off)

Per the guide's phase discipline this is the smallest end-to-end slice: an owned
Gradle project, the native library linked and handshake-checked, and a single
static screen whose every value comes back over JNI. No parity screen is ported
yet and the Dioxus app is untouched.

### What landed

- **Owned Gradle project** at `android/` (root `settings.gradle.kts` + module
  `:app` under `android/app/`). It replaces the dx-generated project, which lives
  under `target/dx/…` and is wiped on every build. Versions are pinned in
  `android/build.gradle.kts`: AGP 8.7.0, Kotlin 2.0.20 (with the Kotlin 2.x
  Compose compiler plugin), Gradle 9.1.0 via the committed wrapper. The
  Step 0 decisions are all in effect: `applicationId com.kal.calendar`,
  `minSdk 30`, `compileSdk`/`targetSdk 34`, `abiFilters = arm64-v8a`.
- **Versioning**: `versionCode = major*1_000_000 + minor*1_000 + patch`,
  `versionName` from `-PkalVersionName=` (default `0.2.0`) — this build is
  `versionCode=2000`/`versionName=0.2.0`, i.e. an upgrade path over the installed
  `versionCode=1`.
- **Auto Backup exclusions** (decision #18) are declared, not deferred:
  `res/xml/backup_rules.xml` (API ≤ 30) and `res/xml/data_extraction_rules.xml`
  (API 31+) exclude `kal/sync-identity.json` and `kal/sync-node.json` from cloud
  backup *and* device transfer, while the calendar database keeps being backed up.
- **No permissions** are declared in Phase 1; nothing needs one yet.
- **`crates/kal-jni`** — the JNI layer, the only place that translates JVM↔C
  types. Six exports (`bridgeVersion`, `lastError`, `open`, `close`,
  `calendarsJson`, `settingsJson`), each `catch_unwind`-guarded so no panic
  unwinds into the JVM, each freeing the native string it copies. All real work
  stays in `kal-ffi`.
- **Kotlin**: `com.kal.calendar.bridge.KalNative` (the `external fun`
  declarations), `KalBridge` (handshake + open + JSON read), `MainActivity`
  (single-activity Compose host), `ui/SettingsScreen`, `ui/GateScreens`
  (bridge-mismatch and startup-error gates), `ui/KalTheme`.
- **Handshake gate**: `KalBridge.EXPECTED_BRIDGE_VERSION = 1` is compared with
  `KalNative.bridgeVersion()` **before any other native call**; a mismatch, or a
  library that cannot be loaded at all, shows a blocking non-dismissable screen
  with the two versions. The database path is `<filesDir>/kal/calendar.db` — the
  same file the Dioxus app and the widgets use, so this reads real data.
- **`scripts/android-env.sh`** — NDK discovery and linker wiring extracted into
  one sourced helper; `scripts/check-android.sh` now sources it (no drift) and
  also covers `kal-jni`.
- **`scripts/build-kotlin-android.sh`** — cross-builds `kal-jni` for aarch64,
  stages `libkal_jni.so` into `android/app/src/main/jniLibs/arm64-v8a`, then
  verifies with `nm` that every `external fun` in `KalNative.kt` has a matching
  exported `Java_com_kal_calendar_bridge_KalNative_*` symbol. `--apk` also runs
  `:app:assembleDebug`.

**Library-name deviation (owner sign-off requested).** Decision D said "one
cdylib (e.g. `libkal_ffi.so`) that both the app and widgets load". Phase 1 ships
**`libkal_jni.so`**: the JNI layer statically links `kal-ffi`, so the single
library carries both the `Java_*` entry points and the `kal_*` C ABI. The
widgets keep loading `libmain.so` until their own migration phase, at which point
they load this same library. The reason for the split is that `kal-ffi` must stay
a platform-neutral C ABI crate (iOS consumes it), so the Android/JNI glue does
not live inside it.

### Gate results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **124 pass / 0 fail** (+2 in `kal-jni`: export presence, pinned version) |
| `scripts/check-bridge-abi.sh` | 16 declared symbols present, version 1 |
| `scripts/check-android.sh` (aarch64) | pass — `kal-app` + `kal-ffi` + `kal-jni` |
| `scripts/build-kotlin-android.sh` | staged `libkal_jni.so`; 6/6 JNI symbols matched to the Kotlin declarations |
| `./gradlew :app:assembleDebug` | **BUILD SUCCESSFUL** (37 s) |
| APK identity | `com.kal.calendar`, `versionCode=2000`, `versionName=0.2.0`, `minSdk=30`, `targetSdk=34`, `native-code: arm64-v8a` only |
| APK contents | contains `lib/arm64-v8a/libkal_jni.so`; launchable activity `com.kal.calendar.MainActivity` |
| APK signing | debug certificate (`CN=Android Debug`) |

### Deliberately not in Phase 1

- Nothing is installed or launched: the owner chose **build-only**, so the
  handshake, the JNI calls and the Settings readout are proven to compile, link
  and package — **not to run on hardware**. The guide's Phase 1 exit ("one static
  screen rendered from JNI data") is therefore half-met: built, not observed.
- The Settings screen is read-only and unstyled; editing, the shell/navigation,
  the token table and every other screen come later.
- R8 keep rules for the JNI entry points land with the release wiring
  (`SIGNING.md` §4); release builds are unminified until then. AGP could not
  strip the staged `.so` (`Unable to strip … libkal_jni.so`), so release
  packaging will need `strip` handled on the cargo side.
- The widget and updater trees still load `libmain.so`.

### Open (owner)

1. **Install the Phase 1 APK?** It is the only way to observe the handshake and
   the native readout on hardware. The owner picked build-only for this gate.
2. **`libkal_jni.so` name** — accept the deviation above, or have the crate emit
   `libkal_ffi.so`?
3. **Disk**: `target/` is now 4.6 G (`target/debug` 3.4 G, `target/aarch64-linux-android`
   1.2 G) against the 1.4 G steady-state budget in `RULES.md`; the Kotlin outputs
   add `android/app/build` 96 M. Run `scripts/tidy.sh` and/or update the budget
   line?

## Phase 2 — the shell and the four read-only views (implemented, installed 2026-10-17)

Owner-approved scope (2026-10-17 round 4): **shell + read-only views**, verified by
**build + JVM unit tests**, keeping the `libkal_jni.so` name. The owner then asked
for it on the phone; it was installed in place over the Dioxus build the same day
(see *Device verification* for what that run did and did not prove).

This is the first slice that navigates and looks like the real app: the top bar
with its drawer and preferences, the calendar sidebar, the ‹ Today › nav strip
and the Month/Week/Day/Agenda views, every value read over JNI and every string
and geometric value taken from the Dioxus implementation. The editor, settings
screen, sync and import/export keep their real labels and open a dialog saying
which phase they land in — no behaviour is faked.

### What landed — bridge (`kal-ffi`, `kal-jni`)

- **Four app-facing view exports**, beside the frozen widget ones:
  `kal_month_json`, `kal_week_json`, `kal_day_json`, `kal_agenda_json`. The
  widget payloads (`kal_month_grid_json`, `kal_upcoming_json`) keep their exact
  shape — the Phase 0 freeze assertions still pass — because the app needs more
  than a widget ever did: which calendar an item belongs to (so the client can
  apply the visibility filter exactly where the Dioxus shell did), the item kind
  (birthday age chips), a raw start instant (so times are formatted from *this
  device's* 12/24-hour preference) and the day view's layout fractions.
- **Shared display entry** every view parses:
  `{id, calendarId, title, start, end, allDay, kind, completed, color, overrideColor, age}`.
  `color` is the effective colour (item override, else calendar) used by widget
  chips and day blocks; `overrideColor` is the item's own, which is *all* the
  month/week chips ever used — the two rules really did differ in the Rust UI,
  so both cross.
- `kal_month_grid_json` was refactored onto two shared helpers
  (`first_weekday`, `month_occurrences`) so the widget and app payloads cannot
  drift; its output is unchanged (proven by the frozen tests).
- **`crates/kal-jni` grew from 6 to 13 exports**: the four view payloads, plus
  `upsertCalendarJson` (the sidebar visibility toggle),
  `getSetting`/`setSetting` (preferences and theme).
- New Rust tests: 9 (payload shapes, day split/positioning, week window, agenda
  order + cap + empty window, birthday age, task completion, override-vs-effective
  colour, malformed dates, and the calendar visibility round-trip the sidebar
  toggle depends on). `widgets/kal_ffi.h` documents all of it.

### What landed — Kotlin (`android/app`)

- `bridge/Payloads.kt` — the wire models and *pure* parsers for all five
  payloads, plus `Visibility` (hidden-calendar filtering, the Kotlin equivalent
  of `use_visible_items`) and `decodeText` (the `kal_get_setting` reply, including
  the legacy JSON-quoted theme the old toggle could leave behind).
- `data/Preferences.kt` — the `preferences` object + the separate `theme` key,
  defaults and normalisation as the Dioxus loader had them.
- `bridge/KalBridge.kt` — `BridgeStatus` + a `KalSession` that owns the handle
  and exposes typed queries/mutations; a closed session returns empty results
  instead of touching a dead pointer.
- `ui/Formats.kt`, `ui/CalendarNav.kt` — every date/time string and all nav
  arithmetic, ported 1:1 (see the parity notes below).
- `ui/ShellState.kt` — the shell's state over one session: cursor, view mode,
  preferences, calendars and the four payloads; visibility toggles and preference
  writes persist and reload what they invalidate.
- `ui/KalTheme.kt` — the real token table (light + dark), the geometry and the
  type scale, replacing the Phase 1 placeholder.
- `ui/Shell.kt` — `header.topbar` (hamburger, title, subtitle, prefs, gear) with
  the stylesheet's own 900px breakpoint deciding inline controls vs the gear
  drawer, the fading scrim + sliding 280px sidebar, the prefs card, the calendar
  rows, and the sync/import-export/new-item sections.
- `ui/ViewNav.kt`, `ui/MonthView.kt`, `ui/WeekView.kt`, `ui/DayView.kt`,
  `ui/AgendaView.kt`, `ui/Components.kt` (chip, item row, dot, buttons, the
  "later phase" dialog).
- Phase 1's read-only `SettingsScreen` was removed: the shell now shows the real
  data it existed to prove.
- **Launcher icon**: the module shipped none (the Phase 1 APK had no
  `android:icon`, so it showed the generic placeholder). The branded art the
  Dioxus build showed is now staged from the committed
  `assets/icons/android/` set — per-density `mipmap-*/ic_launcher.png` plus the
  `mipmap-anydpi-v26` adaptive icon — mirroring what `scripts/apply-icons.sh`
  did for the dx project.

### Parity quirks preserved on purpose

These are the details a "clean" rewrite would silently change:

- The month chip's colour uses the item's override or the accent — **never** the
  calendar's colour — while dots and day blocks do fall back to it.
- The month chip's time comes from the item's start, its birthday age from the
  occurrence's date; `inMonth` compares the month number only, not the year.
- Today's highlight beats out-of-month greying.
- The week view is **always Monday-anchored**, even for a Sunday-first user
  (only the month grid honours the preference) — and monthly ± steps clamp
  (`checked_add_months`), Agenda shares the month step although its title is
  static.
- The day block's own time label is always `HH:MM`, ignoring the 12/24-hour
  preference, and the auto-scroll is a literal 480 CSS px (≈ `8*60`), once per
  entry to the view.
- The agenda window is `today … today+30` inclusive, capped at 100 rows **after**
  the full scan, so the cap drops the latest overflow.
- Empty week days show `—`; the agenda empty state says `Nothing in the next 30
  days.`; birthdays append ` · age`.
- The task checkbox shows real completion state and strikes the title through,
  but cannot be toggled yet (that is an item upsert — the editor phase).
- Copy is pinned to English (`Locale.ENGLISH`) exactly as chrono's `%A`/`%B`
  were, so a differently-localised device does not silently change the UI.

### Device verification (in-place install, 2026-10-17)

Installed over the existing Dioxus app on the owner's phone (motorola edge 30,
Android 14) with `adb install -r` — **no uninstall, no data loss**.

| Fact | Value |
|---|---|
| Signing certificate | identical to the installed build (`CN=Android Debug`, SHA-256 `c7ede5aa…89b6`) — both are Gradle debug builds from the same keystore, so the in-place upgrade was legitimate and `--wipe` was not needed |
| `versionCode` | installed `1` → new `2000` (upgrade accepted) |
| `firstInstallTime` | unchanged (`2026-09-04`) — proves it was an **update**, not a reinstall |
| Database | `files/kal/calendar.db` — exactly the path `KalBridge.databasePath()` computes; the 10 calendars and 50 items survived |
| Safety net | the DB was copied off-device before installing |
| Widgets | the old build registered `ScheduleWidgetProvider`/`MonthWidgetProvider`; this APK does not carry them, and no widget *instance* was placed, so nothing visibly broke (widgets keep `libmain.so` until their own phase) |
| Launched? | installed only — the owner opened it |

**Two real bugs the device data caught**, neither visible to the unit tests as
they were written:

1. **`sourceLabel` matched the wrong wire spelling.** `CalendarSource` is
   `#[serde(rename_all = "snake_case")]`, so the payload says `google_import` /
   `ics_import`, while the Kotlin matched `googleimport` / `icsimport` — every
   imported calendar would have rendered with **no** source label. The test
   fixture was written in PascalCase, which is exactly why the test passed. Fixed;
   fixtures moved to the real values; the wire spelling is now pinned from the
   Rust side by `app_calendar_payload_round_trips_through_upsert`.
2. **The visibility round-trip was untested.** `CalendarModel.patchedVisible`
   sends the payload's own calendar JSON back through
   `kal_upsert_calendar_json`, which only works if that payload is valid `upsert`
   input. It is — now proven rather than assumed.

The same inspection recorded two **pre-existing** conditions of the owner's data
(Dioxus-app history, not something this rewrite introduced):

- The `calendars` table holds **five duplicate Personal/Birthdays pairs** — the
  historical seeding bug that migration `v4` repairs. `user_version` is already
  `4`, so that repair has run and will not run again; the duplicates were created
  *after* it by the older installed build and remain. The sidebar hides them (it
  lists only visible calendars, exactly as Dioxus did) and one item (`Gaga`,
  2026-09-19) sits on an invisible duplicate, so it is hidden in every view. Note
  also that `v4` keeps `MIN(id)` per name — the *oldest*, here the **invisible**
  row — so had it run it would have emptied the sidebar. A `v6` that re-runs the
  dedup and merges `visible` across the merged rows would fix both; it is
  upstream work, out of Phase 2 scope, and reported rather than done.
- **All 50 items are dated 2026-08-18 … 2026-09-26** (44 all-day events, no tasks,
  no birthdays, no colour overrides). Today is 2026-10-17, so the current month
  and the whole agenda window are legitimately empty — ‹ reaches the populated
  months. Nothing to fix, but it must not be mistaken for a rendering failure.

### Gate results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **133 pass / 0 fail** |
| `cargo test -p kal-ffi` | 16 pass (9 new app-facing tests) |
| `scripts/check-bridge-abi.sh` | **20 declared symbols** present, version 1 |
| `scripts/check-android.sh` (aarch64) | pass — `kal-app` + `kal-ffi` + `kal-jni` |
| `scripts/build-kotlin-android.sh` | staged; **13/13** JNI symbols matched to Kotlin declarations |
| `./gradlew :app:testDebugUnitTest` | **36 tests, 0 failures** (parsers, visibility, formats, nav, preferences) |
| `./gradlew :app:assembleDebug` | BUILD SUCCESSFUL |
| APK identity | `com.kal.calendar`, `versionCode=2000`, `versionName=0.2.0`, `targetSdk=34`, `native-code: arm64-v8a`, launcher `MainActivity`, adaptive `@mipmap/ic_launcher` |
| Device install | `adb install -r` succeeded; `firstInstallTime` unchanged; `calendar.db` intact |
| CI | `kotlin-build` job added (JDK 17 + NDK 25.2.9519653 + `build-kotlin-android.sh` + `:app:testDebugUnitTest :app:assembleDebug`), so the Kotlin half is gated on every push |

The unit tests need a real `org.json` (`testImplementation("org.json:json")`)
because `android.jar`'s copy is a throwing stub — the parsers themselves are
plain Kotlin with no Android dependency.

### Deliberately not in Phase 2

- **The editor-phase behaviour is still unproven on screen**: the drawer slide, the
  day view's scroll position and the chip wrapping have been type-checked and
  packaged, and the shell now renders on the owner's device — but interaction
  parity for the controls that exist is the owner's read from the install, not a
  measured claim.
- **Editor** (create/edit/delete, scoped edit, pickers), **Settings** (profile,
  avatar, software/updates), **sync panel**, **ICS import/export** and the **task
  toggle**: real labels, `NotPorted` dialog. Their bridge calls
  (`kal_get_item`, `kal_upsert_item_json`, …) exist in `kal-ffi` but have no JNI
  wrapper yet.
- The sidebar is always the overlay drawer: the Dioxus Android build was
  `cfg!(target_os = "android")`-mobile, so there was no in-flow resizable desktop
  sidebar to port. The top bar's 900px breakpoint *is* reproduced.
- Widgets/updater still load `libmain.so`; release signing, R8 keep rules and
  `.so` stripping stay with `SIGNING.md` §4.

### Open (owner)

1. **The device look is with the owner** (installed 2026-10-17). What the install
   already proves: the in-place upgrade path, the handshake, the DB open, the shell
   rendering against the real database, and the visibility toggle's round-trip.
   What it cannot prove yet: the **editor-phase** paths (create/edit/delete,
   pickers), the task toggle, sync and ICS — their controls say so when tapped.
2. **Upstream DB hygiene** (found on the device, not fixed here): the historical
   duplicate-calendar seeding left five Personal/Birthdays pairs whose ids live in
   the owner's DB; migration `v4` is not re-runnable and keeps `MIN(id)`, i.e. the
   invisible duplicate, so a `v6` that dedupes again and merges `visible` (and
   re-points items) would be the real fix. It is `kal-storage` work outside the
   Kotlin rewrite.
3. **Disk**: the owner will tidy `target/` themselves — 4.6 G against the 1.4 G
   `RULES.md` budget, plus `android/app/build`.
4. **Emulator for later phases?** Device checks currently need the phone; an AVD
   can be set up when a phase wants on-device verification without touching the
   owner's device.

## Phase 3 — the item editor (implemented, installed 2026-10-17)

Owner-approved scope (round 5): **the editor modal and the scoped-recurring edit
UX**, with the save semantics staying native (`§0.19.1`/`§0.19.2`); Material3
picker dialogs instead of typed date/time inputs (decision F); a rejected save
keeping the dialog **open** with the native reason; and the scoped-edit helper
moving into `kal-storage` so both interface layers share one implementation.

This supersedes the "editor-phase behaviour is still unproven" bullets of Phase 2:
the nine `NotPorted` placeholders that meant *"you cannot add or change anything"*
are gone. Every chip, block and row opens the editor anchored on the clicked
**occurrence**, the sidebar's three new-item buttons open it blank, the task
checkbox ticks, and delete is a two-step confirm. Creating an item works in this
app for the first time — and finding that out exposed a real upstream bug (below).

### What landed — bridge (`kal-ffi`, `kal-jni`)

- **`kal_save_item_json(db, form) -> i32`**: one form in, one item written. It owns
  the Dioxus `mk_save` semantics so Kotlin never has to — default-calendar
  resolution (first live+visible → first live → create `Personal`), load-or-create
  by id, the field patch (title, kind, dates, all-day, location, colour, reminders,
  `metadata.birthday_of`), and the scope application (`all` / `this_only` /
  `this_and_following`). A scope with no `occurrenceStart`, and any unknown scope,
  falls through to a plain upsert — the original's catch-all arm.
- **`kal_toggle_item_completed_json(db, id) -> i32`** for the row checkbox: flips
  `completed` and refreshes `updated_at`, mirroring what `ItemRow` did with a
  row-local item clone.
- **`kal_item_json`** (prefill) and **`kal_soft_delete_item`** (delete) gained the
  JNI wrappers they were missing.
- **`Database::apply_scoped_edit`** in `kal-storage`: the recurrence surgery that
  was app-local in `app/src/ui.rs` moved into storage, so the Dioxus app and the
  bridge apply *the same* code instead of two copies (`db_apply_scoped_edit` now
  delegates). This was the plan's single "verify shared" item; the move is
  behaviour-preserving (same writes, same order, same UNTIL/EXDATE rules).
- `crates/kal-jni` grew 13 → 17 exports; `widgets/kal_ffi.h` now declares **22**
  symbols (was 20) and documents both new entry points.
- **12 new Rust tests** in `crates/kal-ffi/tests/bridge.rs`: create from the form,
  all-day + colour + reminder minutes, the birthday linkage, fields the form does
  not show surviving an edit, this-only EXDATE, this-and-following UNTIL, an
  existing UNTIL left alone, bad input reported instead of panicking,
  default-calendar resolution, `Personal` creation, the completion toggle, and an
  end-less task.

### What landed — Kotlin (`android/app`)

- `bridge/Payloads.kt` — `ItemModel` + `parseItem` (the core `CalendarItem`,
  snake_case, including `metadata.birthday_of` and the `minutes_before` reminders;
  an absent item is `null`, never a crash) and `EditorForm`/`EditScope` (the request
  body, camelCase).
- `bridge/KalNative.kt`, `bridge/KalBridge.kt` — 4 new `external fun`s and the 4
  `KalSession` methods over them (`item`, `saveItem`, `toggleItemCompleted`,
  `deleteItem`).
- `ui/EditorState.kt` — the form as plain data: `EditorFields` (per-kind defaults,
  prefill, `toForm(scope)`), `RepeatPreset` (the rrule ↔ preset mapping),
  `REMINDER_PRESETS` with `preset_label`'s captions, `COLOR_PRESETS`.
- `ui/EditorModal.kt` — the dialog: in-tree scrim + card (360dp / 90vw / 88vh,
  radius 12, padding 18, 10dp gap, the stylesheet's own shadow), the original's
  field order and copy, the reminder chips, the colour swatches, the calendar and
  repeat dropdowns, the two-step delete, and the scope buttons (`All events` /
  `…and following` / `This event`) shown **only** for an existing series.
- `ui/ShellState.kt` — `editor`/`editorError`, `newItem`, `editItem`,
  `updateEditor`, `saveEditor`, `deleteEditorItem`, `toggleTask`, and the editor's
  default calendar.
- `ui/Shell.kt`, `ui/MonthView.kt`, `ui/WeekView.kt`, `ui/DayView.kt`,
  `ui/AgendaView.kt`, `ui/Components.kt` — the nine placeholders replaced by real
  opens, the sidebar's three new-item buttons wired, the task checkbox live
  (it consumes its own tap, so ticking never opens the editor).
- **23 new JVM tests**: `bridge/EditorPayloadsTest.kt` (5 — the prefill's
  snake_case fields, the birthday/task shapes, absent-item null, the exact request
  keys, and a blank form's nulls) and `ui/EditorFieldsTest.kt` (18 — defaults per
  kind, occurrence anchoring, duration preservation, the request mapping, the
  preset ↔ rrule map, captions, presets, headings and the wire time/instant
  formats).

### The upstream bug this phase found

`parse_when(date, "")` reads an **empty** end time as *midnight*, so an item with no
end — every task, and any event whose End was cleared — became `end = Some(midnight)`
against a 09:00 start. `validate()` then rejected it as *"end must not be earlier
than start"*, and the Dioxus editor **swallowed that error** (`mk_save` returned
without a word). The observable result, unnoticed upstream: **a task could never be
created in the Dioxus app**, and the Save button simply looked dead.

Two deliberate deviations fix it, both confined to the *new* bridge entry point so
the frozen widget/app payloads and `kal_item_json` are untouched:

1. **An empty `endTime` means "no end" (`None`), not midnight.** `CalendarItem.end`
   is already optional, the views already render a missing end as `"09:00 – "`, and
   tasks/all-day items are the natural end-less cases. Only `kal_save_item_json`
   reads the form, so the Dioxus editor's own behaviour is unchanged.
2. **A rejected save is shown, not swallowed.** `kal_save_item_json` returns `-1`
   with the reason in `kal_last_error` and the Kotlin dialog stays open with the
   message in it (the owner's decision on Q2). An empty title, a malformed date, a
   bad colour or `end < start` now say why.

Parity note the tests pin deliberately: a birthday with an empty Person **saves**.
Upstream wrote `Some(person)` even when the string was empty, and `validate()` only
rejects `None` — so the form reproduces that rather than tightening it, and the item
renders as a plain birth-date entry (the name comes from the title).

### Parity quirks preserved on purpose

- A new item still starts with an explicit `#3366cc` **override**, not `Calendar`.
- A task starts 09:00 with **no** end; an event 09:00–10:00; a birthday is all-day,
  midnight, `FREQ=YEARLY`.
- Prefill anchors on the clicked occurrence and the end keeps the **base duration**
  (`anchor + (end − start)`), so editing one instance of a series neither snaps it
  back onto the series start nor changes how long it lasts.
- Scope: *This event* pushes one EXDATE onto the base and writes the edit as a
  rule-free item; *…and following* shortens the base with `;UNTIL={occ − 1min}`
  even when the rule already has `COUNT=`/`UNTIL=` (a `COUNT` is converted to
  its equivalent `UNTIL`), and the split-off copy inherits the original end so
  it cannot run forever. Both are the shared storage helper.
- No notes field and no custom-RRULE editor — upstream had neither, and notes
  survive an edit because the form never carries them.
- Reminders are `10 / 30 / 60 / 1440 / 10080` minutes with the original captions;
  the custom box adds one positive minute value, dedup/sort/`minutes_before` stay
  native (reminders carry ULIDs).
- Delete is scope-aware: on a recurring item the confirmation offers *This
  event* (one EXDATE / one detached copy), *…and following* (truncate the
  series before the clicked occurrence and tombstone later detached copies), and
  *All events* (the whole series, even when opened on a detached occurrence).
  The delete confirmation replaces the save row, so the same words can never
  run the save/split path by mistake.

### Deliberate substitutions

- **Date and time are pickers** (Material3 `DatePickerDialog` and `TimePicker`),
  not typed inputs — decision F, no new dependency (Compose BOM 2024.09.00,
  material3 1.3.0). The wire time stays `HH:MM`, so the native parser is unchanged.
  The time dialog adds a **Clear** action for End, which is how an event becomes
  end-less; upstream did that by emptying a text input.
- The repeat and calendar dropdowns fill the card width (a content-width `<select>`
  is unusable on a phone); their option lists and order are the original's.
- The card is an in-tree overlay (upstream was an absolutely positioned `div`), with
  a `BackHandler` so the back gesture closes it like a dialog — the one addition.

### Device verification (in-place install, 2026-10-17)

Installed over the Phase 2 build with `adb install -r`. **Not launched** by the
assistant — the owner opens it, as in Phase 2.

| Fact | Value |
|---|---|
| Install result | `Success` (streamed, in place) |
| `firstInstallTime` | unchanged (`2026-09-04 23:23:54`) — an **update**, not a reinstall |
| `lastUpdateTime` | `2026-10-17 01:45:35` (this build) |
| `versionCode` / `versionName` | `2000` / `0.2.0` (unchanged; no ABI bump needed) |
| `primaryCpuAbi` | `arm64-v8a` |
| Database | `files/kal/calendar.db` intact (167936 bytes); the `-wal`/`-shm` mtimes predate the install, so nothing has opened it since |
| Launched? | no — the owner's read is the acceptance step |

### Gate results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **145 pass / 0 fail** (was 133) |
| `cargo test -p kal-ffi` | **28 pass** (12 new editor tests) |
| `scripts/check-bridge-abi.sh` | **22 declared symbols** present, version 1 |
| `scripts/check-android.sh` (aarch64) | pass — `kal-app` + `kal-ffi` + `kal-jni` |
| `scripts/build-kotlin-android.sh` | staged; **17/17** JNI symbols matched to the Kotlin declarations |
| `./gradlew :app:testDebugUnitTest` | **59 tests, 0 failures** (was 36; 23 new) |
| `./gradlew :app:assembleDebug` | BUILD SUCCESSFUL |
| Device install | `adb install -r` succeeded; `firstInstallTime` unchanged; `calendar.db` intact |

### Deliberately not in Phase 3

- **Notifications (Phase 4)** — reminders are *stored* now, nothing fires yet, and
  `POST_NOTIFICATIONS` is still not requested. A saved reminder is inert until the
  notification phase.
- **Settings screen, sync panel, ICS import/export** — still `NotPorted` with their
  real labels (4 sites, down from 9).
- The Dioxus editor's silent-failure *behaviour* is not reproduced on purpose: this
  bridge reports, and the dialog shows it.

### Open (owner)

1. **The device look is with the owner** (installed 2026-10-17). What the install
   proves: the in-place upgrade, the handshake, the DB open and (from Phase 2) the
   shell rendering against the real database. What needs eyeballing: the editor
   itself — the date/time pickers, the keyboard over a centred card, the scope
   buttons wrapping, and that a created task survives a relaunch.
2. **A created item is the real test of the end-time fix.** The owner's data has no
   tasks today (50 items, 2026-08-18 … 2026-09-26), so the fix's happy path has only
   been exercised through the bridge tests; creating a task on the device is the
   first end-to-end use.
3. **Upstream DB hygiene** (Phase 2 finding, still open): the five duplicate
   Personal/Birthdays pairs and the `Gaga` item on an invisible duplicate. A `v6`
   that dedupes again and merges `visible` would fix both; `kal-storage` work,
   outside this rewrite.
4. **Disk**: `target/` + `android/app/build` are the owner's to tidy.

## Phase 4 — reminders (implemented, installed 2026-10-19)

Owner-approved scope (round 6): **land the notifications the Dioxus Android build
never had** (decision A, confirmed 2026-10-16), with the six follow-up choices
answered as recommended:

| # | Question | Answer |
|---|---|---|
| 1 | Where does the Android scheduler live? | **Kotlin schedules, Rust computes** — a new `kal_reminder_firings_json`; `kal-notify` keeps only the desktop backend. Deviates from decision A's literal wording (see below). |
| 2 | Exact-alarm permission | `USE_EXACT_ALARM` (13+, install-granted) + `SCHEDULE_EXACT_ALARM` limited to API ≤ 32, with a graceful fallback to inexact alarms. |
| 3 | `POST_NOTIFICATIONS` prompt | **On app start, once per install** (remembered), so imported/synced reminders are covered too. |
| 4 | Re-arm after reboot | **Yes** — a `BOOT_COMPLETED` receiver. |
| 5 | Notification copy | **Exact desktop parity** — `Kal — {title}` / `{title}` — plus the channel "Reminders" (the one new string). |
| 6 | Small icon | A new monochrome vector `drawable/ic_notification.xml`. |

**Approved deviation from the recorded decision.** Decision A said
"AlarmManager-scheduled `ThreadScheduler` backend in `kal-notify`". Making that
literal would put a `jni` dependency and a `JavaVM` pointer inside the
notification crate and split the platform code across three layers. The owner
approved the split the guide's own boundary tables imply: *firing computation is
logic and stays native* (§0.19.2), *notifications are platform work and move to
Kotlin* (§0.19.1). `kal-notify` is untouched; the desktop `ThreadScheduler` and
its trait remain exactly as they were.

### What landed — bridge (`kal-ffi`, `kal-jni`)

- **`kal_reminder_firings_json(db, horizon_days, cap) -> JSON`**: every firing in
  `[now, now + horizon_days]`, nearest first, capped (`cap <= 0` = uncapped), as
  `[{itemId, reminderId, fireAt, title}]` (camelCase, like the view payloads).
  It is `kal_core::reminders::compute_firings` over `list_items(false)` — i.e.
  **not** visibility-filtered, matching the Dioxus reconcile effect, which read
  the unfiltered items resource. Past firings are dropped (D20), completed tasks
  and tombstoned items are silent, and the core clamps the horizon to 366 days.
- `crates/kal-jni` grew 17 → **18** exports (`reminderFiringsJson`);
  `widgets/kal_ffi.h` declares **23** symbols (was 22).
- **6 new Rust tests** in `crates/kal-ffi/tests/bridge.rs`: nearest-first order +
  identity/title fields, unresolved-instant mismatch, past firings dropped, the
  cap and the uncapped sentinel, completed tasks + deleted items silent, a hidden
  calendar still firing, one firing per recurring instance, and a dead handle
  reporting instead of panicking.

### What landed — Kotlin (`android/app`)

- `bridge/Payloads.kt` — `ReminderFiring` + `parseFirings` (an unreadable payload
  yields *no* firings, so a reconcile never arms something it cannot trust).
- `bridge/KalNative.kt`, `bridge/KalBridge.kt` — the `external fun` and
  `KalSession.reminderFirings()`, with `REMINDER_HORIZON_DAYS = 366` and
  `REMINDER_ALARM_CAP = 64` next to the existing agenda constants.
- `notify/ReminderPlan.kt` — the policy, free of Android: `ReminderFiring.key()`
  (stable per item+reminder+**instant**), `planReminders(armed, firings)` →
  what to cancel and what to (re-)arm, the channel/extra/prefs constants, and the
  notification caption helpers. Arming is idempotent, so the plan re-arms the
  whole wanted set and a reconcile self-heals after a reboot or a force-stop.
- `notify/ReminderAlarms.kt` — the only Android-aware piece: `AlarmManager`
  (`setExactAndAllowWhileIdle`, or `setAndAllowWhileIdle` when exact access is
  denied), the channel, the notification (`Notification.Builder`, no new
  dependency), and the armed-key list in `SharedPreferences`.
- `notify/ReminderReceiver.kt` (shows one notification; the intent carries
  everything, so the app process need not be running) and `notify/BootReceiver.kt`
  (reopens the DB through the same handshake and reconciles).
- `notify/NotificationPermission.kt` — asks for `POST_NOTIFICATIONS` once per
  install, from `onResume` (the flag survives a denial, so it never nags).
- `MainActivity` creates the channel on first launch and asks on resume;
  `ui/ShellState.kt` exposes `reminderFirings` (recomputed in `reload()`, i.e.
  after every mutation); `ui/Shell.kt` reconciles in a `LaunchedEffect` on it and
  on `ON_RESUME`.
- **16 new JVM tests** (75 total, was 59): `bridge/ReminderFiringsTest.kt` (6,
  payload decoding incl. malformed/null) and `notify/ReminderPlanTest.kt` (10,
  cancel/arm/re-arm, key stability across offsets, caption parity, channel ids).

### Reconciling, and how it differs from the Dioxus timer

The Rust UI reconciled in an effect keyed on the **item count** (D21's
acknowledged "cheap approximation" — editing a reminder on an existing item did
not re-arm). Kotlin reconciles in `ShellState.reload()`, which every mutation
funnels through, plus on every foreground. Two deliberate differences, both
because the OS holds the alarms rather than a thread:

- **Horizon 366 days with the nearest 64 armed** instead of a 14-day window.
  Firing *computation* is capped the same way D19 intended ("only next N firings
  armed"); the wider window means a phone that is not opened for a month still
  has its reminders armed.
- **Anything already past is dropped, never caught up** (D20, unchanged).

### Fixed while testing the bridge (a Phase 3 bug)

`kal_soft_delete_item` is *count-shaped* (`1` = a tombstone written, `0` = no such
id, `-1` = error — documented in `widgets/kal_ffi.h` since Phase 0), but the
Phase 3 `KalBridge.deleteItem` tested `== 0`. A successful delete therefore
reported **failure** to the editor (which would have kept the dialog open with a
spurious error while the item really was deleted). Caught by a new Rust test's
expectation, fixed to `>= 0`, and documented on both sides of the bridge. No
Kotlin unit test covers `KalSession` (it is a thin JNI wrapper), so the mismatch
had to be found by reading the contract — it is now stated next to the extern.

### Permissions (guide §9.7 — the whole list)

| Permission | Why |
|---|---|
| `POST_NOTIFICATIONS` | Android 13+ runtime grant for showing a notification at all; asked once. |
| `USE_EXACT_ALARM` | Android 13+: install-granted exact alarms. Reserved for apps whose core function is alarms/calendar — Kal qualifies. No prompt, no Play review while sideloading. |
| `SCHEDULE_EXACT_ALARM` (`maxSdkVersion=32`) | The Android 12/12L gap, where `USE_EXACT_ALARM` does not exist and this one is granted by default. |
| `RECEIVE_BOOT_COMPLETED` | Re-arm after a reboot (`AlarmManager` forgets everything). |

Nothing else was added: no `WAKE_LOCK`, no foreground service, no WorkManager, no
`INTERNET` (that stays with sync).

### New surfaces this phase introduces (owner reviewables)

- **Copy**: the channel name **"Reminders"** (`REMINDER_CHANNEL_NAME`) — the only
  new string; the notification itself is desktop parity. No FTL entry exists
  for it, so it is a Kotlin literal like the rest of the shell.
- **Art**: `res/drawable/ic_notification.xml` — a monochrome calendar glyph
  authored for this phase (the launcher foreground would render as a blob in the
  status bar). Only the name is referenced; replace freely.
- **Behaviour**: tapping a notification opens the activity (no deep link to the
  item — the Dioxus comment left routing "for later"), and there are no
  snooze/action buttons (the original had none either).
- The exact-alarm fallback is a *deliberate* deviation from a strict reading of
  "reminders must be exact": if the user revokes exact-alarm access, the reminder
  still arrives, possibly a few minutes late in Doze, instead of vanishing.

### Device verification (in-place install, 2026-10-19)

Installed over the Phase 3 build with `adb install -r`. **Not launched** by the
assistant — the owner opens it, as before.

| Fact | Value |
|---|---|
| Install result | `Success` (streamed, in place) |
| `firstInstallTime` | unchanged (`2026-09-04 23:23:54`) — an **update**, not a reinstall |
| `lastUpdateTime` | `2026-10-19 19:54:33` (this build) |
| `versionCode` / `versionName` | `2000` / `0.2.0` (unchanged; no bridge bump) |
| `primaryCpuAbi` | `arm64-v8a` |
| Permissions granted at install | `USE_EXACT_ALARM: granted=true` (the design intent — no user flow needed on Android 14), `RECEIVE_BOOT_COMPLETED: granted=true`, `POST_NOTIFICATIONS: granted=false` (the prompt has not run yet; appop `POST_NOTIFICATION: ignore`), `SCHEDULE_EXACT_ALARM` not applicable above API 32 |
| Receivers in the merged manifest | `.notify.ReminderReceiver` (exported=false), `.notify.BootReceiver` (exported=false, `BOOT_COMPLETED` filter) — plus library receivers |
| Database | `files/kal/calendar.db` intact (167936 bytes); WAL mtimes predate the install |
| Armed alarms | none yet, as expected: nothing has reconciled until the app is opened |
| Launched? | no — the owner's read is the acceptance step |

### Gate results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **151 pass / 0 fail** (was 145) |
| `cargo test -p kal-ffi` | **34 pass** (6 new reminder tests) |
| `scripts/check-bridge-abi.sh` | **23 declared symbols** present, version 1 |
| `scripts/check-android.sh` (aarch64) | pass |
| `scripts/build-kotlin-android.sh` | staged; **18/18** JNI symbols matched to the Kotlin declarations |
| `./gradlew :app:testDebugUnitTest` | **75 tests, 0 failures** (was 59; 16 new) |
| `./gradlew :app:assembleDebug` | BUILD SUCCESSFUL |
| APK manifest (`aapt2`) | the four permissions with the right scoping, both receivers `exported=false` |
| Device install | `adb install -r` succeeded; `firstInstallTime` unchanged; `calendar.db` intact |

### Deliberately not in Phase 4

- **Settings screen, sync panel, ICS import/export** — still `NotPorted` (4 sites).
- **A notifications toggle in the UI** — the permission is a system setting; a
  per-app switch belongs to the Settings phase.
- **Deep-link routing on tap, snooze/dismiss actions, a foreground service for
  sync, WorkManager** — none exist upstream either.
- **Widget/updater modules** — `android/widget` and `android/updater` are still
  staged source trees outside `settings.gradle.kts`.

### Open (owner)

1. **The device read is with the owner** (installed 2026-10-19). What the install
   proves: the in-place upgrade, the manifest/permission state, and that
   `USE_EXACT_ALARM` is granted without any prompt. What needs the owner: open the
   app (the first `POST_NOTIFICATIONS` prompt appears once), then
   `adb shell dumpsys alarm | grep com.kal.calendar` should list the armed alarms
   and `adb shell dumpsys notification --noredact | grep reminders` the channel.
2. **A firing has not been observed.** The owner's data ends 2026-09-26 (today is
   2026-10-19), so nothing is scheduled and no notification can arrive until an
   item with a reminder exists in the future. The shortest end-to-end check:
   create an event ~15 minutes ahead with a "10 min" reminder and watch for
   `Kal — {title}`.
3. **Reboot re-arming is unverified** — it needs an actual reboot (or a synthetic
   `BOOT_COMPLETED`), which the assistant does not do. The receiver compiles and
   is registered; the reconcile path it runs is the same one the shell uses.
4. **`USE_EXACT_ALARM` is Play-policy-restricted** (alarm/calendar core function).
   Irrelevant while the APK ships by sideload; a Play release would need the
   declaration justified or the permission swapped for `SCHEDULE_EXACT_ALARM` +
   a settings prompt.
5. **Phase 3's two deviations and the corrected birthday parity note** are still
   awaiting the owner's sign-off (see the Phase 3 section).
6. **Upstream DB hygiene** (Phase 2 finding) and **disk tidy-up** are unchanged.

## Phase 5 — ICS import/export (implemented, installed 2026-10-19)

Owner-approved scope (round 7): the sidebar's two remaining dead buttons, with
four choices answered as recommended:

| # | Question | Answer |
|---|---|---|
| 1 | Which slice is Phase 5? | **ICS import/export** — no new permissions, closes two of the four remaining `NotPorted` sites, and lands the content provider the Software & updates phase also needs. |
| 2 | Upstream reports import/export failures only via `eprintln!` | **Surface them** — the native reason in a red sidebar line, the same deliberate deviation the editor's rejected saves made. |
| 3 | How does an export reach the user? | **FileProvider + share chooser**, byte-for-byte the current behaviour: Rust writes the file, Kotlin owns the `ACTION_SEND` intent. |
| 4 | Import picker filter | **`*/*`**, exactly like the Dioxus Android picker. |

### What landed — bridge (`kal-ffi`, `kal-jni`)

- **`kal_import_ics_json(db, text, calendar_name) -> Int`**: parses the payload
  with `kal_import::import_ics`, stores ONE new calendar (source `IcsImport`,
  visible, named from the file's `X-WR-CALNAME` else `calendar_name`) and upserts
  every parsed item. An item the store rejects is skipped and the rest still
  land — the original's `eprintln!`-and-continue. A parse failure writes nothing
  and returns −1 with the reason on `kal_last_error()`; NULL arguments are
  reported, not dereferenced.
- **`kal_export_ics_file(db, path) -> Int`**: writes
  `kal_import::export_all(list_calendars(), list_items(false))` to `path`. The
  same two calls the Dioxus sidebar made, so the file's contents are unchanged
  (including rows for tombstoned calendars, as upstream). Rust owns the write;
  the interface layer only chooses the path and shares the result (§0.19.1).
- `crates/kal-ffi` now depends on `kal-import` (icalendar 0.17 + chrono-tz; the
  `google`/ureq feature stays off, so no TLS stack enters the Android library).
  `kal-import` was already proven on Android through `kal-app`.
- `crates/kal-jni` grew 18 → **20** exports (`importIcsJson`, `exportIcsFile`);
  `widgets/kal_ffi.h` declares **25** symbols (was 23), bridge version still 1.
- **6 new Rust tests** in `crates/kal-ffi/tests/bridge.rs`: a foreign .ics (with
  `X-WR-CALNAME`, a location and a `VALARM`) becomes a visible `ics_import`
  calendar whose event keeps its instant and its 10-minute reminder; the file
  name wins over the passed fallback and a nameless file uses it; each import
  makes its own calendar (no dedup, like upstream); a parse failure returns −1,
  explains itself, and writes nothing; NULL arguments are rejected; an export
  round-trips into a fresh database; an unwritable path explains itself.

### What landed — Kotlin (`android/app`)

- `KalFileProvider.kt` — the dx-project provider, ported verbatim. It already
  served exactly this file: `content://com.kal.calendar.updates/ics` →
  `filesDir/kal/Kal-export.ics` as `text/calendar` (and it keeps the `apk` route
  for the updater phase). Registered in the owned manifest with
  `exported="false"` + `grantUriPermissions="true"`; no storage permission is
  involved.
- `KalShare.kt` — verbatim apart from its header comment: the Dioxus build called
  it from Rust over JNI, the rewrite calls it from Kotlin, because Kotlin owns
  intents here.
- `bridge/KalNative.kt` / `bridge/KalBridge.kt` — the two `external fun`s and
  `KalSession.importIcs(text, calendarName = "Imported")` /
  `exportIcs(path)`, plus `KalBridge.exportPath(filesDir)`, `EXPORT_FILE` and
  `DEFAULT_ICS_CALENDAR` (the path and the provider's path are pinned to each
  other by a test).
- `data/Ics.kt` — `ICS_PICKER_MIME`, `readIcs(context, uri)` (returns null so the
  UI can say the document could not be read) and `findActivity` (for the
  Activity the verbatim `KalShare` requires).
- `ui/Shell.kt` — the sidebar's two buttons are real:
  `rememberLauncherForActivityResult(OpenDocument)` with `*/*`; the picked
  document is read on `Dispatchers.IO` and imported on the main thread (the same
  thread-policy as every other native call); an export writes the file and, only
  on success, opens the chooser. A failure renders the native reason in
  `KalDanger` under the buttons.
- `ui/SidebarNotices.kt` — the copy (`Import failed: …`, `Export failed: …`,
  `Could not read the selected file`), pure and test-pinned.
- `ui/ShellState.kt` — `sidebarError` (cleared by the next attempt, so it never
  outlives the action) and `importIcs` / `exportIcs` / `sidebarFailed`; a
  successful import reloads through the same `reload()` every mutation uses.
- `ui/KalTheme.kt` — `KalDanger` (`#c0392b`) promoted from the editor's private
  constant, since the sidebar notice uses the stylesheet's error colour too.
- **8 new JVM tests** (`data/IcsTest.kt`) pinning the export path (and that it
  sits beside the database), the provider-matching file name, the `*/*` filter,
  the `"Imported"` fallback, and the three notice strings (including the
  no-reason case, so the UI can never render `Import failed: null`).

### Parity and deliberate differences

- Same import semantics as the Dioxus sidebar: one new calendar per import, no
  deduplication, items upserted individually, failures not fatal.
- Same export contents and the same delivered file path, so the provider, the
  share sheet and the file name are unchanged.
- **Deliberate (approved):** failures are visible. Upstream logged them to
  stderr, which made a rejected import indistinguishable from a dead button.
- **Deliberate (behaviour of SAF vs the old JNI picker):** the picked document is
  read off the main thread, and a document that cannot be read is reported rather
  than imported as empty text. The picker itself (any file, single selection)
  matches.

### Gates (2026-10-19)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **157 pass, 0 fail** (was 151) |
| `cargo test -p kal-ffi --test bridge` | **40 pass** (was 34) |
| `scripts/check-bridge-abi.sh` | **25 symbols**, bridge version 1 |
| `scripts/check-android.sh` | pass — `icalendar`/`chrono-tz` cross-build for aarch64 |
| `scripts/build-kotlin-android.sh` | **20/20** JNI symbols staged |
| `:app:testDebugUnitTest` | **83 tests, 0 failures** (was 75) |
| `:app:assembleDebug` | BUILD SUCCESSFUL |
| `aapt2` manifest check | `KalFileProvider`, authority `com.kal.calendar.updates`, `exported=false`, `grantUriPermissions=true` |

Device: `adb install -r` → Success; `firstInstallTime` unchanged
(2026-09-04 23:23:54), `lastUpdateTime` 2026-10-19 20:10:21; `calendar.db`
intact. The Dioxus build's own `files/kal/Kal-export.ics` (2026-09-05) is still
in place — same path — so the provider serves the old file until the first
export from the new UI. The app was not launched.

### Deliberately not in Phase 5

- **Google Calendar import** — decision 15 puts it after the parity cutover, and
  it is the one part of `kal-import` behind the `google`/ureq feature.
- **ICS subscriptions** (`webcal:`/URL feeds) — not a feature of the current app.
- **A progress dialog or import summary** — upstream showed neither; the new
  calendar appearing in the sidebar is the feedback.
- **Per-item import errors** — skipped items are logged natively, exactly as
  upstream; only a fatal parse/store failure reaches the UI.

### Open (owner)

1. **The device read is with the owner** (installed 2026-10-19, app not launched).
   What the install proves: the in-place upgrade and the provider entry. What
   needs the owner: tap **Export all (.ics)** (the chooser opens; afterwards
   `run-as com.kal.calendar ls -l files/kal/Kal-export.ics` shows a fresh
   timestamp), then tap **Import .ics…**, pick that file, and confirm a second
   "Imported" calendar appears; pick a non-.ics file to see the red
   `Import failed: failed to parse ics: …` line.
2. **The `android/updater/` staging tree** still holds its copies of
   `KalShare.kt` / `KalFileProvider.kt`; the live ones are now in `:app`. The
   updater phase only needs `KalUpdater.kt` from there (`KalFilePicker.kt` dies
   with SAF), but the duplicates should be deleted when that phase lands.
3. **`Kal-export.ics` is not cleaned up** after sharing (neither was it upstream);
   it is overwritten by every export.
4. The Phase 3 deviations and the Phase 4 device read remain open as recorded
   above.

## Phase 6 — settings: profile + Software & updates (implemented, installed 2026-10-19)

Owner-approved scope (round 8): the top bar's gear — the last `NotPorted` site
except Sync — with four choices answered as recommended:

| # | Question | Answer |
|---|---|---|
| 1 | Where do the update check and download run? | **Native, via a new `kal-update` crate** — the guide's §0.19.2 boundary (check/download/hash-verify native, install Kotlin), with the release logic shared with the desktop app instead of duplicated. |
| 2 | Software & updates in this phase? | **Both halves**, as upstream's one screen; needs `INTERNET` + `REQUEST_INSTALL_PACKAGES` (§9.7 sign-off). |
| 3 | Upstream says "Update ready: vX — apply to restart" | **Android wording** — "… — install to update"; on Android the package installer replaces the app rather than restarting it (the only copy divergence). |
| 4 | Startup check | **Parity: on by default** — one check per app start when `auto_check_updates` is set, exactly as the Dioxus shell did. |

### What landed — new crate (`kal-update`)

`app/src/updater.rs` was the only place the release semantics existed, and
`kal-ffi` cannot depend on the Dioxus binary crate. The portable half moved into
`crates/kal-update`: `Transport` (a `ureq` implementation plus a fake for tests
— the same shape `kal-import`'s Google transport uses), `ReleaseInfo`,
`parse_version` / `is_newer`, `pick_asset` (tag → version, asset resolved by
token, `sha256:` digest extracted when GitHub provides one), `latest_release_with`,
`download_with` (size-capped, digest-verified, creates the parent directory) and
`check_with` → `UpToDate` / `Ready` / `Failed`. `app/src/updater.rs` keeps the
status signals, desktop staging/swap and the Android install intent, and now
calls the shared crate, so the desktop behaviour and its tests are unchanged.

### What landed — bridge (`kal-ffi`, `kal-jni`)

- **`kal_check_updates_json(current_version, dest_path) -> JSON`**: one blocking
  pass — query the latest release for the running platform's token, and when it
  is strictly newer, download it to `dest_path` and verify its SHA-256. Returns
  `{"status":"up_to_date"}` / `{"status":"update_ready","version":"…"}` /
  `{"status":"error","message":"…"}`; NULL only for a NULL argument or an
  unsupported platform, with the reason on `kal_last_error()`.
- `current_version` is **passed in** by the interface layer
  (`BuildConfig.VERSION_NAME`), not read from a compiled-in constant: a release
  built with a different `versionName` still compares correctly.
- `kal_check_updates_json`'s core is `pub fn check_updates_json_with(transport,
  token, current, dest)`, so the payload shape is pinned by tests without a
  socket.
- `crates/kal-ffi` depends on `kal-update` (ureq + sha2; the Android library
  already carried a TLS stack through `kal-sync`). `crates/kal-jni` grew 20 → **21**
  exports (`checkUpdatesJson`); `widgets/kal_ffi.h` declares **26** symbols
  (was 25), bridge version still 1.
- **9 new Rust tests**: 12 in `kal-update` (moved `version_ordering` /
  `pick_asset_matches_platform` plus the Android ones, and fake-transport cases:
  a newer release downloads and verifies, an up-to-date check downloads nothing,
  a digest mismatch never writes the file, a network failure and a bad payload
  explain themselves) and 4 in `crates/kal-ffi/tests/bridge.rs` (the three
  payload shapes through the injected transport, and NULL arguments).

### Repaired while landing this phase: the web build no longer compiled

Checking the new crate did not break the wasm target surfaced two breakages the
Android-only gates could not have caught:

- `kal-update` keeps its HTTP transport (`ureq` → ring/rustls/getrandom) behind
  a non-wasm gate, so `kal-app` can depend on it for the version arithmetic
  while the web build still pulls no TLS stack.
- **Pre-existing from Phase 3:** moving the scoped-edit helper into
  `kal-storage` gave it to the SQLite backend only, while `app/src/ui.rs` calls
  `db.apply_scoped_edit(…)` on every target — so `cargo check -p kal-app
  --target wasm32-unknown-unknown` failed with `no method named
  apply_scoped_edit`. The algorithm now lives once in
  `crates/kal-storage/src/scoped.rs` behind a two-method `ScopedStore` seam, and
  both backends delegate to it, so desktop, Android and web share one
  implementation (the Android bridge tests that pin the two scoping cases go
  through it).

The gate table below now includes the wasm check.

### What landed — Kotlin (`android/app`)

- `data/Profile.kt` — the sync-chain profile: the row's serde-compatible JSON
  (`username`, `avatar_mime`, `avatar_b64`, nulls included), `displayName`
  ("You" fallback), `initials`, `avatarBytes()`, `withAvatar` (4 MB cap,
  magic-byte check, `image/png` fallback) and `withoutAvatar`. `java.util.Base64`
  (API 26+, minSdk 30) keeps it pure JVM, so it is unit-testable.
- `data/AvatarPicker.kt` — reads a picked image stopping one byte past the cap,
  so an oversized pick is refused without loading it whole (the Dioxus picker
  read everything and rejected it afterwards).
- `data/UpdateCheck.kt` — `UpdateStatus` (`UpToDate` / `Ready` / `Failed`) and
  its parser (an unrecognised or missing payload is always a failure, never "up
  to date"), plus the status-line and installer-line copy, pure and test-pinned.
- `ui/SettingsModal.kt` — the modal: `Sync-chain profile` (48dp circular avatar
  or initials, "User name" with the "Your name" placeholder, "Upload picture"
  via SAF `GetContent("image/*")`, "Remove" when a picture is set) and
  `Software & updates` ("Kal v…", "Check for updates", "Check at startup",
  status line, "Apply update now" when a release is staged), Cancel/Save.
- `ui/Components.kt` — `ModalCard` (the `.modal` card the editor already
  inlined) and `Field` / `KalTextField`, promoted so the new screen does not
  duplicate them; `KalButton` grew `enabled` (default true, so no caller
  changes) to hold the check button while a check is in flight.
- `ui/ShellState.kt` — `settingsOpen` / `profileEdit` / `profileError` /
  `updateLine` / `updateReady` / `checkingUpdate` and the actions: `openSettings`
  (seeded from the stored profile), `setProfileAvatar`, `clearProfileAvatar`,
  `updateUsername`, `saveProfile` (a refused write keeps the modal open with the
  native reason), `setAutoCheckUpdates` (the same `preferences` row the top bar
  writes), `checkForUpdates(destPath)` (network + download on `Dispatchers.IO`)
  and `finishInstall`.
- `ui/Shell.kt` — the gear opens the modal, the modal renders beside the editor,
  and a `LaunchedEffect(Unit)` runs the startup check when the preference is on.
- `KalUpdater.kt` — ported verbatim: `installApk` fires the PackageInstaller
  intent for `content://com.kal.calendar.updates/apk`, the route
  `KalFileProvider` already served. The interface layer calls it directly now
  that Kotlin owns intents.
- `bridge/KalBridge.kt` — `KalSession.profile()` / `saveProfile()` (a plain
  `profile` settings row: no native profile code), `checkUpdates(current, dest)`
  and `KalBridge.updateApkPath(filesDir)` / `UPDATE_APK_FILE`, pinned by a test
  to the provider's `apk` route.
- Manifest: `INTERNET` + `REQUEST_INSTALL_PACKAGES`, with the permission comment
  updated (the install intent is silently dropped without the latter).
- `build.gradle.kts`: `buildFeatures.buildConfig = true` (AGP 8 no longer
  generates `BuildConfig` by default) for `VERSION_NAME`.
- **23 new JVM tests** (`data/ProfileTest.kt`, `data/UpdateCheckTest.kt`): the
  row's JSON shape and round-trip, an unreadable row degrading to the empty
  profile, display-name and initials rules, every avatar acceptance and refusal
  message, the four image families, removal, an undecodable base64 avatar; the
  four statuses plus every unreadable payload, the three status lines and the
  two installer lines, and the staged-APK path (which must not collide with the
  export file).

### Parity and deliberate differences

- Same screen split, same copy (except the approved "install to update"), same
  fallbacks ("You", `image/png`, "Imported"-style defaults), same startup check.
- **Deliberate (consistent with Phases 3 and 5):** a refused profile write and
  an unreadable picked image are surfaced in the modal; upstream ignored the
  write error and the picker's read failure.
- **Deliberate (protection):** the check button is disabled while a check runs,
  so one tap cannot start two downloads; the status line still only changes when
  the pass finishes, as upstream.
- **Deliberate (Android reality):** the install intent needs the user's
  "install unknown apps" grant; without it the system shows its own prompt. A
  cancelled install keeps the release staged, so "Apply update now" can be
  retried.

### Gates (2026-10-19)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **168 pass, 0 fail** (was 157) |
| `cargo test -p kal-ffi --test bridge` | **44 pass** (was 40) |
| `scripts/check-bridge-abi.sh` | **26 symbols**, bridge version 1 |
| `scripts/check-android.sh` | pass — `kal-update` (ureq/sha2) cross-builds for aarch64 |
| `cargo check -p kal-app --target wasm32-unknown-unknown` | pass (was **broken** since Phase 3 — see above) |
| `scripts/build-kotlin-android.sh` | **21/21** JNI symbols staged |
| `:app:testDebugUnitTest` | **106 tests, 0 failures** (was 83) |
| `:app:assembleDebug` | BUILD SUCCESSFUL |
| `aapt2` manifest check | `INTERNET` + `REQUEST_INSTALL_PACKAGES` present; `versionName=0.2.0`, `versionCode=2000` unchanged |

Device: `adb install -r` → Success; `firstInstallTime` unchanged
(2026-09-04 23:23:54), `lastUpdateTime` 2026-10-19 20:35:27; `calendar.db`
intact (167936 B). The app was not launched.

### Deliberately not in Phase 6

- **Everything sync-side** — the sidebar's `Sync now` panel and its
  `INTERNET`-bearing transport remain the last `NotPorted` site.
- **A profile avatar in the sidebar/sync header** — upstream reads it there in
  the sync section, which is not ported yet.
- **Desktop update staging** — `tar.gz` swap-and-relaunch stays in
  `app/src/updater.rs`; the Android app only ever needs the APK path.
- **`android/updater/`'s duplicate `KalShare.kt` / `KalFileProvider.kt`** — for
  the next cleanup pass; `KalUpdater.kt` is now live in `:app`.

### Open (owner)

1. **The device read is with the owner** (installed 2026-10-19, app not
   launched). What the install proves: the in-place upgrade, the two new
   permissions and the untouched database. What needs the owner: open Settings
   from the gear, set a name, upload a picture (it must appear as a 48dp circle
   and survive **Save** → reopen → a relaunch), then **Check for updates**.
   Note `files/kal/updates/kal-update.apk` still holds a **Dioxus-era APK**
   (2026-10-16) at the same path: if the check reports "up to date" that stale
   file stays untouched and "Apply update now" does not appear (the button only
   shows for a release this build downloaded); pressing it with a genuinely
   newer release re-downloads and replaces the file.
2. `auto_check_updates` is on by default, so the first launch after this install
   makes one GitHub request; the toggle is in the modal.
3. The Phase 3 deviations, the Phase 4 device read and the Phase 5 device read
   remain open as recorded above.

## Corrections after the Phase 6 device read (2026-10-19)

Three defects found from the owner's device read of Phase 6, all fixed, tested
and installed in place. Two were reported by the owner; the third was found
because its tests are time-of-day dependent and happened to go red during the
gate run.

### 1. "All events" deleted the past of a recurring series (pre-existing)

**Symptom (owner):** opening a recurring event's *upcoming* occurrence, changing
its colour and pressing **All events** recoloured the future occurrences but
every past occurrence disappeared (except the ones previously given their own
colour with **This event**).

**Cause:** both editors (`EditorFields.fromItem` in Kotlin,
`EditorState::edit_existing` in Dioxus — so it predates the rewrite) deliberately
anchor the form on the **clicked occurrence**, which is what makes *This event*
and *…and following* land on the right day. The "All events" arm then wrote the
whole form as the base row — including `start` — so the series' start jumped
forward to the clicked occurrence. Every occurrence before that new start was
outside the series and could not be expanded; `This event` copies were separate
rows, which is why they survived. A colour change was just the visible trigger.

**Fix, one implementation for both front ends:**
`kal_storage::scoped::anchored_on_series` (and its `_start` form for callers that
edit a loaded row in place) re-anchors the edited start/end on the series' own
calendar date, keeping the form's wall-clock times and the series' UTC offset.
`kal_save_item_json`'s `all` arm and the Dioxus save closure both use it.
Consequence: the date field of a recurring edit is the clicked occurrence's
date, so a date edit made under **All events** does not move the series — it is
not a way to change the series start (documented on the helper). A missing row
still falls back to a plain save, as before.

**Tests:** `crates/kal-storage/tests/scoped_edit.rs` (3: date/offset kept, times
taken, same-day no-op, absent end stays absent) and
`crates/kal-ffi/tests/bridge.rs::save_form_all_scope_keeps_the_series_on_its_own_start`
(the owner's exact flow: colour change from an occurrence 77 days out keeps the
start, keeps the rule, writes one row, and the first occurrence still renders in
its month).

### 2. "Apply update now" → "app not installed as package appears to be invalid"

**Diagnosis (verified from the device and the published APK):** the staged file
was the **Dioxus-era** build — `com.kal.calendar` `versionCode=1`,
`versionName=0.1.0`, signed by the **CI runner's** debug key
(SHA-256 `3ffe03…`) — while the installed app is the Kotlin debug build
(`versionCode=2000`) signed by this machine's debug key (`c7ede5…`). The
installer refuses on two independent counts: a different signing certificate and
a lower version code. This was already known and written down in SIGNING.md §2:
the release workflow builds the *dx* app (`dx bundle` → `assembleDebug`), never
the Kotlin app, so the release channel has no APK that can replace an installed
Kotlin build. Nothing is wrong with the download path.

**Fixed now:**

- **The app no longer offers an install Android will refuse.**
  `data/InstallCheck.kt` (pure, unit-tested) compares the staged APK's
  `applicationId`, `versionCode` and signing-certificate SHA-256 with the running
  app's, read through `PackageManager` in `KalUpdater`. A staged release that
  cannot replace this install is reported as
  `Update ready: vX — cannot install: <reason>` and **Apply update now** stays
  hidden; the button re-checks before firing the intent. This replaces the
  opaque installer message with the actual reason.
- **Release signing is wired** (`android/app/build.gradle.kts`, SIGNING.md
  §4.2): `signingConfigs.release` reads `android/keystore.properties`
  (gitignored, added) or the documented `KAL_KEYSTORE_FILE` /
  `KAL_KEYSTORE_PASSWORD` / `KAL_KEY_ALIAS` / `KAL_KEY_PASSWORD` env vars, and is
  applied to the release build type only. Without them `assembleRelease` falls
  back to the debug key, so local/CI builds without secrets still work — and
  such an APK can only ever replace a build with that debug key.
- `versionCode` was already the monotonic
  `major*1_000_000 + minor*1_000 + patch` from `-PkalVersionName` (0.2.0 →
  2000), so no change was needed there.

**Still open (owner decision, next section):** which signing identity the app
adopts, and therefore how the release channel starts producing installable APKs.
Until that is answered the guard is the honest behaviour, not the fix.

### 3. A reminder on a midnight-crossing or multi-day event fired once per day

**Found by:** `reminder_firings_are_nearest_first_and_carry_their_item` and
`reminder_firings_drop_the_past_and_honour_the_cap` failing in the gate run.
They seed an event at `now + 2h` with a one-hour duration; after 21:00 local that
event crosses midnight, and the tests — which expect one firing — went red. The
tests were right.

**Cause:** `kal_core::reminders::compute_firings` expanded occurrences with
`viewmodel::expand_occurrences`, whose documented job is to repeat a
**non-recurring** multi-day event on *every grid day it covers*. Fine for the
calendar views, wrong for reminders: the same reminder was resolved once per
covered day, so a 23:00–01:00 event armed two alarms and a week-long event
seven.

**Fix:** `firing_occurrences` — recurring series keep the per-occurrence
expansion; a non-recurring item fires once from its own start (still bounded by
the horizon). This is shared by the Android scheduler and desktop
`kal-notify`. **Tests:** two new cases in `kal-core/src/reminders.rs`
(midnight-crossing and six-day events fire once; a non-recurring item past the
horizon is not armed). Flagged for sign-off as an unrequested fix to Phase 4
behaviour — the alternative was leaving a known duplicate-alarm bug and
time-of-day-flaky tests.

### Gates (2026-10-19, after the corrections)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **174 pass, 0 fail** (was 168) |
| `cargo test -p kal-ffi --test bridge` | **45 pass** (was 44) |
| `scripts/check-bridge-abi.sh` | 26 symbols, bridge version 1 (unchanged) |
| `scripts/check-android.sh` | pass |
| `cargo check -p kal-app --target wasm32-unknown-unknown` | pass |
| `scripts/build-kotlin-android.sh --apk` | **21/21** JNI symbols staged |
| `:app:testDebugUnitTest` | **116 tests, 0 failures** (was 106) |
| `:app:assembleDebug` / `:app:assembleRelease` | BUILD SUCCESSFUL (release falls back to the debug key with no keystore configured) |

Device: `adb install -r` → Success; `firstInstallTime` unchanged
(2026-09-04 23:23:54), `lastUpdateTime` 2026-10-19 21:03:43; `calendar.db`
intact (167936 B). The app was not launched.

### Open (owner decisions)

1. **Signing identity for the release channel.** Two viable paths, both
   requiring the owner to place key material in CI secrets (the agent holds
   none):
   - **Keep this install's identity (no data loss):** put the existing
     `~/.android/debug.keystore` (certificate `c7ede5…`, the one the installed
     app already has) in the CI secrets as `KAL_KEYSTORE_BASE64` et al. The next
     release then installs in place over the current app with no uninstall and
     no data loss. It is an SDK debug key, so it is weaker as an identity than a
     purpose-made keystore — but it is exactly what every build so far has used.
   - **Follow SIGNING.md §3/§5 (one unavoidable break):** generate
     `kal-release.jks`, accept one uninstall/reinstall (ICS export → uninstall →
     install → import; the DB can also be pulled with `run-as` **before** the
     switch, while the installed build is still debuggable).
2. **When the release channel starts shipping the Kotlin app.** The workflow
   still publishes the dx/Dioxus APK. Switching the Android job to
   `scripts/build-kotlin-android.sh` + `:app:assembleRelease` (tag → 
   `-PkalVersionName=${GITHUB_REF_NAME#v}`, asset name unchanged) is the cutover;
   it needs the keystore secrets from (1) and the owner's go-ahead, since it
   changes what every release tag ships. Not done here.
3. The Phase 3 deviations, the Phase 4 device read and the Phase 5 device read
   remain open as recorded above. The Phase 4 read now also covers the
   duplicate-firing fix (a single event crossing midnight must ring once).

## Series-linked occurrences: "This event" stays in the series (owner-approved 2026-10-19)

**Symptom (owner):** changing one repeating event's properties did not reach the
occurrences that had been edited individually; and an individually edited
occurrence lost the *This event* / *All events* buttons and claimed "Doesn't
repeat". The owner's requirement, in their words: *"I should be able to change one
event's properties like color etc with OR without changing its siblings'
properties (my choice), and that shouldn't kick the odd one out."*

**Cause:** `this_only` was implemented as EXDATE + a **brand-new standalone row**.
The copy had no `rrule` and no record of where it came from, so:

- the dialog's repeat control and `offersScopes` (both derived from the item's own
  `rrule`) said "doesn't repeat" and hid the scope buttons;
- the `all` arm wrote only the series row, so an already-edited occurrence was
  never touched;
- deleting a series left its copies behind as orphans.

**Fix — an occurrence keeps a link to its series.** `ItemMetadata` gains
`series_id` (the series row) and `recurrence_id` (the excluded instant). They live
in the existing `metadata_json` column (`#[serde(default)]`, so old rows load
unchanged; no schema migration), travel with the row through sync, and mirror
iCalendar's `RECURRENCE-ID` exception model. The row stays a single item — the
whole view/expansion/reminder/ICS layer is untouched — but it is *one occurrence
of a series*.

| Opened | Dialog shows | **This event** | **All events** |
|---|---|---|---|
| a normal occurrence | the series' rule + 3 buttons (as before) | EXDATE + a linked copy | the series row **and every linked occurrence** |
| an individually edited occurrence | the series' rule + buttons (fixed) | updates that row in place, keeps the link | same as above |

Rules of an "All events" save (`kal_storage::scoped::apply_all_edit`):

- The series row is written from **the series row** (never from the form row), so
  its rule and EXDATEs always survive — the dialog shows the series' rule while an
  occurrence is open, and writing the form verbatim would have turned the
  occurrence into a second series and dropped the exclusions.
- The dialog's shared fields — kind, title, location, calendar, all-day, reminders,
  colour, and a birthday's person — are copied to every linked occurrence, so a
  recolour reaches the odd one out (a birthday's dialog save takes this path too,
  so its metadata is copied as well; the series link in `metadata` never is).
- Each occurrence **keeps its own date** (that is what made it an exception); a
  series-wide time-of-day change moves its clock by the same amount, on its own
  date. The row the dialog was opened on takes the form's date/time verbatim (it
  must not be shifted twice).
- An occurrence is never given a rule: it stays single whatever repeat preset the
  dialog carried.
- `all` on a plain (non-recurring) item behaves exactly as before: the same row,
  re-anchored on itself.
- If the series row is gone, an occurrence that still links to it is saved as a
  plain event and its dead link is cleared.

Other consequences, all owner-approved as defaults:

- **"…and following" is not offered while editing a linked occurrence** (both
  front ends hide the button; the bridge refuses the scope with
  `…and following is not available on an edited occurrence`). Splitting a series
  from a modified instance needs the whole "relink later exceptions" pass and was
  left out of this change.
- **Deleting a series also tombstones its linked occurrences**, so a series delete
  cannot leave copies behind. Deleting one occurrence deletes only it — the series
  keeps its EXDATE, so the excluded instant stays gone (as in Google Calendar).
- **Existing orphans were deliberately not migrated** (owner decision): the live
  database holds ~20 standalone copies of the same event (mostly at the series
  start, in different colours) and four near-identical series whose exclusion lists
  overlap, so only 2–4 rows could be re-linked unambiguously. A heuristic re-link
  would have written guesses into the owner's calendar; copies made before this
  change stay plain events, and new edits behave correctly.
- **ICS round-trip is unchanged**: a linked occurrence exports as its own `VEVENT`
  next to the series with its `EXDATE`, so the *calendar data* is right but the
  link itself is lost on re-import (the occurrence comes back as a single event).
  Proper `RECURRENCE-ID` export/import is the documented follow-up.

**Where:**

- `crates/kal-core/src/models.rs`: `ItemMetadata::{series_id, recurrence_id}`.
- `crates/kal-storage/src/scoped.rs`: `apply_scoped_edit` writes the link on the
  copy (and clears it for a split); new `apply_all_edit`, `save_exception`,
  `delete_with_exceptions`, `copy_shared_fields`; the `ScopedStore` trait gains
  `scoped_list_items` / `scoped_soft_delete_item`. Both backends expose wrappers
  (`Database::{apply_all_edit, save_exception, delete_with_exceptions}`), so the
  Dioxus app and the bridge share one implementation.
- `crates/kal-ffi/src/lib.rs`: the `all` arm calls `apply_all_edit`; `this_only` on
  a linked row calls `save_exception`; `this_and_following` on a linked row is
  refused; `kal_soft_delete_item` cascades to a series' occurrences.
- `android/app`: `Payloads.ItemModel.seriesId`, `EditorFields.exception` +
  `fromItem(seriesRrule)`, `ShellState.editItem` loads the series' rule,
  `EditorModal` hides *…and following* for an occurrence.
- `app/src/ui.rs` (Dioxus, parity): `edit_existing` takes the preset from the
  series, `is_recurring_edit`/`is_exception`, the same three save arms, and delete
  through `delete_with_exceptions`.

**Tests:** `crates/kal-storage/tests/linked_exceptions.rs` (7: link written; a
recolour reaches every occurrence without re-dating it; an All-events save opened
on an occurrence keeps the series' rule and EXDATEs and shifts the other
occurrences' clocks; This event on an occurrence updates it in place; series delete
takes its occurrences; an orphaned occurrence detaches cleanly; a birthday's
person survives the All-events path) and five new `crates/kal-ffi/tests/bridge.rs`
cases over the real form payload
(`save_form_this_only_links_the_copy_to_the_series`,
`…_on_a_linked_occurrence_updates_it_in_place`,
`save_form_all_scope_from_a_linked_occurrence_reaches_every_sibling` — the owner's
exact scenario, `deleting_a_series_removes_its_linked_occurrences`, and
`save_form_all_scope_keeps_a_birthdays_person`). Kotlin: three JVM cases (an
occurrence keeps the series rule and the scope choice; an occurrence whose series
is gone is a plain event again; the payload carries `series_id`).

### Gates (2026-10-19, linked occurrences)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **186 pass, 0 fail** (was 174) |
| `cargo test -p kal-ffi --test bridge` | **50 pass** (was 45) |
| `scripts/check-bridge-abi.sh` | 26 symbols, bridge version 1 (unchanged — no ABI change) |
| `scripts/check-android.sh` | pass |
| `cargo check -p kal-app --target wasm32-unknown-unknown` | pass |
| `scripts/build-kotlin-android.sh --apk` | 21/21 JNI symbols staged |
| `:app:testDebugUnitTest` | **119 tests, 0 failures** (was 116) |
| `:app:assembleDebug` / `:app:assembleRelease` | BUILD SUCCESSFUL (release APK carries the same freshly built `.so`) |

Device: `adb install -r` → Success; `firstInstallTime` unchanged
(2026-09-04 23:23:54), `lastUpdateTime` 2026-10-19 21:36:24; `calendar.db` intact
(167936 B). The app was not launched.

### Device read requested (owner)

1. Open a recurring event's occurrence, pick **This event**, change its colour →
   reopen it: it must still say it repeats and still offer *All events* /
   *This event* (no *…and following*), with its own colour shown.
2. Reopen the series (any occurrence) and recolor it with **All events** → the
   occurrence edited in (1) must change colour too, and it must stay on its own
   date (not jump to the series start).
3. **This event** on an already-edited occurrence → only that one changes; the
   other occurrences keep their colour.
4. Move the series' *time* with **All events** → the individually edited
   occurrence moves by the same amount **on its own date**.
5. Delete the series → its individually edited occurrences are gone too.

### Open (owner sign-off on the deliberate choices)

- Hiding **…and following** while editing an individually edited occurrence.
- Deleting a series also deleting its linked occurrences.
- Existing pre-fix copies left as plain events (no backfill).
- The ICS link loss on export/import (follow-up, not part of this change).


## Phase 7 — Sync panel (implemented 2026-10-21; device install pending)

The last `NotPorted` screen. Owner-approved scope (round 9):

| # | Question | Answer |
|---|---|---|
| 1 | Which slice is Phase 7? | **Sync panel** — the only remaining placeholder; no new permission (`INTERNET` already ships for the updater). |
| 2 | Transport | **Live P2P only** on Android. The desktop's shared-folder fallback lives in app-private storage no peer can read, so it cannot work there; desktop keeps it. |
| 3 | Automatic syncing | **In-process loop** — one round immediately, then every ~10 s while paired and running (the Dioxus Android app's cadence). No foreground service, no WorkManager. |
| 4 | Existing pairing | **Reuse** the files the Dioxus build wrote (`sync-identity.json` + `sync-node.json` next to the DB), so the phone keeps its chain with no re-pairing. |

### What landed — shared core (`kal-sync`, `kal-core`)

- `kal_sync::identity_file` now owns the on-disk contract that was duplicated in
  the app shell: `identity_path` / `node_path` / `outbox_path`,
  `load_identity_at` (never mints), `save_identity_at` (0600), and
  `clear_identity_at` (identity + outbox, keeps the per-device node identity).
  The path/file/JSON shape is pinned by tests because both implementations read
  the same files.
- `kal_sync::client` now owns the live-round orchestration moved out of
  `app/src/sync_live.rs`: `LiveSink`, `live_transport` (built on a dedicated
  thread, cached per chain fingerprint), `sync_round` (peer wait + bounded
  drain), `live_round_core`, `retire_shadowed_placeholders` and
  `reset_transport` (used by "Leave"). The Dioxus app's `sync_live` is a
  re-export shim and its `sync_ui` calls the shared file helpers, so the two
  shells cannot drift.
- `kal_core::DEFAULT_PERSONAL_ID` / `DEFAULT_BIRTHDAYS_ID` moved from the app
  root so the sync layer recognizes the same well-known calendars on both
  platforms. The 7 live-round tests and the 2 identity-file tests moved with the
  code (plus a new clear/keep-node case).

### What landed — bridge (`kal-ffi`, `kal-jni`)

`KalDb` now remembers the database path passed to `kal_open`, so the sync files
resolve next to it (the exact `<files>/kal` directory the Dioxus app used).
Six additive exports, JSON over C strings as usual:

- `kal_sync_status_json(db)` → `{"paired": false, "fingerprint": null}` /
  `{"paired": true, "fingerprint": "…"}`;
- `kal_sync_start_chain_json(db)` → `{"phrase", "fingerprint"}` (generates +
  persists);
- `kal_sync_join_chain_json(db, phrase)` → the same shape; an invalid phrase
  fails with "Invalid recovery phrase" and leaves the device unpaired;
- `kal_sync_phrase_json(db)` → reveal the stored phrase (NULL when unpaired);
- `kal_sync_leave(db)` → deletes the identity/outbox and drops the cached
  transport (0/-1; leaving twice is a clean 0);
- `kal_sync_round_json(db)` → `{"merged": N}`; NULL + the native reason on
  failure ("not paired", "live P2P: no peers yet …"). Blocking by design:
  Kotlin calls it on `Dispatchers.IO`.

`crates/kal-jni` grew 21 → **27** exports; `widgets/kal_ffi.h` declares **32**
symbols (was 26), bridge version still 1 (additive). Six new bridge tests cover
the fresh-install no-mint invariant, start/persist/reload across a handle
restart, join + fingerprint equality + garbage rejection, leave semantics, a
round without a chain, and NULL handles.

### What landed — Kotlin (`android/app`)

- `bridge/Payloads.kt` — `SyncStatus`, `SyncPhrase`, `SyncRound` with their
  parses (an unreadable status reads as unpaired; a failed phrase/round is
  null); `KalSession` gained `syncStatus` / `startSyncChain` / `joinSyncChain` /
  `syncPhrase` / `leaveSyncChain` / `syncRound`.
- `ui/ShellState.kt` — `SyncUi` state (NotPaired / ShowPhrase / Paired / Error),
  the join-phrase field, `syncing`, the last-round line, and the actions. The
  manual round formats the Dioxus copy ("Synced N change(s) — HH:MM" / "No
  changes to sync — HH:MM" / "Sync failed: …") and reloads the views when
  something merged. `backgroundSync` is silent and skips while a manual round
  is in flight. The stored profile is mirrored for the panel's header and
  refreshed on save.
- `ui/SyncPanel.kt` — the sidebar section: profile header (avatar + name →
  Settings), Start/Join with a multi-line phrase field, the paired view
  (fingerprint, Show recovery phrase, Sync now/Syncing…, last result), and the
  two-step Leave confirm; plus the 24-word modal (numbered 3-column grid, the
  "they grant full access" warning, "I wrote it down"). The modal is rendered
  at the shell level like the editor.
- `ui/Shell.kt` — a `LaunchedEffect(state.syncPaired)` driver: one round now,
  then every 10 s, cancelled by leaving the chain. The now-unused `NotPorted`
  machinery (state, dialog, `notYet`) was deleted with the last placeholder.
- JVM: one new `PayloadsTest` case for the three sync payload shapes (plus the
  unreadable-status fallback and null failures).

### Parity and deliberate differences

- Same UI copy, same phrase-once flow, same fingerprint, same leave semantics,
  same 10 s driver, same files → the owner's existing chain is inherited.
- Android is live-only (no outbox fallback) and does not port the internal
  `sync-trace.log` diagnostics. Sync runs only while the Kal process is alive —
  identical to the Dioxus Android app (no foreground service).
- The round's native error text is shown as the Dioxus panel showed it; a
  background round stays silent.

### Gates (2026-10-21)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | **193 pass, 0 fail** (was 186) |
| `cargo test -p kal-ffi --test bridge` | **56 pass** (was 50) |
| `cargo test -p kal-sync` | **32 pass** (moved live-round/identity tests + 3 identity-file cases) |
| `scripts/check-bridge-abi.sh` | **32** declared symbols, bridge version 1 |
| `scripts/check-android.sh` | pass |
| `cargo check -p kal-app --target wasm32-unknown-unknown` | pass |
| `scripts/build-kotlin-android.sh --apk` | **27/27** JNI symbols staged; the `.so` exports all six `kal_sync_*` symbols |
| `:app:testDebugUnitTest` | **120 tests, 0 failures** (was 119) |
| `:app:assembleDebug` / `:app:assembleRelease` | BUILD SUCCESSFUL — debug APK 52.6 MB, release 41.9 MB, both carrying the freshly built 23.3 MB `libkal_jni.so` |

Device: **not installed** — the phone was not reachable (`adb connect
192.168.0.103:39637` → connection refused). The previous debug build
(`versionCode 2000`) is still the one on the device; nothing was launched.

### Device read requested (owner) — once the phone is back on adb

1. Open the sidebar → Sync. It should already show the **fingerprint** from the
   Dioxus build's chain (no re-pairing), with **Show recovery phrase**, **Sync
   now** and **Leave sync chain…**.
2. Reveal the phrase: the same 24 words the old build showed for that chain.
3. With the desktop online on the same chain: tap **Sync now** → either
   "Synced N change(s) — HH:MM" or "No changes to sync — HH:MM"; a change made
   on the desktop appears on the phone without pressing anything within ~10 s.
4. **Leave sync chain…** → Leave → the panel returns to Start/Join; (optional)
   rejoin with the phrase and sync again.
5. Background behaviour to watch: with the app open and paired, the phone's
   battery/network use should look like the old build's driver; closing the app
   stops syncing until it is opened again.

### Open (owner)

- The Phase 7 device read above (pending hardware access).
- The earlier open items still stand: the signing identity / release-workflow
  cutover (`SIGNING.md`), and the device reads for the three post-Phase-6 fixes
  and the linked-occurrence change.

## Corrections after the Phase 7 build (2026-10-23)

Owner read of the installed build found the month grid missing its week
separators — a straight port miss: `.month-row { border-top: 1px solid
var(--border) }` in `app/assets/main.css` had no Kotlin counterpart (the cell
`border-right` lines were ported, the row lines were not).

Fixed in `ui/MonthView.kt` by giving every row a 1 dp top hairline before its
week (so there is also the header-to-grid line), and — while comparing the cell
against the same CSS — two more read-through misses in the same block:

- `.month-grid { padding: 0 8px 8px }`: the grid is inset 8 dp left/right/bottom
  (the nav strip is not);
- `.day-num { align-self: flex-end }`: the day number sits at the cell's
  trailing edge, not the leading one.

Gates: `:app:compileDebugKotlin` + `:app:testDebugUnitTest` pass (120 tests);
`scripts/build-kotlin-android.sh --apk` rebuilt both APKs. Reinstalled with
`adb install -r` (2026-10-23 22:05): `versionCode 2000`, `firstInstallTime`
unchanged (2026-09-04), and `files/kal/` still holds `calendar.db`,
`sync-identity.json` and `sync-node.json` (the chain carries over).

## Phase 8 — widgets, ICS links, release cutover, cleanup (2026-10-23)

Owner scope for this final phase: "release cutover prep, ICS RECURRENCE-ID
links, Android cleanup", with widgets ported **before** the cutover (the
Kotlin APK had no home-screen widgets, so a release cutover would have dropped
them for every user).

### 8.1 ICS: linked occurrences export/import as RFC 5545 overrides

`crates/kal-import` now writes a linked occurrence ("This event") the way every
calendar application understands an exception:

- `UID` = the **series** row's id (not the occurrence's own id);
- `RECURRENCE-ID` = the series instant it replaced, DATE for an all-day series,
  UTC otherwise;
- the occurrence's own `DTSTART`/`DTEND`/`SUMMARY`/alarms,
- no `RRULE`;
- the series keeps its `EXDATE`, so a client that ignores overrides still shows
  one event rather than two.

Only when the series is part of the same export is the override emitted — a
dangling `RECURRENCE-ID` (series deleted/not visible) falls back to a plain
standalone event, exactly as before.

Import is two-pass: masters first (building UID → item-id), then overrides.
An override links only when its master is in the same file: it gets a fresh
item id (the UID belongs to the series, so reusing it would make the two rows
overwrite each other), `metadata.series_id`/`recurrence_id` set, and the
instant added to the master's `EXDATE` (all-day de-duplicated by local date,
because all-day EXDATEs are UTC instants of local midnight). With no master it
imports standalone, so nothing is lost — the old behaviour.

Tests (`crates/kal-import/tests/roundtrip.rs`, 5 new): export shape, round-trip
link, override-before-master ordering + EXDATE insertion, orphan override,
all-day DATE recurrence id + date de-duplication.

### 8.2 Home-screen widgets in the Kotlin app

The two providers moved from the Dioxus staging tree into the owned project:

- `com.kal.calendar.widgets` sources (Schedule + Month providers, JSON mapper,
  boot refresher) and their res/layouts/info XML are now under
  `android/app/src/main/`;
- the manifest declares both `APPWIDGET_UPDATE` receivers plus the
  `BOOT_COMPLETED` refresher (same exported flags as before);
- `KalWidgets` loads `kal_jni` instead of `main`, taps open
  `com.kal.calendar.MainActivity`;
- the two JNI entry points (`nativeSchedule`, `nativeMonth`) now live in
  `crates/kal-jni`, opening `files/kal/calendar.db` for the duration of one
  snapshot and reusing the frozen `kal_upcoming_json`/`kal_month_grid_json`
  payloads — no new C exports, so the 32-symbol ABI is untouched;
- `scripts/build-kotlin-android.sh` now cross-checks the widget `external fun`s
  against the `.so`, like the app's.

Tests: `WidgetsJsonTest` (5 JVM cases) pins both snapshot parsers (defaults,
time label, malformed/empty payloads). Device placement of both widgets is the
remaining manual read.

### 8.3 Release cutover

- `scripts/build-kotlin-android.sh` gained `--apk-release` and
  `--version <tag>` (`-PkalVersionName`), so a tag builds
  `:app:assembleRelease` with the tag-derived `versionCode`.
- `.github/workflows/release.yml`'s `android` job now builds the Kotlin app,
  refuses to run without `KAL_KEYSTORE_BASE64`, decodes the keystore, runs the
  script with the tag, copies `kal-<version>-android.apk`, verifies the
  signature with `apksigner` and prints the package/versionCode.
- `ci.yml` dropped the dx `android-build` job; the Kotlin job runs the JVM
  tests and both APK variants (release variant debug-signed there as a compile
  gate, never uploaded).
- `SIGNING.md` §2/§4/§6 rewritten to match; the one unavoidable uninstall is
  still needed because the installed build is debug-signed (owner action).

### 8.4 Cleanup

Removed now that the dx Android target is retired: `android/widget/`,
`android/updater/`, `android/MainActivity.kt`,
`scripts/{stage-widgets,stage-updater,build-android-install}.sh`,
`app/src/widget_ffi.rs` (and its `mod`); `scripts/check-android.sh` now checks
only `kal-ffi` + `kal-jni`. `scripts/apply-icons.sh` stays for the desktop
icon assets. `ARCHITECTURE.md`/`RULES.md` describe the owned-project build.

### 8.5 Gates

`cargo fmt` clean, `cargo clippy --workspace --all-targets -D warnings` clean,
`cargo test --workspace` green (198 tests, incl. kal-import 10 / kal-jni 2),
JVM **125** tests green, C ABI 32 symbols / version 1 unchanged, wasm check
green, debug + release APKs build (versionCode 2000 / 0.2.0), manifest carries
the widget receivers, and the packaged `libkal_jni.so` exports 29 JNI symbols
(27 app + 2 widget). Debug APK reinstalled in place (2026-10-23 22:21,
`firstInstallTime` unchanged).

Open: owner device read for the two widgets (place them on the home screen),
plus the outstanding Phase 2–7 device reads; stable-keystore secrets for the
first signed release.

### 8.6 Settings moved into the sidebar (owner request)

The top-bar gears are gone in both codebases; the preferences controls, the
sync-chain profile and Software & updates now sit in three sections at the
bottom of the sidebar. Behaviour is unchanged (profile edits are staged and
committed on Save; Cancel restores the saved profile).

Kotlin:

- `ui/SettingsSections.kt` replaces `SettingsModal.kt`: `PreferencesSection`,
  `ProfileSection`, `SoftwareSection`, the `SelectMenu` select and the shared
  `AvatarPreview` (the Sync header still uses it).
- `Shell.kt`: the top bar is hamburger + title + subtitle only; the sidebar
  ends with Preferences, Sync-chain profile and Software & updates; the prefs
  popover and the settings modal are deleted.
- `ShellState`: `prefsOpen`/`settingsOpen` and their open/dismiss helpers are
  gone; `profileEdit` is seeded at construction, Cancel calls the new
  `resetProfileEdit()`, and a successful save no longer closes anything.
- `SyncPanel`: the profile header is a plain summary (it used to open the
  modal).

Dioxus:

- `profile.rs`: `SettingsScreen` became `ProfileSection`, `SoftwareSection`
  is public and no longer renders its own heading; `SETTINGS_SCREEN_OPEN` and
  `open_settings_screen` are gone, `PROFILE_EDIT` is seeded at startup and
  `reset_profile()` backs Cancel.
- `main.rs`: the top-bar controls/prefs drawer and the modal render are gone;
  the three sections are appended to the sidebar. `sync_ui.rs`'s profile
  header is a plain `div`.
- `main.css`: `.prefs-drawer`/`.prefs-toggle`/`.tb-controls` rules removed,
  `.sidebar-settings` added (full-width selects/buttons, wrapping text), and
  the sidebar scrolls vertically.

Gates: `cargo fmt`/`clippy -D warnings` clean, Rust 198 tests, JVM 125 tests,
wasm check green.

### 8.7 One top bar + swipe navigation (owner request)

The second bar (‹ Today ›, period label, four view tabs) is gone. The top bar
now reads: hamburger, "Kal", period label, view tabs. ‹/Today/› are removed;
horizontal swipes over the view body step the period instead.

Kotlin:

- `CalendarNav.title(view, cursor, compact)` adds the phone labels built by new
  `Formats` helpers: `Oct 2026`, `Oct 12 – 18` (`Oct 26 – Nov 1` across a
  month), `Sat, Oct 17`, `Agenda`.
- `KalButton` gained a `dense` variant (12 sp, 7/4 dp padding) for the tabs.
- `TopBar` owns the label (weighted, ellipsized) and the tabs; the old
  `ViewNav.kt` is deleted and `ShellState.goToday()` with it.
- `SwipeableContent` wraps the view: `draggable` (horizontal) follows the
  finger, then slides the outgoing view off (150 ms), calls
  `state.step(forward)`, and slides the incoming view in (210 ms); a short drag
  snaps back (180 ms). Velocity ≥ 900 px/s counts as a fling; the Agenda view
  does not swipe.

Dioxus:

- `TopBar` renders the full/short labels (CSS media query picks one) and the
  tabs; `ViewNav` and the `.month-nav` rules are gone, as is the top-bar
  subtitle and `ui::today_line`.
- `Content` records `pointerdown` x and steps the cursor on a ≥ 70 px
  horizontal `pointerup` (Day ±1, Week ±7, Month ±1, Agenda excluded), then
  plays a 220 ms entry animation (`nav-next-*`/`nav-prev-*`; the class suffix
  alternates so consecutive swipes replay it).
- `.content` clips overflow and `.view-body` restores the flex sizing the
  removed nav strip had provided.

Gates: fmt/clippy clean, Rust 198 tests, JVM 126 tests (new compact-title
case), wasm check green.

### 8.8 Scoped delete + split end bound (owner bug report)

Report: Delete on a recurring event, then **"…and following"**, duplicated
every following occurrence instead of deleting them.

Cause: those scope buttons ran the *save* split. `apply_scoped_edit` only
appended an `UNTIL` when the base had neither `UNTIL` nor `COUNT`; a base that
already ended (`FREQ=DAILY;UNTIL=20261120T055900Z`) kept its old end, while the
new tail started at the split with the form's preset rule and **no end**. Every
following occurrence therefore existed twice, and the tail ran forever. The
phone DB showed three such tails created at 19:46:38/43/47Z, all starting
30 Oct with `FREQ=DAILY` and the base's 23 EXDATEs, beside the still-live base.

Fix (shared storage, both front ends):

- **Real scoped delete.** New `kal_storage::scoped::delete_scoped`
  (`DeleteScope::{ThisOnly, ThisAndFollowing, All}`): *This event* EXDATEs one
  base occurrence or tombstones one detached copy; *…and following* truncates
  the base before the clicked occurrence, drops the tail's EXDATEs and
  tombstones later detached copies; *All* resolves a detached copy back to its
  series. It never creates a replacement row.
- **The delete confirmation replaces the save row.** Kotlin `EditorModal` and
  Dioxus `EditorModal` show the three delete scopes while confirming, so the
  same words can never run the save/split path. *…and following* is offered for
  a detached occurrence on **delete** (the storage resolves its base), while it
  stays hidden for a detached **save**.
- **The split preserves the end.** `truncate_rule_before` now shortens an
  existing `UNTIL` and converts `COUNT` to the earlier equivalent `UNTIL`
  (never extending a series that already ended earlier);
  `kal_core::viewmodel::series_end` computes the original end and the new tail
  carries it. A split no longer turns a bounded series into an infinite tail.
- **Bridge**: new `kal_delete_item_json` / JNI `deleteItemJson`, Kotlin
  `DeleteForm`; `KAL_BRIDGE_VERSION` is now **2** (33 C exports, 30 JNI
  symbols). The widget surface is unchanged.
- **Live DB cleanup (owner-approved)**: after force-stopping the app, the three
  duplicate tails `01M5WW09…`, `01M5WW0F…`, `01M5WW0J…` were tombstoned in
  place (LWW timestamp refreshed); the base `01M1PJBC…` and all older rows were
  left untouched. The new debug APK (same debug cert, versionCode 2000) was
  installed in place; the app was not launched.

Tests: `crates/kal-storage/tests/scoped_delete.rs` (9: all three delete scopes,
later detached copies, first-occurrence delete, existing-UNTIL split, COUNT
split, no replacement rows); `crates/kal-ffi/tests/bridge.rs` (4 delete cases +
the old "leaves an existing UNTIL alone" case rewritten to expect a shortened
base and an inherited tail end); JVM `EditorPayloadsTest` pins the `DeleteForm`
wire shape.

Gates: fmt/clippy clean, Rust **211** tests (FFI bridge 60), JVM **128** tests,
`check-bridge-abi.sh` **33 symbols / version 2**, wasm check green, debug and
release APKs build. The locally built release APK is debug-signed (no stable
keystore on this machine) — `release.yml` still refuses to publish that; the
signed release comes from CI with `KAL_KEYSTORE_BASE64`.

### 8.9 Google-style month widget (owner request)

Request: make the month widget feel like Google Calendar's — month + year on
the left, `<` / `>` month arrows and a `+` on the right, and a Google-style
grid whose days are clickable. Clicking a day must open the app's add-event
menu for that date; the `+` targets today. Keep it fast and smooth.

Kotlin:

- `MonthWidgetProvider` is now per-widget: each instance stores an absolute
  month pin (or no pin, meaning "follow today") in `kal_month_widget`
  SharedPreferences, so the arrows navigate independently, the view does not
  drift when the calendar rolls over, and `onDeleted` cleans the entry up.
  The header is a 56 dp row with a 20 sp month/year title and 40 dp
  add/previous/next buttons. The body is a full-width
  1 dp bordered grid: a Monday-first one-letter weekday row, 42 generated day
  cells, vertical column dividers and horizontal week separators, centered day
  numbers, and a filled 20 dp today circle. Up to two single-line, ellipsised
  event names sit under each day (title, or `time title` for timed items). The
  item colour is mapped to one of eight rounded chip drawables in the light/dark
  palette. The sixth row is hidden when a month only needs five.
- `MonthWidgetModel` holds the pure navigation/grid rules (`monthFor`,
  `showsWeek`, `title`, `requestCode`, spoken date label) and the chip label so
  they are covered by JVM tests. `WidgetIntents.EXTRA_ADD_DATE` is the shared
  date extra.
- Header arrows are broadcast `PendingIntent`s that update only that widget;
  day cells are activity `PendingIntent`s with the tapped date, and the `+`
  asks the app for today at click time. `MainActivity` is `singleTop`, handles
  both `onCreate` and `onNewIntent`, and passes the one-shot date to `KalApp`,
  which calls the new `ShellState.openNewEvent(date)` (cursor moves, sidebar
  closes, add-event editor opens) and then clears the extra.
- `WidgetRefresher` updates both widgets off the main thread after item saves,
  deletes, task toggles, .ics imports and sync merges (a new
  `ShellState.dataVersion` signal), so a just-added event does not wait for the
  30-minute widget period. The schedule widget keeps its existing layout.
- New `values`/`values-night` month-widget colours (including the chip palette)
  and shape/vector drawables; the widget follows the system light/dark surface.
  The default is a 4×6 widget so two event rows fit. No new dependency, no
  Glance, no Compose in the widget process.
- The old `kal_widget_open` month tap extra and `KalMonthHead` style are gone.

Tests/gates: JVM **139** (`MonthWidgetModelTest` covers absolute-month pinning
across year boundaries, the no-drift rollover, five/six-week collapse,
request-code uniqueness, the spoken date and chip labels),
`:app:assembleDebug` and `:app:lintDebug` green,
`scripts/check-android.sh` green. Rust is untouched. Device verification
(place/resize the widget, arrows, `+`, day taps, grid/event rendering in
light/dark, the new-event round trip) remains owner-only; the new APK was built
but not launched or installed here.
