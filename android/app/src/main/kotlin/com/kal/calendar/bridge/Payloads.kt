package com.kal.calendar.bridge

import java.time.LocalDate
import java.time.OffsetDateTime
import org.json.JSONArray
import org.json.JSONObject

/** The three item kinds the core model defines. */
enum class ItemKind(val wire: String) {
    EVENT("event"),
    TASK("task"),
    BIRTHDAY("birthday"),
    ;

    companion object {
        /** Unknown wire values fall back to [EVENT] (the core only emits these three). */
        fun fromWire(raw: String?): ItemKind = entries.firstOrNull { it.wire == raw } ?: EVENT
    }
}

/**
 * One expanded occurrence joined with its item, exactly as the native views
 * report it (`kal_month_json` & friends — see `widgets/kal_ffi.h`).
 *
 * `start` is the occurrence's own instant, so the interface layer formats times
 * from the user's preference instead of receiving them pre-baked. `age` is
 * non-null only for birthdays and is computed against the occurrence date.
 */
data class DisplayEntry(
    val id: String,
    val calendarId: String,
    val title: String,
    val start: OffsetDateTime,
    val end: OffsetDateTime?,
    val allDay: Boolean,
    val kind: ItemKind,
    val completed: Boolean,
    val color: String,
    val overrideColor: String?,
    val age: Int?,
)

/**
 * A calendar as the core model serialises it, plus the original JSON so a
 * visibility toggle can send the whole model back through the upsert export.
 */
data class CalendarModel(
    val id: String,
    val name: String,
    val color: String,
    val source: String,
    val visible: Boolean,
    val deleted: Boolean,
    val raw: String,
) {
    /**
     * Sidebar source label, mirroring the Dioxus `source_label` match: local
     * calendars show nothing, imported ones name their origin.
     *
     * `CalendarSource` is `#[serde(rename_all = "snake_case")]`, so the wire
     * values are `local` / `google_import` / `ics_import` / `birthdays`.
     */
    val sourceLabel: String
        get() = when (source) {
            "google_import" -> "google"
            "ics_import" -> "ics"
            "birthdays" -> "auto"
            else -> ""
        }

    /** The model JSON with `visible` set, for `upsertCalendarJson`. */
    fun patchedVisible(visible: Boolean): String =
        JSONObject(raw).put("visible", visible).toString()
}

/**
 * One item exactly as the core `CalendarItem` model serialises it (snake_case —
 * this is `kal_item_json`, not the camelCase view entries): what the editor needs
 * to prefill a form, including the fields no view shows.
 */
data class ItemModel(
    val id: String,
    val kind: ItemKind,
    val title: String,
    val location: String?,
    val calendarId: String,
    val start: OffsetDateTime,
    val end: OffsetDateTime?,
    val allDay: Boolean,
    /** RFC 5545 rule, or null when the item does not repeat. */
    val rrule: String?,
    val completed: Boolean,
    /** Reminder offsets in minutes before the start. */
    val reminderMinutes: List<Long>,
    val colorOverride: String?,
    /** `metadata.birthday_of`; non-null only for birthdays (possibly empty). */
    val person: String?,
    /**
     * `metadata.series_id`: set when this item is one occurrence detached from a
     * recurring series by "This event". It stays a single item but still belongs
     * to that series.
     */
    val seriesId: String? = null,
)

/** Which part of a recurring series a save applies to. */
enum class EditScope(val wire: String) {
    /** The whole series (and the only scope a non-recurring item can have). */
    ALL("all"),

    /** Only the clicked occurrence: it is EXDATE'd out and re-created. */
    THIS_ONLY("this_only"),

    /** The clicked occurrence and everything after it. */
    THIS_AND_FOLLOWING("this_and_following"),
}

/**
 * One editor form, exactly as `kal_save_item_json` reads it.
 *
 * The field names mirror the Dioxus `EditorState` so the write semantics —
 * default-calendar resolution, load-or-create, recurring scope — stay native
 * while the form widgets stay here (STEP0 §0.19.1/§0.19.2). An empty
 * `startTime`/`endTime` reads as midnight / "no end", and `color` absent means
 * "use the calendar's colour".
 */
