package com.kal.calendar.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.kal.calendar.bridge.AgendaRow
import com.kal.calendar.bridge.CalendarModel
import com.kal.calendar.bridge.DayPayload
import com.kal.calendar.bridge.DeleteForm
import com.kal.calendar.bridge.DisplayEntry
import com.kal.calendar.bridge.EditScope
import com.kal.calendar.bridge.ItemKind
import com.kal.calendar.bridge.KalSession
import com.kal.calendar.bridge.MonthCell
import com.kal.calendar.bridge.ReminderFiring
import com.kal.calendar.bridge.Visibility
import com.kal.calendar.bridge.WeekDay
import com.kal.calendar.BuildConfig
import com.kal.calendar.data.InstallBlock
import com.kal.calendar.data.Preferences
import com.kal.calendar.data.UpdateStatus
import com.kal.calendar.data.UserProfile
import com.kal.calendar.data.blockedUpdateLine
import com.kal.calendar.data.installLine
import com.kal.calendar.data.updateStatusLine
import java.time.LocalDate
import java.time.OffsetDateTime
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Everything the shell shows and does, over one open [KalSession].
 *
 * State is plain Compose state and every native call is synchronous (local
 * SQLite), which mirrors the Dioxus app: it read inside the render pass too.
 * Mutations reload what they invalidate rather than relying on reactivity.
 *
 * The calendar-visibility filter is applied by the views from [hidden], the way
 * `use_visible_items` did in the Rust UI layer.
 */
class ShellState(private val session: KalSession) {

    var preferences by mutableStateOf(session.preferences())
        private set

    /** The view cursor: the highlighted day, month/week/day/agenda all derive from it. */
    var cursor by mutableStateOf(LocalDate.now())
        private set

    /**
     * Bumped after an item mutation so the shell can push a fresh snapshot to
     * the home-screen widgets without waiting for their 30-minute period.
     */
    var dataVersion by mutableIntStateOf(0)
        private set

    var view by mutableStateOf(ViewMode.MONTH)
        private set

    var sidebarOpen by mutableStateOf(false)
        private set

    var calendars by mutableStateOf(emptyList<CalendarModel>())
        private set

    var monthCells by mutableStateOf(emptyList<List<MonthCell>>())
        private set

    var weekDays by mutableStateOf(emptyList<WeekDay>())
        private set

    var dayPayload by mutableStateOf<DayPayload?>(null)
        private set

    var agendaRows by mutableStateOf(emptyList<AgendaRow>())
        private set

    /** Non-null while the item editor is open. */
    var editor by mutableStateOf<EditorFields?>(null)
        private set

    /**
     * The reminder firings the platform scheduler should have armed. Recomputed
     * with everything else, so a reconcile sees the same snapshot the views do.
     */
    var reminderFirings by mutableStateOf(emptyList<ReminderFiring>())
        private set

    /** A rejected save/delete, shown inside the still-open editor. */
    var editorError by mutableStateOf<String?>(null)
        private set

    /** Non-null while a native failure is being reported. */
    var error by mutableStateOf<String?>(null)
        private set

    /**
     * A failed sidebar action (import/export), shown inline under the buttons.
     * Cleared by the next attempt, so it never outlives the action it explains.
     */
    var sidebarError by mutableStateOf<String?>(null)
        private set

    /** The profile being edited in the sidebar section (committed on Save). */
    var profileEdit by mutableStateOf(UserProfile())
        private set

    /** The stored sync-chain profile (name + avatar), shown in the Sync header. */
    var profile by mutableStateOf(UserProfile())
        private set

    /** A refused avatar or a failed profile save, shown in the section. */
    var profileError by mutableStateOf<String?>(null)
        private set

    // -----------------------------------------------------------------------
    // Sync chain (the sidebar's Sync section)
    // -----------------------------------------------------------------------

    /** The Sync panel's state: unpaired, one-time phrase, paired or a failure. */
    var syncUi by mutableStateOf<SyncUi>(SyncUi.NotPaired)
        private set

    /** The paste-the-phrase field on the unpaired panel. */
    var joinPhrase by mutableStateOf("")
        private set

    /** True while a round (manual or background) is in flight. */
    var syncing by mutableStateOf(false)
        private set

    /** The most recent manual round's outcome line, or null before one ran. */
    var lastSync by mutableStateOf<String?>(null)
        private set

    /** The Software & updates status line, or null before the first check. */
    var updateLine by mutableStateOf<String?>(null)
        private set

    /** True once a newer release is staged and the installer can be fired. */
    var updateReady by mutableStateOf(false)
        private set

    /** True while a check (network + download) is in flight. */
    var checkingUpdate by mutableStateOf(false)
        private set

