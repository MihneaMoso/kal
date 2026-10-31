package com.kal.calendar.widgets

import java.time.LocalDate
import java.time.YearMonth
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * Pure month-widget decisions, kept out of [MonthWidgetProvider] so the JVM
 * tests can cover the navigation and grid rules without Android.
 */
internal object MonthWidgetModel {

    /**
     * The month a widget shows. A widget the user never navigated has no pin
     * and follows [today]; a navigated widget keeps its pinned month, so time
     * passing cannot drag the view somewhere it never visited.
     */
    fun monthFor(today: YearMonth, pinned: YearMonth?): YearMonth = pinned ?: today

    /**
     * The pin to store after moving one month from [shown]. Moving back to
     * [today] clears the pin (`null`), so the widget resumes following the
     * calendar instead of freezing on the current month.
     */
    fun pinAfter(shown: YearMonth, delta: Int, today: YearMonth): YearMonth? =
        shown.plusMonths(delta.toLong()).takeUnless { it == today }

    /** Stable prefs text for a pinned month, e.g. "2026-10". */
    fun monthKey(month: YearMonth): String = month.toString()

    /** Reads a pinned month; junk is treated as "not pinned". */
    fun parseMonthKey(raw: String?): YearMonth? =
        raw?.takeIf { it.isNotBlank() }
            ?.let { runCatching { YearMonth.parse(it) }.getOrNull() }

    /**
     * Converts the old per-widget relative offset to an absolute pin, or
     * `null` when that month is [today]'s (the widget follows the calendar
     * again). Used once when upgrading a widget that predates the pin.
     */
    fun pinFromLegacyOffset(offset: Int, today: YearMonth): YearMonth? =
        today.plusMonths(offset.toLong()).takeUnless { it == today }

    /** "October 2026" in the device locale. */
    fun title(month: YearMonth, locale: Locale): String =
        month.format(DateTimeFormatter.ofPattern("MMMM yyyy", locale))

    /**
     * A five-week month does not need its sixth (entirely out-of-month) row;
     * hiding it lets the five real rows fill the widget instead of leaving a
     * blank strip at the bottom.
     */
    fun showsWeek(index: Int, week: List<MonthDay>): Boolean =
        week.isNotEmpty() && (index < 5 || (index == 5 && week.any { it.inMonth }))

    /** Spoken day label for the cell's "Add event on …" content description. */
    fun dayDescription(date: LocalDate, locale: Locale): String =
        date.format(DateTimeFormatter.ofPattern("EEEE, MMMM d, yyyy", locale))

    /**
     * The month-grid chip text: title for all-day items, `time title` for timed
     * events, always one line and ellipsised by the TextView.
     */
    fun chipLabel(title: String, time: String): String {
        val cleanTitle = title.ifBlank { "Untitled" }
        return if (time.isBlank()) cleanTitle else "$time $cleanTitle"
    }

    /**
     * PendingIntent request codes need to differ per widget and per action.
     * Day cells take slots 10…51; the three header actions take 1…3.
     */
    fun requestCode(widgetId: Int, slot: Int): Int = widgetId * 100 + slot
}
