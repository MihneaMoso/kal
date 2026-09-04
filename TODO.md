# TODO — finish phone↔desktop sync

## Context (what already happened, don't redo it)

- Both devices are on the SAME chain phrase: `wise vacuum fall century ride inner hedgehog alarm royal grain pigeon wall tray scatter spare weekend protect avoid early output fringe park level wing` (fingerprint `faad46a92a5c13e2`, topic `kal-sync/faad46a92a5c13e2`).
  - Desktop `~/.local/share/kal/sync-identity.json` already contains it.
  - Phone: after a fresh APK reinstall (data wiped), the **user manually entered/joined** the chain phrase in the app UI, so the phone's `sync-identity.json` is now correct too.
- Live P2P discovery OVER the DHT **works**: traces show both devices occasionally connect as gossip neighbors (`gossip joined`, `NeighborUp/Down`), and `cargo run -p kal-sync --example sync_probe` passes on the host.
- **Root causes of "phone never receives desktop data" (fixed, code + tests pass):**
  1. `app/src/sync_live.rs` → `live_round_core` broadcast then did a **single non-blocking drain** (`elapsed_ms=1-3`), so it returned before the peer's snapshot arrived (gossip delivery runs on a ~169 s DHT bubble-merge cadence in a 2-node mesh). **Fix:** drain `recv()` in a loop for a bounded window (`DRAIN_WINDOW=12s`, `POLL=250ms`, `QUIESCE=2s` after a merge burst, `MAX_SILENT=3s` early-exit if nothing flows) — mirrors `sync_probe`.
  2. `crates/kal-sync/src/live.rs` → `NeighborUp` handler broadcast the **raw sealed blob**, but the receiver only parses JSON `GossipFrame`s → late-joining peers' pushed snapshots were silently dropped. **Fix:** wrap the snapshot in `GossipFrame { from, blob }` before broadcasting.
  3. Added regression test `peer_state_arriving_during_drain_window_is_merged` in `app/src/sync_live.rs` (passes).
- Verified so far: `cargo test --workspace` green, fmt clean, clippy warnings are pre-existing (synce `sync_ui.rs:217` `mut`, `sync_probe` `aj`/`bj`), desktop builds. Android aarch64 NDK cross-build + full `scripts/check-android.sh --build` passed **before** these last two sync fixes — rerun it after any further code changes.
- The other AI's uncommitted background-sync work is in the tree (driver in `sync_ui.rs`, `sync_log.rs`, `sync_probe`, tracing deps in `app/Cargo.toml`, `crates/kal-sync/Cargo.toml`, `Cargo.lock`, scripts). Also unrelated binary icon churn and an unrelated pre-existing `web/site/index.html` change — **do not touch/commit index.html**.

## Immediate next step — verify live convergence END TO END

Both devices should now be running the FIXED binaries (desktop was rebuilt + relaunched; phone APK was rebuilt + reinstalled). The user has joined the chain on Android.

1. Confirm desktop is up: `pgrep -x kal` (desktop binary is named `kal`; watch `~/.local/share/kal/sync-trace.log`).
2. Watch the DESKTOP trace for a merge (NOT just `gossip joined`): `grep -iE "merged|gossip joined|no gossip|background sync" ~/.local/share/kal/sync-trace.log | tail -20`. Expect `background sync merged changes` or `manual sync round done merged=N`.
3. Pull the PHONE DB and check items landed (was 0 items / 2 calendars):
   ```sh
   cd /tmp/opencode && rm -rf pdir3 && mkdir pdir3
   adb shell "run-as com.kal.calendar cat files/kal/calendar.db" > pdir3/calendar.db
   adb shell "run-as com.kal.calendar cat files/kal/calendar.db-wal" > pdir3/calendar.db-wal
   adb shell "run-as com.kal.calendar cat files/kal/calendar.db-shm" > pdir3/calendar.db-shm
   sqlite3 pdir3/calendar.db "SELECT COUNT(*) FROM items;"   # must be >0
   sqlite3 pdir3/calendar.db "SELECT id,name FROM calendars;"
   ```
4. Phone trace: `adb shell "run-as com.kal.calendar cat files/kal/sync-trace.log" > /tmp/opencode/phone_trace4.log` then `grep -iE "merged|gossip joined|no gossip" ...`.
5. If STILL not converging (both `gossip joined` but 0 merges for > 2 min), the likely next suspects, in order:
   - The desktop's sealed state is ~130 KB per its broadcast log line. Check whether iroh-gossip / distributed-topic-tracker drops large broadcast messages (message size limit) — test by sending a small state first, or log the received-frame size on the phone (`Event::Received(message)` in `live.rs`) to see if any frame arrives at all.
   - Confirm the phone's gossip is actually joining with the desktop NOW (not just "no peers within 15s"): the "no gossip peers" WARN lines mean it isn't connected yet — that's DHT/join latency, retried every round.
   - Add a `tracing::debug!` in the `Event::Received` handler (and the NeighborUp wrap) to confirm frames cross the wire.

## After convergence is confirmed

- Run full verification again (code may have changed): `cargo fmt --all`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`, desktop `cargo build -p kal-app`, wasm check `cargo check -p kal-app --target wasm32-unknown-unknown`, and `scripts/check-android.sh --build` (aarch64 NDK).
- Investigate (possibly a bug): on a FRESH Android install, the app auto-created `sync-identity.json` + `sync-node.json` without user action (observed after reinstall before the manual join). If it auto-generates a random chain identity at startup, decide whether that's intended onboarding or an accidental auto-join; if accidental, the phone would silently join a random chain instead of waiting for the user to enter the phrase.

## Commit (only when explicitly asked)

- Commit under the real identity: `git -c user.name="Mihnea Moso" -c user.email="mihnea.mosorescu@gmail.com" commit ...`
- Review the full diff first: `git status --short`, `git diff --stat`, then per-file review. Stage only intended files. Never invent an author.
- Keep: my two sync fixes (`sync_live.rs`, `live.rs`) + prior-session fixes already committed. The other AI's background-sync batch is in the tree uncommitted — review and decide keep/adjust. Do NOT include `web/site/index.html`.

## Gotchas for the next agent

- `pkill -f "target/debug/kal"` or any command string containing `target/debug/kal` will kill the agent's OWN shell (pattern matches the bash `-c` command line). Kill the app with `pkill -x kal` instead (binary name is `kal`).
- `nohup ./target/debug/kal ... &` makes the tool's command appear to hang until its 120 s timeout even though the app started fine — build/launch it in a dedicated step and check `pgrep -x kal` separately.
- `ax serve`/`dx serve` rebuild pipelines may hold the shell; prefer launching the built binary directly.
- Don't `git add` the binary icon churn unless intended.