package com.kal.calendar.bridge

import org.json.JSONObject

/** Display entries shaped exactly the way the native payloads send them. */
internal object Fixtures {

    fun entry(
        id: String,
        calendarId: String = "01CAL",
        title: String = id,
        start: String = "2026-08-24T09:00:00+02:00",
        end: String? = "2026-08-24T09:30:00+02:00",
        allDay: Boolean = false,
        kind: String = "event",
        completed: Boolean = false,
        color: String = "#3366cc",
        overrideColor: String? = null,
        age: Int? = null,
    ): DisplayEntry = Payloads.entry(
        JSONObject()
            .put("id", id)
            .put("calendarId", calendarId)
            .put("title", title)
            .put("start", start)
            .put("end", end ?: JSONObject.NULL)
            .put("allDay", allDay)
            .put("kind", kind)
            .put("completed", completed)
            .put("color", color)
            .put("overrideColor", overrideColor ?: JSONObject.NULL)
            .put("age", age ?: JSONObject.NULL),
    )
}
