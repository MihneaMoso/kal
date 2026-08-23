# Kal Widgets

Native home-screen widgets render from Kal's Rust core through the stable C
ABI in [`crates/kal-ffi`](../crates/kal-ffi). The Rust side is built as:

- Android: `cdylib` (`libkal_ffi.so`) per ABI, loaded via JNI.
- iOS: `staticlib` packaged into an XCFramework linked by the WidgetKit
  extension target.

| Directory | Contents |
|---|---|
| [`android/`](android/) | Glance-based App Widgets (Agenda / Month grid / Tasks / Next event) |
| [`ios/`](ios/) | WidgetKit timeline providers |
| [`kal_ffi.h`](kal_ffi.h) | C header shared by both platforms |

## Contract

Widgets never run the Dioxus app. They open the same SQLite file read-only
through `kal_open`, pull JSON snapshots via `kal_upcoming_json` /
`kal_month_grid_json`, and re-query on system refresh signals (widget update
requests, significant timeline changes). All returned strings must be released
with `kal_free`; DB handles with `kal_close`.
