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
import java.time.LocalDate
import java.time.YearMonth

/** Google-Calendar style month-view home-screen widget. */
class MonthWidgetProvider : AppWidgetProvider() {

    companion object {
        fun update(context: Context, manager: AppWidgetManager, ids: IntArray) {
            val views = buildViews(context)
            for (id in ids) {
                manager.updateAppWidget(id, views)
            }
        }

        /** Parses a "YYYY-MM-DD" date string into a LocalDate, or null. */
        private fun parseDate(s: String): LocalDate? = runCatching {
            LocalDate.parse(s)
        }.getOrNull()

        private fun buildViews(context: Context): RemoteViews {
            val today = LocalDate.now()
            val month = YearMonth.from(today)
            val body = KalWidgets.nativeMonth(
                KalWidgets.dbPath(context),
                month.year,
                month.monthValue,
                0,
            )
            val weeks = parseMonth(body)

            val root = RemoteViews(context.packageName, R.layout.kal_widget_month)

            // Header: e.g. "August 2026".
            val monthName = java.time.format.DateTimeFormatter
                .ofPattern("MMMM yyyy").withLocale(context.resources.configuration.locales[0])
                .format(month)
            root.setTextViewText(R.id.kal_widget_month_title, monthName)

            val weekdays = arrayOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
            val rowIds = intArrayOf(
                R.id.kal_widget_month_row1,
                R.id.kal_widget_month_row2,
                R.id.kal_widget_month_row3,
                R.id.kal_widget_month_row4,
                R.id.kal_widget_month_row5,
                R.id.kal_widget_month_row6,
            )
            val dayLabelIds = arrayOf(
                intArrayOf(R.id.day1t, R.id.day2t, R.id.day3t, R.id.day4t, R.id.day5t, R.id.day6t, R.id.day7t),
                intArrayOf(R.id.day8t, R.id.day9t, R.id.day10t, R.id.day11t, R.id.day12t, R.id.day13t, R.id.day14t),
                intArrayOf(R.id.day15t, R.id.day16t, R.id.day17t, R.id.day18t, R.id.day19t, R.id.day20t, R.id.day21t),
                intArrayOf(R.id.day22t, R.id.day23t, R.id.day24t, R.id.day25t, R.id.day26t, R.id.day27t, R.id.day28t),
                intArrayOf(R.id.day29t, R.id.day30t, R.id.day31t, R.id.day32t, R.id.day33t, R.id.day34t, R.id.day35t),
                intArrayOf(R.id.day36t, R.id.day37t, R.id.day38t, R.id.day39t, R.id.day40t, R.id.day41t, R.id.day42t),
            )
            val dotIds = arrayOf(
                intArrayOf(R.id.dot1, R.id.dot2, R.id.dot3, R.id.dot4, R.id.dot5, R.id.dot6, R.id.dot7),
                intArrayOf(R.id.dot8, R.id.dot9, R.id.dot10, R.id.dot11, R.id.dot12, R.id.dot13, R.id.dot14),
                intArrayOf(R.id.dot15, R.id.dot16, R.id.dot17, R.id.dot18, R.id.dot19, R.id.dot20, R.id.dot21),
                intArrayOf(R.id.dot22, R.id.dot23, R.id.dot24, R.id.dot25, R.id.dot26, R.id.dot27, R.id.dot28),
                intArrayOf(R.id.dot29, R.id.dot30, R.id.dot31, R.id.dot32, R.id.dot33, R.id.dot34, R.id.dot35),
                intArrayOf(R.id.dot36, R.id.dot37, R.id.dot38, R.id.dot39, R.id.dot40, R.id.dot41, R.id.dot42),
            )

            // Column headers.
            val headerIds = colHeaderIds()
            for ((i, name) in weekdays.withIndex()) {
                root.setTextViewText(headerIds[i], name)
            }

            // Cells.
            for (w in weeks.indices) {
                val week = weeks[w]
                val rowId = rowIds[w]
                val isExtraWeek = w >= 5 && weeks.size < 6
                for (d in week.indices) {
                    val day = week[d]
                    val cell = dayLabelIds[w][d]
                    val dot = dotIds[w][d]
                    val date = parseDate(day.date)
                    val isToday = date == today
                    if (isExtraWeek) {
                        // Grid shorter than 6 weeks: collapse leftover week.
                        root.setViewVisibility(rowId, View.GONE)
                        continue
                    }
                    root.setTextViewText(cell, date?.dayOfMonth?.toString() ?: "")
                    root.setTextColor(
                        cell,
                        if (isToday) Color.parseColor("#9CDCFE")
                        else if (day.inMonth) Color.parseColor("#E6E6E6")
                        else Color.parseColor("#6B7280"),
                    )
                    if (day.items.isEmpty()) {
                        root.setViewVisibility(dot, View.GONE)
                    } else {
                        root.setViewVisibility(dot, View.VISIBLE)
                        try {
                            root.setInt(dot, "setColorFilter", Color.parseColor(day.items[0].color))
                        } catch (_: IllegalArgumentException) {
                            root.setInt(dot, "setColorFilter", Color.parseColor("#3366cc"))
                        }
                    }
                    root.setOnClickPendingIntent(
                        cell,
                        tap(context),
                    )
                }
            }

            return root
        }

        private var colHeaders: IntArray? = null

        private fun colHeaderIds(): IntArray {
            if (colHeaders == null) {
                colHeaders = intArrayOf(
                    R.id.kal_widget_month_mon,
                    R.id.kal_widget_month_tue,
                    R.id.kal_widget_month_wed,
                    R.id.kal_widget_month_thu,
                    R.id.kal_widget_month_fri,
                    R.id.kal_widget_month_sat,
                    R.id.kal_widget_month_sun,
                )
            }
            return colHeaders!!
        }

        private fun tap(context: Context): PendingIntent {
            val intent = Intent(context, dev.dioxus.main.MainActivity::class.java)
            intent.action = Intent.ACTION_MAIN
            intent.addCategory(Intent.CATEGORY_LAUNCHER)
            intent.putExtra("kal_widget_open", "month")
            return PendingIntent.getActivity(
                context,
                1,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }

        fun monthWidget(context: Context): ComponentName =
            ComponentName(context, MonthWidgetProvider::class.java)
    }

    override fun onUpdate(
        context: Context,
        manager: AppWidgetManager,
        widgetIds: IntArray,
    ) {
        update(context, manager, widgetIds)
    }
}
