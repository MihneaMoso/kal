# Kal

Free, open-source, local-first calendar for desktop & mobile. Events, tasks
and birthdays; full RFC 5545 recurrence; unlimited locally-scheduled
reminders; `.ics` import/export; Google Calendar import; optional account-free
peer-to-peer sync — all in one Rust codebase (Dioxus 0.7).

## Status

| Area | State |
|---|---|
| Events / tasks / birthdays, multi-calendar | ✅ |
| Month / week / day / agenda views | ✅ |
| RRULE recurrence + per-instance edit scoping | ✅ |
| Unlimited reminders → OS notifications | ✅ desktop |
| .ics round-trip + Google Calendar import | ✅ |
| Sync chain: phrase pairing, encrypted live P2P (iroh) + folder-gossip merge | ✅ |
| Settings (theme, clock, week start, default view) | ✅ |
| i18n scaffolding (en-US), a11y pass | ✅ |
| Desktop always-on-top mini-calendar window | ✅ |
| Widget C ABI (`kal-ffi`) + Android/iOS shim sources | ✅ (shims need SDKs to build) |
| Android home-screen widgets (schedule + month) | ✅ |
| Cross-platform installer (`install.sh`) + in-app updater | ✅ desktop/Android |
| Web app (`/kal/app/` — wasm build with IndexedDB storage) | ✅ built in CI on every master push + release tag |
| Live P2P: iroh gossip + DHT discovery (same-phrase devices find each other) | ✅ native |
| LAN mDNS transport, iOS/mobile app targets | 🔜 |

Design decisions: `DECISIONS.md` · Sync internals: `ARCHITECTURE.md` ·
Contributing: `CONTRIBUTING.md` · Environment gotchas: `RULES.md`.

## Install

Prebuilt binaries are published to the [GitHub Releases](https://github.com/MihneaMoso/kal/releases)
for every version tag. The easiest way to install the latest release is the
curl-able installer:

```sh
curl -fsSL https://raw.githubusercontent.com/MihneaMoso/kal/master/install.sh | bash
```

What it does, by platform:

- **Linux / macOS** — downloads the release binary and installs it to
  `~/.local/bin/kal` (add that to your `PATH` if needed).
- **Windows** — downloads `kal.exe` into `%LOCALAPPDATA%\Programs\Kal\`.
- **Android / Termux** — downloads the APK into your `~/Download` folder;
  open it on the device to install (allow *Install from unknown sources* if
  prompted).

Every download is verified against the SHA-256 digest published on the release
(`shasum -a 256` / `sha256sum`). Env overrides:

| Env var | Meaning |
|---|---|
| `KAL_VERSION` | Pin a version instead of the latest (`v0.1.7`) |
| `KAL_PREFIX` | Install to a custom prefix instead of the default |
| `KAL_DRYRUN=1` | Print what would be downloaded/installed without touching disk |

The app also ships an **in-app updater**: *Settings → Software & updates →
Check for updates* downloads and verifies the newest release. On desktop it
stages the binary and offers **Apply update now** (swaps on restart); on
Android it downloads the APK and hands it to the system package installer.

## Build & run (desktop)

Prerequisites: Rust stable.

- Linux: `libgtk-3-dev libwebkit2gtk-4.1-dev`
- macOS: Xcode command line tools
- Windows: WebView2 runtime (preinstalled on Win11)

```sh
cargo run -p kal-app          # dev build & launch
cargo test --workspace        # test suite (100+ tests)
cargo clippy --workspace      # lints (CI enforces -D warnings)
```

Dev server with hot reload:

```sh
cargo install dioxus-cli --version 0.7   # once
cd app && dx serve                       # opens the desktop window
```

Build artifacts live in `../kal-build` by default (see `.cargo/config.toml`);
override with `CARGO_TARGET_DIR`.

## Mobile

The Rust core compiles for all targets today:

```sh
rustup target add aarch64-linux-android aarch64-apple-ios
cargo build -p kal-core -p kal-storage -p kal-ffi \
  --target aarch64-linux-android     # needs cargo-ndk + NDK
cargo build -p kal-ffi --target aarch64-apple-ios   # staticlib for XCFramework
```

The Dioxus mobile shell and platform notification FFI are the remaining work
tracked under phase 6 in `RULES.md`.

## Widgets

Native widgets read the same SQLite file through the C ABI in
[`widgets/kal_ffi.h`](widgets/kal_ffi.h):

- **Android**: build `libkal_ffi.so` per ABI into `jniLibs/`, then the Glance
  widget in [`widgets/android/`](widgets/android/).
- **iOS**: package the staticlib as an XCFramework for the WidgetKit extension
  in [`widgets/ios/`](widgets/ios/).

## Packaging

- **Linux**: `cargo deb` config lands with phase 10 polish; F-Droid recipe
  follows the Android shell.
- **Windows**: winget manifest points at the CI artifact `kal-windows.exe`.
- **macOS**: `cargo build --release` produces `kal`; notarization tracked in
  phase 10.
- Direct APK / TestFlight come with the mobile shells.

CI (`.github/workflows/ci.yml`) builds and attaches desktop artifacts on every
push to `master`.

## Web

A lightweight landing page lives in [`web/site/`](web/site/) (built from the
`mihneamoso/static-site` template — Pico.css classes only) and is deployed to
**https://mihneamoso.github.io/kal/** by the `pages` workflow
(`.github/workflows/pages.yml`). One-time setup: **Settings → Pages → Source:
GitHub Actions**.

The app itself also runs in the browser as a wasm build at
**https://mihneamoso.github.io/kal/app/** (deployed by the same `pages`
workflow). On `wasm32` the storage crate swaps SQLite for an in-memory,
IndexedDB-backed `Database` that keeps the same synchronous 13-method API and
persists snapshots to the browser's IndexedDB (`base_path = "kal/app"` in
`app/Dioxus.toml`).

The web bundle is never committed — it is built from source automatically:
**every push to `master`** rebuilds it (`.github/workflows/pages.yml`) and
**every version tag** rebuilds it too (`.github/workflows/release.yml`), both
via the shared [`scripts/build-web.sh`](scripts/build-web.sh), then deploys to
`/kal/app/`. To build it locally (requires `dx` + the `wasm32-unknown-unknown`
target):

```
bash scripts/build-web.sh            # assembles ./_deploy (site at root, app at /app)
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
