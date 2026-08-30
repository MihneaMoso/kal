package com.kal.calendar.widgets

import android.content.Context

/**
 * Thin Kotlin wrapper around the native kal snapshot bridges implemented in
 * libmain.so (see app/src/widget_ffi.rs). The calendar itself is a native
 * (Dioxus) activity, so the widgets query it through JNI instead of via a
 * content provider.
 */
object KalWidgets {
    init {
        System.loadLibrary("main")
    }

    external fun nativeSchedule(
        path: String,
        fromEpochSeconds: Long,
        days: Int,
    ): String?

    external fun nativeMonth(
        path: String,
        year: Int,
        month: Int,
        firstDow: Int,
    ): String?

    /** App-private writable data directory (mirrors native query_files_dir). */
    fun dataDir(context: Context): String = context.filesDir.absolutePath

    /** Database file path used by the native calendar. */
    fun dbPath(context: Context): String =
        "${dataDir(context)}/kal/calendar.db"
            .replace("../", "")
}
