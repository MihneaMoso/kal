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
    let head = &bytes[..bytes.len().min(12)];
    matches!(head, [0x89, b'P', b'N', b'G', ..] | [0xFF, 0xD8, 0xFF, ..])
        || head.starts_with(b"GIF8")
        || head.starts_with(b"RIFF")
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

/// Android: launch the native image picker (via JNI) instead of the WebView's
/// file input, whose selected bytes never reach Rust. Runs the blocking JNI
/// pick on a background thread so the UI thread is not frozen.
#[cfg(target_os = "android")]
fn upload_native() {
    let rx = crate::android_picker::pick_image_async();
    spawn(async move {
        let picked = tokio::task::spawn_blocking(move || rx.recv().ok().flatten())
            .await
            .ok()
            .flatten();
        if let Some(picked) = picked {
            *SCREEN_ERROR.write() = String::new();
            if let Err(msg) = set_avatar(&mut PROFILE_EDIT.write(), picked.mime, &picked.bytes) {
                *SCREEN_ERROR.write() = msg;
            }
        } else {
            // Picker dismissed without a selection: leave the current avatar.
        }
    });
}

/// Non-Android: unused (the overlaid file input handles desktop/web).
#[cfg(not(target_os = "android"))]
fn upload_native() {}

/// Modal screen for editing the username + profile picture (avatar).
#[component]
pub fn SettingsScreen() -> Element {
    let db = use_context::<crate::DbHandle>();

    let save = move |_| {
        save_profile(&db, &PROFILE_EDIT.read().clone());
        *PROFILE_VIEW.write() = PROFILE_EDIT.read().clone();
        *SETTINGS_SCREEN_OPEN.write() = false;
    };

    // The profile-picture control differs by platform: Android routes the tap
    // to the native JNI picker (the WebView file input never returns bytes);
    // desktop/web use the overlaid <input type="file">. Computed here so the
    // rsx! below stays single-source and free of attribute-level cfg.
    let upload_control = if cfg!(target_os = "android") {
        rsx! {
            button {
                class: "file-upload",
                r#type: "button",
                onclick: move |_| upload_native(),
                "Upload picture"
            }
        }
    } else {
        rsx! {
            button {
                class: "file-upload",
                r#type: "button",
                span { "Upload picture" }
                input {
                    r#type: "file",
                    accept: "image/*",
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
        }
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
                                {upload_control}
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
                SoftwareSection { }
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

/// Software version + update checking controls. Shows the current release,
/// lets the user check for newer builds (or toggle the startup check) and
/// apply a downloaded update (desktop: restart; Android: package installer).
#[component]
fn SoftwareSection() -> Element {
    let db = use_context::<crate::DbHandle>();
    let mut settings = use_context::<Signal<crate::ui::Settings>>();
    let db_auto = db.clone();

    let status = crate::updater::UPDATE_STATUS.read().clone();
    let ready = *crate::updater::UPDATE_READY.read();
    let auto = settings.read().auto_check_updates;

    rsx! {
        hr {}
        h3 { style: "font-size:14px;margin:12px 0 6px;", "Software & updates" }
        div { style: "display:flex;gap:10px;flex-wrap:wrap;align-items:center;",
            span { style: "font-size:13px;", "Kal v{crate::updater::CURRENT_VERSION}" }
            button {
                onclick: move |_| crate::updater::run_check(),
                "Check for updates"
            }
            label { style: "display:flex;align-items:center;gap:4px;font-size:12px;",
                input {
                    r#type: "checkbox",
                    checked: auto,
                    onchange: move |e| {
                        let mut p = settings.read().clone();
                        p.auto_check_updates = e.checked();
                        p.save(&db_auto);
                        settings.set(p);
                    },
                }
                "Check at startup"
            }
        }
        if let Some(status) = status {
            span { style: "display:block;font-size:12px;color:var(--fg-muted);margin-top:6px;", "{status}" }
        }
        if ready {
            button {
                class: "primary",
                style: "margin-top:6px;",
                onclick: move |_| {
                    crate::updater::apply_now();
                },
                "Apply update now"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DbHandle;
    use kal_storage::Database;
    use std::sync::Arc;

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

    /// Android's native picker (`KalFilePicker`) often returns a file without a
    /// usable MIME string. The path taken by the Android tap must still persist
    /// the avatar — falling back to image/png rather than failing or storing a
    /// broken row.
    #[test]
    fn set_avatar_accepts_missing_mime_from_android_picker() {
        let mut p = UserProfile::default();
        let heic_like_png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        set_avatar(&mut p, None, &heic_like_png).unwrap();
        assert_eq!(p.avatar_mime.as_deref(), Some("image/png"));
        assert!(p.has_avatar());
    }

    /// Real gallery files come in JPEG/WEBP/GIF/PNG. All must be accepted (the
    /// WebView `accept="image/*"` and the native picker both allow them), so an
    /// upload from an Android gallery never fatals.
    #[test]
    fn set_avatar_accepts_common_gallery_formats() {
        let formats: &[&[u8]] = &[
            &[0xFF, 0xD8, 0xFF, 0xE0],             // JPEG
            b"RIFF\x00\x00\x00\x00WEBPVP8",        // WEBP
            b"GIF89a",                             // GIF
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a], // PNG
        ];
        for (i, bytes) in formats.iter().enumerate() {
            let mut p = UserProfile::default();
            set_avatar(&mut p, None, bytes)
                .unwrap_or_else(|e| panic!("gallery format #{i} must be accepted, got {e}"));
            assert!(p.has_avatar(), "gallery format #{i} persists an avatar");
        }
    }

    /// An empty/zero-byte file is not an image and must not set an avatar.
    #[test]
    fn set_avatar_rejects_empty_file() {
        let mut p = UserProfile::default();
        assert!(set_avatar(&mut p, Some("image/jpeg".into()), &[]).is_err());
        assert!(!p.has_avatar());
    }

    /// Settings must round-trip through the same on-device settings table the
    /// UI writes to (this is how the avatar survives restarts on Android).
    #[test]
    fn profile_persists_through_settings_table() {
        let db: DbHandle = Arc::new(Database::open_in_memory().unwrap());
        let mut p = UserProfile {
            username: "Mihnea".into(),
            ..Default::default()
        };
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        set_avatar(&mut p, Some("image/webp".into()), &png).unwrap();

        save_profile(&db, &p);
        let loaded = load_profile(&db);
        assert_eq!(loaded.username, "Mihnea");
        assert_eq!(loaded.avatar_mime.as_deref(), Some("image/webp"));
        assert_eq!(loaded.avatar_b64, p.avatar_b64);
        assert!(loaded.has_avatar());
    }

    /// `display_name` must trim surrounding whitespace and fall back to the
    /// default when blank, so a stray-space username never renders as "".
    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn display_name_trims_and_falls_back() {
        let mut p = UserProfile::default();
        p.username = "  Grace Hopper  ".into();
        assert_eq!(p.display_name(), "Grace Hopper");
        p.username = "   ".into();
        assert_eq!(p.display_name(), DEFAULT_USERNAME);
    }

    /// `initials` handles single names, multi-word names, and empty-name edge.
    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn initials_handles_name_shapes() {
        let mut p = UserProfile::default();
        p.username = "Cher".into();
        assert_eq!(p.initials(), "C");
        p.username = "Ada Lovelace".into();
        assert_eq!(p.initials(), "AL");
        p.username = "  ".into();
        assert_eq!(p.initials(), "Y");
    }

    /// The user can clear their avatar from settings; after doing so the
    /// profile no longer reports or renders one.
    #[test]
    fn clear_avatar_removes_rendering() {
        let mut p = UserProfile::default();
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        set_avatar(&mut p, Some("image/png".into()), &png).unwrap();
        assert!(p.avatar_data_uri().is_some());
        clear_avatar(&mut p);
        assert_eq!(p.avatar_mime, None);
        assert_eq!(p.avatar_b64, None);
        assert!(!p.has_avatar());
        assert!(p.avatar_data_uri().is_none());
    }
}
