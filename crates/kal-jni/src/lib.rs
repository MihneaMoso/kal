//! Thin JNI bridge between the Kotlin Android app and Kal's versioned C ABI
//! (`kal-ffi`).
//!
//! The Kotlin side declares `external fun`s in
//! `com.kal.calendar.bridge.KalNative`; the symbols below are the JNI
//! implementations of exactly those declarations (`Java_<package>_<Class>_<method>`),
//! so the names here and the Kotlin declarations must move together. All real
//! work stays in `kal-ffi` — this layer only translates JVM↔C types, so the
//! single source of truth for the ABI remains `widgets/kal_ffi.h` +
//! `scripts/check-bridge-abi.sh`.
//!
//! Rules this layer keeps (guide §6/§12):
//! * no panic may unwind into the JVM — every export is `catch_unwind`-guarded;
//! * every native string is freed here (never leaked, never freed twice);
//! * an error returns NULL (query) or a negative status (command); the reason is
//!   read back with `kal_last_error()` through [`Java_com_kal_calendar_bridge_KalNative_lastError`].
//!
//! Additions stay additive within a bridge major version: the app-facing view
//! payloads (`monthJson`/`weekJson`/`dayJson`/`agendaJson`) sit beside the
//! frozen widget payloads in `kal-ffi` and are documented in `widgets/kal_ffi.h`.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

use kal_ffi::KalDb;

/// The bridge version the Kotlin app is written against. The app compares its
/// own constant with the value reported by
/// [`Java_com_kal_calendar_bridge_KalNative_bridgeVersion`] at startup and
/// refuses to run on a mismatch (Phase 1 handshake gate).
pub const KOTLIN_EXPECTS_BRIDGE_VERSION: u32 = kal_ffi::KAL_BRIDGE_VERSION;

/// Run `f`, turning a panic into `fallback`.
///
/// `kal-ffi` already guards its own boundary; this covers the JNI glue itself
/// (string conversion, handle casts), because unwinding into the JVM is
/// undefined behaviour.
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Copy a Kal-owned C string into a JVM string, freeing the native allocation.
fn into_jstring(env: &JNIEnv, ptr: *mut c_char) -> jstring {
    if ptr.is_null() {
        return ptr::null_mut();
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { kal_ffi::kal_free(ptr) };
    env.new_string(text)
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// Read a JVM string as a C string.
///
/// Returns `None` when the JVM string cannot be read or contains an interior
/// NUL byte. Callers then pass a NULL argument, so `kal-ffi` rejects the call
/// through its error channel instead of silently acting on an empty value.
fn into_cstring(env: &mut JNIEnv, value: &JString) -> Option<CString> {
    let text = env.get_string(value).ok()?.to_string_lossy().into_owned();
    cstring_from_text(text)
}

/// The pure half of [`into_cstring`]: `None` for any text a C string cannot
/// carry (an interior NUL).
fn cstring_from_text(text: String) -> Option<CString> {
    CString::new(text).ok()
}

/// The C pointer for a converted argument. NULL means "the JVM string could not
/// be represented", which every `kal-ffi` string argument reports as an error.
fn arg_ptr(arg: &Option<CString>) -> *const c_char {
    arg.as_ref().map_or(ptr::null(), |s| s.as_ptr())
}

/// Run a query export against a handle.
///
/// A zero handle is forwarded as NULL so `kal-ffi` records "null database
/// handle" on the error channel; short-circuiting here would leave a stale
/// error from an earlier call readable by the app.
fn query_handle<F>(env: &JNIEnv, handle: jlong, query: F) -> jstring
where
    F: FnOnce(*mut KalDb) -> *mut c_char,
{
    into_jstring(env, query(handle as *mut KalDb))
}

/// Run a command export against a handle. Zero is forwarded as NULL for the
/// same reason as [`query_handle`]: the FFI, not the JNI glue, owns the error
/// message.
fn command_handle<F>(handle: jlong, command: F) -> jint
where
    F: FnOnce(*mut KalDb) -> i32,
{
    command(handle as *mut KalDb)
}

/// `bridgeVersion(): Int` — native bridge schema version for the handshake.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_bridgeVersion(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    guard(0, || kal_ffi::kal_bridge_version() as jint)
}

/// `lastError(): String?` — last error on the calling thread, or `null`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_lastError(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    guard(ptr::null_mut(), move || {
        into_jstring(&env, kal_ffi::kal_last_error())
    })
}

