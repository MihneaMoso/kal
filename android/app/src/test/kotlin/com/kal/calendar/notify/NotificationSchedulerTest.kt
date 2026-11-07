package com.kal.calendar.notify

import com.kal.calendar.bridge.ReminderFiring
import com.kal.calendar.notify.planReminders
import com.kal.calendar.notify.ReminderPlan
import com.kal.calendar.notify.notificationTitle
import com.kal.calendar.notify.notificationBody
import com.kal.calendar.notify.formatNotificationTime
import com.kal.calendar.notify.formatEventDateTime
import com.kal.calendar.notify.REMINDER_CHANNEL_ID
import com.kal.calendar.notify.REMINDER_CHANNEL_NAME
import com.kal.calendar.notify.ACTION_REMINDER
import com.kal.calendar.notify.EXTRA_KEY
import com.kal.calendar.notify.EXTRA_TITLE
import com.kal.calendar.notify.EXTRA_FIRE_AT
import com.kal.calendar.notify.EXTRA_EVENT_DATE
import com.kal.calendar.notify.EXTRA_EVENT_HOUR
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.time.OffsetDateTime
import java.time.ZoneOffset

/**
 * Tests for the reminder scheduling policy and notification formatting.
 */
class ReminderPlanTest {

    @Test
    fun `reminder_plan_cancels_removed_firings`() {
        val armed = listOf("item-1:rem-1:2026-01-01T10:00:00Z", "item-2:rem-2:2026-01-01T11:00:00Z")
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", OffsetDateTime.parse("2026-01-01T10:00:00Z"), "Event 1"),
        )

        val plan = planReminders(armed, firings)

        assertEquals(1, plan.cancel.size)
        assertTrue(plan.cancel.contains("item-2:rem-2:2026-01-01T11:00:00Z"))
        assertEquals(1, plan.arm.size)
        assertEquals("item-1", plan.arm[0].itemId)
    }

    @Test
    fun `reminder_plan_arms_new_firings`() {
        val armed = emptyList<String>()
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", OffsetDateTime.parse("2026-01-01T10:00:00Z"), "Event 1"),
            ReminderFiring("item-2", "rem-2", OffsetDateTime.parse("2026-01-01T11:00:00Z"), "Event 2"),
        )

        val plan = planReminders(armed, firings)

        assertEquals(0, plan.cancel.size)
        assertEquals(2, plan.arm.size)
        assertEquals("item-1", plan.arm[0].itemId)
        assertEquals("item-2", plan.arm[1].itemId)
    }

    @Test
    fun `reminder_plan_no_change_when_same`() {
        val armed = listOf("item-1:rem-1:2026-01-01T10:00:00Z")
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", OffsetDateTime.parse("2026-01-01T10:00:00Z"), "Event 1"),
        )

        val plan = planReminders(armed, firings)

        assertEquals(0, plan.cancel.size)
        assertEquals(1, plan.arm.size)
    }

    @Test
    fun `reminder_plan_empty_armed_and_firings`() {
        val plan = planReminders(emptyList(), emptyList())

        assertEquals(0, plan.cancel.size)
        assertEquals(0, plan.arm.size)
    }

    @Test
    fun `reminder_firing_key_is_stable`() {
        val firing = ReminderFiring(
            itemId = "item-123",
            reminderId = "rem-456",
            fireAt = OffsetDateTime.parse("2026-01-01T10:00:00Z"),
            title = "Test"
        )

        val key1 = firing.key()
        val key2 = firing.key()

        assertEquals(key1, key2)
        assertTrue(key1.contains("item-123"))
        assertTrue(key1.contains("rem-456"))
    }

    @Test
    fun `reminder_firing_key_is_unique_per_firing`() {
        val firing1 = ReminderFiring(
            itemId = "item-1",
            reminderId = "rem-1",
            fireAt = OffsetDateTime.parse("2026-01-01T10:00:00Z"),
            title = "Test"
        )
        val firing2 = ReminderFiring(
            itemId = "item-1",
            reminderId = "rem-1",
            fireAt = OffsetDateTime.parse("2026-01-01T11:00:00Z"),
            title = "Test"
        )

        assertFalse(firing1.key() == firing2.key())
    }

    @Test
    fun `notification_title_format`() {
        assertEquals("Kal — Test Event", notificationTitle("Test Event"))
    }

    @Test
    fun `notification_body_format`() {
        assertEquals("Test Event", notificationBody("Test Event"))
    }

    @Test
    fun `format_notification_time`() {
        val instant = Instant.parse("2026-01-01T10:00:00Z")
        val formatted = formatNotificationTime(instant)
        assertEquals("2026-01-01 10:00", formatted)
    }

    @Test
    fun `format_event_date_time`() {
        val instant = Instant.parse("2026-10-24T09:00:00Z")
        val formatted = formatEventDateTime(instant)
        assertEquals("24 Oct 2026 9:00", formatted)
    }

    @Test
    fun `constants_are_correct`() {
        assertEquals("reminders", REMINDER_CHANNEL_ID)
        assertEquals("Reminders", REMINDER_CHANNEL_NAME)
        assertEquals("com.kal.calendar.action.REMINDER", ACTION_REMINDER)
        assertEquals("com.kal.calendar.extra.REMINDER_KEY", EXTRA_KEY)
        assertEquals("com.kal.calendar.extra.REMINDER_TITLE", EXTRA_TITLE)
        assertEquals("com.kal.calendar.extra.REMINDER_FIRE_AT", EXTRA_FIRE_AT)
        assertEquals("com.kal.calendar.extra.REMINDER_EVENT_DATE", EXTRA_EVENT_DATE)
        assertEquals("com.kal.calendar.extra.REMINDER_EVENT_HOUR", EXTRA_EVENT_HOUR)
    }
}