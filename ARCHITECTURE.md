# Architecture

Kal is a local-first calendar built as a Rust workspace. One core, many
shells: the Dioxus desktop/mobile app, native widgets, and (future) sync
peers all speak to the same domain logic.

```
crates/
├── kal-core/       Pure domain: models, recurrence expansion, view-models,
│                   reminder firing computation. ZERO UI deps — reused by the
│                   app AND linked into widget shims via kal-ffi.
├── kal-storage/    SQLite (rusqlite, bundled). Append-only PRAGMA-versioned
│                   migrations. Repository exposes upsert_* only (sync-ready)
│                   plus soft-delete tombstones.
├── kal-sync/       Account-free P2P sync: LWW CRDT merge, BIP39 sync-chain
│                   identity, XChaCha20-Poly1305 encrypted envelopes,
│                   pluggable Transport trait (folder-gossip today, iroh/mDNS
│                   later).
├── kal-notify/     Reminder scheduling: pure firing computation lives in
│                   kal-core; this crate materializes firings as platform
│                   local notifications (notify-rust on desktop; threads with
│                   cancellation flags; AlarmManager/UNUserNotificationCenter
│                   via FFI on mobile later).
├── kal-import/     .ics export/import (icalendar crate) with round-trip
│                   fidelity incl. VALARM reminders; Google Calendar read-only
│                   import behind a Transport abstraction (RFC 8628 device
│                   flow + REST v3 mapping).
└── kal-ffi/        Stable C ABI (JSON strings) over kal-storage + kal-core,
                    consumed by native widget shims.

app/                Dioxus 0.7 desktop shell (views, editor modal, settings,
                    pairing UI). Single shared use_resource per dataset;
                    mutations restart resources; a global signal re-renders
                    after sync merges.
widgets/            Kotlin Glance + Swift WidgetKit shims calling kal-ffi.
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
first shipped implementation is *file gossip*: sealed `.kalblob` files in a
user-chosen outbox folder that can be moved by anything (Syncthing, Dropbox, a
USB stick). Because payloads are AEAD-encrypted under the chain key, any
carrier works without trusting it. iroh (QUIC hole-punching + relay) and LAN
mDNS plug into the same trait; relays only ever see ciphertext.

**Merge.** State-based CRDT with whole-record LWW registers. Every record
carries `updated_at`; winners are chosen by

1. newer `updated_at`,
2. tombstone flag at equal timestamps (deletes propagate),
3. lexicographic content tie-break (clock-skew safety).

This total order makes convergence independent of delivery order/duplication,
which is all a gossip network guarantees. Peers exchange full snapshots of
what the other is missing (`SyncEnvelope { device_id, state }`), so long
offline periods need no version-vector bookkeeping. Upgrade path: swap
whole-record registers for field-level or Automerge/Yrs documents inside
`SyncState::merge` without touching the wire envelope.

**Revocation.** A device's fingerprint can be blocklisted by peers; its
envelopes are then rejected before merge. Full key-rotation-with-repair UX is
tracked as future work.

**Leaving the chain.** In the UI, "Leave sync chain" (below "Sync now", with a
two-step inline confirm) deletes the local `sync-identity.json` and the local
`sync-outbox/` folder. The gossip chain has no central membership, so this is
purely local: other devices simply stop receiving snapshots from this one.

## Reminders

Reminders are computed, not pushed: `kal_core::reminders::compute_firings`
expands items (RRULE included) over a horizon and yields absolute fire times.
`kal_notify::ThreadScheduler` arms the next N firings as cancellable sleeps →
OS notifications. The schedule is reconciled on app start, after every data
mutation, and after sync merges — no background daemon, no network, works
airplane-mode.

## Data flow on mutation

```
editor save ──► db.upsert_item ──► restart(items resource) ──► views re-render
                                             └──► reminder reconcile effect
sync merge ──► db.upsert_* (merged rows) ──► RESOURCES_DIRTY signal ──► same
```
