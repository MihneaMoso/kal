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

    match try_pick_image(&mut env, &activity) {
        Ok(picked) => picked,
        Err(_) => {
            // A failed JNI call leaves a pending Java exception on this
            // thread.  If it survives until the thread detaches, ART reports
            // it as an uncaught exception and kills the whole app — so a
            // cancelled/failed pick must clear it before returning.
            let _ = env.exception_clear();
            None
        }
    }
}

/// Run the actual pick flow, surfacing JNI errors so the caller can clean up
/// the pending Java exception (see [`pick_image_sync`]).
fn try_pick_image<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'local>,
) -> jni::errors::Result<Option<PickedImage>> {
    use jni::objects::JValue;

    let cls = find_kal_file_picker(env, activity)?;

    // Launch the picker (dispatches to UI thread inside Kotlin).
    env.call_static_method(
        &cls,
        "pickImage",
        "(Landroid/app/Activity;)V",
        &[JValue::Object(activity)],
    )?;

    let bytes = wait_and_read(env, activity, &cls)?;
    let Some((uri, bytes)) = bytes else {
        return Ok(None);
    };
    // The image flow also resolves the MIME type for the avatar pipeline.
    let mime = mime_of(env, activity, &cls, &uri)?;
    Ok(Some(PickedImage { mime, bytes }))
}

/// Block for the pick result, then read the picked content URI fully.
/// Shared by the image and generic-file flows; `Ok(None)` = cancelled/timed out.
#[allow(clippy::type_complexity)]
fn wait_and_read<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'local>,
    cls: &jni::objects::JClass<'local>,
) -> jni::errors::Result<Option<(jni::objects::JObject<'local>, Vec<u8>)>> {
    // Block until the user picks or cancels (up to 30 s).
    let uri = env
        .call_static_method(
            cls,
            "waitForResult",
            "(J)Landroid/net/Uri;",
            &[jni::objects::JValue::Long(30_000)],
        )?
        .l()?;

    if uri.is_null() {
        return Ok(None);
    }

    // Read the bytes from the content:// URI. `readBytes` returns a Java
    // `byte[]`, surfaced here as a `JObject`; convert it to a `JByteArray`
    // (jni 0.21's `JPrimitiveArray<sys::jbyte>`) before reading its region.
    let bytes_obj = env
        .call_static_method(
            cls,
            "readBytes",
            "(Landroid/content/Context;Landroid/net/Uri;)[B",
            &[
                jni::objects::JValue::Object(activity),
                jni::objects::JValue::Object(&uri),
            ],
        )?
        .l()?;

    let byte_arr: jni::objects::JByteArray = bytes_obj.into();
    let len = env.get_array_length(&byte_arr)? as usize;
    let mut buf: Vec<i8> = vec![0i8; len];
    env.get_byte_array_region(&byte_arr, 0, &mut buf)?;
    Ok(Some((uri, buf.into_iter().map(|b| b as u8).collect())))
}

fn mime_of<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'local>,
    cls: &jni::objects::JClass<'local>,
    uri: &jni::objects::JObject<'local>,
) -> jni::errors::Result<Option<String>> {
    use jni::objects::{JString, JValue};

    let mime = env
        .call_static_method(
            cls,
            "mimeType",
            "(Landroid/content/Context;Landroid/net/Uri;)Ljava/lang/String;",
            &[JValue::Object(activity), JValue::Object(uri)],
        )?
        .l()?;
    if mime.is_null() {
        return Ok(None);
    }
    let jstr = JString::from(mime);
    let text = env.get_string(&jstr)?.to_string_lossy().into_owned();
    Ok(Some(text))
}

