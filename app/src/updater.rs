//! In-app update checking.
//!
//! Queries the latest GitHub release for Kal the same way `install.sh` does,
//! then stages the new binary on desktop or hands the APK to the system
//! package installer on Android. The web build ships via the GH Pages app
//! itself and is not auto-updatable this way, so it is compiled out entirely
//! on `target_arch = "wasm32"`.

use dioxus::prelude::*;

/// Human-readable outcome of the last update check, shown in settings.
pub static UPDATE_STATUS: GlobalSignal<Option<String>> = Signal::global(|| None);
/// True once a newer release has been downloaded and is ready to apply.
pub static UPDATE_READY: GlobalSignal<bool> = Signal::global(|| false);

/// The version this binary reports for comparison against release tags.
///
/// Injected at build time by `app/build.rs` (from the nearest git tag, or the
/// `KAL_RELEASE_VERSION` env override in CI), so it always matches the release
/// and never needs hand-editing. Falls back to the workspace manifest version.
pub const CURRENT_VERSION: &str = env!("KAL_VERSION");

use kal_update::{is_newer, ReleaseInfo};

/// A newer release that has been downloaded and verified, ready to apply.
#[derive(Debug, Clone)]
pub struct ReadyUpdate {
    pub version: String,
}

/// Query the GitHub latest release and resolve the current platform's asset.
///
/// The portable half (version arithmetic, asset resolution, download and
/// digest verification) lives in `kal-update`, shared with the Android bridge;
/// this wrapper only picks the transport and the platform token.
#[cfg(not(target_arch = "wasm32"))]
pub fn latest_release() -> Result<ReleaseInfo, String> {
    kal_update::latest_release_with(&kal_update::Http, kal_update::platform_token()?)
}

