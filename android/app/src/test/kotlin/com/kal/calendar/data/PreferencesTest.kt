package com.kal.calendar.data

import com.kal.calendar.bridge.Payloads
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The `preferences` object and the separate `theme` key, read the way the
 * Dioxus loader read them (including the legacy quoting it tolerated).
 */
class PreferencesTest {

    @Test
    fun `defaults match the original`() {
        val prefs = Preferences.DEFAULT
        assertEquals("dark", prefs.theme)
        assertTrue(prefs.time24h)
        assertTrue(prefs.firstDayMonday)
        assertEquals("Month", prefs.defaultView)
        assertTrue(prefs.autoCheckUpdates)
        assertTrue(prefs.isDark)
    }

    @Test
    fun `an absent or unreadable object falls back to the defaults`() {
        assertEquals(Preferences.DEFAULT, Preferences.from(null, null))
        // `kal_get_setting` sends the literal `null` for an absent key.
        assertEquals(Preferences.DEFAULT, Preferences.from("null", null))
        assertFalse(Preferences.from("null", "light").isDark)
    }

    @Test
    fun `the stored object round-trips through its own json`() {
        val prefs = Preferences(
            theme = "light",
            time24h = false,
            firstDayMonday = false,
            defaultView = "Week",
            autoCheckUpdates = false,
        )

        // What the bridge hands back for this object (`kal_get_setting` reply).
        val reply = JSONObject().put("v", prefs.toJson()).getString("v")
        assertEquals(prefs, Preferences.from(Payloads.decodeText(reply), null))
    }

    @Test
    fun `the dedicated theme key wins over the copy in the object`() {
        val prefs = Preferences(theme = "dark")
        assertEquals("light", Preferences.from(prefs.toJson(), "light").theme)
        assertEquals("dark", Preferences.from(Preferences(theme = "light").toJson(), "dark").theme)
    }

    @Test
    fun `the object's own theme is used when the key is absent`() {
        assertEquals("light", Preferences.from(Preferences(theme = "light").toJson(), null).theme)
    }

    @Test
    fun `a legacy quoted theme is unwrapped and garbage normalises to dark`() {
        assertEquals("light", Preferences.from(null, "\"light\"").theme)
        assertEquals("dark", Preferences.from(null, "purple").theme)
        assertEquals("dark", Preferences.from(null, "").theme)
    }

    @Test
    fun `unknown fields are ignored and missing ones default`() {
        val json = """{"theme":"light","unknown":"x"}"""

        val prefs = Preferences.from(json, null)

        assertEquals("light", prefs.theme)
        assertTrue(prefs.time24h)
        assertTrue(prefs.firstDayMonday)
    }

    @Test
    fun `serialisation carries every field the core model declares`() {
        val obj = JSONObject(Preferences().toJson())

        assertEquals(
            listOf("auto_check_updates", "default_view", "first_day_monday", "theme", "time_24h"),
            obj.keys().asSequence().sorted().toList(),
        )
    }
}
