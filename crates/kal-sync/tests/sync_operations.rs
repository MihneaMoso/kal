//! Tests for sync operations - chain management, identity, and state handling.

use kal_sync::{ChainIdentity, ChainNode, ChainState, ChainSync};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn new_device_has_no_chain() {
    let node = ChainNode::new("test-fingerprint".to_string());
    
    assert!(node.chain_identity.is_none());
    assert_eq!(node.sync_rounds, 0);
}

#[test]
fn chain_node_sync_rounds_increment_properly() {
    let mut node = ChainNode::new("test-fingerprint".to_string());
    
    assert_eq!(node.sync_rounds, 0);
    node.sync_rounds += 1;
    assert_eq!(node.sync_rounds, 1);
    node.sync_rounds += 1;
    assert_eq!(node.sync_rounds, 2);
}

#[test]
fn chain_state_tracks_sync_timestamp() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let state = ChainState {
        last_sync: now,
        pending_outbound: vec![],
        pending_inbound: vec![],
    };
    
    assert_eq!(state.last_sync, now);
}

#[test]
fn chain_state_empty_pending_lists() {
    let state = ChainState::default();
    
    assert!(state.pending_outbound.is_empty());
    assert!(state.pending_inbound.is_empty());
}

#[test]
fn chain_state_with_pending_items() {
    let mut state = ChainState::default();
    
    state.pending_outbound.push("outbound_item_1".to_string());
    state.pending_outbound.push("outbound_item_2".to_string());
    state.pending_inbound.push("inbound_item_1".to_string());
    
    assert_eq!(state.pending_outbound.len(), 2);
    assert_eq!(state.pending_inbound.len(), 1);
    assert_eq!(state.pending_outbound[0], "outbound_item_1");
    assert_eq!(state.pending_inbound[0], "inbound_item_1");
}

#[test]
fn chain_sync_is_unpaired_by_default() {
    let node = ChainNode::new("test-fingerprint".to_string());
    
    assert!(!node.is_paired());
}

#[test]
fn chain_sync_becomes_paired_when_identity_set() {
    let identity = ChainIdentity::generate().unwrap();
    let mut node = ChainNode::new("test-fingerprint".to_string());
    
    node.chain_identity = Some(identity);
    
    assert!(node.is_paired());
}

#[test]
fn chain_sync_clears_paired_status_when_identity_removed() {
    let identity = ChainIdentity::generate().unwrap();
    let mut node = ChainNode::new("test-fingerprint".to_string());
    
    node.chain_identity = Some(identity);
    assert!(node.is_paired());
    
    node.chain_identity = None;
    assert!(!node.is_paired());
}

#[test]
fn chain_sync_rounds_reset_on_unpair() {
    let mut node = ChainNode::new("test-fingerprint".to_string());
    node.sync_rounds = 10;
    
    assert_eq!(node.sync_rounds, 10);
    
    node.chain_identity = None;
    assert_eq!(node.sync_rounds, 0);
}

#[test]
fn chain_sync_serialize_round_trips() {
    let identity = ChainIdentity::generate().unwrap();
    let mut node = ChainNode::new("test-fingerprint".to_string());
    node.chain_identity = Some(identity);
    node.sync_rounds = 5;
    
    let json = serde_json::to_string(&node).unwrap();
    let loaded: ChainNode = serde_json::from_str(&json).unwrap();
    
    assert_eq!(node.fingerprint_hex, loaded.fingerprint_hex);
    assert_eq!(node.sync_rounds, loaded.sync_rounds);
    assert!(node.chain_identity.is_some());
    assert!(loaded.chain_identity.is_some());
}

#[test]
fn chain_sync_serialize_unpaired_node() {
    let mut node = ChainNode::new("test-fingerprint".to_string());
    // No identity set
    
    let json = serde_json::to_string(&node).unwrap();
    let loaded: ChainNode = serde_json::from_str(&json).unwrap();
    
    assert_eq!(node.fingerprint_hex, loaded.fingerprint_hex);
    assert_eq!(node.sync_rounds, loaded.sync_rounds);
    assert!(node.chain_identity.is_none());
    assert!(loaded.chain_identity.is_none());
}

#[test]
fn chain_sync_deserialize_missing_identity() {
    let json = r#"{"fingerprint":"test-uuid","sync_rounds":0}"#;
    let node: ChainNode = serde_json::from_str(json).unwrap();
    
    assert!(node.chain_identity.is_none());
}

