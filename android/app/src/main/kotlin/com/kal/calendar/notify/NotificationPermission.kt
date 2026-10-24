package com.kal.calendar.notify

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build

/**
 * Ask for POST_NOTIFICATIONS once per install (Android 13+).
 *
 * Asked at startup rather than from the editor: a reminder can only be delivered
 * once the grant exists, and this also covers reminders that arrive by import or
 * sync rather than a fresh save. The remembered flag stops the prompt from
 * nagging after a denial; the user can still grant it from system settings.
 */
fun askForNotificationPermissionOnce(activity: Activity) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val granted = activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
        PackageManager.PERMISSION_GRANTED
    if (granted) return

    val prefs = activity.getSharedPreferences(REMINDER_PREFS, Activity.MODE_PRIVATE)
    if (prefs.getBoolean(PREF_PERMISSION_ASKED, false)) return
    prefs.edit().putBoolean(PREF_PERMISSION_ASKED, true).apply()

    activity.requestPermissions(
        arrayOf(Manifest.permission.POST_NOTIFICATIONS),
        REMINDER_PERMISSION_REQUEST,
    )
}
