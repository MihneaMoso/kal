package com.kal.calendar.bridge

import java.time.OffsetDateTime
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the two native contracts the editor sits on: the core-model item it
 * prefills from (`kal_item_json`, snake_case) and the request it saves with
 * (`kal_save_item_json`, camelCase).
 *
 * The Rust side freezes the same shapes in `crates/kal-ffi/tests/bridge.rs`.
 */
class EditorPayloadsTest {

    /** One item as the core model serialises it, with both reminder kinds. */
    private val stored = """
        {"id":"01ITEM","kind":"event","title":"Standup","notes":null,"location":"Room 2",
         "calendar_id":"01CAL","start":"2026-08-24T09:00:00+02:00","end":"2026-08-24T09:15:00+02:00",
         "all_day":false,"rrule":"FREQ=WEEKLY;BYDAY=MO","exdates":[],"completed":null,
         "reminders":[{"id":"01REM","offset":{"type":"minutes_before","minutes":10},"method":"push"},
                      {"id":"01ABS","offset":{"type":"absolute","at":"2026-08-24T08:00:00+02:00"},"method":"push"}],
         "color_override":"#e91e63","created_at":"2026-01-01T00:00:00+02:00",
         "updated_at":"2026-01-02T00:00:00+02:00","deleted":false,
         "metadata":{"birthday_of":null}}
    """.trimIndent()

    @Test
    fun `item prefill reads the core model's snake_case fields`() {
        val item = Payloads.parseItem(stored)!!

        assertEquals("01ITEM", item.id)
        assertEquals(ItemKind.EVENT, item.kind)
        assertEquals("Standup", item.title)
        assertEquals("Room 2", item.location)
        assertEquals("01CAL", item.calendarId)
        assertEquals(OffsetDateTime.parse("2026-08-24T09:00:00+02:00"), item.start)
        assertEquals(OffsetDateTime.parse("2026-08-24T09:15:00+02:00"), item.end)
        assertFalse(item.allDay)
        assertEquals("FREQ=WEEKLY;BYDAY=MO", item.rrule)
        assertFalse(item.completed)
        assertEquals("#e91e63", item.colorOverride)
        assertNull(item.person)

        // Only the relative reminder is an offset the editor can represent; an
        // absolute one is skipped rather than guessed at.
        assertEquals(listOf(10L), item.reminderMinutes)
        // An ordinary event is not attached to a series.
        assertNull(item.seriesId)
    }

    @Test
    fun `a detached occurrence prefill carries its series link`() {
        val item = Payloads.parseItem(
            """{"id":"01COPY","kind":"event","title":"Standup (this one)","notes":null,
                "location":null,"calendar_id":"01CAL","start":"2026-08-31T09:00:00+02:00",
                "end":"2026-08-31T09:15:00+02:00","all_day":false,"rrule":null,"exdates":[],
                "completed":null,"reminders":[],"color_override":null,
                "created_at":"2026-08-31T09:00:00+02:00","updated_at":"2026-08-31T09:00:00+02:00",
                "deleted":false,
                "metadata":{"birthday_of":null,"series_id":"01ITEM",
                            "recurrence_id":"2026-08-31T09:00:00+02:00"}}""",
        )!!

        assertNull("the occurrence itself is single", item.rrule)
        assertEquals("01ITEM", item.seriesId)
    }

