//! Sync-chain pairing screen (spec §5.4 steps 1–2, §5.4 step 5).
//!
//! Identity is stored next to the calendar DB as plain JSON; the phrase never
//! leaves the device unencrypted except on screen while pairing.

use std::path::PathBuf;

use dioxus::prelude::*;

use serde::{Deserialize, Serialize};

use kal_sync::{ChainIdentity, FileTransport, SyncSession, SyncState, Transport as _};

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    phrase: String,
}

pub fn identity_path(db_path: &std::path::Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("sync-identity.json")
}

fn db_file() -> PathBuf {
    crate::default_db_path().unwrap_or_else(|| PathBuf::from("kal.db"))
}

/// Load or create nothing — returns `None` when this device has no chain yet.
pub fn load_identity() -> Option<ChainIdentity> {
    let text = std::fs::read_to_string(identity_path(&db_file())).ok()?;
    let stored: StoredIdentity = serde_json::from_str(&text).ok()?;
    ChainIdentity::from_phrase(&stored.phrase).ok()
}

fn persist(identity: &ChainIdentity) -> Result<(), String> {
    let path = identity_path(&db_file());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&StoredIdentity {
        phrase: identity.phrase(),
    })
    .map_err(|e| e.to_string())?;
    // Restrict to owner-only where supported.
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Remove this device from the sync chain: deletes the local identity file and
/// the device-local outbox folder. This is a local-only act — the gossip
/// chain has no central membership to revoke — so other devices simply stop
/// receiving snapshots from this one.
fn leave_chain() {
    let path = identity_path(&db_file());
    let _ = std::fs::remove_file(&path);
    if let Some(outbox) = path.parent().map(|p| p.join("sync-outbox")) {
        let _ = std::fs::remove_dir_all(&outbox);
    }
}

/// Bumped after a successful merge so App-level effects restart resources.
pub static RESOURCES_DIRTY: GlobalSignal<u32> = Signal::global(|| 0);

