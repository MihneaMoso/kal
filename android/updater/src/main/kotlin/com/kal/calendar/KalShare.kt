package com.kal.calendar

import android.app.Activity
import android.content.Intent
import android.net.Uri

/**
 * Shares the last-exported `Kal-export.ics` (written by Rust into
 * `filesDir/kal/` and served over [KalFileProvider]) through the system
 * share sheet, so the user can save it to Downloads/Drive/etc. Called from
 * Rust via JNI after the export file is written.
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
