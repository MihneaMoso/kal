//! Embeds the Kal release version into the binary at compile time so the
//! in-app updater (`env!("KAL_VERSION")`) always matches the release it was
//! built from — no hand-maintained constant to drift.
//!
//! Resolution order:
//!   1. `KAL_RELEASE_VERSION` env override (set in the release CI from the git
//!      tag, e.g. `v0.1.7`).
//!   2. The nearest git tag (`git describe --tags --abbrev=0`) — the release
//!      commits *are* tags, and dev builds compare against the last tag.
//!   3. `CARGO_PKG_VERSION` (the workspace manifest, `0.1.0`) as a last resort
//!      when neither a tag nor the override is available.
//!
//! The leading `v` is stripped so the value is plain "0.1.7".

fn main() {
    if let Ok(v) = std::env::var("KAL_RELEASE_VERSION") {
        if !v.trim().is_empty() {
            println!(
                "cargo:rustc-env=KAL_VERSION={}",
                v.trim().trim_start_matches('v')
            );
            return;
        }
    }

    if let Ok(out) = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
    {
        if out.status.success() {
            let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !tag.is_empty() {
                println!(
                    "cargo:rustc-env=KAL_VERSION={}",
                    tag.trim_start_matches('v')
                );
                return;
            }
        }
    }

    println!("cargo:rustc-env=KAL_VERSION={}", env!("CARGO_PKG_VERSION"));
}