/// One full gossip round against the shared outbox folder.
fn run_sync_once(db: &crate::DbHandle) -> Result<usize, String> {
    let identity = load_identity().ok_or("not paired")?;
    let device_id = ulid::Ulid::new(); // per-round id; stable ids land with settings store
    let outbox = identity_path(&db_file())
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("sync-outbox");
    let transport = FileTransport::new(&outbox).map_err(|e| e.to_string())?;

    let calendars = db.list_calendars().map_err(|e| e.to_string())?;
    let items = db.list_items(true).map_err(|e| e.to_string())?;
    let mut session = SyncSession::new(
        &identity,
        device_id,
        "desktop",
        SyncState::from_parts(calendars.clone(), items.clone()),
    );

    // Publish our state, then drain peers'.
    transport
        .send(
            &device_id.to_string(),
            &session.seal_state().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let self_id = device_id.to_string();
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

    // Persist everything we now know (upsert is CRDT-safe by construction).
    for cal in session.state.calendars.values() {
        db.upsert_calendar(cal).map_err(|e| e.to_string())?;
    }
    for item in session.state.items.values() {
        db.upsert_item(item).map_err(|e| e.to_string())?;
    }
    Ok(merged)
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncUiState {
    NotPaired,
    /// Just created/joined; show the recovery phrase once.
    ShowPhrase(String),
    Paired {
        fingerprint: String,
    },
    Error(String),
}

#[component]
pub fn SyncPanel() -> Element {
    let mut state = use_signal(|| match load_identity() {
        Some(id) => SyncUiState::Paired {
            fingerprint: id.fingerprint().fingerprint_hex,
        },
        None => SyncUiState::NotPaired,
    });
    let mut join_phrase = use_signal(String::new);

    let start_chain = move |_| match kal_sync::ChainIdentity::generate() {
        Ok(id) => {
            let fingerprint = id.fingerprint().fingerprint_hex;
            match persist(&id) {
                Ok(()) => state.set(SyncUiState::ShowPhrase(id.phrase())),
                Err(e) => state.set(SyncUiState::Error(e)),
            }
            let _ = fingerprint;
        }
        Err(e) => state.set(SyncUiState::Error(e.to_string())),
    };

    let join_chain = move |_| {
        let phrase = join_phrase.read().clone();
        match kal_sync::ChainIdentity::from_phrase(&phrase) {
            Ok(id) => match persist(&id) {
                Ok(()) => state.set(SyncUiState::ShowPhrase(id.phrase())),
                Err(e) => state.set(SyncUiState::Error(e)),
            },
            Err(_) => state.set(SyncUiState::Error("Invalid recovery phrase".to_string())),
        }
    };

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:6px;",
            h2 { "Sync" }
            ProfileHeader {}
            match state.read().clone() {
                SyncUiState::NotPaired => rsx! {
                    button { class: "primary", onclick: start_chain, "Start sync chain" }
                    textarea {
                        rows: 3,
                        placeholder: "Paste 24-word recovery phrase…",
                        value: "{join_phrase}",
                        style: "font-size:11px;",
                        oninput: move |e| join_phrase.set(e.value()),
                    }
                    button { onclick: join_chain, "Join chain…" }
                },
                SyncUiState::ShowPhrase(phrase) => rsx! {
                    div { class: "modal-backdrop",
                        div { class: "modal",
                            h2 { "Your sync chain" }
                            p { style: "font-size:12px;color:var(--fg-muted);",
                                "Write these 24 words down and enter them on your other devices. They grant full access."
                            }
                            div { style: "display:grid;grid-template-columns:repeat(3,1fr);gap:4px;font-size:12px;",
                                for (i, w) in phrase.split_whitespace().enumerate() {
                                    span { key: "{i}", "{i + 1}. {w}" }
                                }
                            }
                            button {
                                class: "primary",
                                style: "margin-top:8px",
                                onclick: move |_| {
                                    state.set(match load_identity() {
                                        Some(id) => SyncUiState::Paired {
                                            fingerprint: id.fingerprint().fingerprint_hex,
                                        },
                                        None => SyncUiState::NotPaired,
                                    });
                                },
                                "I wrote it down"
                            }
                        }
                    }
                },
                SyncUiState::Paired { fingerprint } => rsx! {
                    small { class: "when", "Device fingerprint" }
                    code { style: "font-size:11px;", "{fingerprint}" }
                    button {
                        title: "Exchange encrypted snapshots via the shared sync-outbox folder",
                        onclick: move |_| {
                            let db = crate::open_db();
                            match run_sync_once(&db) {
                                Ok(merged) => {
                                    if merged > 0 {
                                        // Refresh views + reminder schedule after merge.
                                        if let Ok(items) = db.list_items(false) {
                                            let _ = items.len();
                                        }
                                        *RESOURCES_DIRTY.write() += 1;
                                    }
                                    state.set(SyncUiState::Paired {
                                        fingerprint: load_identity()
                                            .map(|i| i.fingerprint().fingerprint_hex)
                                            .unwrap_or_default(),
                                    });
                                }
                                Err(e) => state.set(SyncUiState::Error(e)),
                            }
                        },
                        "Sync now"
                    }
                    div { style: "margin-top:10px;border-top:1px solid var(--border,#2a2f3a);padding-top:10px;",
                        LeaveSyncControls { on_leave: move |_| {
                            leave_chain();
                            state.set(SyncUiState::NotPaired);
                        } }
                    }
                },
                SyncUiState::Error(msg) => rsx! {
                    span { class: "when", style: "color:#c0392b", "{msg}" }
                    button {
                        onclick: move |_| state.set(SyncUiState::NotPaired),
                        "Back"
                    }
                },
            }
        }
    }
}

/// Two-step "Leave sync chain" control with inline confirmation.
#[component]
fn LeaveSyncControls(on_leave: EventHandler<()>) -> Element {
    let mut confirm = use_signal(|| false);
    if *confirm.read() {
        rsx! {
            p { style: "font-size:12px;color:var(--fg-muted);",
                "Leave the sync chain? This device will stop exchanging snapshots with other paired devices."
            }
            div { style: "display:flex;gap:8px;",
                button { class: "danger", onclick: move |_| on_leave.call(()), "Leave" }
                button { onclick: move |_| confirm.set(false), "Cancel" }
            }
        }
    } else {
        rsx! {
            button { style: "color:#c0392b;", onclick: move |_| confirm.set(true), "Leave sync chain\u{2026}" }
        }
    }
}

/// Sync-chain identity header: avatar + username, tappable to open settings.
#[component]
fn ProfileHeader() -> Element {
    let db = use_context::<crate::DbHandle>();
    let profile = crate::profile::PROFILE_VIEW.read().clone();

    let initials = profile.initials();
    let avatar = profile.avatar_data_uri();

    rsx! {
        button {
            class: "profile-header",
            onclick: move |_| crate::profile::open_settings_screen(&db),
            if let Some(uri) = &avatar {
                img { class: "profile-avatar", src: "{uri}", alt: "", width: "40", height: "40" }
            } else {
                div { class: "profile-avatar profile-avatar-initials", "{initials}" }
            }
            span { class: "profile-name", "{profile.display_name()}" }
        }
    }
}
