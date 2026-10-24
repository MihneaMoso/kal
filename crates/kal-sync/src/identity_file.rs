//! On-disk contract for the sync-chain identity.
//!
//! The chain identity is stored next to the calendar database as plain JSON
//! (`sync-identity.json`, phrase only). The path, file name and JSON shape are
//! load-bearing across implementations: the Dioxus app wrote them, the Kotlin
//! rewrite reads exactly the same files, and both share one chain after the
//! switch.
//!
//! A fresh install has **no** chain identity, and merely reading must never
//! mint one — no silent random-chain join. The file appears only after the
//! user starts a chain or joins one by phrase.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ChainIdentity;

/// The JSON body of `sync-identity.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredIdentity {
    pub phrase: String,
}

/// Directory that holds the identity, node config and outbox: the calendar
/// database's own directory (`<files>/kal` on Android).
fn data_dir(db_path: &Path) -> &Path {
    db_path.parent().unwrap_or_else(|| Path::new("."))
}

pub fn identity_path(db_path: &Path) -> PathBuf {
    data_dir(db_path).join("sync-identity.json")
}

/// Per-device (not chain) identity: the iroh endpoint secret + a stable ULID.
/// Kept next to the chain identity so restarts reuse the same endpoint id
/// (faster reconnects, stable revocation identity).
pub fn node_path(db_path: &Path) -> PathBuf {
    data_dir(db_path).join("sync-node.json")
}

/// The desktop's shared-folder transport directory. Unused on Android (app-
/// private storage can't reach a peer), but the Dioxus app still uses it.
pub fn outbox_path(db_path: &Path) -> PathBuf {
    data_dir(db_path).join("sync-outbox")
}

/// Load this device's chain identity, or `None` when it has no chain yet.
/// Reading never creates anything.
pub fn load_identity_at(db_path: &Path) -> Option<ChainIdentity> {
    let text = std::fs::read_to_string(identity_path(db_path)).ok()?;
    let stored: StoredIdentity = serde_json::from_str(&text).ok()?;
    ChainIdentity::from_phrase(&stored.phrase).ok()
}

/// Persist `identity` as the device's chain. Creates the data directory.
pub fn save_identity_at(db_path: &Path, identity: &ChainIdentity) -> Result<(), String> {
    let path = identity_path(db_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&StoredIdentity {
        phrase: identity.phrase(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    // Restrict to owner-only where supported: the phrase grants full access to
    // the chain.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Remove this device from the chain: deletes the local identity file and the
/// device-local outbox folder. This is a local-only act — the gossip chain has
/// no central membership to revoke — so other devices simply stop receiving
/// snapshots from this one. The node identity is kept: it is per-device, not
/// per-chain, and reusing it avoids a fresh endpoint on re-pairing.
///
/// Returns whether a chain identity was actually present.
pub fn clear_identity_at(db_path: &Path) -> Result<bool, String> {
    let path = identity_path(db_path);
    let existed = path.exists();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let _ = std::fs::remove_dir_all(outbox_path(db_path));
    Ok(existed)
}

#[cfg(test)]
mod tests {
    use super::*;

    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// A unique, empty directory (the crate has no tempfile dependency here).
    fn scratch_dir() -> PathBuf {
        use std::sync::atomic::Ordering;
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("kal-sync-identity-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Step 0 Q3 invariant: a fresh install has no chain identity, and reading
    /// it (what app boot and the background driver do) must not create one.
    #[test]
    fn fresh_install_has_no_chain_identity_and_creates_no_files() {
        let dir = scratch_dir();
        let db_path = dir.join("kal").join("calendar.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        assert!(
            load_identity_at(&db_path).is_none(),
            "a fresh install must not be in a chain"
        );
        assert!(
            !identity_path(&db_path).exists(),
            "reading the identity must not create it"
        );
        assert_eq!(
            std::fs::read_dir(db_path.parent().unwrap())
                .unwrap()
                .count(),
            0,
            "fresh install wrote sync state without any user action"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The stored file name, location and JSON shape are shared by the Dioxus
    /// app and the Kotlin rewrite: both must read exactly what the other wrote.
    #[test]
    fn stored_phrase_round_trips_from_the_documented_path_and_format() {
        let dir = scratch_dir();
        let db_path = dir.join("kal").join("calendar.db");

        let identity = ChainIdentity::generate().unwrap();
        save_identity_at(&db_path, &identity).unwrap();

        let file = identity_path(&db_path);
        assert_eq!(file.file_name().unwrap(), "sync-identity.json");
        assert_eq!(file.parent().unwrap(), db_path.parent().unwrap());
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(raw["phrase"], identity.phrase());

        let loaded = load_identity_at(&db_path).expect("stored identity loads");
        assert_eq!(loaded.phrase(), identity.phrase());
        assert_eq!(
            loaded.fingerprint().fingerprint_hex,
            identity.fingerprint().fingerprint_hex
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Leaving removes the chain identity (and outbox) but keeps the per-device
    /// node identity; clearing twice is a clean no-op.
    #[test]
    fn clearing_removes_the_chain_but_keeps_the_node_identity() {
        let dir = scratch_dir();
        let db_path = dir.join("kal").join("calendar.db");
        let identity = ChainIdentity::generate().unwrap();
        save_identity_at(&db_path, &identity).unwrap();
        std::fs::write(node_path(&db_path), "{}").unwrap();
        std::fs::create_dir_all(outbox_path(&db_path)).unwrap();

        assert!(clear_identity_at(&db_path).unwrap());
        assert!(!identity_path(&db_path).exists());
        assert!(!outbox_path(&db_path).exists());
        assert!(node_path(&db_path).exists(), "node identity is per-device");
        assert!(load_identity_at(&db_path).is_none());

        assert!(
            !clear_identity_at(&db_path).unwrap(),
            "nothing left to clear"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