    /** Today, fixed for the shell's lifetime (it only drives the highlight). */
    val today: LocalDate = LocalDate.now()

    /** Ids of the calendars whose items the views must not render. */
    val hidden: Set<String> get() = Visibility.hiddenIds(calendars)

    /** Sidebar rows: live, visible calendars. */
    val listedCalendars: List<CalendarModel> get() = Visibility.listedCalendars(calendars)

    init {
        reload()
        profile = session.profile()
        profileEdit = profile
        syncUi = currentSyncState()
    }

    /**
     * Re-read the calendars and all four view payloads. The queries are local
     * SQLite reads against a small database, and only the current view renders.
     */
    fun reload() {
        calendars = session.calendars()
        monthCells = session.month(cursor.year, cursor.monthValue, CalendarNav.firstDow(preferences.firstDayMonday))
        weekDays = session.week(CalendarNav.weekStart(cursor))
        dayPayload = session.day(cursor)
        agendaRows = session.agenda(today)
        // Every mutation funnels through here, which is exactly when the armed
        // alarms may have gone stale (D21's effect did the same, on item count).
        reminderFirings = session.reminderFirings()
    }

    /** ‹ or › on the nav strip. */
    fun step(forward: Boolean) {
        cursor = CalendarNav.step(view, cursor, forward)
        reload()
    }

    /** A view-mode button; the cursor is untouched (as before). */
    fun selectView(mode: ViewMode) {
        view = mode
    }

    fun toggleSidebar() {
        sidebarOpen = !sidebarOpen
    }

    fun closeSidebar() {
        sidebarOpen = false
    }

    /** Flip one calendar's visibility and re-read the views it affects. */
    fun toggleCalendarVisibility(calendar: CalendarModel) {
        if (session.setCalendarVisible(calendar, !calendar.visible)) {
            reload()
        } else {
            reportFailure()
        }
    }

    fun setTime24h(value: Boolean) = persist(preferences.copy(time24h = value))

    fun setFirstDayMonday(value: Boolean) = persist(preferences.copy(firstDayMonday = value))

    /**
     * The theme lives under its own setting key (toggling it must not rewrite the
     * `preferences` object), and the colour switch applies immediately.
     */
    fun toggleTheme() {
        val next = if (preferences.isDark) "light" else "dark"
        if (session.setTheme(next)) {
            preferences = preferences.copy(theme = next)
        } else {
            reportFailure()
        }
    }

    /**
     * Open the editor for a new [kind] on [date]: the sidebar's "+ Event" and
     * friends use today, a month cell passes the cell's own date.
     */
    fun newItem(kind: ItemKind, date: LocalDate = today) {
        editorError = null
        editor = EditorFields.forNew(kind, date, defaultCalendarId())
    }

    /**
     * Open the add-event editor for a date delivered by the home-screen month
     * widget. Moving the cursor first makes the month/week/day views sit on the
     * tapped date behind the dialog; the widget header's + button passes today.
     */
    fun openNewEvent(date: LocalDate) {
        sidebarOpen = false
        if (cursor != date) {
            cursor = date
            reload()
        }
        newItem(ItemKind.EVENT, date)
    }

    /** Open the editor for an existing item, anchored on the clicked occurrence. */
    fun editItem(id: String, occurrenceStart: OffsetDateTime?) {
        val item = session.item(id)
        if (item == null) {
            reportFailure()
            return
        }
        editorError = null
        // An occurrence detached by "This event" is still part of its series:
        // load the series' rule so the dialog offers the scope choice again.
        // A missing series leaves it as the plain event it is on disk.
        val seriesRrule = item.seriesId?.let { session.item(it) }?.rrule
        editor = EditorFields.fromItem(item, occurrenceStart, seriesRrule)
    }

    /** Replace the open form (one field edit). */
    fun updateEditor(fields: EditorFields) {
        editor = fields
    }

    fun closeEditor() {
        editor = null
        editorError = null
    }

    /**
     * Save the open form under [scope] and re-read the views it changed.
     *
     * A rejection keeps the dialog open with the native reason: the Dioxus editor
     * silently did nothing here, which made a rejected edit look like a dead
     * button (and made end-less items unsavable at all).
     */
    fun saveEditor(scope: EditScope) {
        val fields = editor ?: return
        if (session.saveItem(fields.toForm(scope))) {
            closeEditor()
            reload()
            noteDataChanged()
        } else {
            editorError = session.lastError() ?: "the item was not saved"
        }
    }

