//! JNI bridges for the Android home-screen widgets.
//!
//! The widgets run in the app process (same `libmain.so`), so their Kotlin
//! classes call these native methods to snapshot calendar data as JSON. The
//! heavy lifting reuses the stable `kal-ffi` C ABI — the same queries iOS
//! WidgetKit and future shims consume — so logic stays in one place.

use std::ffi::CStr;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

const DAY_SECS: i64 = 86_400;

fn cstring(path: String) -> std::ffi::CString {
    std::ffi::CString::new(path).unwrap_or_else(|_| std::ffi::CString::new("").unwrap())
}

fn take_c_string(ptr: *mut std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let out = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { kal_ffi::kal_free(ptr) };
    out
}

/// `nativeSchedule(path, fromEpochSeconds, days)` → JSON array of upcoming
/// events/tasks/birthdays sorted by start in `[from, from+days)`.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_widgets_KalWidgets_nativeSchedule(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    from_epoch: jlong,
    days: jint,
) -> jstring {
    let path = env
        .get_string(&path)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cpath = cstring(path);
    let db = unsafe { kal_ffi::kal_open(cpath.as_ptr()) };
    if db.is_null() {
        return std::ptr::null_mut();
    }
    let to = from_epoch.saturating_add(days as i64 * DAY_SECS);
    let json_ptr = unsafe { kal_ffi::kal_upcoming_json(db, from_epoch, to) };
    let json = take_c_string(json_ptr);
    let mut slot = db;
    unsafe { kal_ffi::kal_close(&mut slot) };
    env.new_string(&json)
        .ok()
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// `nativeMonth(path, year, month, firstDow)` → JSON 6×7 month grid with
/// per-day item colors. `firstDow` 0 = Monday … 6 = Sunday.
#[no_mangle]
pub extern "system" fn Java_com_kal_calendar_widgets_KalWidgets_nativeMonth(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    year: jint,
    month: jint,
    first_dow: jint,
) -> jstring {
    let path = env
        .get_string(&path)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cpath = cstring(path);
    let db = unsafe { kal_ffi::kal_open(cpath.as_ptr()) };
    if db.is_null() {
        return std::ptr::null_mut();
    }
    let json_ptr =
        unsafe { kal_ffi::kal_month_grid_json(db, year as i32, month as u32, first_dow as u8) };
    let json = take_c_string(json_ptr);
    let mut slot = db;
    unsafe { kal_ffi::kal_close(&mut slot) };
    env.new_string(&json)
        .ok()
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
