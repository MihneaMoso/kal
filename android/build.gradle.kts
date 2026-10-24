// Root build file for the owned Kotlin Android app.
//
// Plugin versions are pinned here (single source of truth) and applied by the
// `:app` module. They match the versions already in ~/.gradle/caches on this
// machine; AGP 8.7.0 / Kotlin 2.0.20 / Gradle 9.1.0 is the combination the
// dx-generated project used, so no build-tool downloads are needed.
plugins {
    id("com.android.application") version "8.7.0" apply false
    id("org.jetbrains.kotlin.android") version "2.0.20" apply false
    // Kotlin 2.x ships the Compose compiler as a Kotlin plugin (no
    // composeOptions/kotlinCompilerExtensionVersion any more).
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.20" apply false
}
