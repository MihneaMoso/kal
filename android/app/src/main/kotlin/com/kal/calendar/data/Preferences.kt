package com.kal.calendar.data

import org.json.JSONObject

/**
 * Device-local preferences (`§5.7`), stored in the `settings` table exactly as
 * the Dioxus app stored them: one `preferences` JSON object, plus the theme
 * under its own `theme` key.
 *
 * The split is deliberate and preserved here — toggling the theme must not
 * rewrite the object the calendar views watch — so [from] reads the dedicated
 * key first and falls back to the copy inside the object.
 */
data class Preferences(
    val theme: String = "dark",
    val time24h: Boolean = true,
    val firstDayMonday: Boolean = true,
    val defaultView: String = "Month",
    val autoCheckUpdates: Boolean = true,
) {
    val isDark: Boolean get() = theme != "light"

    /** The `preferences` object, in the field order the core model declares. */
    fun toJson(): String = JSONObject()
        .put("theme", theme)
        .put("time_24h", time24h)
        .put("first_day_monday", firstDayMonday)
        .put("default_view", defaultView)
        .put("auto_check_updates", autoCheckUpdates)
        .toString()

    companion object {
        val DEFAULT = Preferences()

        /**
         * Read the two stored values. [preferencesJson] is the object text (null
         * when unset or unreadable) and [themeValue] the stored string from the
         * dedicated key (which wins). A legacy JSON-quoted theme (`"dark"`,
         * literal quotes) is unwrapped, and anything unrecognised normalises to
         * `dark`, matching the Dioxus loader.
         */
        fun from(preferencesJson: String?, themeValue: String?): Preferences {
            val obj = runCatching { preferencesJson?.let(::JSONObject) }.getOrNull()
            val fromJson = obj?.optString("theme")?.takeIf { it.isNotEmpty() }
            val stored = themeValue?.trim('"') ?: fromJson
            val theme = if (stored == "dark" || stored == "light") stored else DEFAULT.theme

            return Preferences(
                theme = theme,
                time24h = obj?.optBoolean("time_24h", DEFAULT.time24h) ?: DEFAULT.time24h,
                firstDayMonday = obj?.optBoolean("first_day_monday", DEFAULT.firstDayMonday)
                    ?: DEFAULT.firstDayMonday,
                defaultView = obj?.optString("default_view")?.takeIf { it.isNotEmpty() }
                    ?: DEFAULT.defaultView,
                autoCheckUpdates = obj?.optBoolean("auto_check_updates", DEFAULT.autoCheckUpdates)
                    ?: DEFAULT.autoCheckUpdates,
            )
        }
    }
}
