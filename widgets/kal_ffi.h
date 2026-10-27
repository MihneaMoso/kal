/* Kal — stable C ABI surface consumed by native widget shims and (from the
 * Kotlin Android rewrite on) the Kotlin interface layer.
 * Regenerate with cbindgen if signatures change; keep in sync manually until
 * cbindgen is added to CI. */

#ifndef KAL_FFI_H
#define KAL_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Bridge schema version. The Kotlin declaration set must match this value;
 * any new required entry point bumps it (additive JSON fields do not).
 * Callers handshake on this before any other call. */
#define KAL_BRIDGE_VERSION 2u
uint32_t kal_bridge_version(void);

/* Opaque database handle. */
typedef struct KalDb KalDb;

/* Open (or create) the calendar database at `path` (UTF-8, NUL-terminated).
 * Returns NULL on failure. */
KalDb *kal_open(const char *path);

/* Close the handle and NULL out the caller's slot; double-close is safe. */
void kal_close(KalDb **db);

/* Free any string returned by the kal_* functions below. */
void kal_free(char *s);

/* Last error message set by a failed call on the CALLING THREAD, or NULL when
 * the previous call succeeded. Free with kal_free(). Query functions signal
 * failure with NULL, commands with a negative status; both set this channel. */
char *kal_last_error(void);

/* --- calendar data (the core's own model JSON) ------------------------- */

/* All calendars as a JSON array of the core Calendar model. NULL on error. */
char *kal_calendars_json(KalDb *db);

/* All items as a JSON array of the core CalendarItem model.
 * include_deleted non-zero includes tombstoned rows. NULL on error. */
char *kal_items_json(KalDb *db, int32_t include_deleted);

/* One item as a JSON object, or the literal `null` when absent. NULL on error. */
char *kal_item_json(KalDb *db, const char *id);

/* Full-row upsert of an item from its JSON model. 0 ok / -1 error. */
int32_t kal_upsert_item_json(KalDb *db, const char *json);

/* Tombstone an item (never a physical delete): 1 when a row was written,
 * 0 when no such id exists, -1 on error. Re-deleting refreshes the LWW
 * timestamp that propagates the deletion, so it returns 1 as well. */
int32_t kal_soft_delete_item(KalDb *db, const char *id);

/* Delete one item or a scoped part of a recurring series from the JSON form
 * {id, occurrenceStart?, scope?}. scope is "all" (default), "this_only", or
 * "this_and_following"; a missing id is idempotent success. 0 ok / -1 error. */
int32_t kal_delete_item_json(KalDb *db, const char *json);

/* Full-row upsert of a calendar from its JSON model. 0 ok / -1 error. */
int32_t kal_upsert_calendar_json(KalDb *db, const char *json);

/* --- settings (device-local key/value store) -------------------------- */

/* A setting as JSON: literal `null` when absent, else a JSON string.
 * Never pre-JSON-encode values on write (see kal_set_setting). NULL on error. */
char *kal_get_setting(KalDb *db, const char *key);

/* Write a setting as a PLAIN string value. 0 ok / -1 error. */
int32_t kal_set_setting(KalDb *db, const char *key, const char *value);

/* Every setting as one JSON object of key -> string. NULL on error. */
char *kal_settings_json(KalDb *db);

/* --- expanded views --------------------------------------------------- */

/*
 * Upcoming occurrences overlapping [from_epoch, to_epoch] as a JSON array:
 * [{ "id": str, "title": str, "start": rfc3339, "end": rfc3339|null,
 *    "allDay": bool, "kind": "event"|"task"|"birthday",
 *    "color": "#RRGGBB", "age": int|null }]
 * Sorted by start. Recurring rules are expanded. Returns NULL on error;
 * free with kal_free().
 */
char *kal_upcoming_json(KalDb *db, int64_t from_epoch, int64_t to_epoch);

/*
 * Month grid as JSON rows (6 weeks x 7 days):
 * [{ "date": "YYYY-MM-DD", "inMonth": bool,
 *    "items": [{ "id": str, "title": str, "time": "HH:MM"|"" }] }]
 * first_dow: 0 = Monday ... 6 = Sunday. Returns NULL on error.
 */
char *kal_month_grid_json(KalDb *db, int32_t year, uint32_t month, uint8_t first_dow);

/* --- app-facing expanded views ---------------------------------------- */

