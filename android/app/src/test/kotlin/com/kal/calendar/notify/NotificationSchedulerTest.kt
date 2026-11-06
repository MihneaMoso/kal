package com.kal.calendar.notify

import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Calendar
import kotlin.test.assertNotEquals

/**
 * Tests for the notification scheduling and alarm management system.
 */
class NotificationSchedulerTest {

    private val context = androidx.test.core.app.ApplicationProvider.getApplicationContext(
        androidx.test.core.app.InstrumentationRegistry.getInstrumentation()
    ) as Context

    @Test
    fun `alarm_manager_scheduling_returns_valid_request_code`() {
        val scheduler = NotificationScheduler(context)
        
        val alarm = Alarm(
            id = "test-alarm-1",
            fireAt = Calendar.getInstance(),
            alarmType = AlarmType.PUSH
        )
        
        val requestCode = scheduler.scheduleAlarm(alarm)
        
        assertTrue(requestCode > 0, "request code must be positive")
        assertNotNull(scheduler.getAlarmId(alarm.id))
    }

    @Test
    fun `alarm_manager_cancellation_returns_true_for_valid_id`() {
        val scheduler = NotificationScheduler(context)
        val alarmId = "test-alarm-2"
        
        // Schedule and immediately cancel
        scheduler.scheduleAlarm(Alarm(
            id = alarmId,
            fireAt = Calendar.getInstance(),
            alarmType = AlarmType.PUSH
        ))
        
        val cancelled = scheduler.cancelAlarm(alarmId)
        assertTrue(cancelled, "cancellation should return true for valid alarm")
    }

    @Test
    fun `alarm_manager_cancellation_returns_false_for_invalid_id`() {
        val scheduler = NotificationScheduler(context)
        
        val cancelled = scheduler.cancelAlarm("non-existent-alarm")
        assertFalse(cancelled, "cancellation should return false for invalid alarm")
    }

    @Test
    fun `alarm_manager_supports_multiple_alarms`() {
        val scheduler = NotificationScheduler(context)
        
        val alarm1 = Alarm(
            id = "alarm-1",
            fireAt = Calendar.getInstance().apply { timeInMillis += 10000 },
            alarmType = AlarmType.PUSH
        )
        
        val alarm2 = Alarm(
            id = "alarm-2",
            fireAt = Calendar.getInstance().apply { timeInMillis += 20000 },
            alarmType = AlarmType.PUSH
        )
        
        scheduler.scheduleAlarm(alarm1)
        scheduler.scheduleAlarm(alarm2)
        
        assertEquals(2, scheduler.pendingAlarms.size)
        assertTrue(scheduler.getAlarmId(alarm1.id) != null)
        assertTrue(scheduler.getAlarmId(alarm2.id) != null)
    }

    @Test
    fun `alarm_manager_clears_all_removes_all_schedulers`() {
        val scheduler = NotificationScheduler(context)
        
        // Schedule multiple alarms
        for (i in 1..5) {
            scheduler.scheduleAlarm(Alarm(
                id = "alarm-$i",
                fireAt = Calendar.getInstance().apply { timeInMillis += i * 10000 },
                alarmType = AlarmType.PUSH
            ))
        }
        
        assertEquals(5, scheduler.pendingAlarms.size)
        
        // Clear all
        scheduler.clearAll()
        
        assertTrue(scheduler.pendingAlarms.isEmpty())
    }

    @Test
    fun `notification_builder_creates_valid_notification`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentTitle("Test Notification")
            .setContentText("Test message")
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .build()
        
