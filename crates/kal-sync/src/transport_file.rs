//! Folder-based gossip transport (spec §5.4 step 3, LAN-independent variant).
//!
//! Encrypted envelopes are written as opaque `.kalblob` files into a shared
//! directory. Point that directory at anything that moves files between your
//! devices (Syncthing, Dropbox, a USB stick, an SSH mount) and sync works
//! with zero infrastructure. A future iroh/mDNS transport implements the
//! same [`Transport`] trait for live P2P.

use std::path::PathBuf;

use ulid::Ulid;

use crate::session::{SyncError, Transport};

pub struct FileTransport {
    dir: PathBuf,
}

impl FileTransport {
    pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn blob_name(sender_device: Ulid, salt: u128) -> String {
        format!("{sender_device}-{salt:x}.kalblob")
    }
}

impl Transport for FileTransport {
    fn send(&self, addr: &str, blob: &[u8]) -> Result<(), SyncError> {
        let sender: Ulid = addr
            .parse()
            .map_err(|_| SyncError::Malformed("addr must be a ULID".into()))?;
        let name = Self::blob_name(sender, ulid::Ulid::new().0);
        std::fs::write(self.dir.join(name), blob)
            .map_err(|e| SyncError::Malformed(format!("write failed: {e}")))?;
        Ok(())
    }

    /// Pops one pending blob as `(sender_device_id, bytes)`; the caller is
    /// responsible for skipping its own device id.
    fn recv(&self) -> Option<(String, Vec<u8>)> {
        for entry in self.dir.read_dir().ok()?.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".kalblob") {
                continue;
            }
            let sender = name.split('-').next().unwrap_or_default().to_string();
            match std::fs::read(entry.path()) {
                Ok(bytes) => {
                    // Consume the file so it isn't re-delivered.
                    let _ = std::fs::remove_file(entry.path());
                    return Some((sender, bytes));
                }
                Err(_) => continue,
            }
        }
        None
    }
}

/// Convenience loop used by the UI: drain all pending blobs into a callback.
pub fn drain<F>(transport: &FileTransport, mut on_blob: F)
where
    F: FnMut(String, Vec<u8>),
{
    while let Some((sender, bytes)) = transport.recv() {
        on_blob(sender, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::SyncState;
    use crate::keys::ChainIdentity;
    use crate::session::SyncSession;

    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn blobs_move_between_outboxes_and_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = tmp.path().join("outbox");

        let id1 = ChainIdentity::from_phrase(PHRASE).unwrap();
        let id2 = ChainIdentity::from_phrase(PHRASE).unwrap();

        let dev1 = Ulid::new();
        let dev2 = Ulid::new();

        let t1 = FileTransport::new(&shared).unwrap();
        let t2 = FileTransport::new(&shared).unwrap();

        let mut s1 = SyncSession::new(&id1, dev1, "one", SyncState::default());
        let mut s2 = SyncSession::new(&id2, dev2, "two", SyncState::default());

        // Device 1 publishes its state.
        t1.send(&dev1.to_string(), &s1.seal_state().unwrap()).unwrap();

        // Device 2 drains exactly one blob and accepts it.
        let mut received = 0;
        drain(&t2, |sender, bytes| {
            received += 1;
            assert_eq!(sender, dev1.to_string());
            s2.accept_blob(&bytes).expect("decrypt + merge");
        });
        assert_eq!(received, 1);

        // Outbox drained; second pass yields nothing.
        let mut again = 0;
        drain(&t2, |_, _| again += 1);
        assert_eq!(again, 0);

        // Reverse direction merges back; states converge.
        t2.send(&dev2.to_string(), &s2.seal_state().unwrap()).unwrap();
        drain(&t1, |_, bytes| {
            s1.accept_blob(&bytes).unwrap();
        });
        assert_eq!(s1.state.items.len(), 0); // both empty, but identical
        assert_eq!(s1.state, s2.state);
    }

    #[test]
    fn garbage_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = tmp.path().join("outbox");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("notes.txt"), b"not a blob").unwrap();

        let t = FileTransport::new(&shared).unwrap();
        assert!(t.recv().is_none());
    }
}
