package com.kal.calendar.data

/**
 * What the package installer sees in an APK: its application id, its version
 * code and the SHA-256 of its signing certificate. Both sides of a comparison
 * are plain data, so the decision below stays a pure function the JVM tests can
 * pin without an Android framework.
 */
data class ApkIdentity(
    /** The APK's `applicationId`, or null when the file is not a readable APK. */
    val packageName: String?,
    val versionCode: Long,
    /** SHA-256 of the signing certificate, lowercase hex, or null when unsigned. */
    val signer: String?,
)

/** Why the installer would refuse a staged APK. */
enum class InstallBlock {
    /** The staged file is not an APK at all (truncated or an error page). */
    NotAnApp,

    /** Another application's package: never an update for this one. */
    OtherApp,

    /** No signing certificate to compare: Android installs only signed APKs. */
    Unsigned,

    /**
     * Signed with a different key than the running app — an update is only
     * installable by the same signer, which is the usual reason a release built
     * anywhere other than this install's own signing identity cannot replace it.
     */
    OtherSigner,

    /** Not newer than the running app: the installer refuses a downgrade. */
    Older,
}

/**
 * Whether `staged` can replace `own`, or null when the installer should accept
 * it. Everything is decided *before* the installer intent: Android's own
 * failure ("app not installed as package appears to be invalid") names neither
 * the APK nor the reason, so the app checks what it already knows.
 */
fun installBlockReason(staged: ApkIdentity, own: ApkIdentity): InstallBlock? = when {
    staged.packageName == null -> InstallBlock.NotAnApp
    staged.packageName != own.packageName -> InstallBlock.OtherApp
    staged.signer == null -> InstallBlock.Unsigned
    staged.signer != own.signer -> InstallBlock.OtherSigner
    staged.versionCode < own.versionCode -> InstallBlock.Older
    else -> null
}

/** The settings line for a staged release this build cannot install. */
fun blockedUpdateLine(block: InstallBlock, version: String): String =
    "Update ready: v$version \u2014 cannot install: ${installBlockMessage(block)}"

/** One clause per [InstallBlock], for [blockedUpdateLine]. */
fun installBlockMessage(block: InstallBlock): String = when (block) {
    InstallBlock.NotAnApp -> "the download is not a readable app"
    InstallBlock.OtherApp -> "the download is for a different app"
    InstallBlock.Unsigned -> "the download is not signed"
    InstallBlock.OtherSigner -> "it is signed with a different key than this install"
    InstallBlock.Older -> "it is older than this install"
}
