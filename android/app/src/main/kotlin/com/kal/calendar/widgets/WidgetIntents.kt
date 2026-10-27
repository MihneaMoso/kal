package com.kal.calendar.widgets

/** Intent extras shared by the home-screen widgets and [com.kal.calendar.MainActivity]. */
object WidgetIntents {
    /**
     * ISO local date ("YYYY-MM-DD") for which the app should open the new-event
     * editor. Set by the month widget's day cells.
     */
    const val EXTRA_ADD_DATE = "kal_widget_add_date"

    /**
     * Set by the month widget's + button: open the new-event editor on today,
     * resolved when the app handles the intent rather than when the widget was
     * last refreshed.
     */
    const val EXTRA_ADD_TODAY = "kal_widget_add_today"
}
