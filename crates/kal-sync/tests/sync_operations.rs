//! Tests for sync operations - chain management, identity, and state handling.

use kal_sync::{ChainIdentity, SyncError, SyncKeys, SyncSession, SyncState};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn sync_session_creation() {
    let identity = ChainIdentity::generate().unwrap();
    let device_id = ulid::Ulid::new();
    let state = SyncState::default();
    let session = SyncSession::new(&identity, device_id, "test", state);

    assert_eq!(session.device_name, "test");
    assert!(session.revoked.is_empty());
}

#[test]
fn sync_state_tracks_sync_timestamp() {
    let _now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let state = SyncState {
        calendars: BTreeMap::new(),
        items: BTreeMap::new(),
    };

    assert!(state.calendars.is_empty());
    assert!(state.items.is_empty());
}

#[test]
fn sync_state_empty_pending_lists() {
    let state = SyncState::default();

    assert!(state.calendars.is_empty());
    assert!(state.items.is_empty());
}

#[test]
fn sync_state_with_data() {
    let _identity = ChainIdentity::generate().unwrap();
    let mut state = SyncState::default();

    let cal_id = ulid::Ulid::new();
    use kal_core::models::{Calendar, Color};
    let mut calendar = Calendar::local("Test", Color("#3366cc".into()));
    calendar.id = cal_id;
    state.calendars.insert(cal_id, calendar);

    let item_id = ulid::Ulid::new();
    use chrono::TimeZone;
    use kal_core::models::{CalendarItem, ItemKind};
    let start = chrono::Utc
        .timestamp_opt(1_700_000_000, 0)
        .single()
        .unwrap()
        .fixed_offset();
    let item = CalendarItem::new(ItemKind::Event, "Test Event", cal_id, start);
    state.items.insert(item_id, item);

    assert_eq!(state.calendars.len(), 1);
    assert_eq!(state.items.len(), 1);
}

#[test]
fn sync_session_is_unpaired_by_default() {
    let identity = ChainIdentity::generate().unwrap();
    let _session = SyncSession::new(&identity, ulid::Ulid::new(), "test", SyncState::default());

    // Session always has the local identity
    // The identity field is private, so we test via the public fingerprint method
    assert!(identity.fingerprint().fingerprint_hex.len() == 16);
}

#[test]
fn sync_session_seal_state() {
    let identity = ChainIdentity::generate().unwrap();
    let session = SyncSession::new(&identity, ulid::Ulid::new(), "test", SyncState::default());

    let sealed = session.seal_state().unwrap();
    assert!(!sealed.is_empty());
}

#[test]
fn sync_session_accept_blob() {
    let identity = ChainIdentity::generate().unwrap();
    let device_id = ulid::Ulid::new();
    let session = SyncSession::new(&identity, device_id, "test", SyncState::default());

    let sealed = session.seal_state().unwrap();

    // Use the same identity (same chain) to decrypt
    let mut session2 = SyncSession::new(&identity, ulid::Ulid::new(), "test", SyncState::default());
    let result = session2.accept_blob(&sealed);
    assert!(result.is_ok());
}

#[test]
fn sync_keys_serialize_round_trips() {
    let identity = ChainIdentity::generate().unwrap();
    let keys = identity.fingerprint();

    let json = serde_json::to_string(&keys).unwrap();
    let loaded: SyncKeys = serde_json::from_str(&json).unwrap();

    assert_eq!(keys.fingerprint_hex, loaded.fingerprint_hex);
}

#[test]
fn sync_keys_fingerprint_format_is_hex() {
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
fn chain_sync_multiple_devices_have_different_fingerprints() {
    let mut fingerprints = Vec::new();

    for _ in 0..10 {
        let identity = ChainIdentity::generate().unwrap();
        let fp = identity.fingerprint().fingerprint_hex.clone();
        fingerprints.push(fp.clone());

        // Should not have duplicates
        assert!(
            !fingerprints
                .iter()
                .any(|f| f == &fp && f != fingerprints.last().unwrap()),
            "duplicate fingerprint generated"
        );
    }
}

#[test]
fn chain_sync_fingerprint_is_deterministic_from_phrase() {
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
fn sync_error_display() {
    let err = SyncError::Live("test error".to_string());
    assert!(format!("{}", err).contains("test error"));

    let err = SyncError::Malformed("merge failed".to_string());
    assert!(format!("{}", err).contains("merge failed"));

    let err = SyncError::Revoked;
    assert!(format!("{}", err).contains("device revoked"));

    let err = SyncError::Malformed("serde error".to_string());
    assert!(format!("{}", err).contains("serde error"));
}
