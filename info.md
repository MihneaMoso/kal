To build locally
# One-time: Rust Android target + Android SDK/NDK
rustup target add aarch64-linux-android
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;25.2.9519653"

# Android app (Kotlin UI + libkal_jni.so; the Rust half is cross-built and staged)
export ANDROID_HOME="$HOME/Android/Sdk"
scripts/build-kotlin-android.sh --apk          # debug APK
adb install -r android/app/build/outputs/apk/debug/app-debug.apk

# Signed release APK (tag builds in CI use the same script + repository secrets)
scripts/build-kotlin-android.sh --release --apk-release --version v0.2.0
#   needs android/keystore.properties or KAL_KEYSTORE_FILE/KAL_KEYSTORE_PASSWORD/
#   KAL_KEY_ALIAS/KAL_KEY_PASSWORD; see SIGNING.md

# Desktop
cargo build --release

To get APK from CI
Push to main (or open a PR) → Actions tab → kotlin-build job → download
kal-kotlin-debug.apk. Tag pushes publish kal-<version>-android.apk (signed with
the release key) via the release workflow.

Remaining gaps
- Schedule-widget rows open the app's current view; no per-event deep link yet
  (month-widget day taps do open that day's new-event editor).
- iOS WidgetKit extension and the Dioxus iOS shell.
- Desktop extras (mini-widget, rfd dialogs) stay behind desktop-only gates.
