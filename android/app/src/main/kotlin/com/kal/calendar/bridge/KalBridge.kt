package com.kal.calendar.bridge

import com.kal.calendar.data.Preferences
import com.kal.calendar.data.UpdateStatus
import com.kal.calendar.data.UserProfile
import java.time.LocalDate

/** Outcome of the startup handshake and database open. */
sealed interface BridgeStatus {
    /** Native ABI agrees with this build and the database is open. */
    data class Ready(val session: KalSession) : BridgeStatus

    /** The loaded library speaks a different bridge version — refuse to run. */
    data class Incompatible(val expected: Int, val actual: Int) : BridgeStatus

    /** The library is missing, the wrong ABI, or the database could not be opened. */
    data class Failed(val message: String) : BridgeStatus
}

/**
 * An open database plus the typed queries the interface layer runs against it.
 *
 * Queries decode the native JSON into the models in [Payloads]; a null reply
 * (native error) becomes an empty result so a view renders nothing rather than
 * crashing. Mutations report failure as `false` and leave the reason in
 * [lastError], which is what the UI surfaces.
 */
class KalSession internal constructor(private val handle: Long) {

    private var closed = false

    /** All calendars, in core order. */
    fun calendars(): List<CalendarModel> =
        Payloads.parseCalendars(query { KalNative.calendarsJson(handle) })

    /** Month grid payload. [firstDow] is 0 = Monday … 6 = Sunday. */
    fun month(year: Int, month: Int, firstDow: Int): List<List<MonthCell>> =
        Payloads.parseMonth(query { KalNative.monthJson(handle, year, month, firstDow) })

    /** Seven days from [weekStart]. */
    fun week(weekStart: LocalDate): List<WeekDay> =
        Payloads.parseWeek(query { KalNative.weekJson(handle, weekStart.toString()) })

    /** The day payload for [date] (null when the payload could not be read). */
    fun day(date: LocalDate): DayPayload? {
        if (closed) return null
        return Payloads.parseDay(KalNative.dayJson(handle, date.toString()))
    }

    /** Agenda rows for `[from, from + days]`, chronological, capped at [cap]. */
    fun agenda(from: LocalDate, days: Long = AGENDA_DAYS, cap: Long = AGENDA_CAP): List<AgendaRow> =
        Payloads.parseAgenda(query { KalNative.agendaJson(handle, from.toString(), days, cap) })

    /**
     * The reminder firings the platform scheduler should have armed: every live
     * item's reminders in the coming [REMINDER_HORIZON_DAYS] days, nearest
     * first, capped at [REMINDER_ALARM_CAP].
     *
     * Past firings never appear (no catch-up, D20) and calendar visibility does
     * not filter them — Dioxus reconciled against the unfiltered items too.
     */
    fun reminderFirings(
        horizonDays: Long = REMINDER_HORIZON_DAYS,
        cap: Long = REMINDER_ALARM_CAP,
    ): List<ReminderFiring> =
        Payloads.parseFirings(query { KalNative.reminderFiringsJson(handle, horizonDays, cap) })

    /** The stored preferences, with the defaults applied for anything unset. */
    fun preferences(): Preferences = Preferences.from(
        Payloads.decodeText(query { KalNative.getSetting(handle, KEY_PREFERENCES) }),
        Payloads.decodeText(query { KalNative.getSetting(handle, KEY_THEME) }),
    )

    /** Persist the whole `preferences` object. */
    fun setPreferences(preferences: Preferences): Boolean = command {
        KalNative.setSetting(handle, KEY_PREFERENCES, preferences.toJson()) == 0
    }

    /**
     * Persist the theme under its own key (plain string, as the Dioxus toggle
     * wrote it) so a theme flip never rewrites the `preferences` object.
     */
    fun setTheme(theme: String): Boolean = command {
        KalNative.setSetting(handle, KEY_THEME, theme) == 0
    }

    /** Flip one calendar's visibility, sending the whole model back. */
    fun setCalendarVisible(calendar: CalendarModel, visible: Boolean): Boolean = command {
        KalNative.upsertCalendarJson(handle, calendar.patchedVisible(visible)) == 0
    }

    /**
     * One item as the core model, or null when the id is absent (or unreadable):
     * what the editor prefills from.
     */
    fun item(id: String): ItemModel? = Payloads.parseItem(query { KalNative.itemJson(handle, id) })

