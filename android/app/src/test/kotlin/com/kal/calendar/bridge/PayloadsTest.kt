package com.kal.calendar.bridge

import java.time.LocalDate
import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the exact native JSON contract the Kotlin shell parses (the shapes are
 * frozen on the Rust side by `crates/kal-ffi/tests/bridge.rs`, and the header
 * `widgets/kal_ffi.h` documents them).
 */
class PayloadsTest {

    private val timed = """
        {"id":"01ITEM","calendarId":"01CAL","title":"Standup","start":"2026-08-24T09:00:00+02:00",
         "end":"2026-08-24T09:15:00+02:00","allDay":false,"kind":"event","completed":false,
         "color":"#3366cc","overrideColor":null,"age":null}
    """.trimIndent()

    private val allDay = """
        {"id":"01HOLIDAY","calendarId":"01CAL","title":"Holiday","start":"2026-08-24T00:00:00+02:00",
         "end":null,"allDay":true,"kind":"event","completed":false,"color":"",
         "overrideColor":null,"age":null}
    """.trimIndent()

    @Test
    fun `month payload parses grid cells and their entries`() {
        val json = """
            [[{"date":"2026-07-27","inMonth":false,"items":[]},
              {"date":"2026-08-24","inMonth":true,"items":[$timed,$allDay]}]]
        """.trimIndent()

        val rows = Payloads.parseMonth(json)

        assertEquals(1, rows.size)
        assertEquals(2, rows[0].size)
        assertFalse(rows[0][0].inMonth)
        assertEquals(LocalDate.of(2026, 7, 27), rows[0][0].date)
        assertTrue(rows[0][1].inMonth)

        val first = rows[0][1].items[0]
        assertEquals("01ITEM", first.id)
        assertEquals("01CAL", first.calendarId)
        assertEquals("Standup", first.title)
        assertEquals(ItemKind.EVENT, first.kind)
        assertFalse(first.allDay)
        assertFalse(first.completed)
        assertNull(first.overrideColor)
        assertNull(first.age)
        assertEquals("#3366cc", first.color)
        assertEquals(OffsetDateTime.parse("2026-08-24T09:00:00+02:00"), first.start)
        assertEquals(OffsetDateTime.parse("2026-08-24T09:15:00+02:00"), first.end)

        // A null end and an empty colour survive (the calendar had no colour).
        val second = rows[0][1].items[1]
        assertTrue(second.allDay)
        assertNull(second.end)
        assertEquals("", second.color)
    }

    @Test
    fun `week payload parses seven days including empty ones`() {
        val json = """
            [{"date":"2026-08-24","items":[$timed]},{"date":"2026-08-25","items":[]}]
        """.trimIndent()

        val days = Payloads.parseWeek(json)

        assertEquals(2, days.size)
        assertEquals(LocalDate.of(2026, 8, 24), days[0].date)
        assertEquals(1, days[0].items.size)
        assertTrue(days[1].items.isEmpty())
    }

    @Test
    fun `day payload splits the all-day strip from positioned blocks`() {
        val json = """
            {"date":"2026-08-24","allDay":[$allDay],
             "timed":[{"item":$timed,"topFrac":0.375,"heightFrac":0.020833333333333332,
                       "lane":1,"lanes":2}]}
        """.trimIndent()

        val day = Payloads.parseDay(json)!!

        assertEquals(LocalDate.of(2026, 8, 24), day.date)
        assertEquals(listOf("Holiday"), day.allDay.map { it.title })
        assertEquals(1, day.timed.size)

        val block = day.timed[0]
        assertEquals("Standup", block.item.title)
        assertEquals(0.375, block.topFrac, 1e-12)
        assertEquals(0.020833333333333332, block.heightFrac, 1e-12)
        assertEquals(1, block.lane)
        assertEquals(2, block.lanes)
    }

    @Test
    fun `agenda payload keeps the native chronological order`() {
        val json = """
            [{"date":"2026-08-24","item":$timed},{"date":"2026-08-26","item":$allDay}]
        """.trimIndent()

        val rows = Payloads.parseAgenda(json)

        assertEquals(listOf("Standup", "Holiday"), rows.map { it.item.title })
        assertEquals(LocalDate.of(2026, 8, 26), rows[1].date)
    }

    @Test
    fun `birthday entries carry their age and kind`() {
        val json = """
            [[{"date":"2026-08-24","inMonth":true,"items":[
              {"id":"01B","calendarId":"01C2","title":"Ada","start":"1990-08-24T00:00:00+02:00",
               "end":null,"allDay":true,"kind":"birthday","color":"#e91e63","age":36}]}]]
        """.trimIndent()

        val entry = Payloads.parseMonth(json)[0][0].items[0]

        assertEquals(ItemKind.BIRTHDAY, entry.kind)
        assertEquals(36, entry.age)
    }

    @Test
    fun `task completion crosses in the entry`() {
        val json = """
            [[{"date":"2026-08-24","inMonth":true,"items":[
              {"id":"01T","calendarId":"01CAL","title":"Ship it","start":"2026-08-24T10:00:00+02:00",
               "end":null,"allDay":false,"kind":"task","completed":true,"color":"#3366cc","age":null}]}]]
        """.trimIndent()

        val entry = Payloads.parseMonth(json)[0][0].items[0]

        assertEquals(ItemKind.TASK, entry.kind)
        assertTrue(entry.completed)
    }