    /** Confirmed delete: apply the chosen scope, then close and refresh. */
    fun deleteEditorItem(scope: EditScope) {
        val fields = editor ?: return
        val id = fields.id ?: return
        val form = DeleteForm(
            id = id,
            occurrenceStart = fields.occurrenceStart?.let(Formats::wireInstant),
            scope = scope,
        )
        if (session.deleteItem(form)) {
            closeEditor()
            reload()
            noteDataChanged()
        } else {
            editorError = session.lastError() ?: "the item was not deleted"
        }
    }

    /** The row checkbox: flip a task's completion and re-read the views. */
    fun toggleTask(entry: DisplayEntry) {
        if (session.toggleItemCompleted(entry.id)) {
            reload()
            noteDataChanged()
        } else {
            reportFailure()
        }
    }

    /**
     * Import one picked .ics payload. The native side parses it into a new
     * visible `IcsImport` calendar plus its items, then the views reload so the
     * calendar and its entries appear. A failure surfaces the native reason in
     * the sidebar — upstream only wrote it to stderr.
     */
    fun importIcs(text: String) {
        sidebarError = null
        if (session.importIcs(text)) {
            reload()
            noteDataChanged()
        } else {
            sidebarError = importFailure(session.lastError())
        }
    }

    /**
     * Write the whole calendar to [path] (the native side owns the file), then
     * the caller shares it. Returns whether the file is ready, so the composable
     * only opens the share sheet after a successful write.
     */
    fun exportIcs(path: String): Boolean {
        sidebarError = null
        val written = session.exportIcs(path)
        if (!written) {
            sidebarError = exportFailure(session.lastError())
        }
        return written
    }

    /** Report a picked document the app could not read at all. */
    fun sidebarFailed(message: String) {
        sidebarError = message
    }

    // -----------------------------------------------------------------------
    // Sync chain: pairing, the manual round and the background driver
    // -----------------------------------------------------------------------

    /** Whether this device holds a chain (including the one-time phrase view). */
    val syncPaired: Boolean get() = syncUi is SyncUi.Paired || syncUi is SyncUi.ShowPhrase

    /** Generate a new chain; the phrase is shown once in the modal. */
    fun startSyncChain() {
        val phrase = session.startSyncChain()
        syncUi = if (phrase != null) {
            SyncUi.ShowPhrase(phrase.phrase, phrase.fingerprint)
        } else {
            SyncUi.Error(session.lastError() ?: "the sync chain could not be started")
        }
    }

    fun updateJoinPhrase(value: String) {
        joinPhrase = value
    }

    /** Join by phrase; invalid input keeps the error inline with a Back button. */
    fun joinSyncChain() {
        val joined = session.joinSyncChain(joinPhrase)
        if (joined != null) {
            joinPhrase = ""
            syncUi = SyncUi.ShowPhrase(joined.phrase, joined.fingerprint)
        } else {
            syncUi = SyncUi.Error(session.lastError() ?: "the chain could not be joined")
        }
    }

    /** "I wrote it down": collapse the phrase modal back to the paired panel. */
    fun acknowledgeSyncPhrase() {
        syncUi = currentSyncState()
    }

    /** Reveal the stored phrase for pairing another device. */
    fun showSyncPhrase() {
        val phrase = session.syncPhrase()
        if (phrase != null) {
            syncUi = SyncUi.ShowPhrase(phrase.phrase, phrase.fingerprint)
        } else {
            reportFailure()
        }
    }

    fun dismissSyncError() {
        syncUi = currentSyncState()
    }

    /** Leave the chain (local-only); the per-device node identity is kept. */
    fun leaveSyncChain() {
        if (session.leaveSyncChain()) {
            syncUi = SyncUi.NotPaired
            lastSync = null
        } else {
            reportFailure()
        }
    }

    /**
     * One manual round: blocking (peer discovery can take ~15 s), so it runs on
     * the IO dispatcher and the caller launches it from a UI scope. The result
     * line mirrors the Dioxus panel; a merge reloads the views.
     */
    suspend fun syncNow() {
        if (syncing) return
        syncing = true
        val (round, failure) = withContext(Dispatchers.IO) {
            session.syncRound() to session.lastError()
        }
        syncing = false
        if (round != null) {
            lastSync = if (round.merged > 0) {
                "Synced ${round.merged} change(s) \u2014 ${clock()}"
            } else {
                "No changes to sync \u2014 ${clock()}"
            }
            if (round.merged > 0) {
                reload()
                noteDataChanged()
            }
        } else {
            // Transient (e.g. "no peers yet") — keep the paired panel visible.
            lastSync = "Sync failed: ${failure ?: "unknown error"}"
        }
    }

    /**
     * One background round, silent unless something merged (the Dioxus driver
     * logged to stderr and refreshed the views). Skips while a manual round is
     * in flight so two rounds never drain the same inbox.
     */
    suspend fun backgroundSync() {
        if (syncing || !syncPaired) return
        syncing = true
        val merged = withContext(Dispatchers.IO) { session.syncRound()?.merged }
        syncing = false
        if (merged != null && merged > 0) {
            reload()
            noteDataChanged()
        }
    }