/// `open(path: String): Long` — open (or create) the DB; 0 on failure.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_open(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jlong {
    // Converted outside the guard: `into_cstring` cannot panic, and the closure
    // then only needs to carry the handle. A string the JVM cannot represent
    // becomes NULL, which `kal_open` reports instead of opening an empty path.
    let cpath = into_cstring(&mut env, &path);
    guard(0, move || {
        let db = unsafe { kal_ffi::kal_open(arg_ptr(&cpath)) };
        db as jlong
    })
}

/// `close(handle: Long)` — close a handle from `open`; double-close is safe.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_close(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    guard((), move || {
        if handle != 0 {
            let mut slot = handle as *mut KalDb;
            unsafe { kal_ffi::kal_close(&mut slot) };
        }
    })
}

/// `calendarsJson(handle: Long): String?` — all calendars as core-model JSON.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_calendarsJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_calendars_json(db)
        })
    })
}

/// `settingsJson(handle: Long): String?` — every setting as one JSON object.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_settingsJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe { kal_ffi::kal_settings_json(db) })
    })
}

/// `getSetting(handle: Long, key: String): String?` — the setting as JSON text
/// (the literal `null` when absent).
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_getSetting(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    key: JString,
) -> jstring {
    let key = into_cstring(&mut env, &key);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_get_setting(db, arg_ptr(&key))
        })
    })
}

/// `setSetting(handle: Long, key: String, value: String): Int` — 0 on success.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_setSetting(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    key: JString,
    value: JString,
) -> jint {
    let key = into_cstring(&mut env, &key);
    let value = into_cstring(&mut env, &value);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_set_setting(db, arg_ptr(&key), arg_ptr(&value))
        })
    })
}

/// `upsertCalendarJson(handle: Long, json: String): Int` — 0 on success.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_upsertCalendarJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    json: JString,
) -> jint {
    let json = into_cstring(&mut env, &json);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_upsert_calendar_json(db, arg_ptr(&json))
        })
    })
}

/// `monthJson(handle: Long, year: Int, month: Int, firstDow: Int): String?`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_monthJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    year: jint,
    month: jint,
    first_dow: jint,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_month_json(db, year, month as u32, first_dow as u8)
        })
    })
}

/// `weekJson(handle: Long, weekStart: String): String?` — 7 days from the start.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_weekJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    week_start: JString,
) -> jstring {
    let start = into_cstring(&mut env, &week_start);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_week_json(db, arg_ptr(&start))
        })
    })
}

/// `dayJson(handle: Long, date: String): String?` — all-day strip + positions.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_dayJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    date: JString,
) -> jstring {
    let date = into_cstring(&mut env, &date);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_day_json(db, arg_ptr(&date))
        })
    })
}

/// `agendaJson(handle: Long, from: String, days: Long, cap: Long): String?`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_agendaJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    from: JString,
    days: jlong,
    cap: jlong,
) -> jstring {
    let from = into_cstring(&mut env, &from);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_agenda_json(db, arg_ptr(&from), days, cap)
        })
    })
}

/// `reminderFiringsJson(handle: Long, horizonDays: Long, cap: Long): String?` —
/// the reminder firings the Kotlin AlarmManager backend arms as exact alarms.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_reminderFiringsJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    horizon_days: jlong,
    cap: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_reminder_firings_json(db, horizon_days, cap)
        })
    })
}

/// `importIcsJson(handle: Long, text: String, calendarName: String): Int` —
/// parse and store one .ics as a new calendar plus its items; 0 ok / -1 with
/// the reason on `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_importIcsJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    text: JString,
    calendar_name: JString,
) -> jint {
    let text = into_cstring(&mut env, &text);
    let calendar_name = into_cstring(&mut env, &calendar_name);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_import_ics_json(db, arg_ptr(&text), arg_ptr(&calendar_name))
        })
    })
}

/// `exportIcsFile(handle: Long, path: String): Int` — write the whole calendar
/// to `path` as one VCALENDAR; 0 ok / -1 with the reason on `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_exportIcsFile(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    path: JString,
) -> jint {
    let path = into_cstring(&mut env, &path);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_export_ics_file(db, arg_ptr(&path))
        })
    })
}

/// `checkUpdatesJson(currentVersion: String, destPath: String): String` —
/// check GitHub for a newer release, stage it at destPath, and return the
/// status JSON; null only for a bad argument (reason on `lastError()`).
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_checkUpdatesJson(
    mut env: JNIEnv,
    _class: JClass,
    current_version: JString,
    dest_path: JString,
) -> jstring {
    let current_version = into_cstring(&mut env, &current_version);
    let dest_path = into_cstring(&mut env, &dest_path);
    let ptr = guard(ptr::null_mut(), move || unsafe {
        kal_ffi::kal_check_updates_json(arg_ptr(&current_version), arg_ptr(&dest_path))
    });
    into_jstring(&env, ptr)
}

