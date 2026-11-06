package com.kal.calendar.notify

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Calendar

/**
 * Tests for the reminder scheduler and alarm management.
 */
class ReminderSchedulerTest {

    @Test
    fun `reminder_plan_calculates_firings_correctly_for_single_event`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 30, 60) // 10, 30, 60 minutes before
        val plan = ReminderPlan(eventStart, reminders)
        
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_calculates_firings_in_correct_order`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Firings should be sorted from nearest to farthest
        assertTrue(plan.firings[0].fireAt.timeInMillis <= plan.firings[1].fireAt.timeInMillis)
        assertTrue(plan.firings[1].fireAt.timeInMillis <= plan.firings[2].fireAt.timeInMillis)
    }

    @Test
    fun `reminder_plan_drops_firings_that_already_fired`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis - 1000000 // 16 minutes ago (past)
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // All firings are in the past, so none should be scheduled
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_drops_firings_beyond_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 30, 120) // 120 minutes is beyond 60 minute horizon
        val plan = ReminderPlan(eventStart, reminders)
        
        // Only firings within 60 minutes should be scheduled
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_zero_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf<Int>()
        val plan = ReminderPlan(eventStart, reminders)
        
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_empty_reminder_list`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = emptyList<Int>()
        val plan = ReminderPlan(eventStart, reminders)
        
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_null_reminder_list`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders: List<Int>? = null
        val plan = ReminderPlan(eventStart, reminders)
        
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_negative_reminder_values`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(-10, -30, -60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Negative reminders are invalid, so no firings
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_mixed_valid_invalid_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, -30, 60, -120, 30)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Only positive reminders within horizon should be scheduled
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_duplicate_reminder_values`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 10, 10, 30, 30)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Duplicate firings should be deduplicated
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_same_event_multiple_times`() {
        val now = Calendar.getInstance()
        val eventStart1 = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        val eventStart2 = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 30)
        val plan1 = ReminderPlan(eventStart1, reminders)
        val plan2 = ReminderPlan(eventStart2, reminders)
        
        // Same start time should produce same number of firings
        assertEquals(plan1.firings.size, plan2.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_in_past_with_future_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis - 7200000 // 2 hours ago
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Event is in past, all reminders would have already fired
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_event_in_future_with_past_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 7200000 // 2 hours from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders are in the future and within horizon
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_large_horizon_value`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 30, 60, 180, 300)
        val plan = ReminderPlan(eventStart, reminders, 60) // 60 minute horizon
        
        // Only firings within 60 minutes should be scheduled
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_zero_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 0) // Zero horizon means no future firings
        
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_very_large_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 30, 60, 180, 300)
        val plan = ReminderPlan(eventStart, reminders, 999999) // Very large horizon
        
        // All firings should be within horizon
        assertEquals(5, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_reminder_at_event_time`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(0, 10, 30) // 0 minutes is at event time
        val plan = ReminderPlan(eventStart, reminders)
        
        // Reminder at event time (0 minutes) should be dropped
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_reminder_beyond_event_time`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000
        }
        
        val reminders = listOf(10, 30, 120) // 120 minutes is beyond event time
        val plan = ReminderPlan(eventStart, reminders)
        
        // Reminder beyond event time (120 minutes) should be dropped
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_at_same_time_as_reminder`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3660000 // 61 minutes from now
        }
        
        val reminders = listOf(60) // 60 minutes before event
        val plan = ReminderPlan(eventStart, reminders)
        
        // 60 minutes before event should be scheduled
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_exact_horizon_boundary`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 7200000 // 120 minutes from now
        }
        
        val reminders = listOf(60, 120)
        val plan = ReminderPlan(eventStart, reminders, 120) // 120 minute horizon
        
        // Both reminders should be within or at horizon
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_reminder_just_beyond_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 7200000 // 120 minutes from now
        }
        
        val reminders = listOf(60, 121) // 121 minutes is just beyond 120 minute horizon
        val plan = ReminderPlan(eventStart, reminders, 120)
        
        // Only 60 minute reminder should be within horizon
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_reminder_just_within_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 7200000 // 120 minutes from now
        }
        
        val reminders = listOf(60, 119) // 119 minutes is just within 120 minute horizon
        val plan = ReminderPlan(eventStart, reminders, 120)
        
        // Both reminders should be within horizon
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_far_in_future`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 86400000 // 24 hours from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders are in the future, but may be beyond horizon
        // Depends on default horizon
        assertTrue(plan.firings.isNotEmpty())
    }

    @Test
    fun `reminder_plan_handles_event_in_next_day`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 86400000 // 24 hours from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 1440) // 24 hour horizon
        
        // All reminders should be within 24 hour horizon
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_in_next_week`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 604800000 // 7 days from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 604800) // 7 day horizon
        
        // All reminders should be within 7 day horizon
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_in_next_month`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 2592000000 // 30 days from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 2592000) // 30 day horizon
        
        // All reminders should be within 30 day horizon
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_in_next_year`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 31536000000 // 365 days from now
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 31536000) // 365 day horizon
        
        // All reminders should be within 365 day horizon
        assertEquals(3, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_past_with_long_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis - 3600000 // 1 hour ago
        }
        
        val reminders = listOf(10, 30, 60)
        val plan = ReminderPlan(eventStart, reminders, 86400000) // 1 day horizon
        
        // Event is in past, all reminders would have already fired
        assertTrue(plan.firings.isEmpty())
    }

    @Test
    fun `reminder_plan_handles_event_future_with_short_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 30, 120) // 120 minutes is beyond 60 minute horizon
        val plan = ReminderPlan(eventStart, reminders, 60 * 60 * 1000) // 60 minute horizon
        
        // Only firings within 60 minutes should be scheduled
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_long_horizon`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 30, 120, 180, 300)
        val plan = ReminderPlan(eventStart, reminders, 86400000) // 1 day horizon
        
        // All firings should be within 1 day horizon
        assertEquals(5, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_exact_horizon_reminder`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 1200000 // 20 minutes from now
        }
        
        val reminders = listOf(20) // Exactly at horizon
        val plan = ReminderPlan(eventStart, reminders, 1200000) // 20 minute horizon
        
        // Reminder at exact horizon should be included
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_horizon_plus_one_reminder`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 121000 // 20 minutes 1 second from now
        }
        
        val reminders = listOf(20) // 20 minutes before event
        val plan = ReminderPlan(eventStart, reminders, 120000) // 20 minute horizon
        
        // Reminder is 1 second beyond horizon
        assertEquals(0, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_horizon_minus_one_reminder`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 121000 // 20 minutes 1 second from now
        }
        
        val reminders = listOf(19) // 19 minutes before event
        val plan = ReminderPlan(eventStart, reminders, 120000) // 20 minute horizon
        
        // Reminder is 1 second within horizon
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_many_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = (1..60).toList() // 1 to 60 minutes before
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders should be within horizon (assuming 60 minute default)
        assertEquals(60, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_gap_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 20, 30, 40, 50) // Gaps between reminders
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders should be scheduled
        assertEquals(5, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_random_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(5, 15, 25, 35, 45, 55) // Random intervals
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders should be scheduled
        assertEquals(6, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_very_small_intervals`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(1, 2, 3, 4, 5) // Very small intervals
        val plan = ReminderPlan(eventStart, reminders)
        
        // All reminders should be scheduled
        assertEquals(5, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_very_large_intervals`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(90, 120, 180) // Large intervals
        val plan = ReminderPlan(eventStart, reminders, 60 * 60 * 1000) // 60 minute horizon
        
        // Only reminders within horizon should be scheduled
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_boundary_intervals`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(59, 60, 61) // Around 60 minute boundary
        val plan = ReminderPlan(eventStart, reminders, 60 * 60 * 1000) // 60 minute horizon
        
        // 59 and 60 minute reminders should be within horizon
        // 61 minute reminder should be beyond horizon
        assertEquals(2, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_same_interval_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(30, 30, 30, 30) // Same interval multiple times
        val plan = ReminderPlan(eventStart, reminders)
        
        // Duplicates should be deduplicated
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_sorted_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(10, 20, 30, 40, 50, 60) // Already sorted
        val plan = ReminderPlan(eventStart, reminders)
        
        // Firings should be in correct order
        assertTrue(plan.firings[0].fireAt.timeInMillis <= plan.firings[1].fireAt.timeInMillis)
        assertTrue(plan.firings[1].fireAt.timeInMillis <= plan.firings[2].fireAt.timeInMillis)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_reverse_sorted_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(60, 50, 40, 30, 20, 10) // Reverse sorted
        val plan = ReminderPlan(eventStart, reminders)
        
        // Firings should be sorted from nearest to farthest
        assertTrue(plan.firings[0].fireAt.timeInMillis <= plan.firings[1].fireAt.timeInMillis)
        assertTrue(plan.firings[1].fireAt.timeInMillis <= plan.firings[2].fireAt.timeInMillis)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_unsorted_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(30, 10, 60, 20, 50) // Unsorted
        val plan = ReminderPlan(eventStart, reminders)
        
        // Firings should be sorted from nearest to farthest
        assertTrue(plan.firings[0].fireAt.timeInMillis <= plan.firings[1].fireAt.timeInMillis)
        assertTrue(plan.firings[1].fireAt.timeInMillis <= plan.firings[2].fireAt.timeInMillis)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_single_reminder`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = listOf(30)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Single reminder should be scheduled
        assertEquals(1, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_many_unique_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = (1..100).toList()
        val plan = ReminderPlan(eventStart, reminders)
        
        // All unique reminders within horizon should be scheduled
        assertEquals(100, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_many_duplicate_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = (1..10).repeat(10) // 100 reminders, many duplicates
        val plan = ReminderPlan(eventStart, reminders)
        
        // Only unique reminders should be scheduled
        assertEquals(10, plan.firings.size)
    }

    @Test
    fun `reminder_plan_handles_event_future_with_many_mixed_reminders`() {
        val now = Calendar.getInstance()
        val eventStart = Calendar.getInstance().apply {
            timeInMillis = now.timeInMillis + 3600000 // 1 hour from now
        }
        
        val reminders = (1..10).repeat(10) + (11..20).toList() + listOf(-1, -2, -3)
        val plan = ReminderPlan(eventStart, reminders)
        
        // Only unique positive reminders within horizon should be scheduled
        assertEquals(20, plan.firings.size)
    }
}
