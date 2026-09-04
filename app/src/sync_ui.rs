//! Sync-chain pairing screen (spec §5.4 steps 1–2, §5.4 step 5).
//!
//! Identity is stored next to the calendar DB as plain JSON; the phrase never
//! leaves the device unencrypted except on screen while pairing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Start a daemon thread that periodically runs sync rounds while this device
/// is paired, so same-phrase devices converge automatically. Without it, sync
/// was one-shot and racy: a single "Sync now" waited only ~15 s for a peer,
/// so the phone synced nothing unless the desktop happened to be online (and
/// discoverable) at that exact moment. The driver retries every few seconds
/// until peers are found. Idempotent: only one driver runs per process.
pub fn start_background_sync() {
    use std::sync::OnceLock;
    static STARTED: OnceLock<bool> = OnceLock::new();
    let _ = STARTED.get_or_init(move || {
        std::thread::Builder::new()
            .name("kal-sync-driver".into())
            .spawn(background_sync_loop)
            .ok();
        true
    });
}

/// Body of the background sync driver: every few seconds, if this device has
/// a chain identity, run a full round (live P2P, folder fallback) on a fresh
/// DB connection and refresh the views when anything merged.
fn background_sync_loop() {
    const INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
    loop {
        std::thread::sleep(INTERVAL);
        if load_identity().is_none() {
            continue;
        }
        let db = crate::open_db();
        let started = std::time::Instant::now();
        match run_sync_once(&db) {
            Ok(merged) if merged > 0 => {
                tracing::info!(
                    merged,
                    elapsed_ms = started.elapsed().as_millis(),
                    "background sync merged changes"
                );
                // The driver runs on a plain OS thread with no Dioxus runtime,
                // so it cannot bump the `RESOURCES_DIRTY` signal directly (that
                // panics). Instead it sets a thread-safe counter which an
                // on-runtime poller in main.rs translates into the refresh.
                SYNC_DRIVER_DIRTY.fetch_add(1, Ordering::Relaxed);
            }
            Ok(_) => {
                tracing::debug!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "background sync round: nothing merged"
                );
            }
            Err(e) => {
                tracing::debug!(error = %e, "background sync round failed");
            }
        }
    }
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
/// Thread-safe counter bumped by the background driver (an OS thread with no
/// Dioxus runtime). An on-runtime poller turns increments into the
/// `RESOURCES_DIRTY` refresh signal.
pub static SYNC_DRIVER_DIRTY: AtomicUsize = AtomicUsize::new(0);

/// Retire fresh-install placeholder calendars shadowed by chain data.
///
/// A device that joins a chain with an already-visible `Personal`/`Birthdays`
/// pair shows duplicate rows: its own (empty, well-known-id) placeholders
/// plus the chain's pair. The placeholders can never hold data the chain
/// lacks — they were minted minutes ago by seeding — so once a merge lands,
/// delete the ones shadowed by a same-named visible live calendar. True
/// deletes (tombstones), not hides: union-merge would resurrect hidden-but-
/// live rows from any peer, while tombstones win LWW ties and converge
/// everywhere. Narrowly scoped: only well-known placeholder IDs, only when
/// they hold zero items (live or tombstoned), only when a same-named visible
/// live alternative exists. Runs on every persisted merge; idempotent.
pub(crate) fn retire_shadowed_placeholders(db: &crate::DbHandle) {
    use std::collections::HashSet;
    let cals = db.list_calendars().unwrap_or_default();
    if cals.is_empty() {
        return;
    }
    let used: HashSet<ulid::Ulid> = db
        .list_items(true)
        .unwrap_or_default()
        .iter()
        .map(|i| i.calendar_id)
        .collect();
    let now = chrono::Local::now().fixed_offset();
    for id_str in [crate::DEFAULT_PERSONAL_ID, crate::DEFAULT_BIRTHDAYS_ID] {
        let Ok(id) = id_str.parse::<ulid::Ulid>() else {
            continue;
        };
        let Some(cal) = cals.iter().find(|c| c.id == id) else {
            continue;
        };
        if cal.deleted || used.contains(&id) {
            continue;
        }
        let shadowed = cals.iter().any(|o| {
            o.visible && !o.deleted && o.id != id && o.name == cal.name
        });
        if !shadowed {
            continue;
        }
        let mut retired = cal.clone();
        retired.visible = false;
        retired.deleted = true;
        retired.updated_at = now;
        if db.upsert_calendar(&retired).is_ok() {
            tracing::info!(calendar = %cal.name, "retired shadowed placeholder calendar");
        }
    }
}

