#!/usr/bin/env bash
# Local Android cross-build verification using the installed Android NDK.
#
# CI builds (and the APK we ship) use aarch64-linux-android; this script runs a
# `cargo check` (and optionally a full link build) against that target so Rust
# errors like android_picker.rs JNI misuse are caught BEFORE pushing to CI,
# where a failure only shows up after a ~4 min dependency build.
#
# Requires the Android NDK on disk. We honor the SDK env vars dx/CI set up:
#   ANDROID_NDK_HOME / NDK_HOME / ANDROID_HOME/Sdk/ndk/<ver>
#
#   scripts/check-android.sh                  # cargo check, aarch64
#   scripts/check-android.sh --build          # full cargo build (links too)
#   ARCH=armv7-linux-androideabi scripts/check-android.sh
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${ARCH:-aarch64-linux-android}"
LINK="${1:---check}"

# Locate the NDK toolchain bin dir.
NDK_BIN=""
for cand in \
  "${ANDROID_NDK_HOME:-}" \
  "${NDK_HOME:-}" \
  "${ANDROID_HOME:-}/ndk/"*/ \
  "${ANDROID_SDK_ROOT:-}/ndk/"*/ ; do
  [ -z "$cand" ] && continue
  b="$cand/toolchains/llvm/prebuilt/linux-x86_64/bin"
  [ -x "$b/clang" ] && NDK_BIN="$b" && break
done
if [ -z "$NDK_BIN" ]; then
  echo "Android NDK toolchain not found (set ANDROID_NDK_HOME)." >&2
  exit 1
fi

# Map the Rust target to the NDK clang triple + its prebuilt basename.
case "$TARGET" in
  aarch64-linux-android)    TRIPLE="aarch64-linux-android21" ;;
  armv7-linux-androideabi)  TRIPLE="armv7-linux-androideabi21" ;; # NDK uses ??? below
  x86_64-linux-android)     TRIPLE="x86_64-linux-android21" ;;
  i686-linux-android)       TRIPLE="i686-linux-android21" ;;
  *) echo "Unsupported target: $TARGET" >&2; exit 1 ;;
esac

# Build a thin PATH override hosting a working `aarch64-linux-android-clang`
# (the NDK's versioned clang is a script whose dirname must contain `clang`,
# so we wrap it instead of symlinking).
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
cat > "$workdir/llvm-clang" <<EOF
#!/bin/bash
exec "$NDK_BIN/clang" --target=$TRIPLE "\$@"
EOF
chmod +x "$workdir/llvm-clang"
ln -sf "$workdir/llvm-clang" "$workdir/$TARGET-clang"
ln -s "$NDK_BIN/llvm-ar" "$workdir/$TARGET-ar"
export PATH="$workdir:$PATH"
export CARGO_TARGET_$(echo "$TARGET" | tr '[:lower:]' '[:upper:]' | tr '-' '_')_LINKER="$TARGET-clang"
export CARGO_TARGET_$(echo "$TARGET" | tr '[:lower:]' '[:upper:]' | tr '-' '_')_CC="$TARGET-clang"
export CARGO_TARGET_$(echo "$TARGET" | tr '[:lower:]' '[:upper:]' | tr '-' '_')_AR="$TARGET-ar"

echo "Verifying $TARGET against NDK $NDK_BIN ..."
if [ "$LINK" = "--build" ]; then
  cargo build -p kal-app --target "$TARGET"
else
  cargo check -p kal-app --target "$TARGET"
fi
echo "Android ($TARGET) cross-build OK."
