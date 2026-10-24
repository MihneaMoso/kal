// The owned Kotlin Android app (the Dioxus → Kotlin rewrite).
//
// This project replaces the dx-generated one, which lives under target/dx/… and
// is wiped on every build (see ARCHITECTURE.md / STEP0_ANDROID_KOTLIN_REWRITE.md).
// The Rust half is built and staged by scripts/build-kotlin-android.sh.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "kal"
include(":app")
