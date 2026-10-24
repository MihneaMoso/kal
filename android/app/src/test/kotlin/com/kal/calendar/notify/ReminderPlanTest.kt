package com.kal.calendar.notify

import com.kal.calendar.bridge.ReminderFiring
import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the reconcile policy — which alarms a forecast keeps armed — without an
 * AlarmManager anywhere in sight.
 */
class ReminderPlanTest {

    private fun firing(
        item: String = "01ITEM",
        reminder: String = "01REM",
        at: String = "2026-10-20T09:00:00+03:00",
        title: String = "Standup",
    ) = ReminderFiring(
        itemId = item,
        reminderId = reminder,
        fireAt = OffsetDateTime.parse(at),
        title = title,
    )

    @Test
    fun `a first reconcile arms everything and cancels nothing`() {
        val plan = planReminders(emptyList(), listOf(firing(), firing(item = "01OTHER")))

        assertTrue(plan.cancel.isEmpty())
        assertEquals(2, plan.arm.size)
    }

    @Test
    fun `an alarm whose firing disappeared is cancelled`() {
        val gone = firing(item = "01GONE")
        val kept = firing(item = "01KEPT")

        val plan = planReminders(listOf(gone.key()), listOf(kept))

        assertEquals(listOf(gone.key()), plan.cancel)
        assertEquals(listOf(kept), plan.arm)
    }

    @Test
    fun `a firing that is still wanted is not cancelled`() {
        val still = firing()

        val plan = planReminders(listOf(still.key()), listOf(still))

        assertTrue(plan.cancel.isEmpty())
    }

    @Test
    fun `an unchanged firing is re-armed as well, so a reconcile heals itself`() {
        val still = firing()

        val plan = planReminders(listOf(still.key()), listOf(still))

        // Re-arming is idempotent (AlarmManager replaces an equal PendingIntent)
        // and repairs an alarm a reboot or force-stop silently dropped.
        assertEquals(listOf(still), plan.arm)
    }

    @Test
    fun `no firings cancels everything that was armed`() {
        val a = firing(item = "01A")
        val b = firing(item = "01B")

        val plan = planReminders(listOf(a.key(), b.key()), emptyList())

        assertEquals(listOf(a.key(), b.key()), plan.cancel)
        assertTrue(plan.arm.isEmpty())
    }

    @Test
    fun `nothing armed and nothing to arm is a no-op`() {
        val plan = planReminders(emptyList(), emptyList())

        assertTrue(plan.cancel.isEmpty())
        assertTrue(plan.arm.isEmpty())
    }

    @Test
    fun `the key is stable per firing and distinct across firings`() {
        val base = firing()

        assertEquals(base.key(), base.copy().key())
        assertNotEquals(base.key(), firing(at = "2026-10-20T10:00:00+03:00").key())
        assertNotEquals(base.key(), firing(reminder = "01OTHER").key())
        assertNotEquals(base.key(), firing(item = "01OTHER").key())
    }

    @Test
    fun `the key follows the instant, not the textual offset`() {
        // The same moment rendered in two offsets (a DST flip between two
        // reconciles) must keep its identity, or a re-arm would leak an alarm.
        val plusThree = firing(at = "2026-10-20T09:00:00+03:00")
        val plusTwo = firing(at = "2026-10-20T08:00:00+02:00")

        assertEquals(plusThree.key(), plusTwo.key())
    }

    @Test
    fun `the notification caption matches the desktop scheduler`() {
        assertEquals("Kal — Standup", notificationTitle("Standup"))
        assertEquals("Standup", notificationBody("Standup"))
    }

    @Test
    fun `the channel is the one the receiver posts to`() {
        assertEquals("reminders", REMINDER_CHANNEL_ID)
        assertEquals("Reminders", REMINDER_CHANNEL_NAME)
    }
}