/// Download the release's platform asset into the local updates dir and report
/// the transitioned status. On desktop this stages a swap-on-next-launch; on
/// Android it just downloads the APK (the user then confirms the system
/// PackageInstaller prompt).
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_update(info: &ReleaseInfo) -> Result<ReadyUpdate, String> {
    let Some(dir) = updates_dir() else {
        return Err("no writable data dir".into());
    };
    let _ = std::fs::create_dir_all(&dir);

    #[cfg(target_os = "android")]
    {
        kal_update::download_with(
            &kal_update::Http,
            &info.asset_url,
            &dir.join("kal-update.apk"),
            info.sha256.as_deref(),
        )?;
        return Ok(ReadyUpdate {
            version: info.version.clone(),
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        let archive = dir.join("kal-update.tar.gz");
        kal_update::download_with(
            &kal_update::Http,
            &info.asset_url,
            &archive,
            info.sha256.as_deref(),
        )?;
        stage_desktop_binary(&dir, &archive)?;
        Ok(ReadyUpdate {
            version: info.version.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Update staging directory
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn updates_dir() -> Option<std::path::PathBuf> {
    data_root().map(|p| p.join("kal").join("updates"))
}

#[cfg(not(target_arch = "wasm32"))]
fn data_root() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "android")]
    {
        android_files_dir()
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs_next::data_dir()
    }
}

#[cfg(target_os = "android")]
fn android_files_dir() -> Option<std::path::PathBuf> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let ctx = ndk_context::android_context();
            let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
            let mut env = vm.attach_current_thread().ok()?;
            let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
            let file = env
                .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
                .ok()?
                .l()
                .ok()?;
            let abs: jni::objects::JObject = env
                .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            let jstring = jni::objects::JString::from(abs);
            let s = env.get_string(&jstring).ok()?;
            Some(std::path::PathBuf::from(s.to_string_lossy().to_string()))
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Desktop: stage + restart-to-apply
// ---------------------------------------------------------------------------

/// Extract the single root binary from the gzip'd tarball to `dir/kal.new` and
/// write a marker naming the executable to swap on the next launch.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn stage_desktop_binary(dir: &std::path::Path, archive: &std::path::Path) -> Result<(), String> {
    use std::io::Read;
    let gz = std::fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?;
    let tar = flate2::read::GzDecoder::new(gz);
    let mut ar = tar::Archive::new(tar);
    let staged = dir.join("kal.new");
    let mut found = false;
    for entry in ar.entries().map_err(|e| format!("read archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("entry: {e}"))?;
        if !entry
            .path()
            .map(|p| p.components().count() == 1)
            .unwrap_or(false)
        {
            continue;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("read entry: {e}"))?;
        std::fs::write(&staged, &buf).map_err(|e| format!("write staged: {e}"))?;
        found = true;
        break;
    }
    if !found {
        return Err("archive contained no binary".into());
    }
    let target = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    std::fs::write(dir.join("swap.marker"), target.to_string_lossy().as_bytes())
        .map_err(|e| format!("write marker: {e}"))?;
    let _ = std::fs::remove_file(archive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Swap the running executable for the staged one and relaunch it, then exit
/// this process. Returns false (without exiting) if nothing is staged.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn apply_staged_update() -> bool {
    let Some(dir) = updates_dir() else {
        return false;
    };
    let staged = dir.join("kal.new");
    let marker = dir.join("swap.marker");
    let Ok(raw) = std::fs::read_to_string(&marker) else {
        return false;
    };
    let target = std::path::PathBuf::from(raw.trim().to_string());
    if !staged.exists() || !target.exists() {
        let _ = std::fs::remove_file(&marker);
        return false;
    }

    // Windows cannot overwrite a running .exe but may rename it into place
    // after the canonical name is freed; Unix may overwrite in place.
    #[cfg(windows)]
    {
        let old = target.with_extension("old~");
        let _ = std::fs::remove_file(&old);
        if std::fs::rename(&target, &old).is_err() {
            return false;
        }
        if std::fs::copy(&staged, &target).is_err() {
            return false;
        }
    }
    #[cfg(not(windows))]
    {
        if std::fs::copy(&staged, &target).is_err() {
            return false;
        }
    }
    let _ = std::fs::remove_file(&marker);
    let _ = std::fs::remove_file(&staged);

    // Relaunch the newly swapped binary, then exit the old process.
    if std::process::Command::new(&target).spawn().is_ok() {
        std::process::exit(0);
    }
    false
}

// ---------------------------------------------------------------------------
// Android: confirm the staged APK through the system package installer.
// ---------------------------------------------------------------------------

/// Build and fire the ACTION_VIEW install intent for the staged APK using a
/// FileProvider-backed content URI (wired up in `KalUpdater.kt`/the manifest).
/// Returns Ok once the intent is handed to the system.
#[cfg(target_os = "android")]
pub fn request_android_install() -> Result<(), String> {
    let Some(dir) = updates_dir() else {
        return Err("no writable data dir".into());
    };
    let apk = dir.join("kal-update.apk");
    if !apk.exists() {
        return Err("no staged apk".into());
    }

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach: {e}"))?;
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = env
        .find_class("com/kal/calendar/KalUpdater")
        .map_err(|e| format!("updater class: {e}"))?;
    let path = apk.to_string_lossy().to_string();
    let jpath = env.new_string(&path).map_err(|e| e.to_string())?;

    // KalUpdater.installApk(Context, String): V
    env.call_static_method(
        class,
        "installApk",
        "(Landroid/content/Context;Ljava/lang/String;)V",
        &[
            jni::objects::JValue::Object(&context),
            jni::objects::JValue::Object(&jpath),
        ],
    )
    .map_err(|e| format!("call installApk: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Web stubs (compiled out).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub fn latest_release() -> Result<ReleaseInfo, String> {
    Err("auto-update is not applicable on web".into())
}

#[cfg(target_arch = "wasm32")]
pub fn fetch_update(_info: &ReleaseInfo) -> Result<ReadyUpdate, String> {
    Err("auto-update is not applicable on web".into())
}

#[cfg(target_arch = "wasm32")]
pub fn apply_staged_update() -> bool {
    false
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
#[allow(dead_code)]
pub fn request_android_install() -> Result<(), String> {
    Err("not applicable".into())
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn request_android_install() -> Result<(), String> {
    Err("not applicable".into())
}

// ---------------------------------------------------------------------------
// UI wiring
// ---------------------------------------------------------------------------

/// Run a background update check that downloads the latest release when a
/// newer version exists, updating the shared status globals. Safe to call from
/// any platform; on web it reports "not applicable".
pub fn run_check() {
    // Run the blocking network/staging work as a Dioxus task rather than a raw
    // OS thread: dioxus tasks execute with the runtime context established, so
    // the GlobalSignal writes below (UPDATE_STATUS / UPDATE_READY) are legal
    // and trigger re-renders. A std::thread would not carry the runtime.
    spawn(async move {
        let outcome = (|| -> Result<String, String> {
            let latest = latest_release()?;
            if is_newer(&latest.version, CURRENT_VERSION) {
                let ready = fetch_update(&latest)?;
                *UPDATE_READY.write() = true;
                Ok(format!(
                    "Update ready: v{} — apply to restart",
                    ready.version
                ))
            } else {
                Ok(format!("You're up to date (v{CURRENT_VERSION})"))
            }
        })();
        *UPDATE_STATUS.write() = Some(match outcome {
            Ok(msg) => msg,
            Err(e) => format!("Update check failed: {e}"),
        });
    });
}

/// User tapped "apply/install now": on desktop swap+relaunch, on Android fire
/// the PackageInstaller intent.
pub fn apply_now() -> bool {
    // Android completes through the system installer (this returns after the
    // intent is handed off); desktop does an in-process swap + relaunch.
    #[cfg(target_os = "android")]
    {
        match request_android_install() {
            Ok(()) => {
                *UPDATE_STATUS.write() =
                    Some("Package installer opened — confirm to finish".into());
                true
            }
            Err(e) => {
                *UPDATE_STATUS.write() = Some(format!("Install failed: {e}"));
                false
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        if apply_staged_update() {
            true // process exits here on success
        } else {
            *UPDATE_STATUS.write() = Some("No staged update to apply".into());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kal_update::parse_version;

    #[test]
    fn embedded_version_is_real() {
        // CURRENT_VERSION comes from build.rs (git tag / KAL_RELEASE_VERSION).
        // It must be non-empty and parseable, otherwise the updater can never
        // detect anything (never mind the stale-manual-constant case).
        assert!(!CURRENT_VERSION.is_empty());
        assert!(!CURRENT_VERSION.contains('v'));
        assert!(!parse_version(CURRENT_VERSION).is_empty());
        // Where git is available, the embedded version must match the nearest
        // tag (build.rs's primary source) rather than the "0.1.0" fallback.
        if let Ok(out) = std::process::Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .output()
        {
            if out.status.success() {
                let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !tag.is_empty() {
                    assert_eq!(CURRENT_VERSION, tag.trim_start_matches('v'));
                }
            }
        }
    }
}
