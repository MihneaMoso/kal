package com.kal.calendar.bridge

import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the reminder-firing payload (`kal_reminder_firings_json`, camelCase) the
 * Android scheduler arms from. `crates/kal-ffi/tests/bridge.rs` freezes the same
 * shape on the Rust side.
 */
class ReminderFiringsTest {

    private val payload = """
        [{"itemId":"01ITEM","reminderId":"01REM",
          "fireAt":"2026-10-20T08:50:00+03:00","title":"Standup"},
         {"itemId":"01TASK","reminderId":"01REM2",
          "fireAt":"2026-10-21T09:00:00+02:00","title":"Pay rent"}]
    """.trimIndent()

    @Test
    fun `firings decode their identity, instant and title`() {
        val firings = Payloads.parseFirings(payload)

        assertEquals(2, firings.size)
        assertEquals("01ITEM", firings[0].itemId)
        assertEquals("01REM", firings[0].reminderId)
        assertEquals(OffsetDateTime.parse("2026-10-20T08:50:00+03:00"), firings[0].fireAt)
        assertEquals("Standup", firings[0].title)
        // A different device offset on the second row is preserved as an instant.
        assertEquals(OffsetDateTime.parse("2026-10-21T09:00:00+02:00"), firings[1].fireAt)
    }

    @Test
    fun `the native order is kept, nearest first`() {
        val titles = Payloads.parseFirings(payload).map { it.title }
        assertEquals(listOf("Standup", "Pay rent"), titles)
    }

    @Test
    fun `an empty array means nothing to arm`() {
        assertTrue(Payloads.parseFirings("[]").isEmpty())
    }

    @Test
    fun `a null reply arms nothing`() {
        assertTrue(Payloads.parseFirings(null).isEmpty())
    }

    @Test
    fun `a malformed payload arms nothing rather than guessing`() {
        // Truncated, not an array, and one row missing its instant: a reconcile
        // must not arm something it could not fully read.
        assertTrue(Payloads.parseFirings("""[{"itemId":"01""").isEmpty())
        assertTrue(Payloads.parseFirings("""{"itemId":"01ITEM"}""").isEmpty())
        assertTrue(
            Payloads.parseFirings(
                """[{"itemId":"01ITEM","reminderId":"01REM","title":"Standup"}]""",
            ).isEmpty(),
        )
    }

    @Test
    fun `an unparsable instant rejects the payload`() {
        assertTrue(
            Payloads.parseFirings(
                """[{"itemId":"01ITEM","reminderId":"01REM","fireAt":"tomorrow","title":"X"}]""",
            ).isEmpty(),
        )
    }
}