    /**
     * Save one editor form — create, edit, or a scoped recurring edit, resolved
     * natively. A failure leaves the reason in [lastError] so the editor can keep
     * the dialog open and show it instead of dropping the edit.
     */
    fun saveItem(form: EditorForm): Boolean = command {
        KalNative.saveItemJson(handle, form.toJson()) == 0
    }

    /** Flip a task's completion (the row checkbox). */
    fun toggleItemCompleted(id: String): Boolean = command {
        KalNative.toggleItemCompletedJson(handle, id) == 0
    }

    /**
     * Import one .ics payload: the native side parses it into a new visible
     * `IcsImport` calendar plus its items (named from the file, else
     * [calendarName]) and skips an item it cannot store rather than losing the
     * rest. A parse failure leaves the reason in [lastError] for the sidebar.
     */
    fun importIcs(text: String, calendarName: String = KalBridge.DEFAULT_ICS_CALENDAR): Boolean = command {
        KalNative.importIcsJson(handle, text, calendarName) == 0
    }

    /**
     * Write every calendar and live item to [path] as one VCALENDAR, for the
     * share sheet to hand out. The native side owns the file.
     */
    fun exportIcs(path: String): Boolean = command {
        KalNative.exportIcsFile(handle, path) == 0
    }

    /**
     * Tombstone one item.
     *
     * The native call answers 1 when a row was written and 0 when the id is
     * already gone — both are success for the interface, which only needs to
     * know there is no error. -1 means a real failure (reason in [lastError]).
     */
    fun deleteItem(id: String): Boolean = command { KalNative.softDeleteItem(handle, id) >= 0 }

    /**
     * Delete one item or one scoped part of a recurring series.
     *
     * [EditScope.ALL] from a detached occurrence removes its whole series,
     * [EditScope.THIS_ONLY] removes just one occurrence, and
     * [EditScope.THIS_AND_FOLLOWING] truncates the series before the clicked
     * occurrence — without creating a replacement row.
     */
    fun deleteItem(form: DeleteForm): Boolean =
        command { KalNative.deleteItemJson(handle, form.toJson()) >= 0 }

    /**
     * The stored sync-chain profile: a display name plus an optional avatar,
     * one JSON row under the `profile` key. No native profile code is involved —
     * the row is a setting, exactly as the Dioxus app stored it.
     */
    fun profile(): UserProfile =
        UserProfile.from(Payloads.decodeText(query { KalNative.getSetting(handle, KEY_PROFILE) }))

    /** Persist the whole profile row. */
    fun saveProfile(profile: UserProfile): Boolean = command {
        KalNative.setSetting(handle, KEY_PROFILE, profile.toJson()) == 0
    }

    /**
     * Check for a newer release and stage it at [destPath], reporting the status
     * the settings screen shows. Blocking (network + download) — callers run it
     * off the UI thread.
     */
    fun checkUpdates(currentVersion: String, destPath: String): UpdateStatus {
        if (closed) return UpdateStatus.Failed("the database handle is closed")
        return UpdateStatus.parse(
            KalNative.checkUpdatesJson(currentVersion, destPath),
            lastError(),
        )
    }

    /** Last native error on this thread, or null when the last call succeeded. */
    fun lastError(): String? = if (closed) null else KalNative.lastError()

    /**
     * Whether this device is paired, plus the chain fingerprint. Reading never
     * creates an identity, so a fresh install stays unpaired.
     */
    fun syncStatus(): SyncStatus =
        Payloads.parseSyncStatus(query { KalNative.syncStatusJson(handle) })

    /** Generate + persist a new chain; the phrase is shown once for the user. */
    fun startSyncChain(): SyncPhrase? =
        Payloads.parseSyncPhrase(query { KalNative.syncStartChainJson(handle) })

    /** Join an existing chain; null when the phrase is invalid (reason in [lastError]). */
    fun joinSyncChain(phrase: String): SyncPhrase? =
        Payloads.parseSyncPhrase(query { KalNative.syncJoinChainJson(handle, phrase) })

    /** Reveal the stored recovery phrase for pairing another device. */
    fun syncPhrase(): SyncPhrase? =
        Payloads.parseSyncPhrase(query { KalNative.syncPhraseJson(handle) })

    /** Leave the chain; the per-device node identity is kept. */
    fun leaveSyncChain(): Boolean = command { KalNative.syncLeave(handle) == 0 }

