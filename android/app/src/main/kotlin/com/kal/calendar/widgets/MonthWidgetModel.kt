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

    /** The month a widget shows: today's month plus its stored offset. */
    fun monthFor(base: YearMonth, offset: Int): YearMonth =
        base.plusMonths(offset.toLong())

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