/// One full gossip round: live P2P when the chain has peers, otherwise the
/// shared outbox folder.
fn run_sync_once(db: &crate::DbHandle) -> Result<usize, String> {
    let identity = load_identity().ok_or("not paired")?;
    let device_id = ulid::Ulid::new(); // per-round id; stable ids land with settings store

    // Live P2P first: when the gossip topic has peers, sync over the network
    // (mobile joiners reach the desktop without any shared folder).
    if let Ok(merged) = crate::sync_live::sync_round(&identity, db) {
        return Ok(merged);
    }
    // Not joined yet / unavailable — fall back to the shared folder.

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
    retire_shadowed_placeholders(db);
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
    // True while a "Sync now" round runs off-thread (live discovery can wait
    // ~15 s for a peer; blocking the UI thread on that froze the whole app).
    let mut syncing = use_signal(|| false);
    // Human-readable outcome of the most recent round (shown in the panel).
    let mut last_sync = use_signal(|| String::new());

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
                        title: "Reveal the 24-word phrase for setting up another device",
                        onclick: move |_| {
                            if let Some(id) = load_identity() {
                                state.set(SyncUiState::ShowPhrase(id.phrase()));
                            }
                        },
                        "Show recovery phrase"
                    }
                    button {
                        disabled: *syncing.read(),
                        title: "Exchange encrypted snapshots via the shared sync-outbox folder",
                        onclick: move |_| {
                            if *syncing.read() {
                                return;
                            }
                            // Run the round off the UI thread (peer discovery
                            // can wait ~15 s; doing it inline froze the app)
                            // and post the result back through signals.
                            *syncing.write() = true;
                            let db = crate::open_db();
                            let mut state = state;
                            let mut last_sync = last_sync;
                            spawn(async move {
                                let started = std::time::Instant::now();
                                let result =
                                    match tokio::task::spawn_blocking(move || run_sync_once(&db))
                                        .await
                                    {
                                        Ok(res) => res,
                                        Err(e) => Err(format!("sync task failed: {e}")),
                                    };
                                *syncing.write() = false;
                                match result {
                                    Ok(merged) => {
                                        tracing::info!(merged, elapsed_ms = started.elapsed().as_millis(), "manual sync round done");
                                        if merged > 0 {
                                            *RESOURCES_DIRTY.write() += 1;
                                        }
                                        last_sync.set(if merged > 0 {
                                            format!("Synced {merged} change(s) \u{2014} {}", clock_str())
                                        } else {
                                            format!("No changes to sync \u{2014} {}", clock_str())
                                        });
                                        state.set(SyncUiState::Paired {
                                            fingerprint: load_identity()
                                                .map(|i| i.fingerprint().fingerprint_hex)
                                                .unwrap_or_default(),
                                        });
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "manual sync round failed");
                                        // Transient (e.g. "no peers yet") — keep the paired
                                        // panel visible, just report the outcome inline.
                                        last_sync.set(format!("Sync failed: {e}"));
                                    }
                                }
                            });
                        },
                        if *syncing.read() { "Syncing…" } else { "Sync now" }
                    }
                    if !last_sync.read().is_empty() {
                        small { class: "when", style: "color:var(--fg-muted);", "{last_sync}" }
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

/// Local time "HH:MM" for the last-sync status line.
fn clock_str() -> String {
    chrono::Local::now().format("%H:%M").to_string()
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
