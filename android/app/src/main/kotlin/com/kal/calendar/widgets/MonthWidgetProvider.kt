package com.kal.calendar.widgets

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.widget.RemoteViews

import com.kal.calendar.MainActivity
import com.kal.calendar.R
import java.time.LocalDate
import java.time.YearMonth

/**
 * Google-Calendar-style month-view home-screen widget.
 *
 * The header is a month/year title with add and previous/next controls. Each
 * day cell is a [PendingIntent] that deep-links into [MainActivity] with the
 * tapped date, so the app opens the new-event editor for that day. Month
 * navigation is per-widget: a broadcast updates the stored pin and redraws
 * only that widget (home-screen widgets cannot swipe).
 *
 * An untouched widget follows the calendar month. Once the user navigates, the
 * absolute month is pinned instead of stored as an offset from "today", so the
 * view does not drift forward when the month rolls over. Navigating back to the
 * current month clears the pin and resumes following the calendar.
 */
class MonthWidgetProvider : AppWidgetProvider() {

    companion object {
        private const val PREFS = "kal_month_widget"
        private const val PINNED_PREFIX = "pinned_"
        private const val LEGACY_OFFSET_PREFIX = "offset_"
        private const val NO_LEGACY_OFFSET = Int.MIN_VALUE

        private const val ACTION_PREV = "com.kal.calendar.widgets.action.MONTH_PREV"
        private const val ACTION_NEXT = "com.kal.calendar.widgets.action.MONTH_NEXT"
        private const val EXTRA_WIDGET_ID = "widget_id"

        /** Wire convention from kal-ffi: 0 = Monday … 6 = Sunday. */
        private const val FIRST_DAY_OF_WEEK = 0

        private const val SLOT_PREV = 1
        private const val SLOT_NEXT = 2
        private const val SLOT_ADD = 3
        private const val SLOT_FIRST_DAY = 10

        /** The compact month grid shows two single-line event names per day. */
        private const val CHIPS_PER_DAY = 2

        fun update(context: Context, manager: AppWidgetManager, ids: IntArray) {
            for (id in ids) {
                manager.updateAppWidget(id, buildViews(context, id))
            }
        }

        private fun widgetPrefs(context: Context) =
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

        private fun pinnedKey(widgetId: Int) = "$PINNED_PREFIX$widgetId"

        private fun legacyOffsetKey(widgetId: Int) = "$LEGACY_OFFSET_PREFIX$widgetId"

        /**
         * The month this widget shows. A pinned month wins; otherwise the
         * widget follows today. The one-time legacy branch converts a widget
         * that still has the old relative offset, so an upgrade keeps the
         * month the user had navigated to.
         */
        private fun monthFor(context: Context, widgetId: Int, today: YearMonth): YearMonth {
            val prefs = widgetPrefs(context)
            MonthWidgetModel.parseMonthKey(prefs.getString(pinnedKey(widgetId), null))
                ?.let { return MonthWidgetModel.monthFor(today, it) }

            val legacy = prefs.getInt(legacyOffsetKey(widgetId), NO_LEGACY_OFFSET)
            if (legacy == NO_LEGACY_OFFSET) return today

            val pin = MonthWidgetModel.pinFromLegacyOffset(legacy, today)
            val editor = prefs.edit().remove(legacyOffsetKey(widgetId))
            if (pin == null) {
                editor.remove(pinnedKey(widgetId))
            } else {
                editor.putString(pinnedKey(widgetId), MonthWidgetModel.monthKey(pin))
            }
            editor.apply()
            return pin ?: today
        }

        /** Parses a "YYYY-MM-DD" date string into a LocalDate, or null. */
        private fun parseDate(s: String): LocalDate? = runCatching {
            LocalDate.parse(s)
        }.getOrNull()

        private fun buildViews(context: Context, widgetId: Int): RemoteViews {
            val today = LocalDate.now()
            val locale = context.resources.configuration.locales[0]
            val month = monthFor(context, widgetId, YearMonth.from(today))
            val body = KalWidgets.nativeMonth(
                KalWidgets.dbPath(context),
                month.year,
                month.monthValue,
                FIRST_DAY_OF_WEEK,
            )
            val weeks = parseMonth(body)

            val root = RemoteViews(context.packageName, R.layout.kal_widget_month)
            root.setTextViewText(R.id.kal_widget_month_title, MonthWidgetModel.title(month, locale))
            root.setOnClickPendingIntent(
                R.id.kal_widget_month_prev,
                monthAction(context, widgetId, ACTION_PREV, SLOT_PREV),
            )
            root.setOnClickPendingIntent(
                R.id.kal_widget_month_next,
                monthAction(context, widgetId, ACTION_NEXT, SLOT_NEXT),
            )
            root.setOnClickPendingIntent(
                R.id.kal_widget_month_add,
                addToday(context, widgetId, SLOT_ADD),
            )

            val fg = context.getColor(R.color.kal_widget_month_fg)
            val muted = context.getColor(R.color.kal_widget_month_muted)
            val outside = context.getColor(R.color.kal_widget_month_outside)
            val onAccent = context.getColor(R.color.kal_widget_month_on_accent)

            for (w in 0 until 6) {
                val week = weeks.getOrNull(w)
                if (week == null) {
                    root.setViewVisibility(ROW_IDS[w], View.GONE)
                    if (w > 0) {
                        root.setViewVisibility(SEPARATOR_IDS[w - 1], View.GONE)
                    }
                    continue
                }

                val show = MonthWidgetModel.showsWeek(w, week)
                root.setViewVisibility(ROW_IDS[w], if (show) View.VISIBLE else View.GONE)
                if (w > 0) {
                    root.setViewVisibility(
                        SEPARATOR_IDS[w - 1],
                        if (show) View.VISIBLE else View.GONE,
                    )
                }
                if (!show) continue

                for (d in 0 until 7) {
                    val cell = w * 7 + d
                    val day = week.getOrNull(d)
                    val textId = DAY_TEXT_IDS[cell]
                    val date = day?.let { parseDate(it.date) }
                    if (day == null || date == null) {
                        root.setTextViewText(textId, "")
                        root.setTextColor(textId, muted)
                        root.setViewVisibility(CIRCLE_IDS[cell], View.GONE)
                        for (k in 0 until CHIPS_PER_DAY) {
                            root.setViewVisibility(CHIP_IDS[cell * CHIPS_PER_DAY + k], View.GONE)
                        }
                        continue
                    }

                    root.setTextViewText(textId, date.dayOfMonth.toString())
                    val isToday = date == today
                    root.setTextColor(
                        textId,
                        when {
                            isToday -> onAccent
                            !day.inMonth -> outside
                            else -> fg
                        },
                    )
                    root.setViewVisibility(
                        CIRCLE_IDS[cell],
                        if (isToday) View.VISIBLE else View.GONE,
                    )
                    root.setOnClickPendingIntent(
                        CELL_IDS[cell],
                        addEvent(context, date, widgetId, SLOT_FIRST_DAY + cell),
                    )
                    root.setContentDescription(
                        CELL_IDS[cell],
                        context.getString(
                            R.string.kal_widget_add_event_on,
                            MonthWidgetModel.dayDescription(date, locale),
                        ),
                    )

                    for (k in 0 until CHIPS_PER_DAY) {
                        val chipId = CHIP_IDS[cell * CHIPS_PER_DAY + k]
                        val item = day.items.getOrNull(k)
                        if (item == null) {
                            root.setViewVisibility(chipId, View.GONE)
                        } else {
                            root.setViewVisibility(chipId, View.VISIBLE)
                            root.setInt(
                                chipId,
                                "setBackgroundResource",
                                chipDrawable(item.color),
                            )
                            root.setTextViewText(
                                chipId,
                                MonthWidgetModel.chipLabel(item.title, item.time),
                            )
                        }
                    }
                }
            }

            return root
        }

        /** Nearest of the eight rounded chip drawables, so calendars keep their colour. */
        private fun chipDrawable(color: String): Int {
            val rgb = runCatching { Color.parseColor(color) and 0xFFFFFF }.getOrNull()
                ?: return CHIP_DRAWABLES[0]
            var best = 0
            var bestDistance = Int.MAX_VALUE
            for (i in CHIP_RGB.indices) {
                val palette = CHIP_RGB[i]
                val dr = (rgb shr 16 and 0xFF) - (palette shr 16 and 0xFF)
                val dg = (rgb shr 8 and 0xFF) - (palette shr 8 and 0xFF)
                val db = (rgb and 0xFF) - (palette and 0xFF)
                val distance = dr * dr + dg * dg + db * db
                if (distance < bestDistance) {
                    bestDistance = distance
                    best = i
                }
            }
            return CHIP_DRAWABLES[best]
        }

        private fun addEvent(
            context: Context,
            date: LocalDate,
            widgetId: Int,
            slot: Int,
        ): PendingIntent = activityIntent(context, widgetId, slot) {
            putExtra(WidgetIntents.EXTRA_ADD_DATE, date.toString())
        }

        /**
         * The + button asks the app for *today at click time*, not at the last
         * widget refresh, so it still targets the right day just after midnight.
         */
        private fun addToday(
            context: Context,
            widgetId: Int,
            slot: Int,
        ): PendingIntent = activityIntent(context, widgetId, slot) {
            putExtra(WidgetIntents.EXTRA_ADD_TODAY, true)
        }

        private fun activityIntent(
            context: Context,
            widgetId: Int,
            slot: Int,
            configure: Intent.() -> Unit,
        ): PendingIntent {
            val intent = Intent(context, MainActivity::class.java).apply {
                action = Intent.ACTION_MAIN
                addCategory(Intent.CATEGORY_LAUNCHER)
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
                configure()
            }
            return PendingIntent.getActivity(
                context,
                MonthWidgetModel.requestCode(widgetId, slot),
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }

        private fun monthAction(
            context: Context,
            widgetId: Int,
            action: String,
            slot: Int,
        ): PendingIntent {
            val intent = Intent(context, MonthWidgetProvider::class.java)
                .setAction(action)
                .putExtra(EXTRA_WIDGET_ID, widgetId)
            return PendingIntent.getBroadcast(
                context,
                MonthWidgetModel.requestCode(widgetId, slot),
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }

        fun monthWidget(context: Context): ComponentName =
            ComponentName(context, MonthWidgetProvider::class.java)

        private val ROW_IDS = intArrayOf(
            R.id.kal_widget_month_row1,
            R.id.kal_widget_month_row2,
            R.id.kal_widget_month_row3,
            R.id.kal_widget_month_row4,
            R.id.kal_widget_month_row5,
            R.id.kal_widget_month_row6,
        )

        private val SEPARATOR_IDS = intArrayOf(
            R.id.kal_widget_month_sep2,
            R.id.kal_widget_month_sep3,
            R.id.kal_widget_month_sep4,
            R.id.kal_widget_month_sep5,
            R.id.kal_widget_month_sep6,
        )

        private val CELL_IDS = intArrayOf(
            R.id.day1,
            R.id.day2,
            R.id.day3,
            R.id.day4,
            R.id.day5,
            R.id.day6,
            R.id.day7,
            R.id.day8,
            R.id.day9,
            R.id.day10,
            R.id.day11,
            R.id.day12,
            R.id.day13,
            R.id.day14,
            R.id.day15,
            R.id.day16,
            R.id.day17,
            R.id.day18,
            R.id.day19,
            R.id.day20,
            R.id.day21,
            R.id.day22,
            R.id.day23,
            R.id.day24,
            R.id.day25,
            R.id.day26,
            R.id.day27,
            R.id.day28,
            R.id.day29,
            R.id.day30,
            R.id.day31,
            R.id.day32,
            R.id.day33,
            R.id.day34,
            R.id.day35,
            R.id.day36,
            R.id.day37,
            R.id.day38,
            R.id.day39,
            R.id.day40,
            R.id.day41,
            R.id.day42,
        )

        private val DAY_TEXT_IDS = intArrayOf(
            R.id.day1t,
            R.id.day2t,
            R.id.day3t,
            R.id.day4t,
            R.id.day5t,
            R.id.day6t,
            R.id.day7t,
            R.id.day8t,
            R.id.day9t,
            R.id.day10t,
            R.id.day11t,
            R.id.day12t,
            R.id.day13t,
            R.id.day14t,
            R.id.day15t,
            R.id.day16t,
            R.id.day17t,
            R.id.day18t,
            R.id.day19t,
            R.id.day20t,
            R.id.day21t,
            R.id.day22t,
            R.id.day23t,
            R.id.day24t,
            R.id.day25t,
            R.id.day26t,
            R.id.day27t,
            R.id.day28t,
            R.id.day29t,
            R.id.day30t,
            R.id.day31t,
            R.id.day32t,
            R.id.day33t,
            R.id.day34t,
            R.id.day35t,
            R.id.day36t,
            R.id.day37t,
            R.id.day38t,
            R.id.day39t,
            R.id.day40t,
            R.id.day41t,
            R.id.day42t,
        )

        private val CIRCLE_IDS = intArrayOf(
            R.id.day1bg,
            R.id.day2bg,
            R.id.day3bg,
            R.id.day4bg,
            R.id.day5bg,
            R.id.day6bg,
            R.id.day7bg,
            R.id.day8bg,
            R.id.day9bg,
            R.id.day10bg,
            R.id.day11bg,
            R.id.day12bg,
            R.id.day13bg,
            R.id.day14bg,
            R.id.day15bg,
            R.id.day16bg,
            R.id.day17bg,
            R.id.day18bg,
            R.id.day19bg,
            R.id.day20bg,
            R.id.day21bg,
            R.id.day22bg,
            R.id.day23bg,
            R.id.day24bg,
            R.id.day25bg,
            R.id.day26bg,
            R.id.day27bg,
            R.id.day28bg,
            R.id.day29bg,
            R.id.day30bg,
            R.id.day31bg,
            R.id.day32bg,
            R.id.day33bg,
            R.id.day34bg,
            R.id.day35bg,
            R.id.day36bg,
            R.id.day37bg,
            R.id.day38bg,
            R.id.day39bg,
            R.id.day40bg,
            R.id.day41bg,
            R.id.day42bg,
        )

        private val CHIP_DRAWABLES = intArrayOf(
            R.drawable.kal_widget_chip_bg,
            R.drawable.kal_widget_chip_red,
            R.drawable.kal_widget_chip_green,
            R.drawable.kal_widget_chip_orange,
            R.drawable.kal_widget_chip_purple,
            R.drawable.kal_widget_chip_teal,
            R.drawable.kal_widget_chip_pink,
            R.drawable.kal_widget_chip_gray,
        )

        private val CHIP_RGB = intArrayOf(
            0x3366CC,
            0xC0392B,
            0x2E7D32,
            0xB26A00,
            0x6A3D9A,
            0x00796B,
            0xAD1457,
            0x5F6368,
        )

        private val CHIP_IDS = intArrayOf(
            R.id.day1chip1, R.id.day1chip2, R.id.day2chip1, R.id.day2chip2,
            R.id.day3chip1, R.id.day3chip2, R.id.day4chip1, R.id.day4chip2,
            R.id.day5chip1, R.id.day5chip2, R.id.day6chip1, R.id.day6chip2,
            R.id.day7chip1, R.id.day7chip2, R.id.day8chip1, R.id.day8chip2,
            R.id.day9chip1, R.id.day9chip2, R.id.day10chip1, R.id.day10chip2,
            R.id.day11chip1, R.id.day11chip2, R.id.day12chip1, R.id.day12chip2,
            R.id.day13chip1, R.id.day13chip2, R.id.day14chip1, R.id.day14chip2,
            R.id.day15chip1, R.id.day15chip2, R.id.day16chip1, R.id.day16chip2,
            R.id.day17chip1, R.id.day17chip2, R.id.day18chip1, R.id.day18chip2,
            R.id.day19chip1, R.id.day19chip2, R.id.day20chip1, R.id.day20chip2,
            R.id.day21chip1, R.id.day21chip2, R.id.day22chip1, R.id.day22chip2,
            R.id.day23chip1, R.id.day23chip2, R.id.day24chip1, R.id.day24chip2,
            R.id.day25chip1, R.id.day25chip2, R.id.day26chip1, R.id.day26chip2,
            R.id.day27chip1, R.id.day27chip2, R.id.day28chip1, R.id.day28chip2,
            R.id.day29chip1, R.id.day29chip2, R.id.day30chip1, R.id.day30chip2,
            R.id.day31chip1, R.id.day31chip2, R.id.day32chip1, R.id.day32chip2,
            R.id.day33chip1, R.id.day33chip2, R.id.day34chip1, R.id.day34chip2,
            R.id.day35chip1, R.id.day35chip2, R.id.day36chip1, R.id.day36chip2,
            R.id.day37chip1, R.id.day37chip2, R.id.day38chip1, R.id.day38chip2,
            R.id.day39chip1, R.id.day39chip2, R.id.day40chip1, R.id.day40chip2,
            R.id.day41chip1, R.id.day41chip2, R.id.day42chip1, R.id.day42chip2,
        )
    }

    override fun onUpdate(
        context: Context,
        manager: AppWidgetManager,
        widgetIds: IntArray,
    ) {
        update(context, manager, widgetIds)
    }

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        val delta = when (intent.action) {
            ACTION_PREV -> -1
            ACTION_NEXT -> 1
            else -> return
        }
        val widgetId = intent.getIntExtra(EXTRA_WIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) return

        val today = YearMonth.now()
        val shown = monthFor(context, widgetId, today)
        val pin = MonthWidgetModel.pinAfter(shown, delta, today)
        val editor = widgetPrefs(context).edit()
        if (pin == null) {
            editor.remove(pinnedKey(widgetId))
        } else {
            editor.putString(pinnedKey(widgetId), MonthWidgetModel.monthKey(pin))
        }
        editor.apply()
        AppWidgetManager.getInstance(context).updateAppWidget(widgetId, buildViews(context, widgetId))
    }

    override fun onDeleted(context: Context, widgetIds: IntArray) {
        val editor = widgetPrefs(context).edit()
        for (id in widgetIds) {
            editor.remove(pinnedKey(id))
            editor.remove(legacyOffsetKey(id))
        }
        editor.apply()
    }

    override fun onAppWidgetOptionsChanged(
        context: Context,
        manager: AppWidgetManager,
        appWidgetId: Int,
        newOptions: Bundle,
    ) {
        manager.updateAppWidget(appWidgetId, buildViews(context, appWidgetId))
    }
}
