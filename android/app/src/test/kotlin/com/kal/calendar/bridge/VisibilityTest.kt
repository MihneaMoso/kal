package com.kal.calendar.bridge

import java.time.LocalDate
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The calendar-visibility filter. The Dioxus shell applied it in the UI layer
 * (`use_visible_items`), so the Kotlin shell does the same — which is why the
 * native entries carry `calendarId`.
 */
class VisibilityTest {

    private fun calendar(id: String, visible: Boolean, deleted: Boolean = false) =
        CalendarModel(
            id = id,
            name = id,
            color = "#3366cc",
            source = "Local",
            visible = visible,
            deleted = deleted,
            raw = """{"id":"$id","name":"$id","color":"#3366cc","source":"Local",""" +
                """"visible":$visible,"deleted":$deleted}""",
        )

    @Test
    fun `invisible and tombstoned calendars hide their items`() {
        val calendars = listOf(
            calendar("A", visible = true),
            calendar("B", visible = false),
            calendar("C", visible = true, deleted = true),
        )

        assertEquals(setOf("B", "C"), Visibility.hiddenIds(calendars))

        val entries = listOf(
            Fixtures.entry("1", calendarId = "A"),
            Fixtures.entry("2", calendarId = "B"),
            Fixtures.entry("3", calendarId = "C"),
        )
        assertEquals(listOf("1"), Visibility.filter(entries, Visibility.hiddenIds(calendars)).map { it.id })
    }

    @Test
    fun `sidebar lists only live visible calendars`() {
        val calendars = listOf(
            calendar("A", visible = true),
            calendar("B", visible = false),
            calendar("C", visible = true, deleted = true),
        )

        assertEquals(listOf("A"), Visibility.listedCalendars(calendars).map { it.id })
    }

    @Test
    fun `filtering keeps the payload shape`() {
        val hidden = setOf("B")

        val month = Visibility.filterMonth(
            listOf(listOf(MonthCell(LocalDate.of(2026, 8, 24), true, listOf(
                Fixtures.entry("1", calendarId = "A"),
                Fixtures.entry("2", calendarId = "B"),
            )))),
            hidden,
        )
        assertEquals(listOf("1"), month[0][0].items.map { it.id })
        assertTrue(month[0][0].inMonth)

        val week = Visibility.filterWeek(
            listOf(WeekDay(LocalDate.of(2026, 8, 24), listOf(
                Fixtures.entry("1", calendarId = "A"),
                Fixtures.entry("2", calendarId = "B"),
            ))),
            hidden,
        )
        assertEquals(listOf("1"), week[0].items.map { it.id })

        val day = Visibility.filterDay(
            DayPayload(
                date = LocalDate.of(2026, 8, 24),
                allDay = listOf(Fixtures.entry("1", calendarId = "A"), Fixtures.entry("2", calendarId = "B")),
                timed = listOf(
                    PositionedEntry(Fixtures.entry("3", calendarId = "A"), 0.1, 0.02, 0, 1),
                    PositionedEntry(Fixtures.entry("4", calendarId = "B"), 0.2, 0.02, 0, 1),
                ),
            ),
            hidden,
        )
        assertEquals(listOf("1"), day.allDay.map { it.id })
        assertEquals(listOf("3"), day.timed.map { it.item.id })

        val agenda = Visibility.filterAgenda(
            listOf(
                AgendaRow(LocalDate.of(2026, 8, 24), Fixtures.entry("1", calendarId = "A")),
                AgendaRow(LocalDate.of(2026, 8, 24), Fixtures.entry("2", calendarId = "B")),
            ),
            hidden,
        )
        assertEquals(listOf("1"), agenda.map { it.item.id })
    }

    @Test
    fun `patchedVisible round-trips the model with one field flipped`() {
        val raw = """{"id":"01CAL","name":"Personal","color":"#3366cc","source":"Local",""" +
            """"visible":true,"deleted":false,"updated_at":"2026-08-01T00:00:00+02:00"}"""
        val calendar = Payloads.parseCalendars("[$raw]")[0]

        val patched = JSONObject(calendar.patchedVisible(false))

        assertFalse(patched.getBoolean("visible"))
        assertEquals("01CAL", patched.getString("id"))
        assertEquals("Personal", patched.getString("name"))
        assertEquals("#3366cc", patched.getString("color"))
        assertEquals("Local", patched.getString("source"))
        assertFalse(patched.getBoolean("deleted"))
        assertEquals("2026-08-01T00:00:00+02:00", patched.getString("updated_at"))

        // Re-parsing the patched JSON gives the flipped model back.
        val reparsed = Payloads.parseCalendars("[${calendar.patchedVisible(false)}]")[0]
        assertFalse(reparsed.visible)
        assertTrue(calendar.visible)
    }
}
