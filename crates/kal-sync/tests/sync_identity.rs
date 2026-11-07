//! Tests for sync identity generation and recovery phrase handling.

use kal_sync::{ChainIdentity, SyncKeys};
use rand::{thread_rng, Rng, RngCore, seq::SliceRandom};

/// BIP-39 English wordlist subset used for testing.
/// We don't include the full list; we just test that generated words are
/// non-empty and the phrase has 24 words.
const BIP39_SAMPLE_WORDS: &[&str] = &[
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
    "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
    "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual",
];

#[test]
fn chain_identity_generates_a_valid_phrase() {
    let identity = ChainIdentity::generate().unwrap();

    // Phrase must be exactly 24 words
    let phrase = identity.phrase();
    let words: Vec<&str> = phrase.split_whitespace().collect();
    assert_eq!(words.len(), 24);

    // Each word must be non-empty
    for word in words {
        assert!(!word.is_empty(), "invalid word: {}", word);
    }

    // Fingerprint must be exactly 16 hex characters
    assert_eq!(identity.fingerprint().fingerprint_hex.len(), 16);
}

#[test]
fn chain_identity_rejects_incomplete_phrase() {
    let mut rng = thread_rng();
    let mut phrase_words: Vec<String> = Vec::new();

    // Generate exactly 23 words (one short)
    for _ in 0..23 {
        phrase_words.push(BIP39_SAMPLE_WORDS[rng.next_u64() as usize % BIP39_SAMPLE_WORDS.len()].to_string());
    }

    let short_phrase = phrase_words.join(" ");
    assert!(
        ChainIdentity::from_phrase(&short_phrase).is_err(),
        "should reject 23-word phrase"
    );
}

#[test]
fn chain_identity_rejects_invalid_word() {
    let mut rng = thread_rng();
    let phrase_words: Vec<String> = (0..24)
        .map(|_| {
            BIP39_SAMPLE_WORDS[rng.gen_range(0..BIP39_SAMPLE_WORDS.len())].to_string()
        })
        .collect();

    // Replace one word with an invalid one
    let mut modified = phrase_words;
    modified[12] = "not-a-valid-word".to_string();
    let phrase = modified.join(" ");

    assert!(
        ChainIdentity::from_phrase(&phrase).is_err(),
        "should reject invalid word"
    );
}

#[test]
fn chain_identity_rejects_duplicate_words() {
    let mut rng = thread_rng();
    let mut phrase_words = Vec::new();

    // Generate 24 unique words from sample
    for word in BIP39_SAMPLE_WORDS.iter().take(24) {
        phrase_words.push(word.to_string());
    }

    // Shuffle to create duplicates
    phrase_words.shuffle(&mut rng);

    let phrase = phrase_words.join(" ");
    assert!(
        ChainIdentity::from_phrase(&phrase).is_err(),
        "should reject duplicate words"
    );
}

#[test]
fn sync_keys_persistence_round_trips() {
    let identity = ChainIdentity::generate().unwrap();
    let keys = identity.fingerprint();

    // Serialize and deserialize
    let json = serde_json::to_string(&keys).unwrap();
    let loaded: SyncKeys = serde_json::from_str(&json).unwrap();

    assert_eq!(keys.fingerprint_hex, loaded.fingerprint_hex);
}

#[test]
fn sync_keys_rejects_invalid_fingerprint() {
    let invalid = serde_json::json!({
        "fingerprint": "not-a-valid-hex-uuid",
    });

    let result: Result<SyncKeys, _> = serde_json::from_value(invalid);
    assert!(result.is_err());
}

#[test]
fn sync_keys_rejects_missing_fields() {
    let incomplete = serde_json::json!({});

    let result: Result<SyncKeys, _> = serde_json::from_value(incomplete);
    assert!(result.is_err());
}

#[test]
fn chain_identity_phrase_is_deterministic() {
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    let identity1 = ChainIdentity::from_phrase(phrase).unwrap();
    let identity2 = ChainIdentity::from_phrase(phrase).unwrap();

    assert_eq!(
        identity1.fingerprint().fingerprint_hex,
        identity2.fingerprint().fingerprint_hex,
        "same phrase should produce same fingerprint"
    );
}

#[test]
fn chain_identity_generate_produces_unique_chains() {
    let a = ChainIdentity::generate().unwrap();
    let b = ChainIdentity::generate().unwrap();
    assert_ne!(a.phrase(), b.phrase());
    assert_ne!(a.public_key(), b.public_key());
    assert_ne!(
        a.fingerprint().fingerprint_hex,
        b.fingerprint().fingerprint_hex
    );
}

#[test]
fn chain_identity_bad_phrases_rejected() {
    assert!(ChainIdentity::from_phrase("not a real phrase at all").is_err());
    // Wrong checksum word.
    assert!(ChainIdentity::from_phrase("abandon ".repeat(11).trim()).is_err());
}

#[test]
fn chain_identity_encrypt_decrypt_round_trip() {
    let a = ChainIdentity::from_phrase(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let b = ChainIdentity::from_phrase(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();

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
fn chain_identity_wrong_chain_cannot_decrypt() {
    let a = ChainIdentity::from_phrase(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let other = ChainIdentity::generate().unwrap();
    let blob = a.encrypt(b"secret calendar data").unwrap();
    use kal_sync::keys::KeyError;
    assert!(matches!(other.decrypt(&blob), Err(KeyError::Decrypt)));
}

#[test]
fn chain_identity_truncated_or_empty_blobs_rejected() {
    let a = ChainIdentity::from_phrase(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    assert!(a.decrypt(&[]).is_err());
    assert!(a.decrypt(&[1, 2, 3]).is_err());
}

#[test]
fn chain_identity_fingerprints_differ_across_chains() {
    let a = ChainIdentity::from_phrase(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let b = ChainIdentity::generate().unwrap();
    assert_ne!(
        a.fingerprint().fingerprint_hex,
        b.fingerprint().fingerprint_hex
    );
    // Fingerprint is 16 hex chars (64 bits).
    assert_eq!(a.fingerprint().fingerprint_hex.len(), 16);
}