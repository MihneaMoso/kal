package com.kal.calendar

import android.app.Activity
import android.content.Intent
import android.net.Uri

/**
 * Shares the last-exported `Kal-export.ics` (written by the native side into
 * `filesDir/kal/` and served over [KalFileProvider]) through the system share
 * sheet, so the user can save it to Downloads/Drive/etc.
 *
 * Ported verbatim from the dx-project sources (`android/updater/…`), which had
 * Rust call it over JNI; here the interface layer calls it directly after an
 * export succeeded, since Kotlin owns intents in the rewrite.
 */
object KalShare {
    @JvmStatic
    fun shareIcs(activity: Activity) {
        val uri: Uri = Uri.parse("content://${KalFileProvider.AUTHORITY}/ics")
        activity.runOnUiThread {
            val send = Intent(Intent.ACTION_SEND).apply {
                type = "text/calendar"
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            activity.startActivity(Intent.createChooser(send, "Export calendar"))
        }
    }
}
