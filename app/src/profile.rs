//! Sync-chain user profile: username + optional avatar picture.
//!
//! Persisted as a single JSON row in the device-local `settings` table, so it
//! survives on all platforms (Android uses the same app-private DB). Avatar
//! bytes are stored base64-encoded with their MIME type.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Fallback name shown when no username has been set.
pub const DEFAULT_USERNAME: &str = "You";

/// Size cap for avatar uploads (stored base64 ~1.33x the raw size).
const MAX_AVATAR_BYTES: usize = 4 * 1024 * 1024;

const PROFILE_KEY: &str = "profile";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub username: String,
    /// MIME type of the uploaded image, e.g. "image/png".
    pub avatar_mime: Option<String>,
    /// Base64-encoded avatar bytes.
    pub avatar_b64: Option<String>,
}

impl UserProfile {
    pub fn display_name(&self) -> String {
        let name = self.username.trim();
        if name.is_empty() {
            DEFAULT_USERNAME.to_string()
        } else {
            name.to_string()
        }
    }

    /// Data-URI suitable for an `<img src>` attribute, or None.
    pub fn avatar_data_uri(&self) -> Option<String> {
        let mime = self.avatar_mime.as_deref().unwrap_or("image/png");
        let data = self.avatar_b64.as_deref()?;
        Some(format!("data:{mime};base64,{data}"))
    }

    pub fn initials(&self) -> String {
        let name = self.display_name();
        match name.split_whitespace().collect::<Vec<_>>().as_slice() {
            [first, rest @ ..] => {
                let mut s = String::new();
                if let Some(c) = first.chars().next() {
                    s.push(c);
                }
                if let Some(last) = rest.last().and_then(|w| w.chars().next()) {
                    s.push(last);
                }
                s.to_uppercase()
            }
            [] => "?".into(),
        }
    }

    pub fn has_avatar(&self) -> bool {
        self.avatar_b64.as_deref().is_some_and(|s| !s.is_empty())
    }
}

pub fn load_profile(db: &crate::DbHandle) -> UserProfile {
    db.get_setting(PROFILE_KEY)
        .ok()
        .flatten()
        .and_then(|json| {
            // Tolerate a legacy JSON-encoded wrapper (`"\"…\""`) as elsewhere.
            let text = json.trim_matches('"');
            if text == json && json != text {
                return None;
            }
            serde_json::from_str(text).ok()
        })
        .unwrap_or_default()
}

pub fn save_profile(db: &crate::DbHandle, profile: &UserProfile) {
    if let Ok(json) = serde_json::to_string(profile) {
        let _ = db.set_setting(PROFILE_KEY, &json);
    }
}

/// Encode uploaded file bytes as the profile's avatar. Rejects anything over
/// the size cap or that doesn't look like an image by magic bytes.
pub fn set_avatar(
    profile: &mut UserProfile,
    mime: Option<String>,
    bytes: &[u8],
) -> Result<(), String> {
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err("Image too large (max 4 MB)".into());
    }
    if !looks_like_image(bytes) {
        return Err("Selected file is not an image".into());
    }
    profile.avatar_mime = Some(mime.unwrap_or_else(|| "image/png".into()));
    profile.avatar_b64 = Some(base64_encode(bytes));
    Ok(())
}

pub fn clear_avatar(profile: &mut UserProfile) {
    profile.avatar_mime = None;
    profile.avatar_b64 = None;
}

