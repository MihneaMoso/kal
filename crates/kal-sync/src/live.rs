//! Live P2P transport over iroh-gossip with DHT-keyed topic discovery.
//!
//! Every device derives the same gossip [`TopicId`] from the chain fingerprint,
//! publishes its address to a distributed hash table under that topic, and
//! joins the shared gossip swarm. Two devices that hold the same recovery
//! phrase therefore find each other from anywhere on the internet — no shared
//! folder, relay config, or manual addresses needed.
//!
//! This module is native-only: iroh does not support `wasm32`, and the app
//! already gates `kal-sync` out of the web build.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as channel;
use std::sync::{Arc, Mutex};

use distributed_topic_tracker::{
    AutoDiscoveryGossip, Config as TrackerConfig, GossipSender as TrackerSender, RecordPublisher,
    TopicId as TrackerTopicId,
};
use ed25519_dalek::SigningKey;
use iroh::endpoint::presets;
use iroh::protocol::Router;
use iroh::{Endpoint, SecretKey};
use iroh_gossip::net::Gossip;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::keys::ChainIdentity;
use crate::session::{SyncError, Transport};

/// Topic wire name. Stable across devices sharing a chain because it derives
/// from the (chain-wide) fingerprint rather than any per-device value.
fn topic_name(chain: &ChainIdentity) -> String {
    format!("kal-sync/{}", chain.fingerprint().fingerprint_hex)
}

/// A frame broadcast on the gossip topic.
///
/// Wrapping the (already encrypted) blob lets recipients distinguish who sent
/// it: the gossip transport has no notion of a sender id of its own, while
/// kal's sync round skips its own re-broadcasts and names senders by ULID.
#[derive(Debug, Serialize, Deserialize)]
struct GossipFrame {
    from: String,
    blob: Vec<u8>,
}

/// Long-lived live transport for a sync chain.
///
/// Owns a tokio runtime, the iroh endpoint + router, and the gossip topic.
/// `send`/`recv` mirror the folder transport's contract: publish our state and
/// drain anything peers have written, so the caller (or a background loop)
/// drives convergence exactly like the folder path.
pub struct IrohTransport {
    /// Kept alive so the endpoint's background tasks keep running.
    _runtime: tokio::runtime::Runtime,
    endpoint: Endpoint,
    _router: Router,
    sender: TrackerSender,
    /// Gazette of messages pushed by the background receive loop. Wrapped in a
    /// `Mutex` so the transport stays [`Sync`] (a `std` mpsc `Receiver` is not).
    inbox: Mutex<channel::Receiver<(String, Vec<u8>)>>,
    /// Latest sealed state, re-broadcast to newly joined peers automatically.
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    /// Neighbor count — whether the gossip topic currently has peers.
    neighbors: Arc<AtomicUsize>,
    device_id: String,
}

impl IrohTransport {
    /// A fresh random per-device iroh identity. Persist the result next to the
    /// chain identity so restarts keep the same endpoint id.
    pub fn new_node_secret() -> [u8; 32] {
        SecretKey::generate().to_bytes()
    }

