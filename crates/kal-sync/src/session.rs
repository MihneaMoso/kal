//! Encrypted gossip sessions between paired devices (spec §5.4 steps 3–5).
//!
//! The transport trait keeps protocol logic testable offline; real transports
//! (mDNS/LAN, iroh QUIC+relay) plug in behind the same interface.

use ulid::Ulid;

use crate::crdt::{SyncEnvelope, SyncState};
use crate::keys::{ChainIdentity, KeyError};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("crypto: {0}")]
    Crypto(#[from] KeyError),
    #[error("malformed envelope: {0}")]
    Malformed(String),
    /// Payload decrypts but came from a device that has been revoked.
    #[error("device revoked")]
    Revoked,
}

/// Moves opaque encrypted blobs between devices.
///
/// `addr` semantics are transport-specific (iroh node id, mDNS service name,
/// TCP loopback address in tests).
pub trait Transport {
    fn send(&self, addr: &str, blob: &[u8]) -> Result<(), SyncError>;
    /// Non-blocking poll of anything addressed to us.
    fn recv(&self) -> Option<(String, Vec<u8>)>;
}

/// One device's view of a sync chain.
pub struct SyncSession<'a> {
    identity: &'a ChainIdentity,
    pub device_id: Ulid,
    pub device_name: String,
    pub state: SyncState,
    /// Fingerprints of devices removed by the user; their envelopes are
    /// dropped instead of merged (§5.4 step 5).
    pub revoked: Vec<String>,
}

impl<'a> SyncSession<'a> {
    pub fn new(
        identity: &'a ChainIdentity,
        device_id: Ulid,
        device_name: impl Into<String>,
        state: SyncState,
    ) -> Self {
        Self {
            identity,
            device_id,
            device_name: device_name.into(),
            state,
            revoked: Vec::new(),
        }
    }

    /// Encrypt our current state for the chain.
    pub fn seal_state(&self) -> Result<Vec<u8>, SyncError> {
        let env = SyncEnvelope {
            device_id: self.device_id,
            state: self.state.clone(),
        };
        let plaintext = serde_json::to_vec(&env)
            .map_err(|e| SyncError::Malformed(e.to_string()))?;
        self.identity.encrypt(&plaintext).map_err(Into::into)
    }

    /// Decrypt and merge a peer envelope. Returns the peer's pre-merge state.
    pub fn accept_blob(&mut self, blob: &[u8]) -> Result<SyncState, SyncError> {
        let plaintext = self.identity.decrypt(blob)?;
        let env: SyncEnvelope = serde_json::from_str(
            &String::from_utf8(plaintext)
                .map_err(|e| SyncError::Malformed(e.to_string()))?,
        )
        .map_err(|e| SyncError::Malformed(e.to_string()))?;

        if self.revoked.contains(&format!("{:?}", env.device_id)) {
            return Err(SyncError::Revoked);
        }
        self.state.merge(&env.state);
        Ok(env.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::SyncState;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use kal_core::models::{CalendarItem, ItemKind};

    const PHRASE_A: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    // A different, valid 12-word phrase.
    const PHRASE_B: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";

    /// In-memory mailboxes keyed by logical address.
    #[derive(Default)]
    struct LoopbackTransport {
        boxes: RefCell<HashMap<String, Vec<Vec<u8>>>>,
    }

    impl LoopbackTransport {
        fn deliver_to(&self, addr: &str, blob: Vec<u8>) {
            self.boxes.borrow_mut().entry(addr.to_string()).or_default().push(blob);
        }
    }

    impl Transport for LoopbackTransport {
        fn send(&self, addr: &str, blob: &[u8]) -> Result<(), SyncError> {
            self.deliver_to(addr, blob.to_vec());
            Ok(())
        }
        fn recv(&self) -> Option<(String, Vec<u8>)> {
            None // tests pull directly from boxes
        }
    }

    fn sample_item(title: &str, ts_secs: i64) -> CalendarItem {
        use chrono::TimeZone;
        let start = chrono::Utc.timestamp_opt(ts_secs, 0).single().unwrap().fixed_offset();
        CalendarItem::new(ItemKind::Event, title, ulid::Ulid::new(), start)
    }

    fn make_session<'a>(identity: &'a ChainIdentity, name: &str, state: SyncState) -> SyncSession<'a> {
        SyncSession::new(identity, Ulid::new(), name, state)
    }

    #[test]
    fn two_replicas_exchange_states_and_converge() {
        let id1 = ChainIdentity::from_phrase(PHRASE_A).unwrap();
        let id2 = ChainIdentity::from_phrase(PHRASE_A).unwrap();

        let mut s1_state = SyncState::default();
        s1_state.items.insert(
            sample_item("laptop-event", 1000).id,
            sample_item("laptop-event", 1000),
        );
        let mut s2_state = SyncState::default();
        s2_state.items.insert(
            sample_item("phone-event", 2000).id,
            sample_item("phone-event", 2000),
        );

        let mut sess1 = make_session(&id1, "Laptop", s1_state);
        let mut sess2 = make_session(&id2, "Phone", s2_state);

        let net = LoopbackTransport::default();
        net.send("peer", &sess1.seal_state().unwrap()).unwrap();
        let blob = net.boxes.borrow_mut().remove("peer").unwrap().pop().unwrap();
        sess2.accept_blob(&blob).unwrap();

        net.send("peer", &sess2.seal_state().unwrap()).unwrap();
        let blob = net.boxes.borrow_mut().remove("peer").unwrap().pop().unwrap();
        sess1.accept_blob(&blob).unwrap();

        assert_eq!(sess1.state, sess2.state);
        assert_eq!(sess1.state.items.len(), 2);
    }

    #[test]
    fn unpaired_device_cannot_read_or_join() {
        let legit = ChainIdentity::from_phrase(PHRASE_A).unwrap();
        let intruder = ChainIdentity::generate().unwrap();

        let mut victim = make_session(&legit, "Victim", SyncState::default());
        victim.state.items.insert(
            sample_item("secret", 3000).id,
            sample_item("secret", 3000),
        );
        let blob = victim.seal_state().unwrap();

        // Intruder lacks the chain key entirely.
        assert!(intruder.decrypt(&blob).is_err());

        // Even a valid phrase of another chain fails at accept time.
        let other = ChainIdentity::from_phrase(PHRASE_B).unwrap();
        let mut outsider = make_session(&other, "Outsider", SyncState::default());
        assert!(outsider.accept_blob(&blob).is_err());
        assert!(outsider.state.items.is_empty());
    }

    #[test]
    fn revoked_device_envelopes_are_rejected() {
        let id = ChainIdentity::from_phrase(PHRASE_A).unwrap();
        let rogue_identity = ChainIdentity::from_phrase(PHRASE_A).unwrap();

        let rogue = make_session(&rogue_identity, "Rogue", SyncState::default());
        let blob = rogue.seal_state().unwrap();

        let mut main = make_session(&id, "Main", SyncState::default());
        main.revoked.push(format!("{:?}", rogue.device_id));

        assert!(matches!(main.accept_blob(&blob), Err(SyncError::Revoked)));
    }

    #[test]
    fn corrupted_blob_is_malformed_not_panic() {
        let id = ChainIdentity::from_phrase(PHRASE_A).unwrap();
        let mut sess = make_session(&id, "X", SyncState::default());
        let mut blob = sess.seal_state().unwrap();
        if !blob.is_empty() {
            let mid = blob.len() / 2;
            blob[mid] ^= 0xFF;
        }
        assert!(sess.accept_blob(&blob).is_err());
    }
}
