#!/usr/bin/env bash
# Local Android cross-build verification using the installed Android NDK.
#
# CI builds (and the APK we ship) use aarch64-linux-android; this script runs a
# `cargo check` (and optionally a full link build) against that target so Rust
# errors like android_picker.rs JNI misuse are caught BEFORE pushing to CI,
# where a failure only shows up after a ~4 min dependency build.
#
# Covers the Android artifacts: the versioned C ABI (libkal_ffi.so) and the JNI
# bridge the Kotlin app loads (libkal_jni.so). The Dioxus Android build is
# retired; the desktop app still builds for its own targets.
#
# Requires the Android NDK on disk. We honor the SDK env vars dx/CI set up:
#   ANDROID_NDK_HOME / NDK_HOME / ANDROID_HOME/Sdk/ndk/<ver>
#
#   scripts/check-android.sh                  # cargo check, aarch64
#   scripts/check-android.sh --build          # full cargo build (links too)
#   ARCH=armv7-linux-androideabi scripts/check-android.sh
set -euo pipefail
cd "$(dirname "$0")/.."

LINK="${1:---check}"
# shellcheck source=scripts/android-env.sh
. scripts/android-env.sh

PKGS=(-p kal-ffi -p kal-jni)

echo "Verifying $TARGET against NDK $NDK_BIN ..."
if [ "$LINK" = "--build" ]; then
  cargo build "${PKGS[@]}" --target "$TARGET"
else
  cargo check "${PKGS[@]}" --target "$TARGET"
fi
echo "Android ($TARGET) cross-build OK."
