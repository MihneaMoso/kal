package com.kal.calendar.bridge

/**
 * The JNI surface of the native core.
 *
 * Every `external fun` here binds by name to a symbol in `libkal_jni.so`
 * (`Java_com_kal_calendar_bridge_KalNative_<method>`), implemented in
 * `crates/kal-jni`. The two sides must move together, so:
 *
 *  * `crates/kal-jni` has a test that takes the address of each export, and
 *  * `scripts/build-kotlin-android.sh` greps these declarations and checks the
 *    staged `.so` really exports a matching symbol,
 *
 * which catches a rename on either side at build time instead of on device.
 *
 * Conventions (see `widgets/kal_ffi.h`): queries return UTF-8 JSON or null on
 * error, commands return 0 or -1, and the failure reason is read back with
 * [lastError]. `handle` is the value from [open]; 0 means "no database".
 */
internal object KalNative {
    init {
        System.loadLibrary("kal_jni")
    }

    /** Native bridge schema version, compared against [KalBridge.EXPECTED_BRIDGE_VERSION]. */
    external fun bridgeVersion(): Int

    /** Last native error on the calling thread, or null when the last call succeeded. */
    external fun lastError(): String?

    /** Open (or create) the database at [path]; returns 0 on failure. */
    external fun open(path: String): Long

    /** Close a handle from [open]; passing 0, or closing twice, is safe. */
    external fun close(handle: Long)

    /** All calendars as the core `Calendar` model JSON array, or null on failure. */
    external fun calendarsJson(handle: Long): String?

    /** Every setting as one JSON object, or null on failure. */
    external fun settingsJson(handle: Long): String?

    /** One setting as JSON text (the literal `null` when absent), or null on failure. */
    external fun getSetting(handle: Long, key: String): String?

    /** Write one setting as a plain string value: 0 on success, -1 on failure. */
    external fun setSetting(handle: Long, key: String, value: String): Int

    /** Insert or replace a calendar from its JSON model: 0 on success, -1 on failure. */
    external fun upsertCalendarJson(handle: Long, json: String): Int

    /** Month grid: 6 rows × 7 days of `{date, inMonth, items}`, or null on failure. */
    external fun monthJson(handle: Long, year: Int, month: Int, firstDow: Int): String?

    /** One week: 7 days of `{date, items}` from the given start, or null on failure. */
    external fun weekJson(handle: Long, weekStart: String): String?

    /** One day: `{date, allDay, timed}`, or null on failure. */
    external fun dayJson(handle: Long, date: String): String?

    /** Agenda rows for `[from, from + days]` capped at `cap`, or null on failure. */
    external fun agendaJson(handle: Long, from: String, days: Long, cap: Long): String?

    /**
     * Reminder firings in `[now, now + horizonDays]`, nearest first and capped,
     * as `[{itemId, reminderId, fireAt, title}]`, or null on failure.
     */
    external fun reminderFiringsJson(handle: Long, horizonDays: Long, cap: Long): String?

    /**
     * Parse an .ics payload and store it as one new calendar plus its items:
     * 0 on success, -1 on failure (reason via [lastError]).
     */
    external fun importIcsJson(handle: Long, text: String, calendarName: String): Int

    /** Write every calendar and live item to `path` as one VCALENDAR: 0 on success, -1 on failure. */
    external fun exportIcsFile(handle: Long, path: String): Int

    /**
     * Check GitHub for a release newer than `currentVersion` and stage it at
     * `destPath`; the status JSON, or null when the call itself failed (reason
     * via [lastError]). Blocking: call it off the UI thread.
     */
    external fun checkUpdatesJson(currentVersion: String, destPath: String): String?

    /** One item as the core `CalendarItem` model JSON (literal `null` when absent), or null on failure. */
    external fun itemJson(handle: Long, id: String): String?

    /** Save one editor form: 0 on success, -1 on failure (reason via [lastError]). */
    external fun saveItemJson(handle: Long, formJson: String): Int

    /** Flip a task's completion: 0 on success, -1 on failure. */
    external fun toggleItemCompletedJson(handle: Long, id: String): Int

    /**
     * Tombstone an item (never a physical delete). Unlike the other commands
     * this one is count-shaped: 1 when a row was written, 0 when no such id
     * exists, -1 on failure — so callers must test `< 0`, not `== 0`.
     */
    external fun softDeleteItem(handle: Long, id: String): Int

    /** Delete one item or a scoped part of a recurring series: 0 on success, -1 on failure. */
    external fun deleteItemJson(handle: Long, formJson: String): Int

    /** Sync pairing status `{paired, fingerprint}`, or null on failure. */
    external fun syncStatusJson(handle: Long): String?

    /** Generate + persist a new chain: `{phrase, fingerprint}`, or null on failure. */
    external fun syncStartChainJson(handle: Long): String?

    /** Join a chain by its recovery phrase: `{phrase, fingerprint}`, or null on failure. */
    external fun syncJoinChainJson(handle: Long, phrase: String): String?

    /** Reveal the stored recovery phrase: `{phrase, fingerprint}`, or null when unpaired. */
    external fun syncPhraseJson(handle: Long): String?

    /** Leave the chain (local identity + outbox): 0 on success, -1 on failure. */
    external fun syncLeave(handle: Long): Int

    /**
     * Run one blocking sync round and return `{merged}`, or null on failure
     * (reason via [lastError]). Waits for peer discovery, so call it off the
     * UI thread.
     */
    external fun syncRoundJson(handle: Long): String?

    /** Devices seen on the chain: `{devices:[{id, revoked}]}`, or null on failure. */
    external fun syncDevicesJson(handle: Long): String?

    /**
     * Add a device id to this install's sync blocklist. Its snapshots are
     * dropped from now on. 0 on success, -1 on failure.
     */
    external fun syncRevokeDevice(handle: Long, deviceId: String): Int
}
