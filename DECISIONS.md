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

## Phases 6–7 (partial)

| # | Decision | Rationale |
|---|----------|-----------|
| D31 | **kal-ffi speaks JSON strings over a C ABI** (not #[repr(C)] structs) | Schema evolution without breaking shims; JSON parsing is trivial on iOS/Android; panic-guarded boundaries return NULL. |
| D32 | **kal_close takes a `*mut *mut KalDb` out-param** and nulls it | Makes double-close safe at the ABI level (plain double-free segfaulted in tests — caught early). |
| D33 | **Widget queries expand occurrences server-side (in Rust)** so yearly birthdays appear even though their base row is decades old. | Raw range SQL missed recurring items. |

## Phase 8 — P2P sync

| # | Decision | Rationale |
|---|----------|-----------|
| D34 | **Whole-item/calendar LWW registers** instead of Automerge/Yrs | Spec allows "simplest option preserving architecture": state-based LWW converges under arbitrary reordering, needs no op-log GC, and the envelope format survives an upgrade to field-level or Automerge CRDTs later. |
| D35 | **Total order tie-breaks**: newer timestamp → tombstone flag → lexicographic content | Guarantees all replicas pick the same winner even when timestamps collide (clock skew). |
| D36 | **Key derivation**: BIP39 seed → SHA-256 domain-separated stretch → XChaCha20-Poly1305 payload key + X25519 identity. Fingerprint = first 8 bytes of SHA-256(pubkey) hex. | No extra KDF dep; XChaCha avoids nonce-reuse risk without counter management. |
| D37 | **Revocation = fingerprint blocklist per device**, not key rotation (spec's full re-pair flow deferred) | Simplest correct behavior: revoked peers' envelopes are dropped; key rotation requires re-pairing UX which lands with the settings UI phase. |
| D38 | **Transport is a trait** (`send`/`recv` opaque blobs); iroh/mDNS implementations plug in later behind features. | Protocol logic fully tested offline via LoopbackTransport; real transports are drop-in. |

| # | Decision | Rationale |
|---|----------|-----------|
| D39 | **Build artifacts live outside the repo** (`../kal-build` via .cargo/config.toml) after target/ ballooned to 20G | Keeps checkout at ~2MB; any dev can override with CARGO_TARGET_DIR. |
| D40 | **Sync identity persisted as plain JSON next to the DB with 0600 perms**, not in the SQLite file | Widgets/FFI and future mobile shims need path-based access; keeps secrets out of synced data. |

| D41 | **FileTransport folder-gossip as the first real transport** (encrypted .kalblob files in a user-chosen outbox dir) | Zero-infrastructure P2P that works today via Syncthing/Dropbox/USB and honors the "relay only sees ciphertext" rule; iroh/mDNS plug into the same Transport trait later. |

## Mobile / Android

| # | Decision | Rationale |
|---|----------|-----------|
| D42 | **Android storage = `Context.getFilesDir()` (app-private) via ndk-context+jni**, not `dirs_next::data_dir()` | On Android `dirs_next` has no HOME/XDG so it returns None and we fell back to a relative `kal.db` on the read-only root FS (`os error 30`). filesDir is writable and app-private. |
| D43 | **Mobile sidebar = overlay drawer** (new `UiLayout.mobile` flag; `position:fixed` + translateX + transparent scrim), collapsed by default | Google-Calendar mental model on phones where pushing content wastes screen space; desktop keeps the in-flow push/resize sidebar. Idea 0.7 `#[cfg]` can't appear inside `rsx!`, so the distinction is a runtime `mobile` flag, not a compile-time split. |
| D44 | **Launcher icons staged post-`dx bundle` via `scripts/apply-icons.sh` + a direct `gradlew assemble`**, not a dx config | dx hardcodes and regenerates its template res/ on every bundle with no override hook; overwriting then re-running dx yields duplicate-resource merge failures. Rebuilding through Gradle directly bakes the branded icons into the APK. |
| D45 | **"Leave sync chain" is a purely local act**: delete `sync-identity.json` + the local `sync-outbox/` folder; two-step inline confirm in `sync_ui.rs` below "Sync now". | The chain is folder-gossip with no central membership to revoke, so leaving is local-only; other devices simply stop receiving snapshots. Matches the two-step inline-confirm pattern used elsewhere (no OS dialogs). |
| D46 | **Item-level color = `Option<Color>` in the editor**; `None` renders as "Calendar" (inherit the owning calendar's color). New items default to app blue `#3366cc`. | The model already had `color_override: Option<Color>` — reuse it rather than adding a parallel field; "Calendar" keeps inheritance available while the default satisfies "default blue". |
| D47 | **Per-item delete stays in the editor modal, made two-step** ("Delete" → "Delete this item? Delete/Keep"); no per-row delete button. | User-scoped: editor-only was requested. Two-step inline confirm matches the leave-sync-chain and OS-dialog-free pattern. |
| D48 | **Sync-chain profile stored as one JSON row in the device-local `settings` table** (`profile` key: username + base64 avatar with MIME). | Works on every platform without new files or Android SAF plumbing; the DB is already app-private on Android. Base64 keeps it one row; avatar capped at 4 MB. |
| D49 | **Avatar upload uses a native `<input type="file" accept="image/*">`** (Dioxus `FormData.files()` → `read_bytes()`), not `rfd`/SAF. | Genuinely cross-platform: desktop Wry and Android/iOS WebViews all open the OS file picker automatically for file inputs; no per-platform native code. |
| D50 | **Android home-screen widgets are classic `AppWidgetProvider` + `RemoteViews` (Kotlin, staged into the dx-generated project), not Glance** | Glance would add a large Kotlin dependency tree and its own DSL; direct RemoteViews keeps the widget thin and lets the snapshot logic stay in Rust via JNI. Widgets reuse the exact `kal-ffi` JSON the future iOS WidgetKit shim will call. |
| D51 | **Widget data is delivered by querying the app's own `libmain.so` over JNI at update time, reusing `kal-ffi` (`kal_upcoming_json`/`kal_month_grid_json`)**, refresh via `updatePeriodMillis` + `BOOT_COMPLETED` receiver. | The calendar is a native (Dioxus) activity with no ContentProvider, so a provider/URI model is moot; JNI keeps one code path. Refresh is periodic/boot-driven (not real-time push) per the platform's widget rules. |
| D52 | **Widget `month_grid` JSON gained an additive `color` field per day-item; `kal_upcoming_json` already carried `color` — no ABI break elsewhere.** | RemoteViews needs the effective hex color in the JSON (ABI tests use `contains`, so the addition is backward-compatible). |
| D53 | **Widget taps deep-link by launching `MainActivity` with a `kal_widget_open` extra (written but not yet consumed to jump to a specific day/item).** | Launching the app from a home-screen widget is the platform-mandated minimal path; honoring the extra to land on a date is a small follow-up. |

## Packaging / updates / web

| # | Decision | Rationale |
|---|----------|-----------|
| D54 | **A single curl-able `install.sh` downloads the matching release asset by a platform substring token** (`kal-linux.tar.gz`, `kal-windows.exe`, `android.apk`, …) and verifies it against the release's published `sha256:` digest via the GitHub API. | Right-sized for a local-first tool: no native package managers per OS, works from Termux too, and reuses the exact same JSON the in-app updater parses. JSON is parsed with `python3` (arbitrary-precision) rather than sed/grep because the API payload is minified to one line but also legitimately pretty-printed. |
| D55 | **Desktop updates = "stage + restart to apply"**: background-download the new binary, show *Update ready*, and on the next launch swap the running executable and relaunch. | `apply_staged_update` runs at the very top of desktop `main()` (before any window/DB work) so the swap is atomic with respect to the app: the old inode persists for the still-running process on Unix, and Windows handles the running-exe restriction by renaming the old to `.old~` first. |
| D56 | **Android updates = download the APK app-privately, then hand it to the system PackageInstaller** via a `ACTION_VIEW` intent served over a custom `ContentProvider` (`KalFileProvider`), not a raw `file://` URI. | A `file://` URI throws `FileUriExposedException` on API 24+, and `androidx.core.fileprovider.FileProvider` would pull in a new dependency; a tiny first-party provider is dependency-free and `grantUriPermissions` lets the installer read it with `FLAG_GRANT_READ_URI_PERMISSION`. The intent logic lives in Kotlin (staged by `scripts/stage-updater.sh` — same pattern as the widgets), launched from Rust via JNI. |
| D57 | **In-app updater is compiled out on web** (`target_arch = "wasm32"`), but the app itself now ships as a wasm build at `/kal/app/` (see D59). The landing page (`web/site/`) and the app bundle (`web/app/`) deploy to `mihneamoso.github.io/kal/` and `/kal/app/` from the same `pages` workflow. | The updater is meaningless in the browser — the app "updates" implicitly on each deploy. What blocked a web build was the SQLite-backed storage + native TLS stack, which the wasm storage backend (D59) removed. |
| D58 | **The app version is injected at build time, not a hand-edited constant**: `app/build.rs` emits `KAL_VERSION` from the nearest git tag (`git describe --tags --abbrev=0`), overridable via the `KAL_RELEASE_VERSION` env (set from the tag in the release CI), falling back to `CARGO_PKG_VERSION`; `updater::CURRENT_VERSION = env!("KAL_VERSION")`. | Releases are git-tag-driven (`v0.1.x`) while the workspace manifest stays at `0.1.0`, so a hardcoded "current version" would drift from the actual release every tag and had to be hand-maintained. Deriving it from the build lets the updater always compare against the real version (dev builds compare against the last tag), with a regression test that fails if it ever falls back to the stale manifest value. |
| D59 | **On wasm, `kal-storage` swaps SQLite for an in-memory `Database` backed by an IndexedDB snapshot store**, keeping the same synchronous 13-method CRUD API. Mutations call `schedule_persist()` (dirty-flag coalesced), which serializes the whole DB to an IndexedDB object store; on boot a `load_into()` restores that snapshot into the shared instance. | IndexedDB is inherently async, but the app's `Database` API is deliberately synchronous (`RefCell`-based threading); rather than rewrite every call site to async, IndexedDB is treated as a durable snapshot backend and the exact same `Database` facade is preserved, so the app, model layer, and sync code compile unchanged on both native and web. `rusqlite`, `getrandom`(native), `ureq`/`sha2`/`tokio`, and the TLS stack are gated native-only; the wasm target pulls `indexed_db_futures` + a `wasm_js` `getrandom`. |

- Sync CRDT engine: automerge vs yrs (phase 8).
- Transport: iroh vs libp2p (phase 8).
- ICS crate choice: `icalendar` vs `ics` (phase 5).
- Fluent-rs for i18n (phase 9).
