package com.kal.calendar.ui

import com.kal.calendar.bridge.EditScope
import com.kal.calendar.bridge.ItemKind
import com.kal.calendar.bridge.ItemModel
import java.time.LocalDate
import java.time.LocalTime
import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the editor's form logic against the Dioxus `EditorState`: the defaults per
 * kind, the prefill anchor, and the request the form turns into. All of it is
 * plain data, so none of this needs a device.
 */
class EditorFieldsTest {

    private val monday = LocalDate.of(2026, 8, 24)
    private val calendar = "01CAL"

    /** A weekly series item as the prefill query returns it. */
    private fun series(
        rrule: String? = "FREQ=WEEKLY",
        end: OffsetDateTime? = OffsetDateTime.parse("2026-08-24T10:30:00+02:00"),
        colorOverride: String? = null,
    ) = ItemModel(
        id = "01ITEM",
        kind = ItemKind.EVENT,
        title = "Standup",
        location = "Room 2",
        calendarId = calendar,
        start = OffsetDateTime.parse("2026-08-24T09:00:00+02:00"),
        end = end,
        allDay = false,
        rrule = rrule,
        completed = false,
        reminderMinutes = listOf(10L),
        colorOverride = colorOverride,
        person = null,
    )

    @Test
    fun `a new event runs 09 to 10 with an explicit colour override`() {
        val fields = EditorFields.forNew(ItemKind.EVENT, monday, calendar)

        assertNull(fields.id)
        assertNull(fields.occurrenceStart)
        assertEquals("", fields.title)
        assertEquals(monday, fields.date)
        assertEquals(LocalTime.of(9, 0), fields.startTime)
        assertEquals(LocalTime.of(10, 0), fields.endTime)
        assertFalse(fields.allDay)
        assertEquals(calendar, fields.calendarId)
        assertEquals(RepeatPreset.NONE, fields.repeat)
        assertTrue(fields.reminders.isEmpty())
        // A new item starts with the app's blue *override*, not "Calendar" —
        // the Dioxus editor did the same.
        assertEquals("#3366cc", fields.color)
        assertFalse(fields.recurring)
        assertEquals("New event", fields.heading)
        assertFalse(fields.offersScopes)
    }

    @Test
    fun `a new task starts at 09 with no end`() {
        val fields = EditorFields.forNew(ItemKind.TASK, monday, calendar)

        assertEquals(LocalTime.of(9, 0), fields.startTime)
        assertNull(fields.endTime)
        assertFalse(fields.allDay)
        assertEquals(RepeatPreset.NONE, fields.repeat)
        assertEquals("New task", fields.heading)
    }

    @Test
    fun `a new birthday is an all-day yearly entry at midnight`() {
        val fields = EditorFields.forNew(ItemKind.BIRTHDAY, monday, calendar)

        assertEquals(LocalTime.MIDNIGHT, fields.startTime)
        assertNull(fields.endTime)
        assertTrue(fields.allDay)
        assertEquals(RepeatPreset.YEARLY, fields.repeat)
        assertEquals("New birthday", fields.heading)
    }

    @Test
    fun `prefill anchors on the clicked occurrence and keeps the base duration`() {
        val occurrence = OffsetDateTime.parse("2026-09-07T09:00:00+02:00")
        val fields = EditorFields.fromItem(series(), occurrence)

        assertEquals("01ITEM", fields.id)
        assertEquals(occurrence, fields.occurrenceStart)
        assertEquals(LocalDate.of(2026, 9, 7), fields.date)
        assertEquals(LocalTime.of(9, 0), fields.startTime)
        assertEquals(LocalTime.of(10, 30), fields.endTime)
        assertEquals("Standup", fields.title)
        assertEquals("Room 2", fields.location)
        assertEquals(calendar, fields.calendarId)
        assertEquals(listOf(10L), fields.reminders)
        assertEquals(RepeatPreset.WEEKLY, fields.repeat)
        assertTrue(fields.recurring)
        assertEquals("Edit event", fields.heading)
        assertTrue(fields.offersScopes)
    }

