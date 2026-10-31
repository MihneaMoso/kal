//! Release checking and staging for Kal — the portable half of the updater.
//!
//! Everything here used to live in `app/src/updater.rs`; it moved so the
//! Android bridge can arm the *same* release semantics as the desktop app
//! instead of growing a second copy (guide §0.19.2: update check/download/
//! hash-verify stay native, only the install is the interface layer's job).
//!
//! What stays behind in the app / Kotlin: the status UI, the desktop swap and
//! the Android PackageInstaller intent.
//!
//! Network access goes through [`Transport`] so the release contract is tested
//! without a socket — the same shape `kal-import` uses for its Google transport.

use std::path::Path;

/// GitHub repo owning the releases, as "owner/name".
pub const REPO: &str = "MihneaMoso/kal";

/// Asset name token matched against published asset names. The published
/// basenames embed a git SHA (e.g. `kal-0.1.7-9f77d464-kal-linux.tar.gz`), so
/// we match by this stable substring.
pub const LINUX_TOKEN: &str = "kal-linux.tar.gz";
pub const MACOS_TOKEN: &str = "kal-macos.tar.gz";
pub const WINDOWS_TOKEN: &str = "kal-windows.exe";
pub const ANDROID_TOKEN: &str = "android.apk";

/// Cap on the release JSON we will read (a wrong URL can stream forever).
pub const MAX_RELEASE_BYTES: usize = 4 * 1024 * 1024;
/// Cap on a downloaded artifact.
pub const MAX_ARTIFACT_BYTES: usize = 200 * 1024 * 1024;

/// A transport for the two HTTP reads the updater needs.
///
/// `get` returns at most `limit` bytes; exceeding the cap truncates rather than
/// errors, exactly like the `.take(limit)` readers this replaced (a truncated
/// JSON fails to parse and a truncated artifact fails its digest).
pub trait Transport {
    fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>, String>;
}

/// The real transport, over `ureq`.
///
/// Native only: the web build has no updater, but still links the crate for the
/// version arithmetic.
#[cfg(not(target_arch = "wasm32"))]
pub struct Http;

#[cfg(not(target_arch = "wasm32"))]
impl Transport for Http {
    fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let resp = ureq::Agent::new()
            .get(url)
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "kal-updater")
            .call()
            .map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        resp.into_reader()
            .take(limit as u64)
            .read_to_end(&mut buf)
            .map_err(|e| format!("read failed: {e}"))?;
        Ok(buf)
    }
}

/// A discovered candidate release for one platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub asset_url: String,
    pub sha256: Option<String>,
}

/// Parse a leading `v`-optional, dot-separated numeric version into comparable
/// integers. Ignores non-numeric tail segments (`-rc.1` → `1`).
pub fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['.', '-'])
        .filter_map(|p| p.parse::<u64>().ok())
        .collect()
}

/// True when `latest` is strictly newer than `current`.
///
/// Components compare left to right, like a version: the first differing
/// component decides. (A scan that reports *any* larger component would call
/// `0.1.9` newer than `0.2.0` because `9 > 0`.) Missing components count as
/// zero, so `0.2` and `0.2.0` are the same version.
pub fn is_newer(latest: &str, current: &str) -> bool {
    if latest == current {
        return false;
    }
    let a = parse_version(latest);
    let b = parse_version(current);
    let max = a.len().max(b.len());
    for i in 0..max {
        let (left, right) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if left != right {
            return left > right;
        }
    }
    false
}

/// The asset token for the running platform, as `std::env::consts::OS` spells it.
pub fn platform_token() -> Result<&'static str, String> {
    match std::env::consts::OS {
        "linux" => Ok(LINUX_TOKEN),
        "macos" => Ok(MACOS_TOKEN),
        "windows" => Ok(WINDOWS_TOKEN),
        "android" => Ok(ANDROID_TOKEN),
        _ => Err("unsupported platform for auto-update".into()),
    }
}

/// Find the asset matching `token` in a GitHub release JSON body.
pub fn pick_asset(body: &str, token: &str) -> Result<ReleaseInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("bad release payload: {e}"))?;
    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    let version = tag.trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err("release has no version".into());
    }
    let assets = v["assets"].as_array().ok_or("release has no assets")?;
    let asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(token))
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("no asset matching '{token}'"))?;
    let asset_url = asset["browser_download_url"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if asset_url.is_empty() {
        return Err("asset has no download url".into());
    }
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|s| s.to_string());
    Ok(ReleaseInfo {
        version,
        asset_url,
        sha256,
    })
}

