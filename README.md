# Chrono

Free, open-source, local-first calendar for desktop & mobile. Events, tasks and
birthdays; full RFC 5545 recurrence; unlimited local reminders; `.ics`
import/export; optional account-free peer-to-peer sync (Brave-Sync-style sync
chain) — all in one Rust codebase.

## Status: Phase 1 (Foundation) complete

See `RULES.md` for exact resume state and `DECISIONS.md` for design choices.

| Crate | Purpose |
|---|---|
| `crates/chrono-core` | Pure domain models & logic (no UI deps) |
| `crates/chrono-storage` | SQLite schema, migrations, repository |
| `crates/chrono-sync` | P2P sync chain (phase 8) |
| `crates/chrono-notify` | Reminder scheduling (phase 4) |
| `crates/chrono-import` | ICS + Google import (phase 5) |
| `crates/chrono-ffi` | C ABI for native widget shims (phase 7) |
| `app` | Dioxus 0.6 application |

## Build & run (desktop)

```sh
cargo run -p chrono-app        # Dioxus desktop shell
cargo test --workspace         # test suite
```

Linux desktop builds need GTK/webkit dev packages (`libgtk-3-dev`,
`libwebkit2gtk-4.1-dev`) for `dioxus-desktop`.

## Roadmap

1. ✅ Foundation: workspace, core models, SQLite storage, app shell
2. CRUD + month/week/day/agenda views
3. RRULE recurrence + per-instance editing
4. Reminders & notifications
5. .ics import/export, Google Calendar import
6. Mobile targets (Android/iOS)
7. Widgets via chrono-ffi
8. P2P sync (CRDT + sync chain)
9. Polish (theming, a11y, i18n)
10. Release engineering & packaging

## License

Dual-licensed under MIT or Apache-2.0.