    @Test
    fun `prefill without a clicked occurrence uses the item's own start`() {
        val fields = EditorFields.fromItem(series(), occurrenceStart = null)

        assertNull(fields.occurrenceStart)
        assertEquals(monday, fields.date)
        assertEquals(LocalTime.of(9, 0), fields.startTime)
        assertEquals(LocalTime.of(10, 30), fields.endTime)
    }

    @Test
    fun `a non-recurring item offers no scope choice`() {
        val fields = EditorFields.fromItem(series(rrule = null), null)

        assertEquals(RepeatPreset.NONE, fields.repeat)
        assertFalse(fields.recurring)
        assertFalse(fields.offersScopes)
        assertFalse(fields.exception)
    }

    @Test
    fun `an individually edited occurrence keeps its series rule and the scope choice`() {
        // "This event" leaves a single row that still belongs to the series
        // (`metadata.series_id`); the shell loads the series' rule for the form.
        val copy = series(rrule = null, colorOverride = "#27ae60").copy(
            id = "01COPY",
            title = "Standup (this one)",
            seriesId = "01ITEM",
        )

        val fields = EditorFields.fromItem(copy, copy.start, seriesRrule = "FREQ=WEEKLY")

        assertEquals("01COPY", fields.id)
        assertEquals("the dialog shows the series' rule", RepeatPreset.WEEKLY, fields.repeat)
        assertEquals("its own colour is what the dialog shows", "#27ae60", fields.color)
        assertTrue(fields.recurring)
        assertTrue(fields.offersScopes)
        assertTrue("no later part of the series to split from it", fields.exception)
    }

    @Test
    fun `an occurrence whose series is gone is a plain event again`() {
        val orphan = series(rrule = null).copy(id = "01COPY", seriesId = "01GONE")

        val fields = EditorFields.fromItem(orphan, orphan.start, seriesRrule = null)

        assertEquals(RepeatPreset.NONE, fields.repeat)
        assertFalse(fields.recurring)
        assertFalse(fields.offersScopes)
        assertTrue("the stored row still names the dead series", fields.exception)
    }

    @Test
    fun `an end-less item prefills with no end`() {
        val fields = EditorFields.fromItem(series(end = null), null)

        assertNull(fields.endTime)
    }

    @Test
    fun `the form request maps the fields onto the wire`() {
        val occurrence = OffsetDateTime.parse("2026-09-07T09:00:00+02:00")
        val fields = EditorFields.fromItem(series(), occurrence).copy(
            title = "Moved standup",
            reminders = listOf(30L, 1440L),
            customReminder = "45",
        )

        val form = fields.toForm(EditScope.THIS_AND_FOLLOWING)

        assertEquals("01ITEM", form.id)
        assertEquals("2026-09-07T09:00:00+02:00", form.occurrenceStart)
        assertEquals(ItemKind.EVENT, form.kind)
        assertEquals("Moved standup", form.title)
        assertEquals("2026-09-07", form.date)
        assertEquals("09:00", form.startTime)
        assertEquals("10:30", form.endTime)
        assertFalse(form.allDay)
        assertEquals(calendar, form.calendarId)
        assertEquals("Room 2", form.location)
        assertEquals("FREQ=WEEKLY", form.rrule)
        assertEquals(listOf(30L, 1440L), form.reminderMinutes)
        assertEquals("45", form.customReminder)
        assertNull(form.color)
        assertEquals(EditScope.THIS_AND_FOLLOWING, form.scope)
    }

    @Test
    fun `a blank new form sends empty times and no scope anchor`() {
        val form = EditorFields.forNew(ItemKind.TASK, monday, calendar).toForm(EditScope.ALL)

        assertNull(form.id)
        assertNull(form.occurrenceStart)
        assertNull(form.rrule)
        assertEquals("", form.endTime)
        assertEquals(EditScope.ALL, form.scope)
        assertEquals("#3366cc", form.color)
    }

    @Test
    fun `an all-day item sends its midnight start and no end`() {
        val form = EditorFields.forNew(ItemKind.BIRTHDAY, monday, calendar).toForm(EditScope.ALL)

        assertEquals("00:00", form.startTime)
        assertEquals("", form.endTime)
        assertTrue(form.allDay)
    }