data class EditorForm(
    /** Null creates a new item. */
    val id: String?,
    /** The clicked occurrence's instant (RFC 3339), for scoped edits. */
    val occurrenceStart: String?,
    val kind: ItemKind,
    val title: String,
    /** `YYYY-MM-DD`; times are read in the device's local offset. */
    val date: String,
    val startTime: String,
    val endTime: String,
    val allDay: Boolean,
    /** Null resolves to the first visible live calendar natively. */
    val calendarId: String?,
    val location: String,
    val person: String,
    val rrule: String?,
    val reminderMinutes: List<Long>,
    val customReminder: String,
    val color: String?,
    val scope: EditScope,
) {
    fun toJson(): String = JSONObject().apply {
        put("id", id ?: JSONObject.NULL)
        put("occurrenceStart", occurrenceStart ?: JSONObject.NULL)
        put("kind", kind.wire)
        put("title", title)
        put("date", date)
        put("startTime", startTime)
        put("endTime", endTime)
        put("allDay", allDay)
        put("calendarId", calendarId ?: JSONObject.NULL)
        put("location", location)
        put("person", person)
        put("rrule", rrule ?: JSONObject.NULL)
        put("reminders", JSONArray(reminderMinutes))
        put("customReminder", customReminder)
        put("color", color ?: JSONObject.NULL)
        put("scope", scope.wire)
    }.toString()
}

/** One scoped delete, exactly as `kal_delete_item_json` reads it. */
data class DeleteForm(
    val id: String,
    /** The clicked occurrence's instant (RFC 3339), or null for the whole item. */
    val occurrenceStart: String?,
    val scope: EditScope,
) {
    fun toJson(): String = JSONObject().apply {
        put("id", id)
        put("occurrenceStart", occurrenceStart ?: JSONObject.NULL)
        put("scope", scope.wire)
    }.toString()
}

/** One day cell of the month grid. */
data class MonthCell(val date: LocalDate, val inMonth: Boolean, val items: List<DisplayEntry>)

/** One day of the week view. */
data class WeekDay(val date: LocalDate, val items: List<DisplayEntry>)

/** One timed occurrence positioned on the day canvas (fractions of a 24 h day). */
data class PositionedEntry(
    val item: DisplayEntry,
    val topFrac: Double,
    val heightFrac: Double,
    val lane: Int,
    val lanes: Int,
)

/** The day payload: the all-day strip plus positioned timed blocks. */
data class DayPayload(
    val date: LocalDate,
    val allDay: List<DisplayEntry>,
    val timed: List<PositionedEntry>,
)

/** One agenda row: a dated occurrence. */
data class AgendaRow(val date: LocalDate, val item: DisplayEntry)

/**
 * One reminder firing, as `kal_reminder_firings_json` reports it: the instant
 * the platform scheduler must show it, the item it belongs to, and the reminder
 * that asked for it (both ids are needed to cancel or trace a notification).
 */
data class ReminderFiring(
    val itemId: String,
    val reminderId: String,
    val fireAt: OffsetDateTime,
    val title: String,
)

/**
 * Sync-chain pairing status (`kal_sync_status_json`). A fresh install is
 * unpaired; `fingerprint` is the chain's stable 16-hex-char id when paired.
 */
data class SyncStatus(val paired: Boolean, val fingerprint: String?)

/** A chain's recovery phrase plus its fingerprint (start / join / reveal). */
data class SyncPhrase(val phrase: String, val fingerprint: String)

/** One blocking sync round's outcome (`kal_sync_round_json`). */
data class SyncRound(val merged: Int)

/**
 * One device seen on the chain (`kal_sync_devices_json`). [id] is the stable
 * per-install device id carried in every envelope; [revoked] means this device
 * drops that peer's snapshots.
 */
data class SyncDevice(val id: String, val revoked: Boolean)

/**
 * Parsing for the native JSON payloads — pure functions with no Android or
 * database dependency, so the JVM unit tests pin the exact contract the Rust
 * side emits.
 *
 * Malformed input yields an empty (or absent) payload rather than an exception:
 * a bad payload must not crash the shell. Genuine native failures are reported
 * through the error channel instead ([KalNative.lastError]).
 */
object Payloads {

    /**
     * Decode the text of a `kal_get_setting` reply: the export returns JSON, so
     * an absent setting arrives as the literal `null` and a present one as a
     * JSON string (possibly legacy-quoted). Returns the stored string, or null.
     */
    fun decodeText(jsonText: String?): String? {
        if (jsonText == null) return null
        val value = runCatching { org.json.JSONTokener(jsonText).nextValue() }.getOrNull()
        return when (value) {
            // Not JSON at all: a legacy plain value, returned verbatim.
            null -> jsonText
            JSONObject.NULL -> null
            is String -> value
            else -> value.toString()
        }
    }

    /** Month grid payload → 6 rows of cells. */
    fun parseMonth(json: String?): List<List<MonthCell>> = parse(json) { root ->
        val rows = JSONArray(root)
        (0 until rows.length()).map { r ->
            val row = rows.getJSONArray(r)
            (0 until row.length()).map { c ->
                val cell = row.getJSONObject(c)
                MonthCell(
                    date = LocalDate.parse(cell.getString("date")),
                    inMonth = cell.getBoolean("inMonth"),
                    items = entries(cell.optJSONArray("items")),
                )
            }
        }
    }.orEmpty()

