import java.util.Properties

// The Kal Android app module (the Dioxus → Kotlin rewrite).
//
// The native core is NOT built by Gradle: scripts/build-kotlin-android.sh
// cross-compiles `crates/kal-jni` for aarch64 and stages libkal_jni.so into
// src/main/jniLibs/arm64-v8a. Keeping the Rust build in the repo's own script
// means one build story for the app, the widgets and CI.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Release signing (SIGNING.md §4.2): the keystore is never committed, so it
// arrives either as `android/keystore.properties` (local release builds;
// gitignored) or as the CI env vars the release workflow decodes from secrets.
// Without either, the release build type falls back to the debug key so a
// plain `assembleRelease` still works — such an APK can only replace an app
// signed with that same debug key, which is exactly what the installer refuses.
val keystoreProperties = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

fun signingValue(property: String, env: String): String? =
    (keystoreProperties.getProperty(property))?.takeIf { it.isNotBlank() }
        ?: System.getenv(env)?.takeIf { it.isNotBlank() }

val releaseKeystore = signingValue("storeFile", "KAL_KEYSTORE_FILE")

// versionName follows the release tag (vX.Y.Z); versionCode is the monotonic
// scheme fixed in STEP0_ANDROID_KOTLIN_REWRITE.md: major*1_000_000 + minor*1_000
// + patch (v0.1.9 → 1009). Overridable with -PkalVersionName=… so a release
// build can carry the exact tag.
fun kalVersionName(): String =
    (findProperty("kalVersionName") as String?)?.removePrefix("v") ?: "0.2.0"

fun kalVersionCode(): Int {
    val parts = kalVersionName().split(".", "-")
    val major = parts.getOrNull(0)?.toIntOrNull() ?: 0
    val minor = parts.getOrNull(1)?.toIntOrNull() ?: 0
    val patch = parts.getOrNull(2)?.toIntOrNull() ?: 0
    return major * 1_000_000 + minor * 1_000 + patch
}

android {
    namespace = "com.kal.calendar"
    compileSdk = 34

    defaultConfig {
        // applicationId stays com.kal.calendar: the rewrite must be an in-place
        // upgrade of the installed Dioxus app (same data dir, same DB path).
        applicationId = "com.kal.calendar"
        minSdk = 30
        targetSdk = 34
        versionCode = kalVersionCode()
        versionName = kalVersionName()
        // ABI matrix decision: arm64-v8a only.
        ndk { abiFilters += "arm64-v8a" }
    }

    signingConfigs {
        // Only created when a keystore is configured; `findByName("release")`
        // below is then non-null and the release build type uses it.
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = signingValue("storePassword", "KAL_KEYSTORE_PASSWORD")
                keyAlias = signingValue("keyAlias", "KAL_KEY_ALIAS")
                keyPassword = signingValue("keyPassword", "KAL_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            // R8 keep rules for the JNI entry points land with the release
            // wiring (SIGNING.md); minification is off until then so a debug
            // build and a release build behave identically for the bridge.
            isMinifyEnabled = false
            // The debug fallback keeps `assembleRelease` usable without seats
            // for the signing material; it never replaces a differently-signed
            // install, which the settings update check reports instead.
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        // BuildConfig.VERSION_NAME is the app's own version: the settings
        // screen shows it and the update check compares releases against it.
        // AGP 8 only generates the class when asked.
        buildConfig = true
    }

    testOptions {
        // The JVM unit tests exercise pure Kotlin (payload parsing, formatting,
        // navigation, preferences) and never touch a real Android framework.
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.09.00"))
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.6")
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    // android.jar ships org.json as a throwing stub; unit tests get the real one.
    testImplementation("org.json:json:20240303")
}