    /// Connect to the chain's gossip topic using the public n0 relay + DHT
    /// discovery. Returns once the topic subscription is up; peer discovery
    /// and joining continue in the background (a lone device must not block).
    ///
    /// `node_secret` is this device's long-lived iroh identity and `device_id`
    /// its stable ULID; persist both to keep reconnects fast and revocation
    /// stable across app restarts.
    pub fn connect(
        chain: &ChainIdentity,
        device_id: Ulid,
        node_secret: [u8; 32],
    ) -> Result<Self, SyncError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| SyncError::Live(format!("tokio runtime: {e}")))?;
        let secret = SecretKey::from_bytes(&node_secret);
        let device_str = device_id.to_string();

        let (endpoint, router, sender, inbox, latest, neighbors) = runtime.block_on(async {
            let endpoint = Endpoint::builder(presets::N0)
                .secret_key(secret.clone())
                .bind()
                .await
                .map_err(|e| SyncError::Live(format!("bind endpoint: {e}")))?;

            let gossip = Gossip::builder().spawn(endpoint.clone());
            let router = Router::builder(endpoint.clone())
                .accept(iroh_gossip::ALPN, gossip.clone())
                .spawn();

            // The DHT record's signing key must match the endpoint identity:
            // bootstrap recovers peer node ids from the records' public keys.
            let signing_key = SigningKey::from_bytes(&secret.to_bytes());
            let topic = TrackerTopicId::new(topic_name(chain));
            let initial_secret = topic_name(chain).into_bytes();
            let record_publisher = RecordPublisher::new(
                topic,
                signing_key,
                None,
                initial_secret,
                TrackerConfig::default(),
            );

            let topic = gossip
                .subscribe_and_join_with_auto_discovery_no_wait(record_publisher)
                .await
                .map_err(|e| SyncError::Live(format!("join topic: {e}")))?;
            let (sender, mut receiver) = topic
                .split()
                .await
                .map_err(|e| SyncError::Live(format!("split topic: {e}")))?;

            let (tx, inbox) = channel::channel::<(String, Vec<u8>)>();
            let latest = Arc::new(Mutex::new(None::<Vec<u8>>));
            let neighbors = Arc::new(AtomicUsize::new(0));
            let latest_clone = latest.clone();
            let neighbors_clone = neighbors.clone();
            let sender_clone = sender.clone();
            tokio::spawn(async move {
                let mut last_sent = None::<Vec<u8>>;
                loop {
                    match receiver.next().await {
                        Ok(iroh_gossip::api::Event::Received(message)) => {
                            if let Ok(frame) =
                                serde_json::from_slice::<GossipFrame>(&message.content)
                            {
                                let _ = tx.send((frame.from, frame.blob));
                            }
                        }
                        Ok(iroh_gossip::api::Event::NeighborUp(_)) => {
                            neighbors_clone.fetch_add(1, Ordering::Relaxed);
                            let snapshot = latest_clone.lock().unwrap().clone();
                            if let Some(snapshot) = snapshot {
                                // Push our latest state to the fresh peer without
                                // spamming when nothing changed since last time.
                                if last_sent.as_deref() != Some(snapshot.as_slice())
                                    && sender_clone.broadcast(snapshot.clone()).await.is_ok()
                                {
                                    last_sent = Some(snapshot);
                                }
                            }
                        }
                        Ok(iroh_gossip::api::Event::NeighborDown(_)) => {
                            let _ = neighbors_clone.fetch_update(
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                                |n| Some(n.saturating_sub(1)),
                            );
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            });

            Ok::<_, SyncError>((
                endpoint,
                router,
                sender,
                Mutex::new(inbox),
                latest,
                neighbors,
            ))
        })?;

        Ok(Self {
            _runtime: runtime,
            endpoint,
            _router: router,
            sender,
            inbox,
            latest,
            neighbors,
            device_id: device_str,
        })
    }

    /// Stable device id used as the sender tag in broadcast frames.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Remember the current sealed state so it is re-broadcast to peers that
    /// join later. Call this whenever local state may have changed.
    pub fn set_state(&self, blob: Vec<u8>) {
        *self.latest.lock().unwrap() = Some(blob);
    }

    /// Whether the gossip topic currently has at least one peer connected.
    /// The app uses this to prefer the live channel over the folder transport.
    pub fn is_joined(&self) -> bool {
        self.neighbors.load(Ordering::Relaxed) > 0
    }

    /// The endpoint this transport is bound to (mostly for diagnostics/tests).
    pub fn endpoint_id(&self) -> String {
        format!("{:?}", self.endpoint.id())
    }
}

impl Transport for IrohTransport {
    fn send(&self, _addr: &str, blob: &[u8]) -> Result<(), SyncError> {
        let frame = GossipFrame {
            from: self.device_id.clone(),
            blob: blob.to_vec(),
        };
        let bytes = serde_json::to_vec(&frame)
            .map_err(|e| SyncError::Live(format!("encode frame: {e}")))?;
        self._runtime
            .block_on(self.sender.broadcast(bytes))
            .map_err(|e| SyncError::Live(format!("broadcast: {e}")))?;
        Ok(())
    }

    fn recv(&self) -> Option<(String, Vec<u8>)> {
        self.inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_recv()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn topic_name_is_stable_across_devices_and_unique_across_chains() {
        let a = ChainIdentity::from_phrase(PHRASE).unwrap();
        let b = ChainIdentity::from_phrase(PHRASE).unwrap();
        let other = ChainIdentity::generate().unwrap();
        assert_eq!(topic_name(&a), topic_name(&b));
        assert_ne!(topic_name(&a), topic_name(&other));
    }

    #[test]
    fn frame_round_trips() {
        let frame = GossipFrame {
            from: Ulid::new().to_string(),
            blob: vec![1, 2, 3, 250, 251, 252],
        };
        let bytes = serde_json::to_vec(&frame).unwrap();
        let decoded: GossipFrame = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.from, frame.from);
        assert_eq!(decoded.blob, frame.blob);
    }
}