    @Test
    fun `a birthday prefill carries its person, a task its completion`() {
        val birthday = Payloads.parseItem(
            """{"id":"01B","kind":"birthday","title":"Ada","notes":null,"location":null,
                "calendar_id":"01CAL","start":"1990-12-10T00:00:00+02:00","end":null,"all_day":true,
                "rrule":"FREQ=YEARLY","exdates":[],"completed":null,"reminders":[],
                "color_override":null,"created_at":"2026-01-01T00:00:00+02:00",
                "updated_at":"2026-01-01T00:00:00+02:00","deleted":false,
                "metadata":{"birthday_of":"Ada Lovelace"}}""",
        )!!
        assertEquals(ItemKind.BIRTHDAY, birthday.kind)
        assertTrue(birthday.allDay)
        assertNull(birthday.end)
        assertNull(birthday.location)
        assertNull(birthday.colorOverride)
        assertEquals("Ada Lovelace", birthday.person)
        assertTrue(birthday.reminderMinutes.isEmpty())

        val task = Payloads.parseItem(
            """{"id":"01T","kind":"task","title":"Buy milk","notes":null,"location":null,
                "calendar_id":"01CAL","start":"2026-08-24T09:00:00+02:00","end":null,"all_day":false,
                "rrule":null,"exdates":[],"completed":"2026-08-24T10:00:00+02:00","reminders":[],
                "color_override":null,"created_at":"2026-01-01T00:00:00+02:00",
                "updated_at":"2026-01-01T00:00:00+02:00","deleted":false,"metadata":{"birthday_of":null}}""",
        )!!
        assertEquals(ItemKind.TASK, task.kind)
        assertTrue(task.completed)
        assertNull(task.rrule)
    }

    @Test
    fun `an absent or unreadable item is null, never a crash`() {
        // `kal_item_json` answers the literal `null` for an unknown id.
        assertNull(Payloads.parseItem(null))
        assertNull(Payloads.parseItem("null"))
        assertNull(Payloads.parseItem("not json at all"))
    }

    @Test
    fun `the form serialises the exact keys the native save reads`() {
        val form = EditorForm(
            id = "01ITEM",
            occurrenceStart = "2026-08-24T09:00:00+02:00",
            kind = ItemKind.EVENT,
            title = "Standup",
            date = "2026-08-24",
            startTime = "09:00",
            endTime = "09:30",
            allDay = false,
            calendarId = "01CAL",
            location = "Room 2",
            person = "",
            rrule = "FREQ=WEEKLY",
            reminderMinutes = listOf(10L, 30L),
            customReminder = "45",
            color = "#3366cc",
            scope = EditScope.THIS_ONLY,
        )

        val json = JSONObject(form.toJson())

        assertEquals("01ITEM", json.getString("id"))
        assertEquals("2026-08-24T09:00:00+02:00", json.getString("occurrenceStart"))
        assertEquals("event", json.getString("kind"))
        assertEquals("Standup", json.getString("title"))
        assertEquals("2026-08-24", json.getString("date"))
        assertEquals("09:00", json.getString("startTime"))
        assertEquals("09:30", json.getString("endTime"))
        assertFalse(json.getBoolean("allDay"))
        assertEquals("01CAL", json.getString("calendarId"))
        assertEquals("Room 2", json.getString("location"))
        assertEquals("", json.getString("person"))
        assertEquals("FREQ=WEEKLY", json.getString("rrule"))
        assertEquals(listOf(10L, 30L), json.getJSONArray("reminders").let { arr ->
            (0 until arr.length()).map { arr.getLong(it) }
        })
        assertEquals("45", json.getString("customReminder"))
        assertEquals("#3366cc", json.getString("color"))
        assertEquals("this_only", json.getString("scope"))
    }

    @Test
    fun `a blank form sends nulls so the native defaults apply`() {
        val blank = EditorForm(
            id = null,
            occurrenceStart = null,
            kind = ItemKind.TASK,
            title = "Buy milk",
            date = "2026-08-24",
            startTime = "09:00",
            endTime = "",
            allDay = false,
            calendarId = null,
            location = "",
            person = "",
            rrule = null,
            reminderMinutes = emptyList(),
            customReminder = "",
            color = null,
            scope = EditScope.ALL,
        )

        val json = JSONObject(blank.toJson())

        assertTrue(json.isNull("id"))
        assertTrue(json.isNull("occurrenceStart"))
        assertTrue(json.isNull("calendarId"))
        assertTrue(json.isNull("rrule"))
        assertTrue(json.isNull("color"))
        assertEquals(0, json.getJSONArray("reminders").length())
        assertEquals("all", json.getString("scope"))
        // An empty end time is what a task and an all-day item have.
        assertEquals("", json.getString("endTime"))
    }
}
