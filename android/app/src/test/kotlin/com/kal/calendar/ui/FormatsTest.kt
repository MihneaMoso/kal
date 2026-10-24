package com.kal.calendar.ui

import com.kal.calendar.bridge.Fixtures
import java.time.LocalDate
import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The ported copy and date/time formats. These strings are what the views show,
 * so each one is pinned against the chrono pattern it replaced.
 */
class FormatsTest {

    private fun at(text: String) = OffsetDateTime.parse(text)

    @Test
    fun `time follows the 12 or 24 hour preference`() {
        val nineOhFive = at("2026-08-24T09:05:00+02:00")
        assertEquals("09:05", Formats.time(nineOhFive, time24h = true))
        assertEquals("9:05 AM", Formats.time(nineOhFive, time24h = false))

        assertEquals("12:05 AM", Formats.time(at("2026-08-24T00:05:00+02:00"), time24h = false))
        assertEquals("12:05 PM", Formats.time(at("2026-08-24T12:05:00+02:00"), time24h = false))
        assertEquals("11:05 PM", Formats.time(at("2026-08-24T23:05:00+02:00"), time24h = false))
    }

    @Test
    fun `block time ignores the preference and stays 24 hour`() {
        assertEquals("09:05", Formats.blockTime(at("2026-08-24T09:05:00+02:00")))
        assertEquals("23:59", Formats.blockTime(at("2026-08-24T23:59:00+02:00")))
    }

    @Test
    fun `chip labels cover timed all-day and birthday entries`() {
        assertEquals("09:00 Standup", Formats.chipLabel(Fixtures.entry("1", title = "Standup"), time24h = true))
        assertEquals(
            "Holiday",
            Formats.chipLabel(
                Fixtures.entry("1", title = "Holiday", start = "2026-08-24T00:00:00+02:00", end = null, allDay = true),
                time24h = true,
            ),
        )
        // Birthdays show the title (never the time) plus the age suffix.
        assertEquals(
            "Ada \u00b7 36",
            Formats.chipLabel(
                Fixtures.entry(
                    "1",
                    title = "Ada",
                    start = "1990-08-24T00:00:00+02:00",
                    end = null,
                    allDay = true,
                    kind = "birthday",
                    age = 36,
                ),
                time24h = true,
            ),
        )
        assertEquals(
            "Ada",
            Formats.chipLabel(
                Fixtures.entry("1", title = "Ada", kind = "birthday", age = null),
                time24h = true,
            ),
        )
        // A 12-hour user sees the 12-hour chip time.
        assertEquals("9:00 AM Standup", Formats.chipLabel(Fixtures.entry("1", title = "Standup"), time24h = false))
    }

    @Test
    fun `when labels cover all-day ranges and a missing end`() {
        val timed = Fixtures.entry("1")
        assertEquals("09:00 \u2013 09:30", Formats.whenLabel(timed, time24h = true))

        val allDay = Fixtures.entry("1", allDay = true, end = null)
        assertEquals("all-day", Formats.whenLabel(allDay, time24h = true))

        // The original rendered `end` as an empty string when absent.
        val openEnded = Fixtures.entry("1", end = null)
        assertEquals("09:00 \u2013 ", Formats.whenLabel(openEnded, time24h = true))
    }

    @Test
    fun `row titles carry the birthday age suffix`() {
        assertEquals("Standup", Formats.rowTitle(Fixtures.entry("1", title = "Standup")))
        assertEquals(
            "Ada \u00b7 36",
            Formats.rowTitle(Fixtures.entry("1", title = "Ada", kind = "birthday", age = 36)),
        )
    }

    @Test
    fun `hour labels match the gutter`() {
        assertEquals("00:00", Formats.hourLabel(0, time24h = true))
        assertEquals("23:00", Formats.hourLabel(23, time24h = true))

        assertEquals("12:00 AM", Formats.hourLabel(0, time24h = false))
        assertEquals("11:00 AM", Formats.hourLabel(11, time24h = false))
        assertEquals("12:00 PM", Formats.hourLabel(12, time24h = false))
        assertEquals("1:00 PM", Formats.hourLabel(13, time24h = false))
        assertEquals("11:00 PM", Formats.hourLabel(23, time24h = false))
    }

    @Test
    fun `weekday headers follow the first-day preference`() {
        assertEquals(
            listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"),
            Formats.weekdayHeaders(firstDayMonday = true),
        )
        assertEquals(
            listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"),
            Formats.weekdayHeaders(firstDayMonday = false),
        )
    }

    @Test
    fun `period and item labels match the chrono patterns`() {
        val saturday = LocalDate.of(2026, 10, 17)
        assertEquals("Saturday, October 17 2026", Formats.longDate(saturday))
        assertEquals("Saturday, October 17 2026", Formats.dayTitle(saturday))
        assertEquals("October 2026", Formats.monthTitle(saturday))
        assertEquals("Oct 12 \u2013 Oct 18, 2026", Formats.weekTitle(LocalDate.of(2026, 10, 12)))
        assertEquals("Mon Aug 24", Formats.dayLabel(LocalDate.of(2026, 8, 24)))
        assertEquals("Agenda \u2014 next 30 days", Formats.AGENDA_TITLE)
        assertEquals("Nothing in the next 30 days.", Formats.AGAIN_EMPTY)
        assertEquals("\u2014", Formats.EMPTY_DAY)
        assertEquals("All-day", Formats.ALL_DAY_STRIP)
    }
}
