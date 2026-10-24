package com.kal.calendar.notify

import com.kal.calendar.bridge.ReminderFiring

/**
 * Reminder scheduling policy, kept free of Android so the JVM tests can pin it.
 *
 * The split follows the guide's boundary tables: firing *computation* is native
 * (`kal_reminder_firings_json` over `kal-core::reminders`), while arming and
 * delivering them is platform work and therefore Kotlin (§0.19.1).
 */

/** Action and extras of the intent the alarm fires into [ReminderReceiver]. */
const val ACTION_REMINDER = "com.kal.calendar.action.REMINDER"
const val EXTRA_KEY = "com.kal.calendar.extra.REMINDER_KEY"
const val EXTRA_TITLE = "com.kal.calendar.extra.REMINDER_TITLE"

/** Notification channel the platform shows reminders on. */
const val REMINDER_CHANNEL_ID = "reminders"

/** Channel name, the one piece of copy this phase adds (there was no channel). */
const val REMINDER_CHANNEL_NAME = "Reminders"

/** Private bookkeeping for the scheduler (armed keys, "permission asked"). */
const val REMINDER_PREFS = "kal-reminders"
const val PREF_ARMED = "armed"
const val PREF_PERMISSION_ASKED = "permissionAsked"

/** Request code for the POST_NOTIFICATIONS prompt. */
const val REMINDER_PERMISSION_REQUEST = 0x4b41

/**
 * A stable identity for one firing: the same reminder of the same item at the
 * same instant keeps the same key across reconciles, process deaths and
 * reboots, which is what lets a re-arm replace — and a cancel find — the alarm
 * that already exists.
 */
fun ReminderFiring.key(): String = "$itemId:$reminderId:${fireAt.toInstant()}"

/**
 * What one reconcile does: cancel the armed alarms whose firings are gone, and
 * (re-)arm every firing that should exist.
 *
 * Arming is idempotent — `AlarmManager` replaces an alarm with an equal
 * `PendingIntent` — so the plan re-arms the whole wanted set instead of
 * diffing timestamps. That keeps a reconcile self-healing after an app update
 * or a reboot without tracking "which alarm is stale".
 *
 * Pure: no AlarmManager, no Context.
 */
data class ReminderPlan(
    /** Keys that must be cancelled because their firing disappeared. */
    val cancel: List<String>,
    /** Firings that must be armed, nearest first. */
    val arm: List<ReminderFiring>,
)

fun planReminders(armed: Collection<String>, firings: List<ReminderFiring>): ReminderPlan {
    val wanted = firings.map { it.key() }.toSet()
    return ReminderPlan(
        cancel = armed.filterNot { it in wanted },
        arm = firings,
    )
}

/**
 * Notification caption, exactly what `ThreadScheduler` + `notify-rust` showed
 * on desktop: `Kal — {title}`.
 */
fun notificationTitle(title: String): String = "Kal — $title"

/** The desktop scheduler's body: the item title itself. */
fun notificationBody(title: String): String = title
