package com.kal.calendar.widgets

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.view.View
import android.widget.RemoteViews

import com.kal.calendar.R
import java.time.ZonedDateTime
import java.time.temporal.ChronoUnit

/** Google-Calendar style "next events" home-screen widget. */
class ScheduleWidgetProvider : AppWidgetProvider() {

    companion object {
        const val MAX_ITEMS = 6

        /** Applies the schedule snapshot to the given widget ids. */
        fun update(context: Context, manager: AppWidgetManager, ids: IntArray) {
            val views = buildViews(context)
            for (id in ids) {
                manager.updateAppWidget(id, views)
            }
            manager.notifyAppWidgetViewDataChanged(ids, R.id.kal_widget_items)
        }

        private fun buildViews(context: Context): RemoteViews {
            val root = RemoteViews(context.packageName, R.layout.kal_widget_schedule)
            val body = KalWidgets.nativeSchedule(
                KalWidgets.dbPath(context),
                ZonedDateTime.now().toEpochSecond(),
                7,
            )
            val items = parseSchedule(body).take(MAX_ITEMS)

            val containerId = R.id.kal_widget_items
            if (items.isEmpty()) {
                root.removeAllViews(containerId)
                root.setViewVisibility(R.id.kal_widget_empty, View.VISIBLE)
                root.setViewVisibility(containerId, View.GONE)
                return root
            }
            root.setViewVisibility(R.id.kal_widget_empty, View.GONE)
            root.setViewVisibility(containerId, View.VISIBLE)
            root.removeAllViews(containerId)
            for (item in items) {
                val row = RemoteViews(context.packageName, R.layout.kal_widget_schedule_item)
                row.setTextViewText(R.id.kal_widget_item_title, item.title)
                row.setTextViewText(R.id.kal_widget_item_time, item.startLabel)
                try {
                    row.setInt(R.id.kal_widget_item_dot, "setColorFilter", Color.parseColor(item.color))
                } catch (_: IllegalArgumentException) {
                    row.setInt(R.id.kal_widget_item_dot, "setColorFilter", Color.parseColor("#3366cc"))
                }
                // Tapping any row opens the calendar.
                val open = openIntent(context)
                open.putExtra("kal_widget_open", "schedule")
                row.setOnClickPendingIntent(
                    R.id.kal_widget_item,
                    PendingIntent.getActivity(
                        context,
                        0,
                        open,
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                    ),
                )
                root.addView(containerId, row)
            }
            return root
        }

        private fun openIntent(context: Context): Intent {
            val intent = Intent(context, dev.dioxus.main.MainActivity::class.java)
            intent.action = Intent.ACTION_MAIN
            intent.addCategory(Intent.CATEGORY_LAUNCHER)
            return intent
        }

        fun scheduleWidget(context: Context): ComponentName =
            ComponentName(context, ScheduleWidgetProvider::class.java)
    }

    override fun onUpdate(
        context: Context,
        manager: AppWidgetManager,
        widgetIds: IntArray,
    ) {
        update(context, manager, widgetIds)
    }
}
