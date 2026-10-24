package com.kal.calendar.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.kal.calendar.bridge.BridgeStatus
import com.kal.calendar.bridge.KalBridge

/**
 * Re-arms every reminder after a reboot.
 *
 * `AlarmManager` forgets all alarms when the device restarts, and the app may
 * not be opened for days afterwards — without this, reminders silently stop
 * after a reboot. The firing computation stays native, so this reopens the
 * database through the same handshake the activity uses, reconciles exactly the
 * way the shell does, and closes the handle again.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        val app = context.applicationContext
        val status = KalBridge.start(app.filesDir.absolutePath)
        if (status !is BridgeStatus.Ready) return

        try {
            ReminderAlarms(app).reconcile(status.session.reminderFirings())
        } finally {
            status.session.close()
        }
    }
}
