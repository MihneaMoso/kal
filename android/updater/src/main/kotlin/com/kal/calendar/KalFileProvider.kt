package com.kal.calendar

import android.content.ContentProvider
import android.content.ContentValues
import android.content.UriMatcher
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import java.io.File

/**
 * Minimal content provider that hands out a read-only handle to the staged
 * update APK. Using a content URI (instead of a raw file:// URI) avoids the
 * FileUriExposedException on API 24+ and needs no androidx.core dependency.
 *
 * Registered in the manifest by scripts/stage-updater.sh with authority
 * `com.kal.calendar.updates` and grantUriPermissions so the system package
 * installer may read it (with FLAG_GRANT_READ_URI_PERMISSION on the intent).
 */
class KalFileProvider : ContentProvider() {

    companion object {
        const val AUTHORITY = "com.kal.calendar.updates"
        private const val APK = 1
        private const val ICS = 2
    }

    private val matcher: UriMatcher =
        UriMatcher(UriMatcher.NO_MATCH).apply {
            addURI(AUTHORITY, "apk", APK)
            addURI(AUTHORITY, "ics", ICS)
        }

    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String? = when (matcher.match(uri)) {
        APK -> "application/vnd.android.package-archive"
        ICS -> "text/calendar"
        else -> null
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor? {
        val dir = context?.filesDir ?: return null
        val file = when (matcher.match(uri)) {
            APK -> File(dir, "kal/updates/kal-update.apk")
            // Written by the Rust side on every "Export all (.ics)" tap.
            ICS -> File(dir, "kal/Kal-export.ics")
            else -> return null
        }
        if (!file.exists()) {
            return null
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0
}