/// Query the GitHub latest release and resolve `token`'s asset.
pub fn latest_release_with(t: &dyn Transport, token: &str) -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = t
        .get(&url, MAX_RELEASE_BYTES)
        .map_err(|e| format!("update check failed: {e}"))?;
    pick_asset(&String::from_utf8_lossy(&body), token)
}

/// Download `url` into `dest` and verify the (optional) SHA-256 digest.
pub fn download_with(
    t: &dyn Transport,
    url: &str,
    dest: &Path,
    sha256: Option<&str>,
) -> Result<(), String> {
    let buf = t
        .get(url, MAX_ARTIFACT_BYTES)
        .map_err(|e| format!("download failed: {e}"))?;

    if let Some(hex) = sha256 {
        let actual = hex_digest(&buf);
        if !actual.eq_ignore_ascii_case(hex) {
            return Err(format!("SHA-256 mismatch (expected {hex}, got {actual})"));
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("write failed: {e}"))?;
    }
    std::fs::write(dest, &buf).map_err(|e| format!("write failed: {e}"))
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn hex_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The result of one check-and-stage pass, with no presentation attached:
/// the app and the Android layer each word their own status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The running version is current (nothing downloaded).
    UpToDate { current: String },
    /// A newer release was downloaded and verified into the caller's path.
    Ready { version: String },
    /// The check or the download failed; `message` is the reason.
    Failed { message: String },
}