/// `itemJson(handle: Long, id: String): String?` — one item as core-model JSON
/// (the literal `null` when absent), for prefilling the editor.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_itemJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) -> jstring {
    let id = into_cstring(&mut env, &id);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_item_json(db, arg_ptr(&id))
        })
    })
}

/// `saveItemJson(handle: Long, formJson: String): Int` — create/update one item
/// from an editor form; 0 on success, -1 with the reason on `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_saveItemJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    form_json: JString,
) -> jint {
    let form_json = into_cstring(&mut env, &form_json);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_save_item_json(db, arg_ptr(&form_json))
        })
    })
}

/// `toggleItemCompletedJson(handle: Long, id: String): Int` — flip a task's
/// completion; 0 on success, -1 with the reason on `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_toggleItemCompletedJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) -> jint {
    let id = into_cstring(&mut env, &id);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_toggle_item_completed_json(db, arg_ptr(&id))
        })
    })
}

/// `softDeleteItem(handle: Long, id: String): Int` — tombstone an item. Unlike
/// the other commands this is count-shaped: 1 when a row was written, 0 when no
/// such id exists, -1 on failure (the reason on `lastError()`).
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_softDeleteItem(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) -> jint {
    let id = into_cstring(&mut env, &id);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_soft_delete_item(db, arg_ptr(&id))
        })
    })
}

/// `deleteItemJson(handle: Long, formJson: String): Int` — delete one item or
/// a scoped part of a recurring series; 0 on success, -1 with the reason on
/// `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_deleteItemJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    form_json: JString,
) -> jint {
    let form_json = into_cstring(&mut env, &form_json);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_delete_item_json(db, arg_ptr(&form_json))
        })
    })
}

/// `syncStatusJson(handle: Long): String?` — `{paired, fingerprint}`, or null
/// on failure (the reason on `lastError()`). Reading never creates an identity.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncStatusJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_status_json(db)
        })
    })
}

/// `syncStartChainJson(handle: Long): String?` — generate + persist a new chain
/// and return `{phrase, fingerprint}` so the panel can show the phrase once.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncStartChainJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_start_chain_json(db)
        })
    })
}

/// `syncJoinChainJson(handle: Long, phrase: String): String?` — join an existing
/// chain; null with "Invalid recovery phrase" on bad input.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncJoinChainJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    phrase: JString,
) -> jstring {
    let phrase = into_cstring(&mut env, &phrase);
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_join_chain_json(db, arg_ptr(&phrase))
        })
    })
}

/// `syncPhraseJson(handle: Long): String?` — reveal the stored recovery phrase.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncPhraseJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_phrase_json(db)
        })
    })
}

/// `syncLeave(handle: Long): Int` — leave the chain; 0 on success, -1 on error.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncLeave(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    guard(-1, move || {
        command_handle(handle, |db| unsafe { kal_ffi::kal_sync_leave(db) })
    })
}

/// `syncRoundJson(handle: Long): String?` — one blocking round: `{merged}`, or
/// null with the reason on `lastError()`. Kotlin calls this off the UI thread.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncRoundJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_round_json(db)
        })
    })
}

/// `syncDevicesJson(handle: Long): String?` — `{devices:[{id, revoked}]}`, or
/// null with the reason on `lastError()`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncDevicesJson(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    guard(ptr::null_mut(), move || {
        query_handle(&env, handle, |db| unsafe {
            kal_ffi::kal_sync_devices_json(db)
        })
    })
}

/// `syncRevokeDevice(handle: Long, deviceId: String): Int` — add a device to
/// the local blocklist; 0 on success, -1 on failure.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_bridge_KalNative_syncRevokeDevice(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    device_id: JString,
) -> jint {
    let device_id = into_cstring(&mut env, &device_id);
    guard(-1, move || {
        command_handle(handle, |db| unsafe {
            kal_ffi::kal_sync_revoke_device(db, arg_ptr(&device_id))
        })
    })
}

// ---------------------------------------------------------------------------
// Home-screen widgets (`com.kal.calendar.widgets.KalWidgets`)
//
// The widgets run in the same process as the app but hold no handle of their
// own, so each call opens the database for the duration of one snapshot. The
// payloads are the frozen widget exports (`kal_upcoming_json` /
// `kal_month_grid_json`), unchanged since the Dioxus build.
// ---------------------------------------------------------------------------