    /** The panel state the stored identity implies (paired + fingerprint, or not). */
    private fun currentSyncState(): SyncUi {
        val status = session.syncStatus()
        return if (status.paired && status.fingerprint != null) {
            SyncUi.Paired(status.fingerprint)
        } else {
            SyncUi.NotPaired
        }
    }

    /** Local "HH:MM" for the last-sync line (the Dioxus panel used 24-hour here). */
    private fun clock(): String = java.time.LocalTime.now().format(SYNC_CLOCK)

    // -----------------------------------------------------------------------
    // Settings: the sync-chain profile and the Software & updates section
    // -----------------------------------------------------------------------

    /** Discard the sidebar profile edits, restoring the stored profile. */
    fun resetProfileEdit() {
        profileEdit = profile
        profileError = null
    }

    /**
     * Attach a picked image to the in-progress profile. Validation is the pure
     * Kotlin [UserProfile.withAvatar] (Decision C); a refusal keeps the section
     * showing upstream's message.
     */
    fun setProfileAvatar(mime: String?, bytes: ByteArray) {
        profileEdit.withAvatar(mime, bytes)
            .onSuccess {
                profileEdit = it
                profileError = null
            }
            .onFailure { profileError = it.message }
    }

    fun clearProfileAvatar() {
        profileEdit = profileEdit.withoutAvatar()
        profileError = null
    }

    fun updateUsername(username: String) {
        profileEdit = profileEdit.copy(username = username)
    }

    /** Report a picked image the app could not read at all. */
    fun reportProfileError(message: String) {
        profileError = message
    }

    /** Commit the profile. A refused write keeps the section showing the reason. */
    fun saveProfile() {
        if (session.saveProfile(profileEdit)) {
            profile = profileEdit
            profileError = null
        } else {
            profileError = session.lastError() ?: "the profile was not saved"
        }
    }

    /**
     * The "Check at startup" box. It writes the same `preferences` row the top
     * bar's controls do, so the next launch reads it back.
     */
    fun setAutoCheckUpdates(value: Boolean) = persist(preferences.copy(autoCheckUpdates = value))

    /**
     * Check for a newer release and stage it. Blocking (network + download), so
     * the caller runs it off the UI thread from a main-dispatcher scope.
     *
     * [installBlock] is asked — after the download, once there is something to
     * install — whether the installer would accept the staged APK; a staged
     * release this build cannot replace is reported as such instead of offering
     * a button whose installer then fails with Android's opaque message.
     */
    suspend fun checkForUpdates(destPath: String, installBlock: () -> InstallBlock?) {
        if (checkingUpdate) return
        checkingUpdate = true
        val (status, block) = withContext(Dispatchers.IO) {
            val status = session.checkUpdates(BuildConfig.VERSION_NAME, destPath)
            if (status is UpdateStatus.Ready) status to installBlock() else status to null
        }
        checkingUpdate = false
        updateReady = false
        when {
            status !is UpdateStatus.Ready ->
                updateLine = updateStatusLine(status, BuildConfig.VERSION_NAME)
            block != null -> updateLine = blockedUpdateLine(block, status.version)
            else -> {
                updateReady = true
                updateLine = updateStatusLine(status, BuildConfig.VERSION_NAME)
            }
        }
    }

    /**
     * After "Apply update now" hands the APK to the package installer. The
     * staged release stays ready, so a cancelled install can be retried.
     */
    fun finishInstall(opened: Boolean, reason: String? = null) {
        updateLine = installLine(opened, reason)
    }

    fun dismissEditorError() {
        editorError = null
    }

    /**
     * The editor's default calendar: the first live+visible one, else the first
     * live one. Null — no calendars at all — is left to the native save, which
     * creates the same "Personal" the app seeds on first run.
     */
    private fun defaultCalendarId(): String? =
        listedCalendars.firstOrNull()?.id ?: calendars.firstOrNull { !it.deleted }?.id

    fun dismissError() {
        error = null
    }

    private fun persist(next: Preferences) {
        if (session.setPreferences(next)) {
            preferences = next
            reload()
        } else {
            reportFailure()
        }
    }

    private fun noteDataChanged() {
        dataVersion++
    }

    private fun reportFailure() {
        error = session.lastError() ?: "the native call failed"
    }
}

/** Local "HH:MM" for the Sync panel's last-round line (24-hour, as Dioxus used). */
private val SYNC_CLOCK: java.time.format.DateTimeFormatter =
    java.time.format.DateTimeFormatter.ofPattern("HH:mm")