    /** Week payload → 7 days. */
    fun parseWeek(json: String?): List<WeekDay> = parse(json) { root ->
        val days = JSONArray(root)
        (0 until days.length()).map { i ->
            val day = days.getJSONObject(i)
            WeekDay(date = LocalDate.parse(day.getString("date")), items = entries(day.optJSONArray("items")))
        }
    }.orEmpty()

    /** Day payload → the all-day strip plus positioned blocks. */
    fun parseDay(json: String?): DayPayload? = parse(json) { root ->
        val obj = JSONObject(root)
        val timed = obj.optJSONArray("timed")
        DayPayload(
            date = LocalDate.parse(obj.getString("date")),
            allDay = entries(obj.optJSONArray("allDay")),
            timed = (0 until (timed?.length() ?: 0)).map { i ->
                val block = timed!!.getJSONObject(i)
                PositionedEntry(
                    item = entry(block.getJSONObject("item")),
                    topFrac = block.getDouble("topFrac"),
                    heightFrac = block.getDouble("heightFrac"),
                    lane = block.getInt("lane"),
                    lanes = block.getInt("lanes"),
                )
            },
        )
    }

    /** Agenda payload → rows in the order the core returned them (date, then time). */
    fun parseAgenda(json: String?): List<AgendaRow> = parse(json) { root ->
        val rows = JSONArray(root)
        (0 until rows.length()).map { i ->
            val row = rows.getJSONObject(i)
            AgendaRow(date = LocalDate.parse(row.getString("date")), item = entry(row.getJSONObject("item")))
        }
    }.orEmpty()

    /** Calendar payload → the core calendar models. */
    fun parseCalendars(json: String?): List<CalendarModel> = parse(json) { root ->
        val arr = JSONArray(root)
        (0 until arr.length()).map { i ->
            val cal = arr.getJSONObject(i)
            CalendarModel(
                id = cal.getString("id"),
                name = cal.getString("name"),
                color = cal.getString("color"),
                source = cal.optString("source"),
                visible = cal.optBoolean("visible", true),
                deleted = cal.optBoolean("deleted", false),
                raw = cal.toString(),
            )
        }
    }.orEmpty()

    /** One item as the core model serialises it, or null when absent/unreadable. */
    fun parseItem(json: String?): ItemModel? = parse(json) { root ->
        // `kal_item_json` answers the literal `null` for an absent id, which is
        // not a JSON object — that falls out of the `runCatching` in [parse].
        item(JSONObject(org.json.JSONTokener(root).nextValue().toString()))
    }

    /**
     * Sync-chain pairing status. An unreadable payload reads as unpaired, so the
     * panel never claims a chain it cannot verify.
     */
    fun parseSyncStatus(json: String?): SyncStatus = parse(json) { root ->
        val obj = JSONObject(root)
        SyncStatus(
            paired = obj.optBoolean("paired", false),
            fingerprint = if (obj.isNull("fingerprint")) null else obj.getString("fingerprint"),
        )
    } ?: SyncStatus(paired = false, fingerprint = null)

    /** A recovery phrase payload; null when the native call failed or was malformed. */
    fun parseSyncPhrase(json: String?): SyncPhrase? = parse(json) { root ->
        val obj = JSONObject(root)
        SyncPhrase(
            phrase = obj.getString("phrase"),
            fingerprint = obj.getString("fingerprint"),
        )
    }

    /** One round's merged-envelope count; null when the round failed. */
    fun parseSyncRound(json: String?): SyncRound? = parse(json) { root ->
        SyncRound(merged = JSONObject(root).optInt("merged", 0))
    }

    /**
     * Devices the sync panel can list. An unreadable payload yields none, so a
     * failed read never invents a peer row; rows without an id are dropped.
     */
    fun parseSyncDevices(json: String?): List<SyncDevice> = parse(json) { root ->
        val arr = JSONObject(root).optJSONArray("devices") ?: JSONArray()
        (0 until arr.length()).mapNotNull { i ->
            val obj = arr.optJSONObject(i) ?: return@mapNotNull null
            val id = obj.optString("id", "")
            if (id.isEmpty()) null else SyncDevice(id, obj.optBoolean("revoked", false))
        }
    } ?: emptyList()

    /**
     * Reminder firings, nearest first and already capped by the native side.
     * An unreadable payload yields no firings, so a reconcile never arms
     * something it cannot trust.
     */
    fun parseFirings(json: String?): List<ReminderFiring> = parse(json) { root ->
        val arr = JSONArray(root)
        (0 until arr.length()).map { i ->
            val firing = arr.getJSONObject(i)
            ReminderFiring(
                itemId = firing.getString("itemId"),
                reminderId = firing.getString("reminderId"),
                fireAt = OffsetDateTime.parse(firing.getString("fireAt")),
                title = firing.getString("title"),
            )
        }
    }.orEmpty()