const SECONDS_PER_DAY: i64 = 86_400;

/// `nativeSchedule(path, fromEpochSeconds, days)` → upcoming JSON snapshot.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_widgets_KalWidgets_nativeSchedule(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    from_epoch: jlong,
    days: jint,
) -> jstring {
    let cpath = into_cstring(&mut env, &path);
    guard(ptr::null_mut(), move || {
        let db = unsafe { kal_ffi::kal_open(arg_ptr(&cpath)) };
        if db.is_null() {
            return ptr::null_mut();
        }
        let to = from_epoch.saturating_add(i64::from(days) * SECONDS_PER_DAY);
        let json = unsafe { kal_ffi::kal_upcoming_json(db, from_epoch, to) };
        let mut slot = db;
        unsafe { kal_ffi::kal_close(&mut slot) };
        into_jstring(&env, json)
    })
}

/// `nativeMonth(path, year, month, firstDow)` → JSON 6×7 month grid.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_widgets_KalWidgets_nativeMonth(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    year: jint,
    month: jint,
    first_dow: jint,
) -> jstring {
    let cpath = into_cstring(&mut env, &path);
    guard(ptr::null_mut(), move || {
        let db = unsafe { kal_ffi::kal_open(arg_ptr(&cpath)) };
        if db.is_null() {
            return ptr::null_mut();
        }
        let json = unsafe { kal_ffi::kal_month_grid_json(db, year, month as u32, first_dow as u8) };
        let mut slot = db;
        unsafe { kal_ffi::kal_close(&mut slot) };
        into_jstring(&env, json)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every JNI symbol the Kotlin `KalNative` object declares must exist with
    /// the expected signature: taking each address fails compilation if one is
    /// renamed or removed, which is the Rust half of the guide §13
    /// signature-compatibility gate.
    #[test]
    fn jni_exports_match_the_kotlin_declarations() {
        let exports: [*const (); 32] = [
            Java_com_kal_calendar_bridge_KalNative_bridgeVersion as *const (),
            Java_com_kal_calendar_bridge_KalNative_lastError as *const (),
            Java_com_kal_calendar_bridge_KalNative_open as *const (),
            Java_com_kal_calendar_bridge_KalNative_close as *const (),
            Java_com_kal_calendar_bridge_KalNative_calendarsJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_settingsJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_getSetting as *const (),
            Java_com_kal_calendar_bridge_KalNative_setSetting as *const (),
            Java_com_kal_calendar_bridge_KalNative_upsertCalendarJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_monthJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_weekJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_dayJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_agendaJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_reminderFiringsJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_itemJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_saveItemJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_toggleItemCompletedJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_softDeleteItem as *const (),
            Java_com_kal_calendar_bridge_KalNative_deleteItemJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_importIcsJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_exportIcsFile as *const (),
            Java_com_kal_calendar_bridge_KalNative_checkUpdatesJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncStatusJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncStartChainJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncJoinChainJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncPhraseJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncLeave as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncRoundJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncDevicesJson as *const (),
            Java_com_kal_calendar_bridge_KalNative_syncRevokeDevice as *const (),
            Java_com_kal_calendar_widgets_KalWidgets_nativeSchedule as *const (),
            Java_com_kal_calendar_widgets_KalWidgets_nativeMonth as *const (),
        ];
        assert!(exports.iter().all(|p| !p.is_null()));
    }

    /// The handshake is only meaningful if both sides agree on the number; a
    /// change here must be a deliberate ABI bump.
    #[test]
    fn bridge_version_is_pinned() {
        assert_eq!(KOTLIN_EXPECTS_BRIDGE_VERSION, 3);
    }

    /// An interior NUL used to collapse to `""` (`unwrap_or_default`), so a
    /// command could act on an empty key/path instead of failing. The argument
    /// must now become NULL, which the FFI reports through `lastError`.
    #[test]
    fn strings_a_c_string_cannot_carry_become_null_arguments() {
        assert!(cstring_from_text("a\0b".to_string()).is_none());
        assert!(arg_ptr(&cstring_from_text("a\0b".to_string())).is_null());

        let ok = cstring_from_text("kal".to_string());
        assert!(ok.is_some());
        assert!(!arg_ptr(&ok).is_null());
        assert_eq!(
            unsafe { CStr::from_ptr(arg_ptr(&ok)) }.to_str().unwrap(),
            "kal"
        );
    }
}