fn looks_like_image(bytes: &[u8]) -> bool {
    matches!(
        &bytes[..bytes.len().min(12)],
        [0x89, b'P', b'N', b'G', ..] | [0xFF, 0xD8, 0xFF, ..] | b"GIF8" | b"RIFF" // WEBP
    )
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Settings screen
// ---------------------------------------------------------------------------

/// Whether the profile settings screen modal is open.
pub static SETTINGS_SCREEN_OPEN: GlobalSignal<bool> = Signal::global(|| false);
/// The in-progress profile being edited by the settings screen.
pub static PROFILE_EDIT: GlobalSignal<UserProfile> = Signal::global(UserProfile::default);
/// Latest saved profile, read by the sync section so it re-renders on change.
pub static PROFILE_VIEW: GlobalSignal<UserProfile> = Signal::global(UserProfile::default);
/// Error message shown inside the settings screen.
pub static SCREEN_ERROR: GlobalSignal<String> = Signal::global(String::new);

/// Open the profile settings screen, seeded from the stored profile.
pub fn open_settings_screen(db: &crate::DbHandle) {
    *PROFILE_EDIT.write() = load_profile(db);
    *SCREEN_ERROR.write() = String::new();
    *SETTINGS_SCREEN_OPEN.write() = true;
}

/// Modal screen for editing the username + profile picture (avatar).
#[component]
pub fn SettingsScreen() -> Element {
    let db = use_context::<crate::DbHandle>();

    let save = move |_| {
        save_profile(&db, &PROFILE_EDIT.read().clone());
        *PROFILE_VIEW.write() = PROFILE_EDIT.read().clone();
        *SETTINGS_SCREEN_OPEN.write() = false;
    };

    rsx! {
        div { class: "modal-backdrop",
            onclick: move |_| *SETTINGS_SCREEN_OPEN.write() = false,
            div {
                class: "modal",
                role: "dialog",
                "aria-modal": true,
                "aria-label": "Settings",
                onclick: move |e| e.stop_propagation(),
                h2 { "Settings" }
                label { "Sync-chain profile"
                    div { style: "display:flex;gap:10px;align-items:center;",
                        AvatarPreview { }
                        div { style: "display:flex;flex-direction:column;gap:6px;",
                            label { "User name"
                                input {
                                    r#type: "text",
                                    placeholder: "Your name",
                                    value: "{PROFILE_EDIT.read().username}",
                                    oninput: move |e| {
                                        PROFILE_EDIT.write().username = e.value();
                                    },
                                }
                            }
                            div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                                label {
                                    class: "file-upload",
                                    "Upload picture"
                                    input {
                                        r#type: "file",
                                        accept: "image/*",
                                        style: "display:none",
                                        onchange: move |e: Event<FormData>| {
                                            let Some(file) = e.files().first().cloned() else { return; };
                                            let mime = file.content_type();
                                            spawn(async move {
                                                match file.read_bytes().await {
                                                    Ok(bytes) => {
                                                        *SCREEN_ERROR.write() = String::new();
                                                        if let Err(msg) = set_avatar(
                                                            &mut PROFILE_EDIT.write(),
                                                            mime,
                                                            &bytes,
                                                        ) {
                                                            *SCREEN_ERROR.write() = msg;
                                                        }
                                                    }
                                                    Err(err) => {
                                                        *SCREEN_ERROR.write() =
                                                            format!("Could not read image: {err}");
                                                    }
                                                }
                                            });
                                        },
                                    }
                                }
                                if PROFILE_EDIT.read().has_avatar() {
                                    button {
                                        onclick: move |_| clear_avatar(&mut PROFILE_EDIT.write()),
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
                if !SCREEN_ERROR.read().is_empty() {
                    span { style: "color:#c0392b;font-size:12px;", "{SCREEN_ERROR.read()}" }
                }
                div { class: "modal-actions",
                    span {}
                    div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                        button { onclick: move |_| *SETTINGS_SCREEN_OPEN.write() = false, "Cancel" }
                        button { class: "primary", onclick: save, "Save" }
                    }
                }
            }
        }
    }
}

/// Circular avatar preview (image, or initials fallback).
#[component]
fn AvatarPreview() -> Element {
    let uri = PROFILE_EDIT.read().avatar_data_uri();
    let initials = PROFILE_EDIT.read().initials();
    if let Some(uri) = uri {
        rsx! {
            img {
                class: "profile-avatar",
                src: uri,
                "aria-label": "Profile picture",
                alt: "",
                width: "48",
                height: "48",
            }
        }
    } else {
        rsx! {
            div { class: "profile-avatar profile-avatar-initials", "{initials}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_and_display_name() {
        let mut p = UserProfile::default();
        assert_eq!(&p.display_name(), DEFAULT_USERNAME);
        assert_eq!(&p.initials(), "Y");
        p.username = "Ada Lovelace".into();
        assert_eq!(&p.display_name(), "Ada Lovelace");
        assert_eq!(&p.initials(), "AL");
    }

    #[test]
    fn avatar_round_trip_and_rejects_non_images() {
        let mut p = UserProfile::default();
        assert!(set_avatar(&mut p, Some("image/png".into()), b"not an image").is_err());
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        set_avatar(&mut p, Some("image/png".into()), &png).unwrap();
        assert!(p.has_avatar());
        let uri = p.avatar_data_uri().unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        clear_avatar(&mut p);
        assert!(!p.has_avatar());
        assert!(p.avatar_data_uri().is_none());
    }

    #[test]
    fn avatar_over_size_cap_rejected() {
        let mut p = UserProfile::default();
        let mut big =
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a].repeat(MAX_AVATAR_BYTES / 8 + 1);
        big.push(0);
        assert!(set_avatar(&mut p, None, &big).is_err());
    }
}
