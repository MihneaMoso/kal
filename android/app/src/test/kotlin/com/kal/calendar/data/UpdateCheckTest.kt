package com.kal.calendar.data

import com.kal.calendar.bridge.KalBridge
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the Software & updates slice: how the bridge's status payload is read,
 * the status-line copy (upstream's, with Android's "install to update" wording),
 * and the staged-APK path the provider serves.
 */
class UpdateCheckTest {

    @Test
    fun an_up_to_date_status_parses() {
        assertEquals(
            UpdateStatus.UpToDate,
            UpdateStatus.parse("""{"status":"up_to_date"}""", null),
        )
    }

    @Test
    fun a_ready_status_carries_the_new_version() {
        assertEquals(
            UpdateStatus.Ready("0.3.0"),
            UpdateStatus.parse("""{"status":"update_ready","version":"0.3.0"}""", null),
        )
    }

    @Test
    fun an_error_status_keeps_the_native_reason() {
        val status = UpdateStatus.parse(
            """{"status":"error","message":"update check failed: dns sadness"}""",
            null,
        )

        assertEquals(UpdateStatus.Failed("update check failed: dns sadness"), status)
    }

    @Test
    fun a_failed_native_call_reports_its_reason() {
        // The bridge returns null only for a NULL argument or an unsupported
        // platform; the reason is read back from lastError().
        assertEquals(
            UpdateStatus.Failed("null download path"),
            UpdateStatus.parse(null, "null download path"),
        )
        // A missing reason must still be a failure, never "up to date".
        assertTrue(UpdateStatus.parse(null, null) is UpdateStatus.Failed)
    }

    @Test
    fun an_unrecognised_payload_is_a_failure_not_an_ok_status() {
        assertTrue(UpdateStatus.parse("""{"status":"weird"}""", null) is UpdateStatus.Failed)
        assertTrue(UpdateStatus.parse("""{"status":"update_ready"}""", null) is UpdateStatus.Failed)
        assertTrue(UpdateStatus.parse("""{"status":"error"}""", null) is UpdateStatus.Failed)
        assertTrue(UpdateStatus.parse("not json", null) is UpdateStatus.Failed)
    }

    @Test
    fun the_up_to_date_line_matches_upstream() {
        assertEquals(
            "You're up to date (v0.2.0)",
            updateStatusLine(UpdateStatus.UpToDate, "0.2.0"),
        )
    }

    @Test
    fun the_ready_line_says_install_because_android_replaces_the_app() {
        val line = updateStatusLine(UpdateStatus.Ready("0.3.0"), "0.2.0")

        assertTrue(line.contains("Update ready: v0.3.0"))
        assertTrue(line.contains("install to update"))
        assertFalse(line.contains("restart"))
    }

    @Test
    fun the_failure_line_matches_upstream() {
        assertEquals(
            "Update check failed: no asset matching 'android.apk'",
            updateStatusLine(
                UpdateStatus.Failed("no asset matching 'android.apk'"),
                "0.2.0",
            ),
        )
    }

    @Test
    fun the_installer_lines_match_upstream() {
        assertEquals(
            "Package installer opened \u2014 confirm to finish",
            installLine(opened = true, reason = null),
        )
        assertEquals(
            "Install failed: no activity found",
            installLine(opened = false, reason = "no activity found"),
        )
        // A missing reason must not render as "Install failed: null".
        assertFalse(installLine(opened = false, reason = null).contains("null"))
    }

    @Test
    fun the_staged_apk_path_is_the_one_the_provider_serves() {
        // KalFileProvider serves filesDir/kal/updates/kal-update.apk as
        // content://com.kal.calendar.updates/apk — this is the download side.
        assertEquals(
            "/data/user/0/com.kal.calendar/files/kal/updates/kal-update.apk",
            KalBridge.updateApkPath("/data/user/0/com.kal.calendar/files"),
        )
        assertEquals("kal-update.apk", KalBridge.UPDATE_APK_FILE)
        // It must not collide with the export file the share sheet hands out.
        assertNull(
            KalBridge.updateApkPath("/data/user/0/com.kal.calendar/files")
                .takeIf { it == KalBridge.exportPath("/data/user/0/com.kal.calendar/files") },
        )
    }
}
