package com.kal.calendar.widgets

import android.appwidget.AppWidgetManager
import android.content.Context

/**
 * Pushes fresh database snapshots into both home-screen widgets after an app
 * mutation. Widgets also have a periodic update, but a just-added event should
 * show its dot as soon as the user returns to the home screen.
 */
object WidgetRefresher {
    fun refresh(context: Context) {
        val appContext = context.applicationContext
        val manager = AppWidgetManager.getInstance(appContext)

        val monthIds = manager.getAppWidgetIds(MonthWidgetProvider.monthWidget(appContext))
        if (monthIds.isNotEmpty()) {
            MonthWidgetProvider.update(appContext, manager, monthIds)
        }

        val scheduleIds = manager.getAppWidgetIds(ScheduleWidgetProvider.scheduleWidget(appContext))
        if (scheduleIds.isNotEmpty()) {
            ScheduleWidgetProvider.update(appContext, manager, scheduleIds)
        }
    }
}
