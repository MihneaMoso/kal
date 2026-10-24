package com.kal.calendar.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Delivers one armed alarm: shows the reminder. Deliberately tiny — the intent
 * carries everything the notification needs, so the app process does not have
 * to be running.
 */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_REMINDER) return
        val key = intent.getStringExtra(EXTRA_KEY) ?: return
        val title = intent.getStringExtra(EXTRA_TITLE) ?: return
        ReminderAlarms(context).show(key, title)
    }
}