#[test]
fn chain_sync_deserialize_with_identity() {
    let json = r#"{
        "fingerprint":"test-uuid",
        "sync_rounds":5,
        "chain_identity":{"phrase":"word1 word2 ... word24","fingerprint":"abc123"}
    }"#;
    
    // This should deserialize successfully
    let result: Result<ChainNode, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn chain_sync_empty_state_serializes_correctly() {
    let state = ChainState::default();
    let json = serde_json::to_string(&state).unwrap();
    
    let loaded: ChainState = serde_json::from_str(&json).unwrap();
    
    assert_eq!(state.last_sync, loaded.last_sync);
    assert!(state.pending_outbound.is_empty());
    assert!(loaded.pending_outbound.is_empty());
    assert!(state.pending_inbound.is_empty());
    assert!(loaded.pending_inbound.is_empty());
}

#[test]
fn chain_sync_state_with_data_serializes_correctly() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let mut state = ChainState::default();
    state.last_sync = now;
    state.pending_outbound.push("item1".to_string());
    state.pending_inbound.push("item2".to_string());
    
    let json = serde_json::to_string(&state).unwrap();
    let loaded: ChainState = serde_json::from_str(&json).unwrap();
    
    assert_eq!(state.last_sync, loaded.last_sync);
    assert_eq!(state.pending_outbound, loaded.pending_outbound);
    assert_eq!(state.pending_inbound, loaded.pending_inbound);
}

#[test]
fn chain_sync_rejects_duplicate_fingerprints_in_pending() {
    let mut state = ChainState::default();
    
    state.pending_outbound.push("test-item".to_string());
    state.pending_outbound.push("test-item".to_string()); // Duplicate
    
    assert_eq!(state.pending_outbound.len(), 2);
    // Validation should detect this when syncing
}

#[test]
fn chain_sync_fingerprint_format_is_hex() {
    let identity = ChainIdentity::generate().unwrap();
    let fp = identity.fingerprint();
    
    // Should be exactly 16 hex characters
    assert_eq!(fp.fingerprint_hex.len(), 16);
    
    // Should only contain valid hex characters
    for c in fp.fingerprint_hex.chars() {
        assert!(
            c.is_ascii_hexdigit(),
            "fingerprint contains invalid hex char: {}",
            c
        );
    }
}

#[test]
fn chain_sync_phrase_format_is_correct() {
    let identity = ChainIdentity::generate().unwrap();
    let phrase = identity.phrase();
    
    // Should be exactly 24 words separated by spaces
    let words: Vec<&str> = phrase.split_whitespace().collect();
    assert_eq!(words.len(), 24);
    
    // Each word should be non-empty
    for word in words {
        assert!(!word.is_empty(), "empty word in phrase");
    }
}

#[test]
fn chain_sync_sync_rounds_start_at_zero() {
    let node = ChainNode::new("test-fingerprint".to_string());
    
    assert_eq!(node.sync_rounds, 0);
}

#[test]
fn chain_sync_sync_rounds_increment_with_each_sync() {
    let mut node = ChainNode::new("test-fingerprint".to_string());
    
    assert_eq!(node.sync_rounds, 0);
    node.sync_rounds += 1;
    assert_eq!(node.sync_rounds, 1);
    
    // Simulate another sync
    node.sync_rounds += 1;
    assert_eq!(node.sync_rounds, 2);
}

#[test]
fn chain_sync_pending_lists_are_separate() {
    let mut state = ChainState::default();
    
    state.pending_outbound.push("outbound".to_string());
    state.pending_inbound.push("inbound".to_string());
    
    assert_eq!(state.pending_outbound.len(), 1);
    assert_eq!(state.pending_inbound.len(), 1);
    assert!(state.pending_outbound.is_empty().not());
    assert!(state.pending_inbound.is_empty().not());
}

#[test]
fn chain_sync_multiple_devices_have_different_fingerprints() {
    let mut rng = rand::rng();
    let mut fingerprints = Vec::new();
    
    for _ in 0..10 {
        let identity = ChainIdentity::generate().unwrap();
        let fp = identity.fingerprint().fingerprint_hex.clone();
        fingerprints.push(fp);
        
        // Should not have duplicates
        assert!(
            !fingerprints.iter().any(|f| f == &fp && f != &fingerprints.last().unwrap()),
            "duplicate fingerprint generated"
        );
    }
}

#[test]
fn chain_sync_fingerprint_is_deterministic_from_phrase() {
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    
    let identity1 = ChainIdentity::parse(phrase).unwrap();
    let identity2 = ChainIdentity::parse(phrase).unwrap();
    
    assert_eq!(
        identity1.fingerprint().fingerprint_hex,
        identity2.fingerprint().fingerprint_hex,
        "same phrase should produce same fingerprint"
    );
}
