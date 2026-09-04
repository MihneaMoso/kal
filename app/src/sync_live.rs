//! Long-lived live P2P transport for the sync chain (native only).
//!
//! Owns the per-device iroh identity + a gossip-joined transport for the
//! current chain. The DHT-keyed topic makes same-phrase devices reachable from
//! anywhere, so mobile joiners pull state from desktop automatically once the
//! two gossip neighbours meet.

use std::sync::{Arc, Mutex, OnceLock};

use kal_sync::live::IrohTransport;
use kal_sync::{ChainIdentity, SyncSession, SyncState};

use crate::DbHandle;

/// One transport per device, keyed by the chain fingerprint it joined.
type LiveSlot = (String, Arc<IrohTransport>);
static LIVE: OnceLock<Mutex<Option<LiveSlot>>> = OnceLock::new();

/// The slice of a live transport that a sync round needs.
///
/// Kept as a trait (rather than reaching for `IrohTransport` directly) so the
/// orchestration in [`live_round_core`] can be unit-tested against a fake,
/// in-memory transport — the real DHT/gossip networking can't run in CI.
pub trait LiveSink {
    fn device_id(&self) -> &str;
    /// Whether the gossip topic currently has at least one peer.
    fn is_joined(&self) -> bool;
    /// Remember the latest sealed state so newly-joined peers get it pushed.
    fn set_state(&self, blob: Vec<u8>);
    /// Broadcast our sealed state to all current peers.
    fn broadcast(&self, blob: &[u8]) -> Result<(), String>;
    /// Drain any state peers have pushed to us.
    fn recv(&self) -> Option<(String, Vec<u8>)>;
}

impl LiveSink for IrohTransport {
    fn device_id(&self) -> &str {
        IrohTransport::device_id(self)
    }
    fn is_joined(&self) -> bool {
        IrohTransport::is_joined(self)
    }
    fn set_state(&self, blob: Vec<u8>) {
        IrohTransport::set_state(self, blob)
    }
    fn broadcast(&self, blob: &[u8]) -> Result<(), String> {
        // addr is unused by the gossip transport, so "" is fine.
        kal_sync::Transport::send(self, "", blob).map_err(|e| e.to_string())
    }
    fn recv(&self) -> Option<(String, Vec<u8>)> {
        kal_sync::Transport::recv(self)
    }
}

/// Per-device (not chain) identity: the iroh endpoint secret + a stable ULID.
/// Stored next to the chain identity so app restarts reuse the same endpoint
/// id (faster reconnects, stable revocation identity).
#[derive(serde::Serialize, serde::Deserialize)]
struct NodeCfg {
    node_key: String,
    device_id: String,
}

impl NodeCfg {
    fn path() -> std::path::PathBuf {
        crate::sync_ui::identity_path(&crate::default_db_path().unwrap_or_default())
            .with_file_name("sync-node.json")
    }

    fn load_or_create() -> Option<Self> {
        let path = Self::path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<NodeCfg>(&text) {
                return Some(cfg);
            }
        }
        let cfg = NodeCfg {
            // Reuse iroh's RNG via kal-sync rather than pulling a rand dependency.
            node_key: encode_hex(&IrohTransport::new_node_secret()),
            device_id: ulid::Ulid::new().to_string(),
        };
        let dir = path.parent()?;
        std::fs::create_dir_all(dir).ok()?;
        let json = serde_json::to_string(&cfg).ok()?;
        std::fs::write(&path, json).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Some(cfg)
    }
}

/// The live transport for `identity`, creating it on first use. Returns `None`
/// when the chain changed or the transport could not start.
///
/// Safe to call from any thread: `IrohTransport::connect` internally calls
/// `tokio::runtime::Runtime::block_on`, which panics if invoked from within an
/// existing tokio runtime (e.g. a Dioxus render/effect thread). So whenever a
/// fresh transport must be built, it is built on a dedicated OS thread that is
/// not driving any runtime; callers here may block briefly for that build.
pub fn live_transport(identity: &ChainIdentity) -> Option<Arc<IrohTransport>> {
    let fingerprint = identity.fingerprint().fingerprint_hex.clone();
    let guard = LIVE.get_or_init(|| Mutex::new(None));
    let mut slot = guard.lock().ok()?;
    if let Some((fp, t)) = slot.as_ref() {
        if fp == &fingerprint {
            return Some(t.clone());
        }
    }
    let cfg = NodeCfg::load_or_create()?;
    let device_id = cfg.device_id.parse().ok()?;
    let node_key = decode_hex(&cfg.node_key)?;
    tracing::info!(%fingerprint, %device_id, "building live transport");

    // Clone the data we need into a 'static closure so the builder can run on
    // its own thread. The construction is synchronous and needs the ChainIdentity
    // only to derive the topic; rebuild entries are cheap enough.
    let phrase = identity.to_owned();
    let build = std::thread::Builder::new()
        .name("kal-live-connect".into())
        .spawn(move || IrohTransport::connect(&phrase, device_id, node_key))
        .ok()?;
    let transport = build.join().ok()?.ok()?;
    tracing::info!(%fingerprint, endpoint = %transport.endpoint_id(), "live transport ready");
    *slot = Some((fingerprint, Arc::new(transport)));
    slot.as_ref().map(|(_, t)| t.clone())
}

