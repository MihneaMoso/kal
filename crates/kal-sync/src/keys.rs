//! Sync-chain identity & encryption (spec §5.4 steps 1–3).
//!
//! A 24-word BIP39 recovery phrase is the root of trust. From its seed we
//! deterministically derive:
//! - an X25519 identity keypair (device fingerprint = SHA-256 of the pubkey),
//! - a symmetric XChaCha20-Poly1305 key for payload encryption.
//!
//! Nothing is transmitted until a peer proves possession of the same phrase;
//! the relay (if used) only ever sees opaque ciphertext.

use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("invalid recovery phrase")]
    BadPhrase,
    #[error("decryption failed (wrong chain or tampered payload)")]
    Decrypt,
}

type Result<T> = std::result::Result<T, KeyError>;

/// Full sync-chain identity held by every paired device.
pub struct ChainIdentity {
    pub mnemonic_words: Vec<String>,
    secret: StaticSecret,
    cipher: XChaCha20Poly1305,
}

/// Derived material, safe to persist (the phrase itself should stay
/// user-memorized / securely stored).
pub struct SyncKeys {
    /// Hex-encoded X25519 public key — the device fingerprint shown in UI.
    pub fingerprint_hex: String,
}

impl ChainIdentity {
    /// Create a brand-new sync chain with a fresh 24-word phrase.
    pub fn generate() -> Result<Self> {
        let mnemonic = bip39::Mnemonic::generate(24).map_err(|_| KeyError::BadPhrase)?;
        Self::from_mnemonic(&mnemonic)
    }

    /// Join an existing chain by entering the recovery phrase.
    pub fn from_phrase(phrase: &str) -> Result<Self> {
        let normalized = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
        let mnemonic = bip39::Mnemonic::parse_in(bip39::Language::English, normalized)
            .map_err(|_| KeyError::BadPhrase)?;
        Self::from_mnemonic(&mnemonic)
    }

    fn from_mnemonic(mnemonic: &bip39::Mnemonic) -> Result<Self> {
        let seed = mnemonic.to_seed_normalized("Kal sync chain");

        // Symmetric payload key.
        let sym_key: [u8; 32] = Sha256::digest(chain(&seed, b"payload-key")).into();
        let cipher =
            XChaCha20Poly1305::new_from_slice(&sym_key).map_err(|_| KeyError::BadPhrase)?;

        // Identity keypair.
        let id_bytes: [u8; 32] = Sha256::digest(chain(&seed, b"identity")).into();
        let secret = StaticSecret::from(id_bytes);

        Ok(Self {
            mnemonic_words: mnemonic.words().map(str::to_string).collect(),
            secret,
            cipher,
        })
    }

    pub fn words(&self) -> &[String] {
        &self.mnemonic_words
    }

    pub fn phrase(&self) -> String {
        self.mnemonic_words.join(" ")
    }

    /// Public key of this device's chain identity.
    pub fn public_key(&self) -> PublicKey {
        PublicKey::from(&self.secret)
    }

    /// Short human-comparable device fingerprint (first 16 hex chars).
    pub fn fingerprint(&self) -> SyncKeys {
        let digest = Sha256::digest(self.public_key().as_bytes());
        SyncKeys {
            fingerprint_hex: hex(&digest[..8]),
        }
    }

    /// Encrypt bytes under the chain key: output = nonce ‖ ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut out = nonce.to_vec();
        let ct = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| KeyError::Decrypt)?;
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Decrypt a payload produced by [`encrypt`].
    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>> {
        if blob.len() < 24 {
            return Err(KeyError::Decrypt);
        }
        let (nonce, ct) = blob.split_at(24);
        self.cipher
            .decrypt(XNonce::from_slice(nonce), ct)
            .map_err(|_| KeyError::Decrypt)
    }
}

fn chain(seed: &[u8], tag: &[u8]) -> [u8; 64] {
    // Domain-separated stretch: H(H(seed‖tag)‖seed).
    let mut h = Sha256::new();
    h.update(seed);
    h.update(tag);
    let first = h.finalize();
    let mut h2 = Sha256::new();
    h2.update(first);
    h2.update(seed);
    let mut out = [0u8; 64];
    let second = h2.finalize();
    out[..32].copy_from_slice(&second);
    let third = Sha256::digest(second);
    out[32..].copy_from_slice(&third);
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PHRASE: &str = "        abandon abandon abandon abandon abandon abandon abandon         abandon abandon abandon abandon about";

    #[test]
    fn same_phrase_yields_same_keys_on_both_devices() {
        let a = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        let b = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        assert_eq!(a.public_key(), b.public_key());
        assert_eq!(
            a.fingerprint().fingerprint_hex,
            b.fingerprint().fingerprint_hex
        );
    }

    #[test]
    fn generate_produces_24_words_and_unique_chains() {
        let a = ChainIdentity::generate().unwrap();
        let b = ChainIdentity::generate().unwrap();
        assert_eq!(a.words().len(), 24);
        assert_ne!(a.phrase(), b.phrase());
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn bad_phrases_rejected() {
        assert!(ChainIdentity::from_phrase("not a real phrase at all").is_err());
        // Wrong checksum word.
        assert!(ChainIdentity::from_phrase(&"abandon ".repeat(11).trim().to_string()).is_err());
    }

    #[test]
    fn encrypt_decrypt_round_trip_between_devices() {
        let a = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        let b = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();

        let payload = br#"{"device_id":"...","state":{}}"#;
        let blob = a.encrypt(payload).unwrap();

        // Same chain decrypts.
        assert_eq!(b.decrypt(&blob).unwrap(), payload.to_vec());

        // Ciphertext differs each time (fresh nonce) but still decrypts.
        let blob2 = a.encrypt(payload).unwrap();
        assert_ne!(blob, blob2);
        assert_eq!(b.decrypt(&blob2).unwrap(), payload.to_vec());
    }

    #[test]
    fn wrong_chain_cannot_decrypt() {
        let a = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        let other = ChainIdentity::generate().unwrap();
        let blob = a.encrypt(b"secret calendar data").unwrap();
        assert!(matches!(other.decrypt(&blob), Err(KeyError::Decrypt)));
    }

    #[test]
    fn truncated_or_empty_blobs_rejected() {
        let a = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        assert!(a.decrypt(&[]).is_err());
        assert!(a.decrypt(&[1, 2, 3]).is_err());
    }

    #[test]
    fn fingerprints_differ_across_chains() {
        let a = ChainIdentity::from_phrase(TEST_PHRASE).unwrap();
        let b = ChainIdentity::generate().unwrap();
        assert_ne!(
            a.fingerprint().fingerprint_hex,
            b.fingerprint().fingerprint_hex
        );
        // Fingerprint is 16 hex chars (64 bits).
        assert_eq!(a.fingerprint().fingerprint_hex.len(), 16);
    }
}