/// Resolve `com.kal.calendar.KalFilePicker` through the activity's class
/// loader.
///
/// `JNIEnv::find_class` on a thread attached from native code (via
/// `AttachCurrentThread`) looks the class up with the *system* class loader,
/// which only knows framework classes — application classes like
/// `KalFilePicker` throw `ClassNotFoundException`, and the pending exception
/// crashes the app when the thread detaches.  Going through
/// `Class.getClassLoader().loadClass(...)` uses the app's real class loader
/// and works from any attached thread (all the classes/methods involved are
/// framework classes, so no `FindClass` of an app class is ever needed).
fn find_kal_file_picker<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'local>,
) -> jni::errors::Result<jni::objects::JClass<'local>> {
    use jni::objects::{JString, JValue};

    let activity_cls: jni::objects::JObject = env.get_object_class(activity)?.into();
    let loader = env
        .call_method(
            &activity_cls,
            "getClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )?
        .l()?;

    let name: JString = env.new_string("com/kal/calendar/KalFilePicker")?;
    let name_obj: jni::objects::JObject = name.into();
    let cls = env
        .call_method(
            loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name_obj)],
        )?
        .l()?;

    Ok(cls.into())
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

/// Open the system document picker for `mime` and block until a file is
/// selected. Same threading constraints as [`pick_image_sync`].
fn pick_file_sync(mime: &str) -> Option<Vec<u8>> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    match try_pick_file(&mut env, &activity, mime) {
        Ok(picked) => picked,
        Err(_) => {
            let _ = env.exception_clear();
            None
        }
    }
}

fn try_pick_file<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'local>,
    mime: &str,
) -> jni::errors::Result<Option<Vec<u8>>> {
    use jni::objects::JValue;

    let cls = find_kal_file_picker(env, activity)?;
    let jmime = env.new_string(mime)?;
    env.call_static_method(
        &cls,
        "pickFile",
        "(Landroid/app/Activity;Ljava/lang/String;)V",
        &[JValue::Object(activity), JValue::Object(&jmime.into())],
    )?;

    Ok(wait_and_read(env, activity, &cls)?.map(|(_, bytes)| bytes))
}

/// Spawn a blocking task that opens the native document picker and return a
/// channel with the picked file's bytes (or `None` on cancel / error).
/// `.ics` files are rarely registered under `text/calendar`, so callers
/// should pass `"*/*"` and validate content themselves.
pub fn pick_file_async(mime: &str) -> mpsc::Receiver<Option<Vec<u8>>> {
    let mime = mime.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = pick_file_sync(&mime);
        let _ = tx.send(result);
    });
    rx
}

/// Fire the system share sheet for the last-exported `Kal-export.ics` (the
/// caller writes it first; it is served over `KalFileProvider`). Safe on any
/// thread; failures clear the JNI exception and no-op.
pub fn share_ics_via_intent() {
    let ctx = ndk_context::android_context();
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else {
        return;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    // Resolve KalShare through the app class loader (same reason as
    // `find_kal_file_picker`: system-loader FindClass cannot see app classes).
    let res: jni::errors::Result<()> = (|| {
        use jni::objects::{JString, JValue};
        let activity_cls: jni::objects::JObject = env.get_object_class(&activity)?.into();
        let loader = env
            .call_method(
                &activity_cls,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )?
            .l()?;
        let name: JString = env.new_string("com/kal/calendar/KalShare")?;
        let name_obj: jni::objects::JObject = name.into();
        let cls_obj = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name_obj)],
            )?
            .l()?;
        let cls: jni::objects::JClass = cls_obj.into();
        env.call_static_method(
            &cls,
            "shareIcs",
            "(Landroid/app/Activity;)V",
            &[JValue::Object(&activity)],
        )?;
        Ok(())
    })();
    if res.is_err() {
        let _ = env.exception_clear();
    }
}

/// Stub for non-Android builds so the module compiles everywhere.
#[cfg(not(target_os = "android"))]
pub fn pick_image_async() -> mpsc::Receiver<Option<PickedImage>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(None);
    rx
}

/// Stub for non-Android builds so the module compiles everywhere.
#[cfg(not(target_os = "android"))]
pub fn pick_file_async(_mime: &str) -> mpsc::Receiver<Option<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(None);
    rx
}

/// Stub for non-Android builds so the module compiles everywhere.
#[cfg(not(target_os = "android"))]
pub fn share_ics_via_intent() {}
