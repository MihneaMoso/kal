package com.kal.calendar.widgets

import java.time.LocalDate
import java.time.YearMonth
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure navigation/grid rules behind the month widget. */
class MonthWidgetModelTest {

    private fun week(inMonth: Boolean): List<MonthDay> =
        (1..7).map { MonthDay("2026-10-0$it", inMonth, emptyList()) }

    @Test
    fun monthOffsetsCrossYearBoundaries() {
        assertEquals(YearMonth.of(2027, 1), MonthWidgetModel.monthFor(YearMonth.of(2026, 12), 1))
        assertEquals(YearMonth.of(2025, 12), MonthWidgetModel.monthFor(YearMonth.of(2026, 1), -1))
        assertEquals(YearMonth.of(2026, 10), MonthWidgetModel.monthFor(YearMonth.of(2026, 10), 0))
    }

    @Test
    fun titleUsesMonthAndYear() {
        assertEquals(
            "October 2026",
            MonthWidgetModel.title(YearMonth.of(2026, 10), Locale.US),
        )
    }

    @Test
    fun sixthWeekIsHiddenOnlyWhenItHasNoInMonthDay() {
        assertTrue(MonthWidgetModel.showsWeek(4, week(inMonth = false)))
        assertFalse(MonthWidgetModel.showsWeek(5, week(inMonth = false)))
        assertTrue(MonthWidgetModel.showsWeek(5, week(inMonth = true)))
        assertFalse(MonthWidgetModel.showsWeek(5, emptyList()))
    }

    @Test
    fun requestCodesAreDistinctPerWidgetAndSlot() {
        assertNotEquals(
            MonthWidgetModel.requestCode(1, 10),
            MonthWidgetModel.requestCode(1, 11),
        )
        assertNotEquals(
            MonthWidgetModel.requestCode(1, 10),
            MonthWidgetModel.requestCode(2, 10),
        )
        assertNotEquals(
            MonthWidgetModel.requestCode(1, 1),
            MonthWidgetModel.requestCode(1, 3),
        )
    }

    @Test
    fun dayDescriptionIsSpokenFriendly() {
        assertEquals(
            "Friday, October 30, 2026",
            MonthWidgetModel.dayDescription(LocalDate.of(2026, 10, 30), Locale.US),
        )
    }

    @Test
    fun chipLabelIsSingleLineWithTimeForTimedItems() {
        assertEquals("Standup", MonthWidgetModel.chipLabel("Standup", ""))
        assertEquals("10:00 Standup", MonthWidgetModel.chipLabel("Standup", "10:00"))
        assertEquals("Untitled", MonthWidgetModel.chipLabel("", ""))
    }

}
