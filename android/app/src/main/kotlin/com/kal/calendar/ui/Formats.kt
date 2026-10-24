package com.kal.calendar.ui

import com.kal.calendar.bridge.DisplayEntry
import com.kal.calendar.bridge.ItemKind
import java.time.LocalDate
import java.time.LocalTime
import java.time.OffsetDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * Every date/time string the shell and the four views show, ported 1:1 from the
 * Dioxus implementation (chrono patterns, English names).
 *
 * `chrono`'s `%A`/`%B` are not locale-aware, so the formatters are pinned to
 * [Locale.ENGLISH] here to keep the copy identical when the device is set to
 * another language — parity first; localisation is its own later step.
 */
object Formats {

    /** `%A, %B %d %Y` — e.g. `Saturday, October 17 2026` (top bar + day title). */
    private val LONG_DATE = DateTimeFormatter.ofPattern("EEEE, MMMM d yyyy", Locale.ENGLISH)

    /** `%B %Y` — e.g. `October 2026`. */
    private val MONTH_TITLE = DateTimeFormatter.ofPattern("MMMM yyyy", Locale.ENGLISH)

    /** `%b %Y` — e.g. `Oct 2026` (phone-width top bar). */
    private val MONTH_TITLE_SHORT = DateTimeFormatter.ofPattern("MMM yyyy", Locale.ENGLISH)

    /** `%b %d` — e.g. `Oct 12` (week-range endpoints). */
    private val SHORT_DAY = DateTimeFormatter.ofPattern("MMM d", Locale.ENGLISH)

    /** `%a, %b %d` — e.g. `Sat, Oct 17` (phone-width day title). */
    private val DAY_TITLE_SHORT = DateTimeFormatter.ofPattern("EEE, MMM d", Locale.ENGLISH)

    /** `%b %d, %Y` — e.g. `Oct 18, 2026` (week-range end). */
    private val SHORT_DAY_YEAR = DateTimeFormatter.ofPattern("MMM d, yyyy", Locale.ENGLISH)

    /** `%a %b %d` — e.g. `Mon Aug 24` (week headings, agenda date column). */
    private val DAY_LABEL = DateTimeFormatter.ofPattern("EEE MMM d", Locale.ENGLISH)

    /** `%a` — e.g. `Mon` (month-grid weekday headers). */
    private val WEEKDAY = DateTimeFormatter.ofPattern("EEE", Locale.ENGLISH)

    /** `%H:%M`. */
    private val TIME_24 = DateTimeFormatter.ofPattern("HH:mm", Locale.ENGLISH)

    /** RFC 3339 with a numeric offset — never `Z`, matching serde's output. */
    private val WIRE_INSTANT =
        DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ssxxx", Locale.ENGLISH)

    /** `%l:%M %p`, trimmed — e.g. `9:05 AM`. */
    private val TIME_12 = DateTimeFormatter.ofPattern("h:mm a", Locale.ENGLISH)

    /** The agenda view's fixed heading (`%B`-free, hardcoded in the original). */
    const val AGENDA_TITLE = "Agenda — next 30 days"

    /** Week/agenda label for an all-day item. */
    const val ALL_DAY = "all-day"

    /** Day view's all-day strip heading. */
    const val ALL_DAY_STRIP = "All-day"

    /** Agenda empty state. */
    const val AGAIN_EMPTY = "Nothing in the next 30 days."

    /** Week view's placeholder for a day with no items (`—`, em dash). */
    const val EMPTY_DAY = "—"

    /** Birthday suffix separator (` · `, middle dot). */
    private const val AGE_SUFFIX = " · "

    /** Range separator in the item rows (` – `, en dash). */
    private const val RANGE = " – "

    /** `%H:%M` / `%l:%M %p` per the user's 12/24-hour preference. */
    fun time(at: OffsetDateTime, time24h: Boolean): String =
        (if (time24h) TIME_24 else TIME_12).format(at)

    /** Formatted from a time-of-day (the day view's hour gutter). */
    fun time(at: LocalTime, time24h: Boolean): String =
        (if (time24h) TIME_24 else TIME_12).format(at)

    /** The top bar subtitle / day title: `Saturday, October 17 2026`. */
    fun longDate(date: LocalDate): String = LONG_DATE.format(date)

