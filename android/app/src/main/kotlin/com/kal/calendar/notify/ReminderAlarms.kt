package com.kal.calendar.notify

import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import com.kal.calendar.MainActivity
import com.kal.calendar.R
import com.kal.calendar.bridge.ReminderFiring
import org.json.JSONArray

/**
 * The Android half of reminder scheduling: AlarmManager alarms, the
 * notification channel and the notification itself.
 *
 * Firing *computation* stays native (`kal_reminder_firings_json` over
 * `kal-core::reminders`); the platform mechanisms are Kotlin, per §0.19.1.
 * Everything here is deliberately idempotent so a reconcile can run on every
 * mutation, on every foreground, and after a reboot.
 */
class ReminderAlarms(context: Context) {

    private val appContext = context.applicationContext

    private val prefs by lazy {
        appContext.getSharedPreferences(REMINDER_PREFS, Context.MODE_PRIVATE)
    }

    /**
     * Make the armed alarms match [firings]: cancel the ones whose firing
     * disappeared, (re-)arm the rest. Safe to call as often as the caller likes.
     */
    fun reconcile(firings: List<ReminderFiring>) {
        ensureChannel()
        val manager =
            appContext.getSystemService(Context.ALARM_SERVICE) as? AlarmManager ?: return

        val plan = planReminders(armedKeys(), firings)
        for (key in plan.cancel) {
            manager.cancel(pendingIntent(key))
        }

        // Only the firings that really armed are remembered, so a failure (a
        // revoked grant, a rejected intent) is retried by the next reconcile.
        storeArmed(plan.arm.filter { arm(manager, it) }.map { it.key() })
    }

    /** Create the channel if it does not exist yet; harmless to call often. */
    fun ensureChannel() {
        val manager = appContext.getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(REMINDER_CHANNEL_ID) != null) return
        manager.createNotificationChannel(
            NotificationChannel(
                REMINDER_CHANNEL_ID,
                REMINDER_CHANNEL_NAME,
                NotificationManager.IMPORTANCE_DEFAULT,
            ),
        )
    }

    /**
     * Show one reminder, named by [key] so two firings never overwrite each
     * other's notification. Without POST_NOTIFICATIONS the system drops it
     * silently — the alarm has already done its job by then.
     */
    fun show(key: String, title: String) {
        ensureChannel()
        val manager = appContext.getSystemService(NotificationManager::class.java) ?: return
        val notification = Notification.Builder(appContext, REMINDER_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(notificationTitle(title))
            .setContentText(notificationBody(title))
            .setAutoCancel(true)
            .setContentIntent(openApp())
            .build()
        manager.notify(requestCode(key), notification)
    }

    /** Alarm keys the last successful reconcile armed. */
    fun armedKeys(): List<String> = runCatching {
        val array = JSONArray(prefs.getString(PREF_ARMED, "[]"))
        (0 until array.length()).map { array.getString(it) }
    }.getOrDefault(emptyList())

    private fun storeArmed(keys: List<String>) {
        prefs.edit().putString(PREF_ARMED, JSONArray(keys).toString()).apply()
    }

    private fun arm(manager: AlarmManager, firing: ReminderFiring): Boolean {
        val at = firing.fireAt.toInstant().toEpochMilli()
        val intent = pendingIntent(firing.key(), firing)

        if (canScheduleExact(manager)) {
            try {
                manager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
                return true
            } catch (_: SecurityException) {
                // The grant vanished between the check and the call; degrade.
            }
        }

        return try {
            // Exact alarms are not allowed (special access never granted): the
            // reminder still arrives, possibly a few minutes late in Doze.
            manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
            true
        } catch (_: SecurityException) {
            false
        }
    }

    private fun canScheduleExact(manager: AlarmManager): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S || manager.canScheduleExactAlarms()

    /**
     * The alarm's intent. [firing] is only read when arming; a cancel builds the
     * same intent without the display extras, which is still "equal" for
     * PendingIntent purposes (the request code is what identifies it).
     */
    private fun pendingIntent(key: String, firing: ReminderFiring? = null): PendingIntent {
        val intent = Intent(appContext, ReminderReceiver::class.java)
            .setAction(ACTION_REMINDER)
            .putExtra(EXTRA_KEY, key)
        if (firing != null) {
            intent.putExtra(EXTRA_TITLE, firing.title)
        }
        return PendingIntent.getBroadcast(
            appContext,
            requestCode(key),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun openApp(): PendingIntent {
        val intent = Intent(appContext, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        return PendingIntent.getActivity(
            appContext,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    /** A stable, non-negative request code derived from the firing key. */
    private fun requestCode(key: String): Int = key.hashCode() and 0x7fffffff
}
