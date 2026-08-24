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
| Sync chain: phrase pairing, encrypted folder-gossip merge | ✅ |
| Settings (theme, clock, week start, default view) | ✅ |
| i18n scaffolding (en-US), a11y pass | ✅ |
| Desktop always-on-top mini-calendar window | ✅ |
| Widget C ABI (`kal-ffi`) + Android/iOS shim sources | ✅ (shims need SDKs to build) |
| Live P2P transports (iroh/mDNS), mobile app targets, packaging pipelines | 🔜 |

Design decisions: `DECISIONS.md` · Sync internals: `ARCHITECTURE.md` ·
Contributing: `CONTRIBUTING.md` · Environment gotchas: `RULES.md`.

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
push to `main`.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
