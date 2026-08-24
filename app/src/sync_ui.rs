//! Sync-chain pairing screen (spec §5.4 steps 1–2, §5.4 step 5).
//!
//! Identity is stored next to the calendar DB as plain JSON; the phrase never
//! leaves the device unencrypted except on screen while pairing.

use std::path::PathBuf;

use dioxus::prelude::*;

use serde::{Deserialize, Serialize};

use kal_sync::ChainIdentity;

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

#[derive(Debug, Clone, PartialEq)]
pub enum SyncUiState {
    NotPaired,
    /// Just created/joined; show the recovery phrase once.
    ShowPhrase(String),
    Paired { fingerprint: String },
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
            Err(_) => state.set(SyncUiState::Error(
                "Invalid recovery phrase".to_string(),
            )),
        }
    };

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:6px;",
            h2 { "Sync" }
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
