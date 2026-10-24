package com.kal.calendar.widgets

import android.appwidget.AppWidgetManager
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent

/** Refreshes all Kal widgets after a device reboot. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        val manager = AppWidgetManager.getInstance(context)
        ScheduleWidgetProvider.update(
            context,
            manager,
            manager.getAppWidgetIds(ScheduleWidgetProvider.scheduleWidget(context)),
        )
        MonthWidgetProvider.update(
            context,
            manager,
            manager.getAppWidgetIds(MonthWidgetProvider.monthWidget(context)),
        )
    }
}
