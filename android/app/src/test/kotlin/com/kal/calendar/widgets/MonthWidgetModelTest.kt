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
    fun widgetFollowsTodayUntilTheUserNavigates() {
        val october = YearMonth.of(2026, 10)
        assertEquals(october, MonthWidgetModel.monthFor(october, null))
        assertEquals(october, MonthWidgetModel.monthFor(october, october))
    }

    @Test
    fun aPinnedMonthDoesNotDriftWhenTheCalendarRollsOver() {
        // The old offset was relative to `today`, so a widget navigated to
        // November 2026 silently became December 2026 once the clock moved on.
        val pinned = YearMonth.of(2026, 11)
        assertEquals(pinned, MonthWidgetModel.monthFor(YearMonth.of(2026, 10), pinned))
        assertEquals(pinned, MonthWidgetModel.monthFor(YearMonth.of(2026, 12), pinned))
        assertEquals(pinned, MonthWidgetModel.monthFor(YearMonth.of(2027, 6), pinned))
    }

    @Test
    fun navigationCrossesYearBoundariesAndClearsThePinAtToday() {
        val october = YearMonth.of(2026, 10)
        assertEquals(
            YearMonth.of(2027, 1),
            MonthWidgetModel.pinAfter(YearMonth.of(2026, 12), 1, october),
        )
        assertEquals(
            YearMonth.of(2025, 12),
            MonthWidgetModel.pinAfter(YearMonth.of(2026, 1), -1, october),
        )
        assertEquals(
            YearMonth.of(2026, 11),
            MonthWidgetModel.pinAfter(october, 1, october),
        )
        // Stepping back onto the current month resumes calendar tracking.
        assertEquals(null, MonthWidgetModel.pinAfter(YearMonth.of(2026, 11), -1, october))
        assertEquals(null, MonthWidgetModel.pinAfter(YearMonth.of(2024, 10), 24, october))
    }

    @Test
    fun pinnedMonthsRoundTripThroughTheirPrefsKey() {
        val month = YearMonth.of(2026, 10)
        assertEquals("2026-10", MonthWidgetModel.monthKey(month))
        assertEquals(month, MonthWidgetModel.parseMonthKey("2026-10"))
        assertEquals(YearMonth.of(2027, 1), MonthWidgetModel.parseMonthKey("2027-01"))
        // Junk or an absent key means "not pinned", never a crash.
        assertEquals(null, MonthWidgetModel.parseMonthKey(null))
        assertEquals(null, MonthWidgetModel.parseMonthKey(""))
        assertEquals(null, MonthWidgetModel.parseMonthKey("not a month"))
    }

    @Test
    fun legacyRelativeOffsetMigratesToAnAbsolutePin() {
        val october = YearMonth.of(2026, 10)
        assertEquals(
            YearMonth.of(2026, 11),
            MonthWidgetModel.pinFromLegacyOffset(1, october),
        )
        assertEquals(
            YearMonth.of(2025, 12),
            MonthWidgetModel.pinFromLegacyOffset(-10, october),
        )
        // Offset 0 is the widget's untracked state, so it becomes "no pin".
        assertEquals(null, MonthWidgetModel.pinFromLegacyOffset(0, october))
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