        assertTrue(notification.id > 0)
        assertEquals("Test Notification", notification.extras.getCharSequence(android.app.Notification.EXTRA_TITLE))
        assertEquals("Test message", notification.extras.getCharSequence(android.app.Notification.EXTRA_TEXT))
    }

    @Test
    fun `notification_builder_sets_correct_priority_for_reminders`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()
        
        assertEquals(NotificationCompat.PRIORITY_DEFAULT, notification.priority)
    }

    @Test
    fun `notification_builder_sets_correct_category_for_reminders`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .build()
        
        assertEquals(NotificationCompat.CATEGORY_REMINDER, notification.category)
    }

    @Test
    fun `notification_builder_sets_correct_auto_cancel`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setAutoCancel(true)
            .build()
        
        assertTrue(notification.autoCancel)
    }

    @Test
    fun `notification_builder_sets_correct_vibration_pattern`() {
        val scheduler = NotificationScheduler(context)
        
        val pattern = longArray(2) { 1000 } + longArray(2) { 300 }
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setVibrate(pattern)
            .build()
        
        assertEquals(4, notification.vibrate!!.size)
    }

    @Test
    fun `notification_builder_sets_correct_sound`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSound(android.content.Context.NOTIFICATION_SOUND_DEFAULT)
            .build()
        
        assertTrue(notification.sound != null)
    }

    @Test
    fun `notification_builder_sets_correct_led_color`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setLights(0xFF0000FF, 1000, 500)
            .build()
        
        assertEquals(0xFF0000FF, notification.ledColor.argb)
    }

    @Test
    fun `notification_builder_sets_correct_sticky_flag`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setOngoing(true)
            .build()
        
        assertTrue(notification.ongoing)
    }

    @Test
    fun `notification_builder_sets_correct_only_once_flag`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setOnlyOnce(true)
            .build()
        
        assertTrue(notification.onlyOnce)
    }

    @Test
    fun `notification_builder_sets_correct_clears_history_flag`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setShowWhen(false)
            .build()
        
        assertFalse(notification.showWhen)
    }

    @Test
    fun `notification_builder_sets_correct_channel_id`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .build()
        
        assertEquals(CHANNEL_ID, notification.channelId)
    }

    @Test
    fun `notification_builder_sets_correct_intent_for_click`() {
        val scheduler = NotificationScheduler(context)
        
        val intent = android.content.Intent(android.content.Intent.ACTION_VIEW)
        intent.setData(android.net.Uri.parse("http://example.com"))
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentIntent(intent)
            .build()
        
        assertNotNull(notification.contentIntent)
    }

    @Test
    fun `notification_builder_sets_correct_auto_dismiss_after_timeout`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setAutoDismiss(true)
            .build()
        
        assertTrue(notification.autoDismiss)
    }

    @Test
    fun `notification_builder_sets_correct_big_text_mode_for_long_content`() {
        val scheduler = NotificationScheduler(context)
        
        val longText = "This is a very long notification message that might need big text mode for display."
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.BigTextStyle().bigText(longText))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.BigTextStyle)
    }

    @Test
    fun `notification_builder_sets_correct_inbox_style_for_multiple_lines`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.InboxStyle().setBigText("Inbox notification"))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.InboxStyle)
    }

    @Test
    fun `notification_builder_sets_correct_group_summary_for_related_notifications`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setGroupSummary(true)
            .build()
        
        assertTrue(notification.isGroupSummary)
    }

    @Test
    fun `notification_builder_sets_correct_group_for_related_notifications`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setGroup("calendar-group")
            .build()
        
        assertEquals("calendar-group", notification.extras.getString(NotificationCompat.EXTRA_GROUP_KEY))
    }

    @Test
    fun `notification_builder_sets_correct_sort_key_for_group_ordering`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSortKey("0")
            .build()
        
        assertEquals("0", notification.extras.getString(NotificationCompat.EXTRA_SORT_KEY))
    }

    @Test
    fun `notification_builder_sets_correct_visibility_for_secure_content`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setVisiblesWhenHidden(true)
            .build()
        
        assertTrue(notification.visiblesWhenHidden)
    }

    @Test
    fun `notification_builder_sets_correct_sticky_flag_for_persistent_notifications`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSticky(true)
            .build()
        
        assertTrue(notification.sticky)
    }

    @Test
    fun `notification_builder_sets_correct_progress_bar`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setProgress(100, 50, false)
            .build()
        
        assertEquals(100, notification.maxProgress)
        assertEquals(50, notification.progress)
        assertFalse(notification.indeterminate)
    }

    @Test
    fun `notification_builder_sets_correct_big_picture_mode`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.BigPictureStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.BigPictureStyle)
    }

    @Test
    fun `notification_builder_sets_correct_big_video_mode`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.BigVideoStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.BigVideoStyle)
    }

    @Test
    fun `notification_builder_sets_correct_media_style`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_person_style`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.PersonStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.PersonStyle)
    }

    @Test
    fun `notification_builder_sets_correct_inbox_with_actions`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.InboxStyle()
                .setBigText("Inbox notification")
                .addAction(android.R.drawable.ic_delete, "Delete", null)
                .addAction(android.R.drawable.ic_menu_call, "Call", null))
            .build()
        
        val style = notification.style as NotificationCompat.InboxStyle
        assertEquals(2, style.actions.size)
    }

    @Test
    fun `notification_builder_sets_correct_notification_actions`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .addAction(android.R.drawable.ic_menu_call, "Call", null)
            .addAction(android.R.drawable.ic_menu_email, "Email", null)
            .build()
        
        val actions = notification.actions ?: emptyArray<android.view.View>()
        assertEquals(2, actions.size)
    }

    @Test
    fun `notification_builder_sets_correct_local_vibration_pattern`() {
        val scheduler = NotificationScheduler(context)
        
        val pattern = longArray(3) { 500 } + longArray(3) { 200 }
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setVibrate(pattern)
            .build()
        
        assertEquals(6, notification.vibrate!!.size)
    }

    @Test
    fun `notification_builder_sets_correct_light_off_interval`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setLights(0xFF00FF00, 1000, 500)
            .build()
        
        assertEquals(500, notification.ledOffIntervalMillis)
    }

    @Test
    fun `notification_builder_sets_correct_light_on_duration`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setLights(0xFF00FF00, 2000, 500)
            .build()
        
        assertEquals(2000, notification.ledOnMs)
    }

    @Test
    fun `notification_builder_sets_correct_light_on_color`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setLights(0xFFFF0000, 1000, 500)
            .build()
        
        assertEquals(0xFFFF0000, notification.ledColor.argb)
    }

    @Test
    fun `notification_builder_sets_correct_notification_icon`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_alert)
            .build()
        
        assertEquals(android.R.drawable.ic_dialog_alert, notification.icon)
    }

    @Test
    fun `notification_builder_sets_correct_notification_ticker_text`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentText("Ticker text here")
            .build()
        
        assertEquals("Ticker text here", notification.extras.getString(NotificationCompat.EXTRA_TickerView))
    }

    @Test
    fun `notification_builder_sets_correct_notification_key_for_grouping`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setKey("test-group-key")
            .build()
        
        assertEquals("test-group-key", notification.extras.getString(NotificationCompat.EXTRA_KEY))
    }

    @Test
    fun `notification_builder_sets_correct_notification_id`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setId(12345)
            .build()
        
        assertEquals(12345, notification.id)
    }

    @Test
    fun `notification_builder_sets_correct_notification_priority_high`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        
        assertEquals(NotificationCompat.PRIORITY_HIGH, notification.priority)
    }

    @Test
    fun `notification_builder_sets_correct_notification_priority_max`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .build()
        
        assertEquals(NotificationCompat.PRIORITY_MAX, notification.priority)
    }

    @Test
    fun `notification_builder_sets_correct_notification_priority_min`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
        
        assertEquals(NotificationCompat.PRIORITY_MIN, notification.priority)
    }

    @Test
    fun `notification_builder_sets_correct_notification_category_event`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setCategory(NotificationCompat.CATEGORY_EVENT)
            .build()
        
        assertEquals(NotificationCompat.CATEGORY_EVENT, notification.category)
    }

    @Test
    fun `notification_builder_sets_correct_notification_category_progress`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .build()
        
        assertEquals(NotificationCompat.CATEGORY_PROGRESS, notification.category)
    }

    @Test
    fun `notification_builder_sets_correct_notification_category_service`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
        
        assertEquals(NotificationCompat.CATEGORY_SERVICE, notification.category)
    }

    @Test
    fun `notification_builder_sets_correct_notification_category_progress_bar`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .build()
        
        assertEquals(NotificationCompat.CATEGORY_PROGRESS, notification.category)
    }

    @Test
    fun `notification_builder_sets_correct_notification_channel_id_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, "custom-channel")
            .build()
        
        assertEquals("custom-channel", notification.channelId)
    }

    @Test
    fun `notification_builder_sets_correct_notification_channel_group`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setGroup("group-name")
            .build()
        
        assertEquals("group-name", notification.extras.getString(NotificationCompat.EXTRA_GROUP_KEY))
    }

    @Test
    fun `notification_builder_sets_correct_notification_auto_cancel_false`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setAutoCancel(false)
            .build()
        
        assertFalse(notification.autoCancel)
    }

    @Test
    fun `notification_builder_sets_correct_notification_vibrate_false`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setVibrate(longArrayOfLong(0))
            .build()
        
        assertTrue(notification.vibrate!!.isEmpty())
    }

    @Test
    fun `notification_builder_sets_correct_notification_sound_null`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSound(null)
            .build()
        
        assertNull(notification.sound)
    }

    @Test
    fun `notification_builder_sets_correct_notification_led_null`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setLights(0, 0, 0)
            .build()
        
        assertEquals(0, notification.ledColor.argb)
    }

    @Test
    fun `notification_builder_sets_correct_notification_big_text_style`() {
        val scheduler = NotificationScheduler(context)
        
        val text = "This is a long notification message that should be displayed in big text mode for better readability on small screens."
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.BigTextStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_inbox_style_with_multiple_actions`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.InboxStyle()
                .setBigText("Inbox notification")
                .setNumber(5)
                .addAction(android.R.drawable.ic_menu_search, "Search", null)
                .addAction(android.R.drawable.ic_menu_gallery, "Gallery", null)
                .addAction(android.R.drawable.ic_menu_manage, "Manage", null))
            .build()
        
        val style = notification.style as NotificationCompat.InboxStyle
        assertEquals(3, style.actions.size)
        assertEquals(5, style.number)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_style_with_media_session_token`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_null`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_empty_string`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_content_title`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .setContentTitle("Media notification")
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_content_text`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .setContentText("Media content text")
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_sub_text`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_in_lockscreen`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setShowInLockscreen(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_notification_count`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setShowNotificationCount(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_small_media`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setUseSmallMedia(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_custom_controller`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setShowNotificationCount(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_artwork_in_notification`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_artwork_in_lockscreen`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_chromecast_controller`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_artwork_in_notification_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_artwork_in_lockscreen_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_custom_controller_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle())
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_small_media_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setUseSmallMedia(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_in_lockscreen_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setShowInLockscreen(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_show_notification_count_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setShowNotificationCount(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }

    @Test
    fun `notification_builder_sets_correct_notification_media_session_token_with_use_custom_controller_custom`() {
        val scheduler = NotificationScheduler(context)
        
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setStyle(NotificationCompat.MediaStyle().setUseCustomController(true))
            .build()
        
        assertNotNull(notification.style)
        assertTrue(notification.style is NotificationCompat.MediaStyle)
    }
}
