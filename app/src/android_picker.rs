//! Native Android image picker via JNI.
//!
//! The WebView's `<input type="file">` does not reliably propagate selected
//! bytes back to Dioxus on Android (the `onchange` fires but `file.read_bytes`
//! returns empty).  This module bypasses the WebView entirely by calling
//! `KalFilePicker.pickImage()` through JNI, which opens the system gallery and
//! blocks until the user selects or cancels.

use std::sync::mpsc;

/// A picked image: its MIME type (from the content resolver) and raw bytes.
pub struct PickedImage {
    pub mime: Option<String>,
    pub bytes: Vec<u8>,
}

/// Open the system image picker and block until a file is selected.
///
/// Returns the raw image, or `None` on cancel / timeout / error.
/// Must NOT be called on the Android main thread (it blocks for up to 30 s).
fn pick_image_sync() -> Option<PickedImage> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let cls = env.find_class("com/kal/calendar/KalFilePicker").ok()?;

    // Launch the picker (dispatches to UI thread inside Kotlin).
    env.call_static_method(
        &cls,
        "pickImage",
        "(Landroid/app/Activity;)V",
        &[jni::objects::JValue::Object(&activity)],
    )
    .ok()?;

    // Block until the user picks or cancels (up to 30 s).
    let uri = env
        .call_static_method(
            &cls,
            "waitForResult",
            "(J)Landroid/net/Uri;",
            &[jni::objects::JValue::Long(30_000)],
        )
        .ok()?
        .l()
        .ok()?;

    if uri.is_null() {
        return None;
    }

    let mime = env
        .call_static_method(
            &cls,
            "mimeType",
            "(Landroid/content/Context;Landroid/net/Uri;)Ljava/lang/String;",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Object(&uri),
            ],
        )
        .ok()?
        .l()
        .ok()
        .filter(|s| !s.is_null())
        .and_then(|s| {
            let jstr = jni::objects::JString::from(s);
            env.get_string(&jstr)
                .ok()
                .map(|x| x.to_string_lossy().into_owned())
        });

    // Read the bytes from the content:// URI. `readBytes` returns a Java
    // `byte[]`, surfaced here as a `JObject`; convert it to a `JByteArray`
    // (jni 0.21's `JPrimitiveArray<sys::jbyte>`) before reading its region.
    let bytes_obj = env
        .call_static_method(
            &cls,
            "readBytes",
            "(Landroid/content/Context;Landroid/net/Uri;)[B",
            &[
                jni::objects::JValue::Object(&activity),
                jni::objects::JValue::Object(&uri),
            ],
        )
        .ok()?
        .l()
        .ok()?;

    let byte_arr: jni::objects::JByteArray = bytes_obj.into();
    let len = env.get_array_length(&byte_arr).ok()? as usize;
    let mut buf: Vec<i8> = vec![0i8; len];
    env.get_byte_array_region(&byte_arr, 0, &mut buf).ok()?;
    let bytes: Vec<u8> = buf.into_iter().map(|b| b as u8).collect();
    Some(PickedImage { mime, bytes })
}

/// Spawn a blocking task that opens the native image picker and return a
/// channel that will receive the picked image (or `None` on cancel / error).
pub fn pick_image_async() -> mpsc::Receiver<Option<PickedImage>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = pick_image_sync();
        let _ = tx.send(result);
    });
    rx
}

/// Stub for non-Android builds so the module compiles everywhere.
#[cfg(not(target_os = "android"))]
pub fn pick_image_async() -> mpsc::Receiver<Option<PickedImage>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(None);
    rx
}
