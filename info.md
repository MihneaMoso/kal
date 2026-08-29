To build locally
# One-time: install Rust Android targets + Android SDK/NDK
rustup target add aarch64-linux-android
sdkmanager "platforms;android-34" "build-tools;34.0.0" "ndk;25.2.9519653" "cmake;3.22.1"

# Build APK
export NDK_HOME="$ANDROID_HOME/ndk/25.2.9519653"
cd app && dx bundle --android --release --target aarch64-linux-android --package-types apk
To get APK from CI
Push to main (or open a PR) → Actions tab → android-build job → download kal-android.apk artifact.
What's NOT yet wired up
- Android notifications (uses NullNotifier — needs AlarmManager FFI)
- Android file import/export (sidebar section hidden — needs Storage Access Framework)
- Desktop features (mini-widget, rfd dialogs) are cleanly behind gates for when you add them back
