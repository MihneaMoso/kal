#!/usr/bin/env bash
# Shared Android cross-compilation environment: NDK discovery + linker wiring.
#
# SOURCED, not executed (`. scripts/android-env.sh`) — it exports into the
# calling shell:
#   NDK_BIN   the NDK toolchain bin dir
#   TARGET    the Rust target (from $TARGET, else $ARCH, else aarch64-linux-android)
#   CARGO_TARGET_<TARGET>_{LINKER,CC,AR}
#
# The NDK's versioned clang is a launcher script whose directory must contain
# `clang`, so we wrap it in a shim dir rather than symlinking.
#
# Requires the NDK on disk. Honors the env vars dx/CI set up:
#   ANDROID_NDK_HOME / NDK_HOME / ANDROID_HOME/ndk/<ver> / ANDROID_SDK_ROOT/ndk/<ver>

TARGET="${TARGET:-${ARCH:-aarch64-linux-android}}"

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
  return 1 2>/dev/null || exit 1
fi

# Map the Rust target to the NDK clang triple. The `-android21` suffix is the
# NDK's minimum API level for the wrapper; the app's own floor is minSdk 30.
case "$TARGET" in
  aarch64-linux-android)   TRIPLE="aarch64-linux-android21" ;;
  armv7-linux-androideabi) TRIPLE="armv7-linux-androideabi21" ;;
  x86_64-linux-android)    TRIPLE="x86_64-linux-android21" ;;
  i686-linux-android)      TRIPLE="i686-linux-android21" ;;
  *) echo "Unsupported target: $TARGET" >&2; return 1 2>/dev/null || exit 1 ;;
esac

_SHIM="$(mktemp -d)"
cat > "$_SHIM/llvm-clang" <<EOF
#!/bin/bash
exec "$NDK_BIN/clang" --target=$TRIPLE "\$@"
EOF
chmod +x "$_SHIM/llvm-clang"
ln -sf "$_SHIM/llvm-clang" "$_SHIM/$TARGET-clang"
ln -s "$NDK_BIN/llvm-ar" "$_SHIM/$TARGET-ar"

export PATH="$_SHIM:$PATH"
_UPPER="$(echo "$TARGET" | tr '[:lower:]' '[:upper:]' | tr '-' '_')"
export "CARGO_TARGET_${_UPPER}_LINKER=$TARGET-clang"
export "CARGO_TARGET_${_UPPER}_CC=$TARGET-clang"
export "CARGO_TARGET_${_UPPER}_AR=$TARGET-ar"

cleanup_android_env() { rm -rf "$_SHIM"; }
trap cleanup_android_env EXIT