    /**
     * One blocking sync round: waits for peer discovery, then merges. Null on
     * failure (reason in [lastError]); callers run it off the UI thread.
     */
    fun syncRound(): SyncRound? =
        Payloads.parseSyncRound(query { KalNative.syncRoundJson(handle) })

    /** Close the handle; further queries return empty results. */
    fun close() {
        if (!closed) {
            KalNative.close(handle)
            closed = true
        }
    }

    private inline fun query(body: () -> String?): String? = if (closed) null else body()

    private inline fun command(body: () -> Boolean): Boolean = if (closed) false else body()

    companion object {
        /** The agenda window and row cap the Dioxus agenda view used. */
        const val AGENDA_DAYS = 30L
        const val AGENDA_CAP = 100L

        /**
         * How far ahead firings are computed. The core clamps this to its own
         * 366-day recurrence horizon; the point of the wider window is that the
         * OS (not a running process) holds the alarms, so they must survive a
         * stretch without the app being opened.
         */
        const val REMINDER_HORIZON_DAYS = 366L

        /**
         * The nearest N firings to arm (D19's "only next N firings armed").
         * AlarmManager is practical to a few hundred alarms; re-reconciling on
         * every mutation and foreground rolls the window forward.
         */
        const val REMINDER_ALARM_CAP = 64L

        const val KEY_PREFERENCES = "preferences"
        const val KEY_THEME = "theme"
        const val KEY_PROFILE = "profile"
    }
}

/**
 * The Kotlin face of the native core.
 *
 * Rules this layer keeps:
 *  * the version handshake happens before any other native call, so a mismatched
 *    `.so` can never be used to read data under the wrong assumptions;
 *  * the database path is the one the Dioxus app used, so an upgrade opens the
 *    existing calendar rather than starting from scratch;
 *  * a native failure surfaces as [BridgeStatus.Failed] with the message from
 *    `kal_last_error()`, never as a crash.
 */
object KalBridge {
    /**
     * Bridge version this Kotlin build was compiled against. Bump together with
     * `kal_ffi::KAL_BRIDGE_VERSION` and the pinned tests in `crates/kal-jni`.
     */
    const val EXPECTED_BRIDGE_VERSION = 2

    /** `<filesDir>/kal/calendar.db` — the path the Dioxus app (and the widgets) use. */
    fun databasePath(filesDir: String): String = "$filesDir/kal/calendar.db"

    /**
     * `<filesDir>/kal/Kal-export.ics` — where an export is written, and the one
     * path `KalFileProvider` serves as `content://com.kal.calendar.updates/ics`.
     * These two must stay in step.
     */
    fun exportPath(filesDir: String): String = "$filesDir/kal/$EXPORT_FILE"

    /** The export's file name (the Dioxus build's, kept so the provider matches). */
    const val EXPORT_FILE = "Kal-export.ics"

    /** Fallback calendar name for an .ics without `X-WR-CALNAME` (upstream's). */
    const val DEFAULT_ICS_CALENDAR = "Imported"

    /**
     * `<filesDir>/kal/updates/kal-update.apk` — where a staged release is
     * downloaded, and the path `KalFileProvider` serves as
     * `content://com.kal.calendar.updates/apk`. These two must stay in step.
     */
    fun updateApkPath(filesDir: String): String = "$filesDir/kal/updates/$UPDATE_APK_FILE"

    /** The staged update's file name (upstream's). */
    const val UPDATE_APK_FILE = "kal-update.apk"

    /** Load `libkal_jni.so`, check the handshake, open the DB. */
    fun start(filesDir: String): BridgeStatus {
        val native = try {
            KalNative.bridgeVersion()
        } catch (e: UnsatisfiedLinkError) {
            return BridgeStatus.Failed(
                "libkal_jni.so could not be loaded — wrong ABI, or the native " +
                    "library was not staged into the APK (${e.message})",
            )
        }

        if (native != EXPECTED_BRIDGE_VERSION) {
            return BridgeStatus.Incompatible(expected = EXPECTED_BRIDGE_VERSION, actual = native)
        }

        val path = databasePath(filesDir)
        val handle = KalNative.open(path)
        if (handle == 0L) {
            return BridgeStatus.Failed(KalNative.lastError() ?: "kal_open returned no handle for $path")
        }

        return BridgeStatus.Ready(KalSession(handle))
    }
}
