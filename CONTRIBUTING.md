# Contributing to Kal

Thanks for helping build a fast, private, local-first calendar!

## Ground rules

- **License**: MIT OR Apache-2.0. Any contribution is dual-licensed under both.
- **Local-first**: features must work fully offline. No telemetry, ever.
- **`kal-core` stays UI-free** — domain logic goes there so widgets and sync
  peers can reuse it.
- **Sync-ready storage**: writes go through `upsert_*`; deletes are
  tombstones. Never add `DELETE FROM`.

## Workflow

1. Fork / branch from `main`.
2. Make your change with tests:
   - `cargo test --workspace` must stay green;
   - `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`.
3. Open a PR describing the *what* and the *why*.

CI runs fmt + clippy + tests on Linux/macOS/Windows; keep it green.

## Where things live

See `ARCHITECTURE.md` for the crate map and `DECISIONS.md` for why things are
the way they are. `RULES.md` collects hard-won environment gotchas — add one
when a toolchain bites you.

## Good first issues

- More locales in `app/i18n/` (copy `en-US/main.ftl`, translate).
- Reminder presets beyond the default five.
- Calendar color picker UI (palette already supports any `#RRGGBB`).
- An iroh or mDNS `Transport` implementation behind a cargo feature.