/*
 * The interface layer's views render these four payloads. They differ from the
 * widget surface above by carrying the shared display entry, which adds
 * "calendarId" (so the client can apply the calendar-visibility filter),
 * "kind" (birthday chips/labels) and the raw "start" instant (so times are
 * formatted from the user's own preference rather than pre-baked):
 *   { "id": str, "calendarId": str, "title": str, "start": rfc3339,
 *     "end": rfc3339|null, "allDay": bool,
 *     "kind": "event"|"task"|"birthday", "completed": bool,
 *     "color": "#RRGGBB", "overrideColor": "#RRGGBB"|null,
 *     "age": int|null }
 * `color` is the effective colour (item override, else the calendar's) used by
 * item dots and day blocks; `overrideColor` is the item's own, which is all the
 * month/week chips ever used.
 * All four return NULL on error; free with kal_free().
 */

/*
 * Month grid: 6 rows x 7 days of
 * [{ "date": "YYYY-MM-DD", "inMonth": bool, "items": [entry] }].
 * first_dow: 0 = Monday ... 6 = Sunday.
 */
char *kal_month_json(KalDb *db, int32_t year, uint32_t month, uint8_t first_dow);

/*
 * One week: 7 days of
 * [{ "date": "YYYY-MM-DD", "items": [entry] }] from week_start onward.
 * week_start: "YYYY-MM-DD" (the caller anchors it on Monday, as before).
 */
char *kal_week_json(KalDb *db, const char *week_start);

/*
 * One day: { "date": "YYYY-MM-DD", "allDay": [entry],
 *            "timed": [{ "item": entry, "topFrac": f64, "heightFrac": f64,
 *                        "lane": int, "lanes": int }] }
 * Fractions are of a 24-hour canvas (positioned by the same layout the Dioxus
 * day view used). date: "YYYY-MM-DD".
 */
char *kal_day_json(KalDb *db, const char *date);

/*
 * Agenda rows for [from, from + days] (inclusive) capped at cap rows, in
 * date-then-time order: [{ "date": "YYYY-MM-DD", "item": entry }].
 * from: "YYYY-MM-DD".
 */
char *kal_agenda_json(KalDb *db, const char *from, int64_t days, int64_t cap);

/* --- editor writes (app-facing) --------------------------------------- */

/*
 * Save one editor form: create or update an item and, for a recurring series,
 * apply the requested scope. The form mirrors the app's EditorState so the
 * write semantics (default-calendar resolution, load-or-create, EXDATE/UNTIL
 * surgery) stay native while the form widgets live in the interface layer:
 *
 *   { "id": ulid|null,                 // null/absent creates a new item
 *     "occurrenceStart": rfc3339|null, // the CLICKED occurrence, for scoping
 *     "kind": "event"|"task"|"birthday",
 *     "title": str,
 *     "date": "YYYY-MM-DD",            // times read in the device offset
 *     "startTime": "HH:MM"|"",         // "" = midnight
 *     "endTime": "HH:MM"|"",           // "" = no end
 *     "allDay": bool,
 *     "calendarId": ulid|null,         // null -> first visible live calendar
 *     "location": str,                 // "" -> null
 *     "person": str,                   // birthday linkage, other kinds ignore
 *     "rrule": str|null,               // null/"" -> doesn't repeat
 *     "reminders": [int],              // minutes before start
 *     "customReminder": str,           // raw text, used when positive
 *     "color": "#RRGGBB"|null,         // null/"" -> use the calendar colour
 *     "scope": "all"|"this_only"|"this_and_following" }
 *
 * Returns 0 on success, -1 on failure with the reason on kal_last_error() —
 * including validation rejections (empty title, end before start), which the
 * editor surfaces instead of silently dropping. Note that an empty "person"
 * is still written as an empty birthday linkage: the core only rejects a
 * MISSING one, and the editor never sends it missing.
 */
int32_t kal_save_item_json(KalDb *db, const char *json);

/*
 * Flip a task's completion and refresh its LWW timestamp. 0 on success,
 * -1 on failure with the reason on kal_last_error().
 */
int32_t kal_toggle_item_completed_json(KalDb *db, const char *id);

/* --- reminder firings (app-facing) ------------------------------------ */

