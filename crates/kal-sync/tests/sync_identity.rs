//! Tests for sync identity generation and recovery phrase handling.

use kal_sync::{ChainIdentity, ChainNode, ChainState};
use rand::Rng;

#[test]
fn chain_identity_generates_a_valid_phrase() {
    let identity = ChainIdentity::generate().unwrap();
    
    // Phrase must be exactly 24 words
    let words: Vec<&str> = identity.phrase().split_whitespace().collect();
    assert_eq!(words.len(), 24);
    
    // Each word must be in BIP-39 English wordlist
    for word in words {
        assert!({
            let words = include_str!("../../../src/bip39_wordlist.txt")
                .split_whitespace()
                .collect::<Vec<_>>();
            words.contains(&word)
        }, "invalid word: {}", word);
    }
    
    // Fingerprint must be exactly 16 hex characters
    assert_eq!(identity.fingerprint().fingerprint_hex.len(), 16);
}

#[test]
fn chain_identity_rejects_incomplete_phrase() {
    let mut rng = rand::rng();
    let mut phrase_words: Vec<String> = Vec::new();
    
    // Generate exactly 23 words (one short)
    for _ in 0..23 {
        phrase_words.push({
            let words = include_str!("../../../src/bip39_wordlist.txt")
                .split_whitespace()
                .collect::<Vec<_>>();
            rng.choose(words).unwrap().to_string()
        });
    }
    
    let short_phrase = phrase_words.join(" ");
    assert!(ChainIdentity::parse(&short_phrase).is_err(), "should reject 23-word phrase");
}

#[test]
fn chain_identity_rejects_invalid_word() {
    let mut rng = rand::rng();
    let phrase_words: Vec<String> = (0..24)
        .map(|_| {
            let words = include_str!("../../../src/bip39_wordlist.txt")
                .split_whitespace()
                .collect::<Vec<_>>();
            rng.choose(words).unwrap().to_string()
        })
        .collect();
    
    // Replace one word with an invalid one
    phrase_words[12] = "not-a-valid-word".to_string();
    let phrase = phrase_words.join(" ");
    
    assert!(ChainIdentity::parse(&phrase).is_err(), "should reject invalid word");
}

#[test]
fn chain_identity_rejects_duplicate_words() {
    let mut rng = rand::rng();
    let mut phrase_words = Vec::new();
    
    // Generate 24 unique words
    let words = include_str!("../../../src/bip39_wordlist.txt")
        .split_whitespace()
        .collect::<Vec<_>>();
    
    for word in words.iter().take(24) {
        phrase_words.push(word.to_string());
    }
    
    // Shuffle to create duplicates
    rng.shuffle(&mut phrase_words);
    
    let phrase = phrase_words.join(" ");
    assert!(ChainIdentity::parse(&phrase).is_err(), "should reject duplicate words");
}

#[test]
fn chain_node_persistence_round_trips() {
    let identity = ChainIdentity::generate().unwrap();
    let node = ChainNode::new(identity.fingerprint());
    
    // Serialize and deserialize
    let json = serde_json::to_string(&node).unwrap();
    let loaded: ChainNode = serde_json::from_str(&json).unwrap();
    
    assert_eq!(node.fingerprint_hex, loaded.fingerprint_hex);
    assert_eq!(node.sync_rounds, loaded.sync_rounds);
}

#[test]
fn chain_state_persistence_round_trips() {
    let identity = ChainIdentity::generate().unwrap();
    let state = ChainState {
        last_sync: 0,
        pending_outbound: vec![],
        pending_inbound: vec![],
    };
    
    // Serialize and deserialize
    let json = serde_json::to_string(&state).unwrap();
    let loaded: ChainState = serde_json::from_str(&json).unwrap();
    
    assert_eq!(state.last_sync, loaded.last_sync);
    assert_eq!(state.pending_outbound.len(), loaded.pending_outbound.len());
    assert_eq!(state.pending_inbound.len(), loaded.pending_inbound.len());
}

#[test]
fn chain_node_rejects_invalid_fingerprint() {
    let invalid = serde_json::json!({
        "fingerprint": "not-a-valid-hex-uuid",
        "sync_rounds": 0
    });
    
    let result: Result<ChainNode, _> = serde_json::from_value(invalid);
    assert!(result.is_err());
}

#[test]
fn chain_node_rejects_missing_fields() {
    let incomplete = serde_json::json!({
        "sync_rounds": 0
    });
    
    let result: Result<ChainNode, _> = serde_json::from_value(incomplete);
    assert!(result.is_err());
}

#[test]
fn chain_state_rejects_missing_fields() {
    let incomplete = serde_json::json!({
        "last_sync": 0
    });
    
    let result: Result<ChainState, _> = serde_json::from_value(incomplete);
    assert!(result.is_err());
}

#[test]
fn chain_state_rejects_invalid_fields() {
    let invalid = serde_json::json!({
        "last_sync": "not-a-number",
        "pending_outbound": ["not-an-array"],
        "pending_inbound": "not-an-array"
    });
    
    let result: Result<ChainState, _> = serde_json::from_value(invalid);
    assert!(result.is_err());
}
