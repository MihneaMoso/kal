#!/usr/bin/env bash
# Build the Rust half for Android, stage the JNI library into the Kotlin
# project's jniLibs, and (optionally) assemble the debug APK.
#
#   scripts/build-kotlin-android.sh              # cross-build + stage the .so
#   scripts/build-kotlin-android.sh --apk        # …then gradle :app:assembleDebug
#   scripts/build-kotlin-android.sh --release    # release-profile .so
#   scripts/build-kotlin-android.sh --release --apk-release --version v0.2.0
#                                                # signed release APK for a tag
#
# The Kotlin app ships arm64-v8a only (Step 0 decision: ABI matrix), so this
# refuses to run for other targets rather than silently producing a library the
# APK will not contain.
set -euo pipefail
cd "$(dirname "$0")/.."

PROJECT="android"
JNI_DIR="$PROJECT/app/src/main/jniLibs/arm64-v8a"
JNI_LIB="libkal_jni.so"
PROFILE=debug
BUILD_APK=0
BUILD_APK_RELEASE=0
VERSION=""

while [ $# -gt 0 ]; do
  case "$1" in
    --apk) BUILD_APK=1 ;;
    --apk-release) BUILD_APK_RELEASE=1; PROFILE=release ;;
    --release) PROFILE=release ;;
    --version)
      shift
      VERSION="${1:-}"
      [ -n "$VERSION" ] || { echo "--version needs a value" >&2; exit 2; }
      ;;
    *) echo "unknown argument: $1 (usage: $0 [--apk] [--apk-release] [--release] [--version <tag>])" >&2; exit 2 ;;
  esac
  shift
done

# shellcheck source=scripts/android-env.sh
. scripts/android-env.sh

if [ "$TARGET" != "aarch64-linux-android" ]; then
  echo "the Kotlin app ships arm64-v8a only; refusing to build for $TARGET" >&2
  exit 1
fi

echo "==> cargo build kal-jni ($PROFILE, $TARGET)"
if [ "$PROFILE" = "release" ]; then
  cargo build -p kal-jni --release --target "$TARGET"
  SRC="target/$TARGET/release/$JNI_LIB"
else
  cargo build -p kal-jni --target "$TARGET"
  SRC="target/$TARGET/debug/$JNI_LIB"
fi
[ -f "$SRC" ] || { echo "missing $SRC" >&2; exit 1; }

mkdir -p "$JNI_DIR"
cp "$SRC" "$JNI_DIR/$JNI_LIB"
echo "==> staged $JNI_DIR/$JNI_LIB"

# Prove the JNI entry points the Kotlin declarations bind to are really exported
# (a typo in the symbol name fails here, not at runtime on the device).
NM="${NM:-nm}"
missing=0
check_exports() {
  # $1 = Kotlin file, $2 = JNI symbol prefix (class path)
  local file="$1" prefix="$2" sym
  for sym in $(grep -oE 'external fun [a-zA-Z]+' "$file" | awk '{print $3}' | sort -u); do
    if ! "$NM" -D --defined-only "$JNI_DIR/$JNI_LIB" | grep -qE "[[:space:]]T[[:space:]]+${prefix}${sym}$"; then
      echo "missing JNI export: ${prefix}${sym}" >&2
      missing=1
    fi
  done
}

check_exports "$PROJECT/app/src/main/kotlin/com/kal/calendar/bridge/KalNative.kt" \
  "Java_com_kal_calendar_bridge_KalNative_"
# The home-screen widgets go through their own class (ported from the Dioxus
# build), but the same library.
check_exports "$PROJECT/app/src/main/kotlin/com/kal/calendar/widgets/KalWidgets.kt" \
  "Java_com_kal_calendar_widgets_KalWidgets_"
[ "$missing" -eq 0 ] && echo "==> JNI symbols OK"

if [ "$BUILD_APK" = 1 ]; then
  echo "==> gradle :app:assembleDebug"
  ( cd "$PROJECT" && ./gradlew :app:assembleDebug --no-daemon )
  echo "==> APK:"
  find "$PROJECT/app/build/outputs/apk" -name '*.apk' 2>/dev/null | sed 's/^/    /'
fi

if [ "$BUILD_APK_RELEASE" = 1 ]; then
  echo "==> gradle :app:assembleRelease (version ${VERSION:-default})"
  args=(:app:assembleRelease --no-daemon)
  [ -n "$VERSION" ] && args+=("-PkalVersionName=$VERSION")
  ( cd "$PROJECT" && ./gradlew "${args[@]}" )
  echo "==> APK:"
  ls -l "$PROJECT/app/build/outputs/apk/release/"*.apk
fi