/*
 * Every reminder firing in [now, now + horizon_days], nearest first, at most
 * cap of them (cap <= 0 = no cap), as:
 *
 *   [{ "itemId": ulid, "reminderId": ulid,
 *      "fireAt": rfc3339,              // instant the alarm must fire
 *      "title": str }]
 *
 * Computed by kal-core::reminders::compute_firings over EVERY live item — not
 * filtered by calendar visibility, exactly like the Dioxus reconcile effect.
 * Past firings are dropped (no catch-up), completed tasks and tombstoned items
 * are silent, and horizon_days is clamped by the core to 366 days.
 * NULL on failure with the reason on kal_last_error().
 */
char *kal_reminder_firings_json(KalDb *db, int64_t horizon_days, int64_t cap);

/* --- ics import / export (app-facing) ---------------------------------- */

/*
 * Parse an .ics payload and store it as ONE new calendar plus its items,
 * mirroring the Dioxus sidebar's import: the calendar is an IcsImport source,
 * visible, named from the file's X-WR-CALNAME when present and from
 * calendar_name otherwise; an item the store rejects is skipped while the rest
 * still land. 0 on success, -1 on failure (parse error, NULL argument, dead
 * handle) with the reason on kal_last_error().
 */
int32_t kal_import_ics_json(KalDb *db, const char *text, const char *calendar_name);

/*
 * Write every calendar and live item to `path` as one VCALENDAR — the file the
 * system share sheet hands out. Rust owns the write (the interface layer only
 * chooses the path and shares the result). 0 on success, -1 on failure with the
 * reason on kal_last_error().
 */
int32_t kal_export_ics_file(KalDb *db, const char *path);

/* --- update check (app-facing) ---------------------------------------- */

/*
 * Check the latest GitHub release against current_version and, when it is
 * strictly newer, download and SHA-256-verify it into dest_path. One blocking
 * pass (the Dioxus button downloaded before it reported too), so the interface
 * layer must call it off the UI thread.
 *
 * Always a JSON object:
 *   {"status": "up_to_date"}
 *   {"status": "update_ready", "version": "0.3.0"}
 *   {"status": "error", "message": "update check failed: ..."}
 *
 * current_version is supplied by the caller (the app's own build version), not
 * compiled into the library. NULL only for a NULL argument or an unsupported
 * platform, with the reason on kal_last_error().
 */
char *kal_check_updates_json(const char *current_version, const char *dest_path);

/* --- sync chain (app-facing) ------------------------------------------- */

/*
 * The Sync panel's whole surface. The identity files (sync-identity.json,
 * sync-node.json, sync-outbox/) live next to the database path passed to
 * kal_open — the exact files the Dioxus app wrote, so a device keeps its chain
 * across the rewrite. Reading status never mints an identity.
 */

/*
 * Pairing status:
 *   {"paired": false, "fingerprint": null}
 *   {"paired": true,  "fingerprint": "<16 hex chars>"}
 * NULL on error.
 */
char *kal_sync_status_json(KalDb *db);

/*
 * Generate a fresh 24-word chain and persist it:
 *   {"phrase": "word word ...", "fingerprint": "..."}
 * The phrase is shown once so the user can write it down. NULL on error.
 */
char *kal_sync_start_chain_json(KalDb *db);

/*
 * Join an existing chain by its recovery phrase (whitespace-trimmed).
 * Same JSON as start; an invalid phrase fails with "Invalid recovery phrase"
 * on kal_last_error() and leaves the device unpaired. NULL on error.
 */
char *kal_sync_join_chain_json(KalDb *db, const char *phrase);

/* Reveal the stored phrase for pairing another device. NULL when unpaired. */
char *kal_sync_phrase_json(KalDb *db);

/*
 * Leave the chain: delete the local identity + outbox and drop the cached
 * transport. The per-device sync-node.json is kept. 0 ok / -1 error.
 */
int32_t kal_sync_leave(KalDb *db);

/*
 * One BLOCKING sync round (waits up to ~15 s for a peer, then drains for up to
 * ~12 s), so call it off the UI thread:
 *   {"merged": N}    // N peer snapshots merged; 0 = nothing new
 * NULL on failure with the reason on kal_last_error() ("not paired",
 * "live P2P: no peers yet ...").
 */
char *kal_sync_round_json(KalDb *db);

#ifdef __cplusplus
}
#endif

#endif /* KAL_FFI_H */
