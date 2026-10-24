package com.kal.calendar.ui

import java.time.LocalDate

/** The four calendar views, in the order the top bar's tabs show them. */
enum class ViewMode(val label: String) {
    MONTH("Month"),
    WEEK("Week"),
    DAY("Day"),
    AGENDA("Agenda"),
}

/**
 * Period arithmetic and labels for the top bar and the swipe gestures, ported
 * from the Dioxus shell:
 *
 *  * Day steps one day, Week steps seven, Month and Agenda step one month;
 *  * a month step that would leave the supported range is a no-op
 *    (`checked_add_months`/`checked_sub_months`);
 *  * the week view is **always** Monday-anchored, even for a Sunday-first user
 *    (only the month grid honours the preference) — a deliberately preserved
 *    quirk.
 */
object CalendarNav {

    /** The Monday that anchors [cursor]'s week. */
    fun weekStart(cursor: LocalDate): LocalDate =
        cursor.minusDays((cursor.dayOfWeek.value - 1).toLong())

    /** One swipe step: `forward` false steps back a period. */
    fun step(view: ViewMode, cursor: LocalDate, forward: Boolean): LocalDate = when (view) {
        ViewMode.DAY -> cursor.plusDays(if (forward) 1 else -1)
        ViewMode.WEEK -> cursor.plusDays(if (forward) 7 else -7)
        ViewMode.MONTH, ViewMode.AGENDA ->
            runCatching { if (forward) cursor.plusMonths(1) else cursor.minusMonths(1) }
                .getOrDefault(cursor)
    }

    /** The top bar's period label for the current view and cursor. */
    fun title(view: ViewMode, cursor: LocalDate, compact: Boolean = false): String =
        if (compact) shortTitle(view, cursor) else when (view) {
            ViewMode.MONTH -> Formats.monthTitle(cursor)
            ViewMode.WEEK -> Formats.weekTitle(weekStart(cursor))
            ViewMode.DAY -> Formats.dayTitle(cursor)
            ViewMode.AGENDA -> Formats.AGENDA_TITLE
        }

    /**
     * The top bar's phone-width label. Same periods, short enough to sit between
     * the title and the view tabs: `Oct 2026`, `Oct 12 – 18`, `Tue, Oct 21`.
     */
    fun shortTitle(view: ViewMode, cursor: LocalDate): String = when (view) {
        ViewMode.MONTH -> Formats.shortMonthTitle(cursor)
        ViewMode.WEEK -> {
            val monday = weekStart(cursor)
            val sunday = monday.plusDays(6)
            // Within one month the second month name is redundant; across a
            // boundary keep both so "Oct 26 – Nov 1" is unambiguous.
            val end = if (monday.month == sunday.month) {
                sunday.dayOfMonth.toString()
            } else {
                Formats.shortDay(sunday)
            }
            "${Formats.shortDay(monday)} \u2013 $end"
        }
        ViewMode.DAY -> Formats.shortDayTitle(cursor)
        ViewMode.AGENDA -> "Agenda"
    }

    /** The month grid's first weekday in the bridge's wire convention (0 = Mon … 6 = Sun). */
    fun firstDow(firstDayMonday: Boolean): Int = if (firstDayMonday) 0 else 6
}
