#!/usr/bin/env bash
# Build the Android APK (mirroring the CI release workflow) and install it to
# a connected device, replacing the previously installed Kal.
#
#   scripts/build-android-install.sh
#
# Steps: dx bundle -> branded icons -> widgets -> updater/picker Kotlin ->
# gradle assembleDebug -> uninstall old package -> install new APK -> launch.
# Uninstalling first avoids signature/update-incompatible failures when the
# installed build was signed differently. NOTE: this wipes the app's local
# data on the phone.
set -euo pipefail
cd "$(dirname "$0")/.."        # repo root (scripts/../..)

TARGET="${TARGET:-aarch64-linux-android}"
PKG="com.kal.calendar"

# dx regenerates its template launcher icons on every bundle, but a previous
# apply-icons.sh leaves branded .pngs in the generated res/, so the next dx
# bundle merges both AND fails on duplicate resources. CI avoids this because
# it starts from a fresh checkout; locally we wipe the generated project (it
# is rebuilt from templates; the cargo build cache under target/ is untouched).
rm -rf target/dx

echo "==> dx bundle (${TARGET}, release)"
( cd app && dx bundle --android --release --target "$TARGET" --package-types apk )

echo "==> staging branded icons, widgets, updater+picker"
bash scripts/apply-icons.sh
bash scripts/stage-widgets.sh
bash scripts/stage-updater.sh

echo "==> gradle assembleDebug"
( cd target/dx/kal/release/android/app && ./gradlew :app:assembleDebug --no-daemon )

APK="$(find target/dx/kal/release/android/app/app/build/outputs/apk -name '*.apk' | head -1)"
test -n "$APK" || { echo "no APK produced" >&2; exit 1; }
echo "==> APK: $APK"

echo "==> uninstalling old $PKG (wipes app data)"
adb uninstall "$PKG" >/dev/null 2>&1 || true

echo "==> installing $PKG"
adb install "$APK"

echo "==> launching Kal"
adb shell am start -n "$PKG/dev.dioxus.main.MainActivity"

echo "Done. Watch the device."