    @Test
    fun `the override colour is kept apart from the effective colour`() {
        val json = """
            [[{"date":"2026-08-24","inMonth":true,"items":[
              {"id":"01T","calendarId":"01CAL","title":"Tinted","start":"2026-08-24T10:00:00+02:00",
               "end":null,"allDay":false,"kind":"event","completed":false,"color":"#112233",
               "overrideColor":"#112233","age":null}]}]]
        """.trimIndent()

        val entry = Payloads.parseMonth(json)[0][0].items[0]

        assertEquals("#112233", entry.color)
        assertEquals("#112233", entry.overrideColor)
    }

    @Test
    fun `calendar payload keeps visibility source and the raw model for round-trips`() {
        // The wire spellings are `CalendarSource`'s snake_case serde names, and
        // this is the shape `kal_calendars_json` really emits (also pinned from
        // the Rust side by `app_calendar_payload_round_trips_through_upsert`).
        val json = """
            [{"id":"01CAL","name":"Personal","color":"#3366cc","source":"local",
              "visible":true,"deleted":false,"updated_at":"2026-08-01T00:00:00+02:00"},
             {"id":"01BD","name":"Birthdays","color":"#e91e63","source":"birthdays",
              "visible":false,"deleted":false,"updated_at":"2026-08-01T00:00:00+02:00"},
             {"id":"01G","name":"Work","color":"#0f9d58","source":"google_import",
              "visible":true,"deleted":true,"updated_at":"2026-08-01T00:00:00+02:00"},
             {"id":"01I","name":"Holidays","color":"#f4b400","source":"ics_import",
              "visible":true,"deleted":false,"updated_at":"2026-08-01T00:00:00+02:00"}]
        """.trimIndent()

        val cals = Payloads.parseCalendars(json)

        assertEquals(listOf("Personal", "Birthdays", "Work", "Holidays"), cals.map { it.name })
        assertEquals("", cals[0].sourceLabel)
        assertEquals("auto", cals[1].sourceLabel)
        assertEquals("google", cals[2].sourceLabel)
        assertEquals("ics", cals[3].sourceLabel)
        assertFalse(cals[1].visible)
        assertTrue(cals[2].deleted)
        // `deleted` omitted by older rows defaults to false, not a parse failure.
        assertFalse(cals[1].deleted)
    }

    @Test
    fun `malformed payloads are empty rather than fatal`() {
        assertTrue(Payloads.parseMonth(null).isEmpty())
        assertTrue(Payloads.parseMonth("not json").isEmpty())
        assertTrue(Payloads.parseMonth("[{\"date\":\"nope\"}]").isEmpty())
        assertTrue(Payloads.parseWeek("{}").isEmpty())
        assertTrue(Payloads.parseAgenda("[{\"date\":\"2026-08-24\"}]").isEmpty())
        assertTrue(Payloads.parseCalendars(null).isEmpty())
        assertNull(Payloads.parseDay(null))
        assertNull(Payloads.parseDay("{ not json"))
        // A day object missing `timed` still parses (the export always sends it).
        assertTrue(Payloads.parseDay("{\"date\":\"2026-08-24\"}")!!.timed.isEmpty())
    }

    @Test
    fun `sync status and phrase payloads follow the bridge contract`() {
        val unpaired = Payloads.parseSyncStatus("""{"paired":false,"fingerprint":null}""")
        assertFalse(unpaired.paired)
        assertNull(unpaired.fingerprint)
        // Unreadable status must never claim a chain.
        assertFalse(Payloads.parseSyncStatus("not json").paired)
        assertFalse(Payloads.parseSyncStatus(null).paired)

        val paired = Payloads.parseSyncStatus("""{"paired":true,"fingerprint":"abc123"}""")
        assertTrue(paired.paired)
        assertEquals("abc123", paired.fingerprint)

        val phrase = Payloads.parseSyncPhrase(
            """{"phrase":"abandon abandon about","fingerprint":"abc123"}""",
        )!!
        assertEquals("abandon abandon about", phrase.phrase)
        assertEquals("abc123", phrase.fingerprint)
        assertNull(Payloads.parseSyncPhrase(null))
        assertNull(Payloads.parseSyncPhrase("{}"))

        assertEquals(3, Payloads.parseSyncRound("""{"merged":3}""")!!.merged)
        assertEquals(0, Payloads.parseSyncRound("""{"merged":0}""")!!.merged)
        assertNull(Payloads.parseSyncRound(null))
    }

    @Test
    fun `getSetting replies decode to the stored string`() {
        assertEquals("dark", Payloads.decodeText("\"dark\""))
        // A legacy JSON-encoded theme arrives with literal quotes on the value;
        // Preferences unwraps it.
        assertEquals("\"dark\"", Payloads.decodeText("\"\\\"dark\\\"\""))
        assertEquals("{\"theme\":\"dark\"}", Payloads.decodeText("\"{\\\"theme\\\":\\\"dark\\\"}\""))
        assertNull(Payloads.decodeText("null"))
        assertNull(Payloads.decodeText(null))
        // Legacy plain (non-JSON) values still come through verbatim.
        assertEquals("dark", Payloads.decodeText("dark"))
    }
}
