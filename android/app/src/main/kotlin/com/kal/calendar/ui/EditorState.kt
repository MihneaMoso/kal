package com.kal.calendar.ui

import com.kal.calendar.bridge.EditScope
import com.kal.calendar.bridge.EditorForm
import com.kal.calendar.bridge.ItemKind
import com.kal.calendar.bridge.ItemModel
import java.time.Duration
import java.time.LocalDate
import java.time.LocalTime
import java.time.OffsetDateTime

/** Default colour of a new item — the app's blue, as the Dioxus editor used. */
const val DEFAULT_ITEM_COLOR = "#3366cc"

/** The preset swatches of the editor's colour picker, in the original order. */
val COLOR_PRESETS = listOf(
    "#3366cc",
    "#6c99f0",
    "#e91e63",
    "#e67e22",
    "#27ae60",
    "#8e44ad",
    "#16a085",
    "#b0bec5",
)

/** Reminder presets, in minutes before the start. */
val REMINDER_PRESETS = listOf(10L, 30L, 60L, 1440L, 10080L)

/** `preset_label`: the caption of a reminder preset button. */
fun reminderPresetLabel(minutes: Long): String = when (minutes) {
    10L -> "10 min"
    30L -> "30 min"
    60L -> "1 hour"
    1440L -> "1 day"
    10080L -> "1 week"
    else -> "custom"
}

/**
 * The editor's simplified repeat picker. Upstream never exposed custom RRULE
 * editing either, so neither does this.
 */
enum class RepeatPreset(val label: String, val rrule: String?) {
    NONE("Doesn't repeat", null),
    DAILY("Daily", "FREQ=DAILY"),
    WEEKLY("Weekly", "FREQ=WEEKLY"),
    MONTHLY("Monthly", "FREQ=MONTHLY"),
    YEARLY("Annually", "FREQ=YEARLY"),
    ;

    companion object {
        /**
         * `rrule_to_preset`: a prefix match, so a rule with BYDAY/UNTIL parts
         * still reads as its frequency; anything unknown is "doesn't repeat".
         */
        fun fromRrule(rule: String?): RepeatPreset {
            val raw = rule.orEmpty()
            return when {
                raw.startsWith("FREQ=DAILY") -> DAILY
                raw.startsWith("FREQ=WEEKLY") -> WEEKLY
                raw.startsWith("FREQ=MONTHLY") -> MONTHLY
                raw.startsWith("FREQ=YEARLY") -> YEARLY
                else -> NONE
            }
        }
    }
}

/**
 * The editor's form state: the fields the dialog shows, detached from Compose and
 * from the database so the defaults, the prefill and the save request are all
 * plain functions the JVM tests pin.
 *
 * `startTime == null` means the native parser's empty time (midnight), and
 * `endTime == null` means "no end" — which is what a task and an all-day item
 * have. `color == null` means "use the owning calendar's colour".
 */
data class EditorFields(
    /** Null while creating. */
    val id: String?,
    /** The clicked occurrence's instant, for a scoped recurring edit. */
    val occurrenceStart: OffsetDateTime?,
    val kind: ItemKind,
    val title: String,
    val date: LocalDate,
    val startTime: LocalTime?,
    val endTime: LocalTime?,
    val allDay: Boolean,
    val calendarId: String?,
    val location: String,
    val person: String,
    val repeat: RepeatPreset,
    val reminders: List<Long>,
    val customReminder: String,
    val color: String?,
    /** True when the item being edited is already a recurring series. */
    val recurring: Boolean,
    /**
     * True when this row is one occurrence detached from a series by "This
     * event": it is still part of that series (the scope choice stays), but it
     * is the row itself, so there is no later part to split with "…and
     * following".
     */
    val exception: Boolean = false,
) {
    /** `modal_title`: `Edit event`, `New birthday`, … */
    val heading: String
        get() = if (id == null) "New ${kindNoun()}" else "Edit ${kindNoun()}"

    /** The scope buttons replace Save only for an existing series. */
    val offersScopes: Boolean get() = id != null && recurring

    private fun kindNoun(): String = when (kind) {
        ItemKind.EVENT -> "event"
        ItemKind.TASK -> "task"
        ItemKind.BIRTHDAY -> "birthday"
    }

    /** The exact request body `kal_save_item_json` reads. */
    fun toForm(scope: EditScope): EditorForm = EditorForm(
        id = id,
        occurrenceStart = occurrenceStart?.let(Formats::wireInstant),
        kind = kind,
        title = title,
        date = date.toString(),
        startTime = startTime?.let(Formats::wireTime).orEmpty(),
        endTime = endTime?.let(Formats::wireTime).orEmpty(),
        allDay = allDay,
        calendarId = calendarId,
        location = location,
        person = person,
        rrule = repeat.rrule,
        reminderMinutes = reminders,
        customReminder = customReminder,
        color = color,
        scope = scope,
    )

    companion object {
        /**
         * `EditorState::build`: a blank item of [kind] on [date]. Events run
         * 09:00–10:00, tasks start at 09:00 with no end, birthdays are all-day
         * yearly entries at midnight — and every new item starts with an explicit
         * `#3366cc` override rather than "Calendar", exactly as before.
         */
        fun forNew(kind: ItemKind, date: LocalDate, calendarId: String?): EditorFields {
            val birthday = kind == ItemKind.BIRTHDAY
            return EditorFields(
                id = null,
                occurrenceStart = null,
                kind = kind,
                title = "",
                date = date,
                startTime = if (birthday) LocalTime.MIDNIGHT else LocalTime.of(9, 0),
                endTime = if (birthday || kind == ItemKind.TASK) null else LocalTime.of(10, 0),
                allDay = birthday,
                calendarId = calendarId,
                location = "",
                person = "",
                repeat = if (birthday) RepeatPreset.YEARLY else RepeatPreset.NONE,
                reminders = emptyList(),
                customReminder = "",
                color = DEFAULT_ITEM_COLOR,
                recurring = false,
            )
        }

        /**
         * `EditorState::edit_existing`: prefill from the stored item, anchored on
         * the *clicked occurrence* rather than the series base — otherwise a
         * scoped edit would move the occurrence back onto the series start — with
         * the end preserving the base duration.
         */
        fun fromItem(
            item: ItemModel,
            occurrenceStart: OffsetDateTime?,
            /** The owning series' rule, for an occurrence detached from it. */
            seriesRrule: String? = null,
        ): EditorFields {
            val anchor = occurrenceStart ?: item.start
            val anchorEnd = item.end?.let { anchor + Duration.between(item.start, it) }
            // A detached occurrence is still part of its series: showing the
            // series' rule keeps the scope choice available and stops the dialog
            // claiming the event no longer repeats.
            val rule = seriesRrule ?: item.rrule
            return EditorFields(
                id = item.id,
                occurrenceStart = occurrenceStart,
                kind = item.kind,
                title = item.title,
                date = anchor.toLocalDate(),
                startTime = anchor.toLocalTime(),
                endTime = anchorEnd?.toLocalTime(),
                allDay = item.allDay,
                calendarId = item.calendarId,
                location = item.location.orEmpty(),
                person = item.person.orEmpty(),
                repeat = RepeatPreset.fromRrule(rule),
                reminders = item.reminderMinutes,
                customReminder = "",
                color = item.colorOverride,
                recurring = rule != null,
                exception = item.seriesId != null,
            )
        }
    }
}
