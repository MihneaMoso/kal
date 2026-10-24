package com.kal.calendar

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.Signature
import android.net.Uri
import com.kal.calendar.data.ApkIdentity
import com.kal.calendar.data.InstallBlock
import com.kal.calendar.data.installBlockReason
import java.security.MessageDigest

/**
 * Kicks off an APK self-update by firing the system PackageInstaller intent
 * for the staged APK, served over the [KalFileProvider] content URI.
 *
 * Ported verbatim from the dx-project sources (`android/updater/…`), which
 * native code called through JNI; here the interface layer calls it directly
 * after the native check has staged and verified the download, since Kotlin
 * owns intents in the rewrite. Needs `REQUEST_INSTALL_PACKAGES`, without which
 * Android 8+ silently drops the intent.
 */
object KalUpdater {

    @JvmStatic
    fun installApk(context: Context, path: String) {
        // `path` is the native-side staged APK path; we serve the canonical
        // staged file over the provider regardless.
        val uri: Uri = Uri.parse("content://${KalFileProvider.AUTHORITY}/apk")
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        context.startActivity(intent)
    }

    /**
     * Why the installer would refuse the APK staged at [path], or null when it
     * can replace this install. Read before the installer intent: Android's own
     * refusal ("app not installed as package appears to be invalid") does not
     * say which APK or which of the three reasons it was.
     */
    @JvmStatic
    fun stagedInstallBlock(context: Context, path: String): InstallBlock? =
        installBlockReason(stagedIdentity(context, path), ownIdentity(context))

    /** The staged APK as the installer reads it; an unreadable file is an empty identity. */
    private fun stagedIdentity(context: Context, path: String): ApkIdentity {
        val archive = context.packageManager
            .getPackageArchiveInfo(path, PackageManager.GET_SIGNING_CERTIFICATES)
            ?: return ApkIdentity(null, 0, null)
        return ApkIdentity(
            packageName = archive.packageName,
            versionCode = archive.longVersionCode,
            signer = archive.signingInfo?.apkContentsSigners?.firstOrNull()?.digest(),
        )
    }

    /** This installed app's own package, version code and signer. */
    private fun ownIdentity(context: Context): ApkIdentity {
        val pm = context.packageManager
        val info = pm.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        return ApkIdentity(
            packageName = info.packageName,
            versionCode = info.longVersionCode,
            signer = info.signingInfo?.apkContentsSigners?.firstOrNull()?.digest(),
        )
    }

    /** Lowercase hex SHA-256 of a signing certificate, the identity both sides compare. */
    private fun Signature.digest(): String =
        MessageDigest.getInstance("SHA-256")
            .digest(toByteArray())
            .joinToString("") { "%02x".format(it) }
}

