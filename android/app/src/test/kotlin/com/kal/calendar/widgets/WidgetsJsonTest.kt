package com.kal.calendar.widgets

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The two home-screen widget snapshots: native JSON → RemoteViews rows. */
class WidgetsJsonTest {

    @Test
    fun scheduleSnapshotMapsTitleTimeKindAndColor() {
        val body = """
            [
              {"title":"Standup","start":"2026-08-31T14:00:00+02:00","kind":"event","color":"#3366cc"},
              {"title":"Taxes","start":"2026-09-01T00:00:00+02:00","kind":"task","color":"#cc3300"}
            ]
        """.trimIndent()

        val items = parseSchedule(body)

        assertEquals(2, items.size)
        assertEquals("Standup", items[0].title)
        assertEquals("14:00", items[0].startLabel)
        assertEquals("event", items[0].kind)
        assertEquals("#3366cc", items[0].color)
        assertEquals("00:00", items[1].startLabel)
    }

    @Test
    fun scheduleSnapshotDefaultsMissingFields() {
        val items = parseSchedule("""[{"start":""}]""")

        assertEquals(1, items.size)
        assertEquals("Untitled", items[0].title)
        assertEquals("", items[0].startLabel)
        assertEquals("event", items[0].kind)
        assertEquals("#3366cc", items[0].color)
    }

    @Test
    fun emptyAndBrokenScheduleSnapshotsShowNothing() {
        assertEquals(emptyList<ScheduleItem>(), parseSchedule(null))
        assertEquals(emptyList<ScheduleItem>(), parseSchedule(""))
        assertEquals(emptyList<ScheduleItem>(), parseSchedule("not json"))
        assertEquals(emptyList<ScheduleItem>(), parseSchedule("""{"not":"an array"}"""))
    }

    @Test
    fun monthSnapshotMapsWeeksDaysAndDots() {
        val body = """
            [
              [
                {"date":"2026-08-31","inMonth":true,"items":[{"title":"Standup","color":"#3366cc"}]},
                {"date":"2026-09-01","inMonth":false,"items":[]}
              ]
            ]
        """.trimIndent()

        val weeks = parseMonth(body)

        assertEquals(1, weeks.size)
        assertEquals(2, weeks[0].size)
        assertEquals("2026-08-31", weeks[0][0].date)
        assertTrue(weeks[0][0].inMonth)
        assertEquals(1, weeks[0][0].items.size)
        assertEquals("Standup", weeks[0][0].items[0].title)
        assertEquals("#3366cc", weeks[0][0].items[0].color)
        assertFalse(weeks[0][1].inMonth)
        assertTrue(weeks[0][1].items.isEmpty())
    }

    @Test
    fun emptyAndBrokenMonthSnapshotsShowNothing() {
        assertEquals(emptyList<List<MonthDay>>(), parseMonth(null))
        assertEquals(emptyList<List<MonthDay>>(), parseMonth(""))
        assertEquals(emptyList<List<MonthDay>>(), parseMonth("not json"))
    }
}
