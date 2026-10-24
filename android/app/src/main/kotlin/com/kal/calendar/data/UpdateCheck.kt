package com.kal.calendar.data

import org.json.JSONObject

/**
 * The outcome of one update check, as `kal_check_updates_json` reports it. The
 * native side owns the release semantics (which asset, which version, verified
 * SHA-256) and downloads the APK; this type is only the status the settings
 * screen shows.
 */
sealed interface UpdateStatus {
    /** The running version is the latest release. */
    data object UpToDate : UpdateStatus

    /** A newer release is downloaded and staged, ready for the installer. */
    data class Ready(val version: String) : UpdateStatus

    /** The check or the download failed; [message] is the native reason. */
    data class Failed(val message: String) : UpdateStatus

    companion object {
        /**
         * Read the bridge's status payload. A missing payload (the call itself
         * failed) or an unrecognised one becomes [Failed], so a broken check can
         * never look like "up to date".
         */
        fun parse(json: String?, fallbackReason: String?): UpdateStatus {
            val obj = runCatching { json?.let(::JSONObject) }.getOrNull()
                ?: return Failed(fallbackReason ?: UNREADABLE)
            return when (obj.optString("status")) {
                "up_to_date" -> UpToDate
                "update_ready" ->
                    obj.optString("version").takeIf { it.isNotEmpty() }
                        ?.let { Ready(it) }
                        ?: Failed(UNREADABLE)
                "error" -> Failed(obj.optString("message").takeIf { it.isNotEmpty() } ?: UNREADABLE)
                else -> Failed(UNREADABLE)
            }
        }

        private const val UNREADABLE = "unreadable update status"
    }
}

/**
 * The status line under the Software & updates controls. The wording is
 * upstream's, except the ready line, which says "install to update" on Android
 * (the package installer replaces the app rather than restarting it).
 */
fun updateStatusLine(status: UpdateStatus, currentVersion: String): String = when (status) {
    UpdateStatus.UpToDate -> "You're up to date (v$currentVersion)"
    is UpdateStatus.Ready -> "Update ready: v${status.version} \u2014 install to update"
    is UpdateStatus.Failed -> "Update check failed: ${status.message}"
}

/** The line after "Apply update now" hands the APK to the package installer. */
fun installLine(opened: Boolean, reason: String?): String =
    if (opened) {
        "Package installer opened \u2014 confirm to finish"
    } else {
        "Install failed: ${reason ?: "unknown error"}"
    }