    /** One display entry. Shared by every payload above. */
    fun entry(obj: JSONObject): DisplayEntry = DisplayEntry(
        id = obj.getString("id"),
        calendarId = obj.getString("calendarId"),
        title = obj.getString("title"),
        start = OffsetDateTime.parse(obj.getString("start")),
        end = if (obj.isNull("end")) null else OffsetDateTime.parse(obj.getString("end")),
        allDay = obj.getBoolean("allDay"),
        kind = ItemKind.fromWire(obj.optString("kind")),
        completed = obj.optBoolean("completed", false),
        color = obj.optString("color"),
        overrideColor = if (obj.isNull("overrideColor")) null else obj.getString("overrideColor"),
        age = if (obj.isNull("age")) null else obj.getInt("age"),
    )

    private fun entries(array: JSONArray?): List<DisplayEntry> =
        if (array == null) emptyList() else (0 until array.length()).map { entry(array.getJSONObject(it)) }

    /** One core-model item (the shape `kal_item_json` emits). */
    private fun item(obj: JSONObject): ItemModel = ItemModel(
        id = obj.getString("id"),
        kind = ItemKind.fromWire(obj.optString("kind")),
        title = obj.getString("title"),
        location = if (obj.isNull("location")) null else obj.getString("location"),
        calendarId = obj.getString("calendar_id"),
        start = OffsetDateTime.parse(obj.getString("start")),
        end = if (obj.isNull("end")) null else OffsetDateTime.parse(obj.getString("end")),
        allDay = obj.getBoolean("all_day"),
        rrule = if (obj.isNull("rrule")) null else obj.getString("rrule"),
        completed = !obj.isNull("completed"),
        reminderMinutes = reminderMinutes(obj.optJSONArray("reminders")),
        colorOverride = if (obj.isNull("color_override")) null else obj.getString("color_override"),
        person = obj.optJSONObject("metadata")
            ?.let { if (it.isNull("birthday_of")) null else it.getString("birthday_of") },
        seriesId = obj.optJSONObject("metadata")
            ?.let { if (it.isNull("series_id")) null else it.getString("series_id") },
    )

    /**
     * Reminder offsets from the model's tagged array
     * (`[{"type":"minutes_before","minutes":10,"method":"push"}]`). Anything the
     * editor does not understand is skipped rather than guessed at.
     */
    private fun reminderMinutes(array: JSONArray?): List<Long> =
        (0 until (array?.length() ?: 0)).mapNotNull { i ->
            val offset = array?.optJSONObject(i)?.optJSONObject("offset") ?: return@mapNotNull null
            if (offset.optString("type") == "minutes_before") offset.optLong("minutes") else null
        }

    /** Run [body] over the non-null payload, turning any parse failure into null. */
    private inline fun <T> parse(json: String?, body: (String) -> T): T? =
        if (json == null) null else runCatching { body(json) }.getOrNull()
}

/**
 * Calendar visibility, mirroring `use_visible_items` in the Dioxus shell: the
 * sidebar lists only live+visible calendars and every view hides the items that
 * belong to the others. Applied here (on the client) because that is where the
 * toggle state lives.
 */
object Visibility {
    /** Ids of the calendars whose items must not be rendered. */
    fun hiddenIds(calendars: List<CalendarModel>): Set<String> =
        calendars.filter { !it.visible || it.deleted }.map { it.id }.toSet()

    /** Sidebar rows: live, visible calendars in the order the core returned them. */
    fun listedCalendars(calendars: List<CalendarModel>): List<CalendarModel> =
        calendars.filter { it.visible && !it.deleted }

    fun filter(entries: List<DisplayEntry>, hidden: Set<String>): List<DisplayEntry> =
        entries.filter { it.calendarId !in hidden }

    fun filterMonth(rows: List<List<MonthCell>>, hidden: Set<String>): List<List<MonthCell>> =
        rows.map { row -> row.map { cell -> cell.copy(items = filter(cell.items, hidden)) } }

    fun filterWeek(days: List<WeekDay>, hidden: Set<String>): List<WeekDay> =
        days.map { day -> day.copy(items = filter(day.items, hidden)) }

    fun filterDay(payload: DayPayload, hidden: Set<String>): DayPayload = payload.copy(
        allDay = filter(payload.allDay, hidden),
        timed = payload.timed.filter { it.item.calendarId !in hidden },
    )

    fun filterAgenda(rows: List<AgendaRow>, hidden: Set<String>): List<AgendaRow> =
        rows.filter { it.item.calendarId !in hidden }
}
