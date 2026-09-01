//! Long-lived live P2P transport for the sync chain (native only).
//!
//! Owns the per-device iroh identity + a gossip-joined transport for the
//! current chain. The DHT-keyed topic makes same-phrase devices reachable from
//! anywhere, so mobile joiners pull state from desktop automatically once the
//! two gossip neighbours meet.

use std::sync::{Arc, Mutex, OnceLock};

use kal_sync::live::IrohTransport;
use kal_sync::{ChainIdentity, SyncSession, SyncState, Transport as _};

use crate::DbHandle;

/// One transport per device, keyed by the chain fingerprint it joined.
type LiveSlot = (String, Arc<IrohTransport>);
static LIVE: OnceLock<Mutex<Option<LiveSlot>>> = OnceLock::new();

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
    let transport = IrohTransport::connect(identity, device_id, node_key).ok()?;
    *slot = Some((fingerprint, Arc::new(transport)));
    slot.as_ref().map(|(_, t)| t.clone())
}

/// Run one full live sync round. Succeeds only when the gossip topic has peers
/// (revealing current local state and draining theirs); callers fall back to
/// the folder transport when it returns `Err`.
pub fn sync_round(identity: &ChainIdentity, db: &DbHandle) -> Result<usize, String> {
    let transport = live_transport(identity).ok_or("live P2P unavailable")?;
    if !transport.is_joined() {
        return Err("live P2P: no peers yet".to_string());
    }

    let device_id: ulid::Ulid = transport
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
    transport.set_state(sealed.clone());
    transport.send("", &sealed).map_err(|e| e.to_string())?;

    let self_id = transport.device_id().to_string();
    let mut merged = 0usize;
    while let Some((sender, bytes)) = transport.recv() {
        if sender == self_id {
            continue;
        }
        if session.accept_blob(&bytes).is_ok() {
            merged += 1;
        }
    }
    if merged == 0 {
        return Ok(0);
    }

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
