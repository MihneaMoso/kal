package com.kal.calendar.widgets

import org.json.JSONArray
import org.json.JSONObject

/** Snapshot of one upcoming item (from nativeSchedule). */
data class ScheduleItem(
    val title: String,
    val startLabel: String,
    val kind: String,
    val color: String,
)

// RFC3339 timestamps come in as local ISO-8601 with an offset (e.g.
// "2026-08-31T14:00:00+02:00"). We only show the wall-clock time.
private fun timeLabel(isodate: String?): String {
    if (isodate.isNullOrBlank()) return ""
    val t = isodate.substringAfter('T', "")
    if (t.isEmpty()) return ""
    // Take HH:MM from the time portion.
    return t.substring(0, minOf(5, t.length))
}

/** Parses the native schedule JSON array. */
fun parseSchedule(body: String?): List<ScheduleItem> {
    if (body.isNullOrBlank()) return emptyList()
    return runCatching {
        val arr = JSONArray(body)
        (0 until arr.length()).map { i ->
            val o: JSONObject = arr.getJSONObject(i)
            ScheduleItem(
                title = o.optString("title", "Untitled"),
                startLabel = timeLabel(o.optString("start", "")),
                kind = o.optString("kind", "event"),
                color = o.optString("color", "#3366cc"),
            )
        }
    }.getOrDefault(emptyList())
}

/** Snapshot of one month-grid day (from nativeMonth). */
data class MonthDay(
    val date: String,
    val inMonth: Boolean,
    val items: List<DayItem>,
)

data class DayItem(val title: String, val color: String)

/**
 * Parses the native month grid JSON. The native side returns a bare array of
 * weeks (each week an array of 7 days).
 */
fun parseMonth(body: String?): List<List<MonthDay>> {
    if (body.isNullOrBlank()) return emptyList()
    return runCatching {
        val weeks = JSONArray(body)
        (0 until weeks.length()).map { w ->
            val week = weeks.getJSONArray(w)
            (0 until week.length()).map { d ->
                val day = week.getJSONObject(d)
                val itemsArr = day.optJSONArray("items") ?: JSONArray()
                val items = (0 until itemsArr.length()).map { i ->
                    val it = itemsArr.getJSONObject(i)
                    DayItem(
                        title = it.optString("title", ""),
                        color = it.optString("color", "#3366cc"),
                    )
                }
                MonthDay(
                    date = day.optString("date", ""),
                    inMonth = day.optBoolean("inMonth", true),
                    items = items,
                )
            }
        }
    }.getOrDefault(emptyList())
}