/// Check the latest release for `token` and, when it is newer than `current`,
/// download and verify it into `dest`. One pass, like the Dioxus "Check for
/// updates" button (which downloads before it reports).
pub fn check_with(t: &dyn Transport, token: &str, current: &str, dest: &Path) -> CheckOutcome {
    let latest = match latest_release_with(t, token) {
        Ok(latest) => latest,
        Err(message) => return CheckOutcome::Failed { message },
    };
    if !is_newer(&latest.version, current) {
        return CheckOutcome::UpToDate {
            current: current.to_string(),
        };
    }
    match download_with(t, &latest.asset_url, dest, latest.sha256.as_deref()) {
        Ok(()) => CheckOutcome::Ready {
            version: latest.version,
        },
        Err(message) => CheckOutcome::Failed { message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn version_ordering() {
        assert_eq!(parse_version("0.1.7"), vec![0, 1, 7]);
        assert_eq!(parse_version("v1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version("1.10.0"), vec![1, 10, 0]);
        assert!(is_newer("0.1.8", "0.1.7"));
        assert!(is_newer("1.0.0", "0.1.7"));
        assert!(!is_newer("0.1.7", "0.1.7"));
        assert!(!is_newer("0.1.6", "0.1.7"));
        // A shorter latest with a bigger numeric tail still wins.
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(is_newer("0.2", "0.1.9"));
    }

    #[test]
    fn a_later_component_cannot_outvote_an_earlier_one() {
        // The first differing component decides the order; a bigger patch
        // number must not make a lower minor look newer.
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("1.0.1", "1.1.0"));
        assert!(!is_newer("1.9.9", "2.0.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.1.0", "1.0.1"));
        assert!(is_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn pick_asset_matches_platform() {
        let body = r#"{
            "tag_name": "v0.1.7",
            "assets": [
                {"name": "kal-0.1.7-abcd1234-kal-linux.tar.gz",
                 "browser_download_url": "https://example/linux.tar.gz",
                 "digest": "sha256:deadbeef"},
                {"name": "kal-0.1.7-android.apk",
                 "browser_download_url": "https://example/android.apk",
                 "digest": "sha256:f00d"},
                {"name": "kal-0.1.7-abcd1234-kal-windows.exe.tar.gz",
                 "browser_download_url": "https://example/win.exe.tar.gz",
                 "digest": "sha256:beef"}
            ]
        }"#;

        let linux = pick_asset(body, LINUX_TOKEN).unwrap();
        assert_eq!(linux.version, "0.1.7");
        assert_eq!(linux.asset_url, "https://example/linux.tar.gz");
        assert_eq!(linux.sha256.as_deref(), Some("deadbeef"));

        let android = pick_asset(body, ANDROID_TOKEN).unwrap();
        assert_eq!(android.asset_url, "https://example/android.apk");
        assert_eq!(android.sha256.as_deref(), Some("f00d"));

        // The windows token must not be fooled by the linux asset's name.
        let windows = pick_asset(body, WINDOWS_TOKEN).unwrap();
        assert_eq!(windows.asset_url, "https://example/win.exe.tar.gz");

        assert!(pick_asset(body, "kal-nothing.tar.gz")
            .unwrap_err()
            .contains("no asset matching"));
    }

    #[test]
    fn pick_asset_rejects_incomplete_releases() {
        assert!(pick_asset("not json", LINUX_TOKEN)
            .unwrap_err()
            .contains("bad release payload"));
        assert_eq!(
            pick_asset(r#"{"assets": []}"#, LINUX_TOKEN).unwrap_err(),
            "release has no version"
        );
        assert_eq!(
            pick_asset(r#"{"tag_name": "v1.0.0"}"#, LINUX_TOKEN).unwrap_err(),
            "release has no assets"
        );
        assert_eq!(
            pick_asset(
                r#"{"tag_name": "v1.0.0", "assets": [{"name": "x-kal-linux.tar.gz"}]}"#,
                LINUX_TOKEN
            )
            .unwrap_err(),
            "asset has no download url"
        );
        // Releases published before GitHub exposed digests simply have no hash.
        let bare = pick_asset(
            r#"{"tag_name": "v1.0.0", "assets": [
                 {"name": "x-android.apk", "browser_download_url": "https://e/a.apk"}]}"#,
            ANDROID_TOKEN,
        )
        .unwrap();
        assert_eq!(bare.sha256, None);
    }

    /// Serves canned bodies per URL, so no test touches the network.
    struct Fake {
        bodies: HashMap<String, Vec<u8>>,
        failure: Option<String>,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(bodies: &[(&str, &[u8])]) -> Self {
            Self {
                bodies: bodies
                    .iter()
                    .map(|(u, b)| ((*u).to_string(), b.to_vec()))
                    .collect(),
                failure: None,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                bodies: HashMap::new(),
                failure: Some(message.into()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for Fake {
        fn get(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
            self.calls.borrow_mut().push(url.to_string());
            if let Some(message) = &self.failure {
                return Err(message.clone());
            }
            self.bodies
                .get(url)
                .cloned()
                .ok_or_else(|| format!("unexpected url {url}"))
        }
    }

    const RELEASE: &str = r#"{
        "tag_name": "v0.3.0",
        "assets": [
            {"name": "kal-0.3.0-abc-android.apk",
             "browser_download_url": "https://example/android.apk",
             "digest": "sha256:IGNORED"}
        ]
    }"#;

    #[test]
    fn a_newer_release_is_downloaded_and_verified() {
        let payload = b"new apk bytes";
        let hash = hex_digest(payload);
        let release = RELEASE.replace("IGNORED", &hash);
        let fake = Fake::new(&[
            (
                "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
                release.as_bytes(),
            ),
            ("https://example/android.apk", payload),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("kal/updates/kal-update.apk");

        let outcome = check_with(&fake, ANDROID_TOKEN, "0.2.0", &dest);

        assert_eq!(
            outcome,
            CheckOutcome::Ready {
                version: "0.3.0".into()
            }
        );
        // The parent directory is created and the exact bytes land.
        assert_eq!(std::fs::read(&dest).unwrap(), payload);
        // The release query is first, the artifact second — never the reverse.
        assert_eq!(
            fake.calls.borrow().as_slice(),
            [
                "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
                "https://example/android.apk"
            ]
        );
    }

    #[test]
    fn an_up_to_date_check_downloads_nothing() {
        let release = RELEASE.replace("v0.3.0", "v0.2.0");
        let fake = Fake::new(&[(
            "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
            release.as_bytes(),
        )]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("kal-update.apk");

        let outcome = check_with(&fake, ANDROID_TOKEN, "0.2.0", &dest);

        assert_eq!(
            outcome,
            CheckOutcome::UpToDate {
                current: "0.2.0".into()
            }
        );
        assert!(!dest.exists());
        assert_eq!(fake.calls.borrow().len(), 1);
    }

    #[test]
    fn a_digest_mismatch_never_writes_the_file() {
        let fake = Fake::new(&[
            (
                "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
                RELEASE.as_bytes(),
            ),
            ("https://example/android.apk", b"tampered".as_slice()),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("kal-update.apk");

        // RELEASE promises the hash "IGNORED", which the payload cannot match.
        let outcome = check_with(&fake, ANDROID_TOKEN, "0.2.0", &dest);

        match outcome {
            CheckOutcome::Failed { message } => {
                assert!(message.starts_with("SHA-256 mismatch"), "{message}");
            }
            other => panic!("expected a failed check, got {other:?}"),
        }
        assert!(!dest.exists(), "a bad digest must not leave a staged APK");
    }

    #[test]
    fn a_network_failure_is_reported_not_panicked() {
        let fake = Fake::failing("dns sadness");
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            check_with(&fake, ANDROID_TOKEN, "0.2.0", &dir.path().join("a.apk")),
            CheckOutcome::Failed {
                message: "update check failed: dns sadness".into()
            }
        );
    }

    #[test]
    fn a_bad_release_payload_is_reported() {
        let fake = Fake::new(&[(
            "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
            b"<html>nope</html>".as_slice(),
        )]);
        let dir = tempfile::tempdir().unwrap();

        match check_with(&fake, ANDROID_TOKEN, "0.2.0", &dir.path().join("a.apk")) {
            CheckOutcome::Failed { message } => {
                assert!(message.starts_with("bad release payload: "), "{message}");
            }
            other => panic!("expected a failed check, got {other:?}"),
        }
    }

    #[test]
    fn a_failed_artifact_download_explains_itself() {
        let fake = Fake::new(&[(
            "https://api.github.com/repos/MihneaMoso/kal/releases/latest",
            RELEASE.as_bytes(),
        )]);
        let dir = tempfile::tempdir().unwrap();

        // The release points at an asset the fake does not serve.
        match check_with(&fake, ANDROID_TOKEN, "0.2.0", &dir.path().join("a.apk")) {
            CheckOutcome::Failed { message } => {
                assert!(
                    message.contains("download failed: unexpected url"),
                    "{message}"
                );
            }
            other => panic!("expected a failed check, got {other:?}"),
        }
    }

    #[test]
    fn parse_generic_asset_prefix() {
        // The matcher uses substring containment, so the git-sha-embedded
        // names all resolve.
        assert!("kal-0.1.7-9f77d464-kal-linux.tar.gz".contains(LINUX_TOKEN));
        assert!("kal-0.1.7-9f77d464-kal-windows.exe.tar.gz".contains(WINDOWS_TOKEN));
        assert!("kal-0.1.7-android.apk".contains(ANDROID_TOKEN));
    }

    /// Android self-update behavior: an APK is only applied when it is strictly
    /// newer than what is installed. Equal or older versions (downgrades) must
    /// never be installed, even across minor/`v`-prefix differences.
    #[test]
    fn android_apk_updates_only_when_strictly_newer() {
        assert!(is_newer("0.1.8", "0.1.7"), "patch bump is newer");
        assert!(
            is_newer("v0.2.0", "0.1.9"),
            "v-prefix + minor bump is newer"
        );
        assert!(is_newer("0.10.0", "0.9.9"), "10.0 > 9.9 (component-wise)");
        assert!(!is_newer("0.1.7", "0.1.7"), "no-op same version");
        assert!(!is_newer("0.1.6", "0.1.7"), "downgrade rejected");
        assert!(!is_newer("0.1.7", "0.1.8"), "older build rejected");
    }

    /// Android release asset naming uses a version-only `.apk` (no git-sha in
    /// the name). Confirm that resolves cleanly and that a malformed asset
    /// (no android token) is rejected rather than silently chosen.
    #[test]
    fn android_asset_without_git_sha_resolves() {
        let body = r#"{
            "tag_name": "v0.1.9",
            "assets": [
                {"name": "kal-0.1.9-android.apk",
                 "browser_download_url": "https://example/ka.apk",
                 "digest": "sha256:cafe"},
                {"name": "kal-0.1.9-linux.tar.gz",
                 "browser_download_url": "https://example/linux.tar.gz",
                 "digest": "sha256:beef"}
            ]
        }"#;
        let android = pick_asset(body, ANDROID_TOKEN).unwrap();
        assert_eq!(android.version, "0.1.9");
        assert_eq!(android.asset_url, "https://example/ka.apk");
        assert_eq!(android.sha256.as_deref(), Some("cafe"));

        let missing = r#"{"tag_name": "v0.1.9", "assets": [
            {"name": "kal-0.1.9-linux.tar.gz", "browser_download_url": "u", "digest": "s"}
        ]}"#;
        assert!(
            pick_asset(missing, ANDROID_TOKEN).is_err(),
            "no .apk asset must not silently pick the wrong binary"
        );
    }
}
