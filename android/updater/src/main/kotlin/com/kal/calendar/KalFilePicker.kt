package com.kal.calendar

import android.app.Activity
import android.content.Intent
import android.net.Uri
import java.io.ByteArrayOutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference

/**
 * Opens the system image picker via an Android intent, blocks until the user
 * selects (or cancels), and returns the raw image bytes.  Called from Rust via
 * JNI: the background thread calls [pickImage] then [waitForResult] (which
 * blocks until [onActivityResult] fires on the UI thread).
 */
object KalFilePicker {
    private const val PICK_IMAGE = 9998
    private const val PICK_FILE = 9997
    private val resultUri = AtomicReference<Uri?>(null)
    private var latch: CountDownLatch? = null

    /** Launch the image picker on the UI thread. */
    @JvmStatic
    fun pickImage(activity: Activity) {
        launch(activity, PICK_IMAGE, "image/*")
    }

    /** Launch a generic document picker for [mimeType] on the UI thread. */
    @JvmStatic
    fun pickFile(activity: Activity, mimeType: String) {
        launch(activity, PICK_FILE, mimeType)
    }

    private fun launch(activity: Activity, requestCode: Int, mimeType: String) {
        resultUri.set(null)
        latch = CountDownLatch(1)
        activity.runOnUiThread {
            val intent = Intent(Intent.ACTION_GET_CONTENT).apply {
                type = mimeType
                addCategory(Intent.CATEGORY_OPENABLE)
            }
            @Suppress("DEPRECATION")
            activity.startActivityForResult(intent, requestCode)
        }
    }

    /** Called by [MainActivity.onActivityResult].  Returns `true` if handled. */
    @JvmStatic
    fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        if (requestCode != PICK_IMAGE && requestCode != PICK_FILE) return false
        if (resultCode == Activity.RESULT_OK) {
            resultUri.set(data?.data)
        }
        latch?.countDown()
        return true
    }

    /** Block until the picker returns (or timeout). */
    @JvmStatic
    fun waitForResult(timeoutMs: Long): Uri? {
        latch?.await(timeoutMs, java.util.concurrent.TimeUnit.MILLISECONDS)
        return resultUri.get()
    }

    /** Read all bytes from a content:// URI via the ContentResolver. */
    @JvmStatic
    fun readBytes(context: android.content.Context, uri: Uri): ByteArray {
        val input = context.contentResolver.openInputStream(uri)
            ?: throw IllegalStateException("Cannot open URI: $uri")
        return input.use { stream ->
            val output = ByteArrayOutputStream()
            stream.copyTo(output)
            output.toByteArray()
        }
    }

    /** Resolve the MIME type of a content:// URI, or null if unknown. */
    @JvmStatic
    fun mimeType(context: android.content.Context, uri: Uri): String? {
        return context.contentResolver.getType(uri)
    }
}