    /** Month view period label: `October 2026`. */
    fun monthTitle(date: LocalDate): String = MONTH_TITLE.format(date)

    /** Week view period label: `Oct 12 – Oct 18, 2026` (from the Monday). */
    fun weekTitle(monday: LocalDate): String =
        "${SHORT_DAY.format(monday)}$RANGE${SHORT_DAY_YEAR.format(monday.plusDays(6))}"

    /** Day view period label. */
    fun dayTitle(date: LocalDate): String = LONG_DATE.format(date)

    /** `Oct 2026` — the phone-width top bar's month label. */
    fun shortMonthTitle(date: LocalDate): String = MONTH_TITLE_SHORT.format(date)

    /** `Oct 12` — the phone-width top bar's week range endpoints. */
    fun shortDay(date: LocalDate): String = SHORT_DAY.format(date)

    /** `Sat, Oct 17` — the phone-width top bar's day label. */
    fun shortDayTitle(date: LocalDate): String = DAY_TITLE_SHORT.format(date)

    /** `Mon Aug 24` — week day headings and the agenda date column. */
    fun dayLabel(date: LocalDate): String = DAY_LABEL.format(date)

    /**
     * Month-grid weekday headers in grid column order. Monday-first is the
     * default; the CSS uppercases them, so callers render them upper-cased.
     */
    fun weekdayHeaders(firstDayMonday: Boolean): List<String> {
        // 2024-01-01 is a Monday; Sunday-first starts one day earlier.
        val monday = LocalDate.of(2024, 1, 1)
        val start = if (firstDayMonday) monday else monday.minusDays(1)
        return (0L..6L).map { WEEKDAY.format(start.plusDays(it)) }
    }

    /**
     * The month-grid chip text: `title` for all-day items and birthdays,
     * `time title` otherwise; birthdays gain ` · age`.
     */
    fun chipLabel(entry: DisplayEntry, time24h: Boolean): String {
        val base = if (entry.allDay || entry.kind == ItemKind.BIRTHDAY) {
            entry.title
        } else {
            "${time(entry.start, time24h)} ${entry.title}"
        }
        return if (entry.kind == ItemKind.BIRTHDAY && entry.age != null) {
            base + AGE_SUFFIX + entry.age
        } else {
            base
        }
    }

    /** The week/agenda row's when-column: `all-day` or `start – end`. */
    fun whenLabel(entry: DisplayEntry, time24h: Boolean): String {
        if (entry.allDay) return ALL_DAY
        val start = time(entry.start, time24h)
        val end = entry.end?.let { time(it, time24h) } ?: ""
        return start + RANGE + end
    }

    /** The item-row title, with ` · age` appended for birthdays. */
    fun rowTitle(entry: DisplayEntry): String =
        if (entry.kind == ItemKind.BIRTHDAY && entry.age != null) {
            entry.title + AGE_SUFFIX + entry.age
        } else {
            entry.title
        }

    /**
     * Day-gutter hour label: `09:00` in 24-hour mode, `9:00 AM` otherwise
     * (`h12 = 12` at both midnight and noon, as the original did).
     */
    fun hourLabel(hour: Int, time24h: Boolean): String {
        if (time24h) return String.format(Locale.ENGLISH, "%02d:00", hour)
        val suffix = if (hour < 12) "AM" else "PM"
        val h12 = if (hour % 12 == 0) 12 else hour % 12
        return "$h12:00 $suffix"
    }

    /**
     * The day block's own time label — always `HH:MM`, ignoring the 12/24-hour
     * preference, exactly like `TimedEventBlock` did.
     */
    fun blockTime(start: OffsetDateTime): String = TIME_24.format(start)

    /** `Oct 18, 2026` — the editor's date button. */
    fun editedDate(date: LocalDate): String = SHORT_DAY_YEAR.format(date)

    /**
     * `HH:MM` — the form wire time (`%H:%M` to the native parser), independent of
     * the 12/24-hour display preference.
     */
    fun wireTime(time: LocalTime): String = TIME_24.format(time)

    /**
     * RFC 3339 with an explicit numeric offset (`2026-08-17T09:00:00+03:00`) —
     * the shape both the core's serde output and the native form parser use.
     */
    fun wireInstant(at: OffsetDateTime): String = WIRE_INSTANT.format(at)
}
