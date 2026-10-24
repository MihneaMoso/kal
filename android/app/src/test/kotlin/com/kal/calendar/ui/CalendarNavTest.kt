package com.kal.calendar.ui

import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Test

/** Nav-strip arithmetic and labels, including the deliberately preserved quirks. */
class CalendarNavTest {

    private val saturday = LocalDate.of(2026, 10, 17)

    @Test
    fun `the week always starts on monday`() {
        assertEquals(LocalDate.of(2026, 10, 12), CalendarNav.weekStart(saturday))
        // Sunday is the last day of its own Monday-anchored week, not the first.
        assertEquals(LocalDate.of(2026, 10, 12), CalendarNav.weekStart(LocalDate.of(2026, 10, 18)))
        assertEquals(LocalDate.of(2026, 10, 19), CalendarNav.weekStart(LocalDate.of(2026, 10, 19)))
    }

    @Test
    fun `day and week steps are whole days and weeks`() {
        assertEquals(LocalDate.of(2026, 10, 18), CalendarNav.step(ViewMode.DAY, saturday, forward = true))
        assertEquals(LocalDate.of(2026, 10, 16), CalendarNav.step(ViewMode.DAY, saturday, forward = false))
        assertEquals(LocalDate.of(2026, 10, 24), CalendarNav.step(ViewMode.WEEK, saturday, forward = true))
        assertEquals(LocalDate.of(2026, 10, 10), CalendarNav.step(ViewMode.WEEK, saturday, forward = false))
    }

    @Test
    fun `month and agenda steps move a month and clamp short months`() {
        assertEquals(LocalDate.of(2026, 11, 17), CalendarNav.step(ViewMode.MONTH, saturday, forward = true))
        assertEquals(LocalDate.of(2026, 9, 17), CalendarNav.step(ViewMode.MONTH, saturday, forward = false))
        // 31 January + 1 month clamps to the last valid day of February.
        assertEquals(
            LocalDate.of(2026, 2, 28),
            CalendarNav.step(ViewMode.MONTH, LocalDate.of(2026, 1, 31), forward = true),
        )
        assertEquals(
            LocalDate.of(2025, 12, 31),
            CalendarNav.step(ViewMode.MONTH, LocalDate.of(2026, 1, 31), forward = false),
        )
        // The agenda view shares the month step even though its title is static.
        assertEquals(LocalDate.of(2026, 11, 17), CalendarNav.step(ViewMode.AGENDA, saturday, forward = true))
        assertEquals(LocalDate.of(2026, 9, 17), CalendarNav.step(ViewMode.AGENDA, saturday, forward = false))
    }

    @Test
    fun `titles match the view mode`() {
        assertEquals("October 2026", CalendarNav.title(ViewMode.MONTH, saturday))
        assertEquals("Oct 12 \u2013 Oct 18, 2026", CalendarNav.title(ViewMode.WEEK, saturday))
        assertEquals("Saturday, October 17 2026", CalendarNav.title(ViewMode.DAY, saturday))
        assertEquals("Agenda \u2014 next 30 days", CalendarNav.title(ViewMode.AGENDA, saturday))
    }

    @Test
    fun `compact titles fit the phone top bar`() {
        assertEquals("Oct 2026", CalendarNav.title(ViewMode.MONTH, saturday, compact = true))
        assertEquals("Oct 12 \u2013 18", CalendarNav.title(ViewMode.WEEK, saturday, compact = true))
        assertEquals("Sat, Oct 17", CalendarNav.title(ViewMode.DAY, saturday, compact = true))
        assertEquals("Agenda", CalendarNav.title(ViewMode.AGENDA, saturday, compact = true))
        // A week that crosses a month boundary keeps both months visible.
        val lateOctober = LocalDate.of(2026, 10, 28)
        assertEquals("Oct 26 \u2013 Nov 1", CalendarNav.title(ViewMode.WEEK, lateOctober, compact = true))
    }

    @Test
    fun `first weekday maps to the bridge wire convention`() {
        assertEquals(0, CalendarNav.firstDow(firstDayMonday = true))
        assertEquals(6, CalendarNav.firstDow(firstDayMonday = false))
    }

    @Test
    fun `view modes are listed in nav order`() {
        assertEquals(
            listOf("Month", "Week", "Day", "Agenda"),
            ViewMode.entries.map { it.label },
        )
    }
}