    @Test
    fun `the repeat presets map both ways`() {
        assertNull(RepeatPreset.NONE.rrule)
        assertEquals("FREQ=DAILY", RepeatPreset.DAILY.rrule)
        assertEquals("FREQ=WEEKLY", RepeatPreset.WEEKLY.rrule)
        assertEquals("FREQ=MONTHLY", RepeatPreset.MONTHLY.rrule)
        assertEquals("FREQ=YEARLY", RepeatPreset.YEARLY.rrule)

        assertEquals(RepeatPreset.NONE, RepeatPreset.fromRrule(null))
        assertEquals(RepeatPreset.NONE, RepeatPreset.fromRrule(""))
        assertEquals(RepeatPreset.WEEKLY, RepeatPreset.fromRrule("FREQ=WEEKLY;BYDAY=MO,WE"))
        assertEquals(RepeatPreset.YEARLY, RepeatPreset.fromRrule("FREQ=YEARLY"))
        assertEquals(RepeatPreset.MONTHLY, RepeatPreset.fromRrule("FREQ=MONTHLY;UNTIL=2027-01-01T00:00:00+02:00"))
        // A rule the picker cannot express reads as "doesn't repeat", as before.
        assertEquals(RepeatPreset.NONE, RepeatPreset.fromRrule("FREQ=HOURLY"))
    }

    @Test
    fun `every repeat preset has a label and the picker lists them in order`() {
        assertEquals(
            listOf("Doesn't repeat", "Daily", "Weekly", "Monthly", "Annually"),
            RepeatPreset.entries.map { it.label },
        )
    }

    @Test
    fun `the reminder preset labels match the original captions`() {
        assertEquals(listOf(10L, 30L, 60L, 1440L, 10080L), REMINDER_PRESETS)
        assertEquals("10 min", reminderPresetLabel(10))
        assertEquals("30 min", reminderPresetLabel(30))
        assertEquals("1 hour", reminderPresetLabel(60))
        assertEquals("1 day", reminderPresetLabel(1440))
        assertEquals("1 week", reminderPresetLabel(10080))
        assertEquals("custom", reminderPresetLabel(45))
    }

    @Test
    fun `the colour picker opens on the same blue the editor defaults to`() {
        assertEquals(8, COLOR_PRESETS.size)
        assertEquals(DEFAULT_ITEM_COLOR, COLOR_PRESETS.first())
        assertTrue(COLOR_PRESETS.all { it.matches(Regex("#[0-9a-f]{6}")) })
    }

    @Test
    fun `headings name the kind and whether it is new`() {
        assertEquals("New event", EditorFields.forNew(ItemKind.EVENT, monday, calendar).heading)
        assertEquals("New task", EditorFields.forNew(ItemKind.TASK, monday, calendar).heading)
        assertEquals("New birthday", EditorFields.forNew(ItemKind.BIRTHDAY, monday, calendar).heading)
        assertEquals("Edit event", EditorFields.fromItem(series(), null).heading)
    }

    @Test
    fun `the occurrence instant is RFC 3339 with a numeric offset`() {
        // The native parser is chrono's `parse_from_rfc3339`; serde writes a
        // numeric offset and never `Z`, so the form does the same.
        assertEquals(
            "2026-09-07T09:00:00+02:00",
            Formats.wireInstant(OffsetDateTime.parse("2026-09-07T09:00:00+02:00")),
        )
        assertEquals(
            "2026-09-07T09:00:00+00:00",
            Formats.wireInstant(OffsetDateTime.parse("2026-09-07T09:00:00Z")),
        )
    }

    @Test
    fun `the wire time is always 24-hour HH MM`() {
        assertEquals("09:00", Formats.wireTime(LocalTime.of(9, 0)))
        assertEquals("23:45", Formats.wireTime(LocalTime.of(23, 45)))
        assertEquals("00:00", Formats.wireTime(LocalTime.MIDNIGHT))
    }

    @Test
    fun `the date button reads like the app's other short dates`() {
        assertEquals("Aug 24, 2026", Formats.editedDate(monday))
    }
}
