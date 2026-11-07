package com.kal.calendar.notify

import com.kal.calendar.bridge.ReminderFiring
import com.kal.calendar.notify.planReminders
import com.kal.calendar.notify.ReminderPlan
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.OffsetDateTime
import java.time.ZoneOffset

/**
 * Tests for the reminder scheduler and alarm management.
 */
class ReminderSchedulerTest {

    @Test
    fun `reminder_plan_calculates_firings_correctly_for_single_event`() {
        val now = OffsetDateTime.now(ZoneOffset.UTC)
        val eventStart = now.plusHours(1)
        
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", eventStart.minusMinutes(10), "Event"),
            ReminderFiring("item-1", "rem-2", eventStart.minusMinutes(30), "Event"),
            ReminderFiring("item-1", "rem-3", eventStart.minusMinutes(60), "Event"),
        )
        val plan = planReminders(emptyList(), firings)
        
        assertEquals(3, plan.arm.size)
    }

    @Test
    fun `reminder_plan_calculates_firings_in_correct_order`() {
        val now = OffsetDateTime.now(ZoneOffset.UTC)
        val eventStart = now.plusHours(1)
        
        // Firings in random order - plan keeps the order as provided (native side sorts them)
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", eventStart.minusMinutes(10), "Event"),
            ReminderFiring("item-1", "rem-3", eventStart.minusMinutes(60), "Event"),
            ReminderFiring("item-1", "rem-2", eventStart.minusMinutes(30), "Event"),
        )
        val plan = planReminders(emptyList(), firings)
        
        // The plan keeps the order of firings as provided (native side sorts them)
        assertEquals(3, plan.arm.size)
        // Order is preserved from input list
        assertEquals("rem-1", plan.arm[0].reminderId)
        assertEquals("rem-3", plan.arm[1].reminderId)
        assertEquals("rem-2", plan.arm[2].reminderId)
    }

    @Test
    fun `reminder_plan_handles_past_firings`() {
        val now = OffsetDateTime.now(ZoneOffset.UTC)
        val eventStart = now.minusHours(1) // Event already started
        
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", eventStart.minusMinutes(10), "Event"),
        )
        val plan = planReminders(emptyList(), firings)
        
        // Past firings are still included - it's up to the native side to filter
        assertEquals(1, plan.arm.size)
    }

    @Test
    fun `reminder_plan_handles_multiple_events`() {
        val now = OffsetDateTime.now(ZoneOffset.UTC)
        val event1 = now.plusHours(1)
        val event2 = now.plusHours(2)
        
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", event1.minusMinutes(10), "Event 1"),
            ReminderFiring("item-1", "rem-2", event1.minusMinutes(30), "Event 1"),
            ReminderFiring("item-2", "rem-1", event2.minusMinutes(15), "Event 2"),
        )
        val plan = planReminders(emptyList(), firings)
        
        assertEquals(3, plan.arm.size)
        assertEquals(2, plan.arm.count { it.itemId == "item-1" })
        assertEquals(1, plan.arm.count { it.itemId == "item-2" })
    }

    @Test
    fun `reminder_plan_cancels_removed_firings`() {
        val armed = listOf(
            "item-1:rem-1:2026-01-01T10:00:00Z",
            "item-1:rem-2:2026-01-01T10:30:00Z",
            "item-2:rem-1:2026-01-01T11:00:00Z",
        )
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", OffsetDateTime.parse("2026-01-01T10:00:00Z"), "Event 1"),
            ReminderFiring("item-2", "rem-1", OffsetDateTime.parse("2026-01-01T11:00:00Z"), "Event 2"),
        )
        
        val plan = planReminders(armed, firings)
        
        assertEquals(1, plan.cancel.size)
        assertTrue(plan.cancel.contains("item-1:rem-2:2026-01-01T10:30:00Z"))
        assertEquals(2, plan.arm.size)
    }

    @Test
    fun `reminder_plan_arms_all_when_armed_empty`() {
        val armed = emptyList<String>()
        val firings = listOf(
            ReminderFiring("item-1", "rem-1", OffsetDateTime.parse("2026-01-01T10:00:00Z"), "Event 1"),
            ReminderFiring("item-2", "rem-1", OffsetDateTime.parse("2026-01-01T11:00:00Z"), "Event 2"),
        )
        
        val plan = planReminders(armed, firings)
        
        assertEquals(0, plan.cancel.size)
        assertEquals(2, plan.arm.size)
    }

    @Test
    fun `reminder_plan_cancels_all_when_firings_empty`() {
        val armed = listOf(
            "item-1:rem-1:2026-01-01T10:00:00Z",
            "item-2:rem-1:2026-01-01T11:00:00Z",
        )
        val firings = emptyList<ReminderFiring>()
        
        val plan = planReminders(armed, firings)
        
        assertEquals(2, plan.cancel.size)
        assertEquals(0, plan.arm.size)
    }

    @Test
    fun `reminder_firing_key_format`() {
        val firing = ReminderFiring(
            itemId = "item-123",
            reminderId = "rem-456",
            fireAt = OffsetDateTime.parse("2026-01-01T10:00:00Z"),
            title = "Test"
        )
        
        val key = firing.key()
        
        assertTrue(key.startsWith("item-123:rem-456:"))
        assertTrue(key.contains("2026-01-01T10:00:00"))
    }

    @Test
    fun `reminder_firing_key_includes_instant`() {
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
}