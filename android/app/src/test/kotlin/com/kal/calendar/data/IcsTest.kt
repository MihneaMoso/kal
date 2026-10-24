package com.kal.calendar.data

import com.kal.calendar.bridge.KalBridge
import com.kal.calendar.ui.exportFailure
import com.kal.calendar.ui.importFailure
import com.kal.calendar.ui.unreadableFile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the parts of the import/export slice that have no Android framework in
 * them: the file path the native export writes (which must match the one the
 * content provider serves), the picker filter, and the sidebar's copy for a
 * rejected action.
 */
class IcsTest {

    @Test
    fun export_path_is_the_one_the_provider_serves() {
        // KalFileProvider serves filesDir/kal/Kal-export.ics as
        // content://com.kal.calendar.updates/ics — this is the write side of it.
        assertEquals(
            "/data/user/0/com.kal.calendar/files/kal/Kal-export.ics",
            KalBridge.exportPath("/data/user/0/com.kal.calendar/files"),
        )
        assertEquals("Kal-export.ics", KalBridge.EXPORT_FILE)
    }

    @Test
    fun export_path_sits_beside_the_database() {
        val filesDir = "/data/user/0/com.kal.calendar/files"
        val dbDir = KalBridge.databasePath(filesDir).substringBeforeLast('/')
        assertEquals(dbDir, KalBridge.exportPath(filesDir).substringBeforeLast('/'))
    }

    @Test
    fun the_picker_accepts_any_file_like_upstream_did() {
        assertEquals("*/*", ICS_PICKER_MIME)
    }

    @Test
    fun an_import_without_a_file_calendar_name_uses_upstreams_fallback() {
        assertEquals("Imported", KalBridge.DEFAULT_ICS_CALENDAR)
    }

    @Test
    fun a_failed_import_says_so_and_keeps_the_native_reason() {
        val notice = importFailure("failed to parse ics: missing END:VCALENDAR")

        assertTrue(notice.startsWith("Import failed: "))
        assertTrue(notice.contains("failed to parse ics: missing END:VCALENDAR"))
    }

    @Test
    fun a_failed_export_says_so_and_keeps_the_native_reason() {
        val notice = exportFailure("write /x/Kal-export.ics failed: Permission denied")

        assertTrue(notice.startsWith("Export failed: "))
        assertTrue(notice.contains("Permission denied"))
    }

    @Test
    fun a_failure_with_no_reason_still_explains_itself() {
        // The bridge normally sets a reason; a missing one must not render as
        // "Import failed: null".
        assertFalse(importFailure(null).contains("null"))
        assertTrue(importFailure(null).startsWith("Import failed: "))
        assertTrue(exportFailure(null).startsWith("Export failed: "))
    }

    @Test
    fun an_unreadable_document_is_reported_not_silently_skipped() {
        assertEquals("Could not read the selected file", unreadableFile())
    }
}