/// Run one full live sync round. Creates the transport on first call (if not
/// already cached) and waits up to `PEER_TIMEOUT` for DHT peer discovery
/// before giving up.  Callers fall back to the folder transport when this
/// returns `Err`.
pub fn sync_round(identity: &ChainIdentity, db: &DbHandle) -> Result<usize, String> {
    let transport = live_transport(identity).ok_or("live P2P unavailable")?;
    let sink: &dyn LiveSink = &*transport;

    // The transport's DHT record + gossip join happen asynchronously.  Give
    // the network up to 15 s to discover the first peer before we bail.
    const PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);
    let deadline = std::time::Instant::now() + PEER_TIMEOUT;
    while !sink.is_joined() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }
    if !sink.is_joined() {
        tracing::warn!("live P2P: no gossip peers within {PEER_TIMEOUT:?}");
        return Err("live P2P: no peers yet — try again in a moment".into());
    }
    tracing::info!("gossip joined — running live round");
    live_round_core(identity, db, sink)
}

/// The pure orchestration of one live sync round, factored out so it can be
/// tested against an in-memory fake transport instead of the real gossip
/// network.
///
/// Behavior contract (the "should work" spec):
///  1. Seal our current local state and remember it for the transport
///     (`set_state`) so late-joining peers receive it via the transport's
///     auto-rebroadcast.
///  2. Broadcast our full state to the topic.
///  3. Drain everything peers sent and merge it into our session; count merged
///     peer envelopes.
///  4. Self-broadcasts (our own device id) are ignored — never counted/merged.
///  5. After any merge, persist the converged calendars + items to the DB.
///  6. Returns the number of peer envelopes merged (0 means nothing changed).
pub fn live_round_core(
    identity: &ChainIdentity,
    db: &DbHandle,
    sink: &dyn LiveSink,
) -> Result<usize, String> {
    let device_id: ulid::Ulid = sink
        .device_id()
        .parse()
        .map_err(|e| format!("bad device id: {e}"))?;
    let calendars = db.list_calendars().map_err(|e| e.to_string())?;
    let items = db.list_items(true).map_err(|e| e.to_string())?;
    let mut session = SyncSession::new(
        identity,
        device_id,
        "live",
        SyncState::from_parts(calendars.clone(), items.clone()),
    );

    let sealed = session.seal_state().map_err(|e| e.to_string())?;
    tracing::info!(sealed_bytes = sealed.len(), "live: sealed local state");
    sink.set_state(sealed.clone());
    sink.broadcast(&sealed).map_err(|e| e.to_string())?;

    let self_id = sink.device_id().to_string();
    let mut merged = 0usize;
    // Gossip delivery is asynchronous: peer discovery, relay mappings and QUIC
    // handshakes take seconds on real networks (and a 2-node mesh falls back to
    // the DHT bubble-merge/overlap cadence, ~2-3 min). A single non-blocking
    // drain measures milliseconds and nearly always misses the peer's snapshot,
    // leaving both devices broadcasting at each other forever. Mirror the
    // host-side `sync_probe`: keep draining for a bounded window.
    const DRAIN_WINDOW: std::time::Duration = std::time::Duration::from_secs(12);
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);
    // After a peer frame lands, allow stragglers right behind it a brief grace
    // period, then return — the driver's next round picks up anything later.
    const QUIESCE: std::time::Duration = std::time::Duration::from_secs(2);
    // If absolutely nothing has arrived since broadcasting (not even our own
    // gossip echo), the topic is effectively silent right now; don't burn the
    // whole window — the next round retries.
    const MAX_SILENT: std::time::Duration = std::time::Duration::from_secs(3);
    let drain_deadline = std::time::Instant::now() + DRAIN_WINDOW;
    let mut last_any_arrival = std::time::Instant::now();
    loop {
        // Drain everything currently sitting in the inbox...
        while let Some((sender, bytes)) = sink.recv() {
            last_any_arrival = std::time::Instant::now();
            if sender == self_id {
                continue;
            }
            if session.accept_blob(&bytes).is_ok() {
                merged += 1;
            }
        }
        if std::time::Instant::now() >= drain_deadline {
            break;
        }
        if merged > 0 && last_any_arrival.elapsed() > QUIESCE {
            // Consumed the burst of peer frames; anything sent later is caught
            // by the next round.
            break;
        }
        if merged == 0 && last_any_arrival.elapsed() > MAX_SILENT {
            break;
        }
        // ...then wait a little for more to arrive (self-broadcast echoes,
        // peer snapshots) until the window closes.
        std::thread::sleep(POLL);
    }
    if merged == 0 {
        tracing::info!("live: round done, nothing merged");
        return Ok(0);
    }
    tracing::info!(merged, "live: round merged peer envelopes");

    for cal in session.state.calendars.values() {
        db.upsert_calendar(cal).map_err(|e| e.to_string())?;
    }
    for item in session.state.items.values() {
        db.upsert_item(item).map_err(|e| e.to_string())?;
    }
    Ok(merged)
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    let bytes = s.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kal_core::models::{Calendar, CalendarItem, Color, ItemKind};
    use kal_storage::Database;
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// Shared gossip mailboxes keyed by recipient device id. When any sink
    /// broadcasts, the blob is delivered to every OTHER sink's inbox — the
    /// same fan-out a real gossip topic provides.
    type MailboxMap = HashMap<String, VecDeque<(String, Vec<u8>)>>;
    #[derive(Clone, Default)]
    struct Mailboxes(Arc<StdMutex<MailboxMap>>);

    impl Mailboxes {
        fn push(&self, to: &str, from: String, blob: Vec<u8>) {
            self.0
                .lock()
                .unwrap()
                .entry(to.to_string())
                .or_default()
                .push_back((from, blob));
        }
        fn pop(&self, me: &str) -> Option<(String, Vec<u8>)> {
            self.0
                .lock()
                .unwrap()
                .entry(me.to_string())
                .or_default()
                .pop_front()
        }
        fn members(&self) -> Vec<String> {
            self.0.lock().unwrap().keys().cloned().collect()
        }
    }

    /// A fake live transport with the same contract as `IrohTransport`, but
    /// fully in-memory so the sync orchestration can be exercised in CI.
    ///
    /// `me` must be a well-formed ULID (the real transport's device id always
    /// is), because `live_round_core` parses it into a `Ulid`.
    struct FakeSink {
        me: String,
        joined: bool,
        mail: Mailboxes,
        state: StdMutex<Option<Vec<u8>>>,
        /// When set, `broadcast` arms a thread that delivers `pending_peer`
        /// (a peer's sealed state) into our own inbox after the delay —
        /// simulating real gossip latency.
        delayed: Option<Duration>,
        pending_peer: Vec<u8>,
    }

    impl FakeSink {
        fn new(me: &str, mail: Mailboxes) -> Self {
            Self {
                me: me.to_string(),
                joined: true, // fake sinks are considered joined to keep the test focused on merge logic
                mail,
                state: StdMutex::new(None),
                delayed: None,
                pending_peer: Vec::new(),
            }
        }
        /// Fake sink whose broadcast schedules a peer snapshot to land in our
        /// inbox a short while LATER — mirroring real gossip latency where the
        /// peer's state doesn't arrive before our own broadcast returns. This
        /// exercises the bounded drain window in `live_round_core` (a single
        /// instant drain would return before the message lands and miss it).
        #[allow(clippy::too_many_arguments)]
        fn new_with_delayed_peer(
            me: &str,
            mail: Mailboxes,
            deliver_after: Duration,
            peer_blob: Vec<u8>,
        ) -> Self {
            Self {
                me: me.to_string(),
                joined: true,
                mail,
                state: StdMutex::new(None),
                delayed: Some(deliver_after),
                pending_peer: peer_blob,
            }
        }
        fn ulid() -> String {
            ulid::Ulid::new().to_string()
        }
    }

    impl LiveSink for FakeSink {
        fn device_id(&self) -> &str {
            &self.me
        }
        fn is_joined(&self) -> bool {
            self.joined
        }
        fn set_state(&self, blob: Vec<u8>) {
            *self.state.lock().unwrap() = Some(blob);
        }
        fn broadcast(&self, blob: &[u8]) -> Result<(), String> {
            for peer in self.mail.members() {
                if peer != self.me {
                    self.mail.push(&peer, self.me.clone(), blob.to_vec());
                }
            }
            if let Some(delay) = self.delayed {
                let me = self.me.clone();
                let peer_id = FakeSink::ulid();
                let blob = self.pending_peer.clone();
                let mail = self.mail.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    mail.push(&me, peer_id, blob);
                });
            }
            Ok(())
        }
        fn recv(&self) -> Option<(String, Vec<u8>)> {
            self.mail.pop(&self.me)
        }
    }

    fn sample_item(calendar_id: ulid::Ulid, title: &str, ts_secs: i64) -> CalendarItem {
        use chrono::TimeZone;
        let start = chrono::Utc
            .timestamp_opt(ts_secs, 0)
            .single()
            .unwrap()
            .fixed_offset();
        CalendarItem::new(ItemKind::Event, title, calendar_id, start)
    }

    fn cal(name: &str) -> Calendar {
        Calendar::local(name, Color("#3366cc".into()))
    }

    /// Seed one calendar + one event onto the chain and return the sealed blob
    /// a peer would broadcast. The event is attached to the seeded calendar so
    /// the in-memory DB's `items.calendar_id REFERENCES calendars(id)` FK holds.
    fn sealed_state_with(identity: &ChainIdentity, cal_name: &str, item_title: &str) -> Vec<u8> {
        let mut s = SyncState::default();
        let cal_id = ulid::Ulid::new();
        let mut calendar = cal(cal_name);
        calendar.id = cal_id; // keep map key in lockstep with the row id so the FK holds
        s.calendars.insert(cal_id, calendar);
        s.items.insert(
            ulid::Ulid::new(),
            sample_item(cal_id, item_title, 1_700_000_000),
        );
        let session = SyncSession::new(identity, ulid::Ulid::new(), "seeder", s);
        session.seal_state().unwrap()
    }

    fn empty_db() -> DbHandle {
        Arc::new(Database::open_in_memory().unwrap())
    }

    /// Phone (fresh Android install, empty DB) joins a chain that a desktop has
    /// already populated. The desktop's state is sitting in the phone's inbox
    /// (pushed via the transport's NeighborUp rebroadcast). Tapping "Sync now"
    /// must bring the desktop's calendar + event into the phone's DB.
    #[test]
    fn phone_joining_chain_pulls_desktops_state_into_its_db() {
        let identity = ChainIdentity::from_phrase(PHRASE).unwrap();
        let mail = Mailboxes::default();

        // Desktop: publishes its populated state once, which lands in the
        // phone's mailbox (and any other peer) via the transport's
        // NeighborUp-style rebroadcast.
        let desktop_id = FakeSink::ulid();
        let desktop_blob = sealed_state_with(&identity, "Work", "Standup");

        // Phone: joins with an empty DB, with the desktop's state sitting in
        // its inbox, then syncs.
        let phone_id = FakeSink::ulid();
        let phone = FakeSink::new(&phone_id, mail.clone());
        mail.push(&phone_id, desktop_id, desktop_blob);
        let db = empty_db();
        let merged = live_round_core(&identity, &db, &phone).unwrap();

        assert!(merged >= 1, "phone must merge the desktop's state");
        let cals = db.list_calendars().unwrap();
        let items = db.list_items(false).unwrap();
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0].name, "Work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Standup");
    }

    /// A peer's state may only appear in our inbox AFTER we've already
    /// broadcast and begun draining (real gossip latency) — the bounded drain
    /// window must catch it. A pre-gossip single drain (the old behaviour)
    /// returned before the message landed and both devices stayed unsynced
    /// forever, which is exactly what the phone↔desktop trace showed.
    #[test]
    fn peer_state_arriving_during_drain_window_is_merged() {
        let identity = ChainIdentity::from_phrase(PHRASE).unwrap();
        let mail = Mailboxes::default();

        // The peer has a calendar + event it will push to us, but it arrives in
        // our inbox ~1s after our broadcast (well within the 12s drain window,
        // after a single instant drain would already have returned).
        let peer_blob = sealed_state_with(&identity, "Work", "Standup");

        let phone_id = FakeSink::ulid();
        let phone = FakeSink::new_with_delayed_peer(
            &phone_id,
            mail.clone(),
            Duration::from_secs(1),
            peer_blob,
        );
        let db = empty_db();

        let merged = live_round_core(&identity, &db, &phone).unwrap();

        assert!(
            merged >= 1,
            "peer state that arrives mid-round must be merged"
        );
        let cals = db.list_calendars().unwrap();
        let items = db.list_items(false).unwrap();
        assert_eq!(cals.len(), 1);
        assert_eq!(cals[0].name, "Work");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Standup");
    }

    /// Gossip broadcasts come back to the sender; a round must never count or
    /// persist its own echo.
    #[test]
    fn self_broadcast_is_never_merged_or_counted() {
        let identity = ChainIdentity::from_phrase(PHRASE).unwrap();
        let mail = Mailboxes::default();
        let phone_id = FakeSink::ulid();
        let phone = FakeSink::new(&phone_id, mail.clone());
        let own_blob = sealed_state_with(&identity, "Me", "Mine");
        mail.push(&phone_id, phone_id.clone(), own_blob);

        let db = empty_db();
        let merged = live_round_core(&identity, &db, &phone).unwrap();
        assert_eq!(merged, 0, "self-broadcast must not count as a merge");
        assert_eq!(db.list_items(false).unwrap().len(), 0);
        assert_eq!(db.list_calendars().unwrap().len(), 0);
    }

    /// Merging from two independent peers unions both — a phone receiving
    /// different events from two desktops keeps both.
    #[test]
    fn merges_from_multiple_peers_are_unioned() {
        let identity = ChainIdentity::from_phrase(PHRASE).unwrap();
        let mail = Mailboxes::default();

        let a_blob = sealed_state_with(&identity, "CalA", "A-event");
        let b_blob = sealed_state_with(&identity, "CalB", "B-event");

        let phone_id = FakeSink::ulid();
        let phone = FakeSink::new(&phone_id, mail.clone());
        mail.push(&phone_id, FakeSink::ulid(), a_blob);
        mail.push(&phone_id, FakeSink::ulid(), b_blob);

        let db = empty_db();
        let merged = live_round_core(&identity, &db, &phone).unwrap();
        assert_eq!(merged, 2, "both peers' envelopes are merged");
        let cals = db.list_calendars().unwrap();
        let items = db.list_items(false).unwrap();
        let titles: std::collections::HashSet<String> =
            items.iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains("A-event"));
        assert!(titles.contains("B-event"));
        let cal_names: std::collections::HashSet<String> =
            cals.iter().map(|c| c.name.clone()).collect();
        assert!(cal_names.contains("CalA"));
        assert!(cal_names.contains("CalB"));
    }

    /// A fresh identity has no data, so an empty chain round is a clean
    /// no-op — the phone DB stays empty but the round must not error/panic.
    #[test]
    fn empty_chain_round_is_a_clean_noop() {
        let identity = ChainIdentity::from_phrase(PHRASE).unwrap();
        let mail = Mailboxes::default();
        let phone = FakeSink::new(&FakeSink::ulid(), mail);
        let db = empty_db();
        let merged = live_round_core(&identity, &db, &phone).unwrap();
        assert_eq!(merged, 0);
        assert_eq!(db.list_items(false).unwrap().len(), 0);
    }

    /// The node-secret hex codec that keys the per-device transport must be a
    /// lossless 32-byte / 64-char round trip (corrupting it would make a phone
    /// unable to reconnect to its own DHT identity across restarts).
    #[test]
    fn node_secret_hex_round_trips_and_chains_differ() {
        let key = IrohTransport::new_node_secret();
        let hex = encode_hex(&key);
        assert_eq!(hex.len(), 64);
        assert_eq!(decode_hex(&hex), Some(key));
        // Distinct chains must never collide on the same topic fingerprint.
        let a = ChainIdentity::from_phrase(PHRASE).unwrap();
        let b = ChainIdentity::generate().unwrap();
        assert_ne!(
            a.fingerprint().fingerprint_hex,
            b.fingerprint().fingerprint_hex
        );
        // The same phrase always resolves to the same fingerprint on any device.
        let a2 = ChainIdentity::from_phrase(PHRASE).unwrap();
        assert_eq!(
            a.fingerprint().fingerprint_hex,
            a2.fingerprint().fingerprint_hex
        );
    }
}
