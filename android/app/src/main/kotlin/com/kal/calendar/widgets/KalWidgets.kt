package com.kal.calendar.widgets

import android.content.Context

/**
 * Thin Kotlin wrapper around the native kal snapshot bridges implemented in
 * libkal_jni.so (`crates/kal-jni`: the widget entry points beside the app's
 * `KalNative` ones). The widgets run in the app process, so they query the
 * database through JNI instead of a content provider.
 */
object KalWidgets {
    init {
        System.loadLibrary("kal_jni")
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
