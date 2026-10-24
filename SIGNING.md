# Android signing runbook (stable release keystore)

Owner-facing steps for creating the Kal release keystore and wiring it into CI.
The agent does **not** hold key material: you generate the keystore, store it in
CI secrets and a safe backup; the agent wires the Gradle signing config,
`versionCode` and the release job.

Related decisions: Step 0 Q2 (stable keystore now + monotonic `versionCode`),
`DECISIONS.md` D31/D32 (bridge), `STEP0_ANDROID_KOTLIN_REWRITE.md`.

## 1. Why this is required

Android only replaces an installed app in place when **all three** hold:

1. the same `applicationId` (`com.kal.calendar` — keeping it, Step 0 Q8),
2. the **same signing certificate**, and
3. a **`versionCode` greater than or equal to** the installed one (Play requires
   strictly greater; `adb install -r` tolerates equal).

Any of these failing means the install is rejected (`INSTALL_FAILED_UPDATE_INCOMPATIBLE`)
and the only way forward is uninstall + reinstall, which **wipes app data**.

## 2. Current reality (verified in this repo)

| Fact | Evidence |
|---|---|
| Android releases are the owned Kotlin app | `.github/workflows/release.yml` `android` job → `scripts/build-kotlin-android.sh --release --apk-release` |
| Tag builds are signed with the stable key; a missing secret **fails the job** instead of shipping debug | `release.yml` "Decode the release keystore" + Gradle `signingConfigs.release` |
| `versionCode` is tag-derived: `major*1_000_000 + minor*1_000 + patch` | `android/app/build.gradle.kts` (`kalVersionCode`) |
| Day-to-day local installs keep the debug key | `:app:assembleDebug`; the release variant falls back to debug only when no keystore is configured |
| App data path depends on the id | `<app files>/kal/calendar.db`, `sync-identity.json`, `sync-node.json` (0600) |

**Consequence:** the build currently installed on the owner's phone is
debug-signed, so the first *stable-key* release still needs the one uninstall in
§5. After that, `adb install -r` / Play upgrades work in place.

## 3. Owner steps (run once)

Requires JDK 17 (`keytool` ships with it).

```bash
# 1. Generate the release keystore (PKCS12). Use a long, unique password and
#    store it in your password manager BEFORE continuing — it is unrecoverable.
keytool -genkeypair -v \
  -keystore kal-release.jks \
  -alias kal-release \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -storetype PKCS12 \
  -dname "CN=Kal, OU=Kal, O=Kal, L=, ST=, C="

# 2. Back it up in two separate places (offline copy + password manager).
#    Losing this file permanently ends the upgrade path for every installed user.

# 3. Emit the base64 blob CI will hold as a secret.
base64 -w0 kal-release.jks > kal-release.jks.b64     # GNU coreutils
# macOS: base64 -i kal-release.jks -o kal-release.jks.b64

# 4. Inspect what you produced (sanity check before uploading).
keytool -list -v -keystore kal-release.jks -alias kal-release | head -20
```

Then add **repository secrets** (Settings → Secrets and variables → Actions):

| Secret | Value |
|---|---|
| `KAL_KEYSTORE_BASE64` | contents of `kal-release.jks.b64` (single line) |
| `KAL_KEYSTORE_PASSWORD` | the store password |
| `KAL_KEY_ALIAS` | `kal-release` |
| `KAL_KEY_PASSWORD` | the key password (same as store for PKCS12) |

Do **not** commit `kal-release.jks` / `.b64`; they are ignored by `.gitignore`
(verify locally with `git status --ignored`).

## 4. What is wired (2026-10-23)

1. `.github/workflows/release.yml` decodes `KAL_KEYSTORE_BASE64` to
   `$RUNNER_TEMP/kal-release.jks`, exports `KAL_KEYSTORE_FILE` / passwords, and
   builds the release APK with `scripts/build-kotlin-android.sh --release
   --apk-release --version "$GITHUB_REF_NAME"`. `ci.yml` builds both variants as
   a compile gate (no secrets: the release variant is debug-signed there and is
   not uploaded).
2. Gradle `signingConfigs.release` reads those env vars (or
   `android/keystore.properties` for local release builds) and is applied to the
   release build type only.
3. `versionCode`/`versionName` come from `-PkalVersionName` (the tag), with the
   scheme above.
4. No post-`dx` injection is needed any more: the Android project is checked in
   (`android/`), so the signing config and `versionCode` live in the build file.
5. The old `scripts/build-android-install.sh` (which uninstalled first) is gone
   with the Dioxus Android target; nothing in the current build path touches
   app data.

## 5. The one unavoidable break

Moving to the stable key requires **one** uninstall of whatever is installed
today (it is debug-signed), which wipes the app's local data. Plan it:

1. Export the calendar (**Settings → export ICS**) — the only supported
   user-facing data path.
2. `adb uninstall com.kal.calendar`.
3. Install the first **stable-key-signed** build.
4. Import the ICS.

Optional pre-break safety net while the installed build is still debuggable:
`adb exec-out run-as com.kal.calendar cat files/kal/calendar.db > calendar.db`
(only works on a `debuggable` app; a release build cannot be read this way).

From that point on, every release must be signed with the same keystore and keep
an increasing `versionCode`, and installs must **not** uninstall first.

## 6. Verification checklist

```bash
# Certificate + versionCode of a built APK
apksigner verify --print-certs kal-<version>-android.apk
aapt2 dump badging kal-<version>-android.apk | grep -E "package:|versionCode"
# Expected: package name com.kal.calendar, versionCode from the scheme above,
# and the same SHA-256 cert digest on consecutive releases.

# In-place upgrade must succeed without uninstalling:
adb install -r kal-<newer>-android.apk
```

Gate: two consecutive **stable-key** releases must install over each other with
`adb install -r` and keep the DB — the first stable-key install needs the §5
uninstall because today's build is debug-signed. The release asset name stays
`kal-<version>-android.apk` (Step 0 §17).

## 7. Rotation / compromise

Rotating the key is equivalent to a new app identity: no installed user can
upgrade in place. Treat the keystore as permanent; if it is ever exposed,
`versionCode` continuity and the upgrade path are lost, and the replacement must
ship as a fresh install (with the ICS export/import path above).
