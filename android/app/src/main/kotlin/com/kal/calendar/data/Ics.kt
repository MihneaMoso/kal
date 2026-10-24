package com.kal.calendar.data

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.net.Uri

/**
 * The MIME filter the import picker offers. Upstream's Android picker accepted
 * any file (the `*` slash `*` filter), which keeps a mislabelled `.ics`
 * selectable.
 */
const val ICS_PICKER_MIME = "*/*"

/**
 * Read a document the user picked as .ics text.
 *
 * Returns null when the document cannot be opened or read, so the caller can
 * say so in the sidebar instead of importing nothing silently.
 */
fun readIcs(context: Context, uri: Uri): String? = runCatching {
    context.contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
}.getOrNull()

/** The [Activity] hosting this context, for the APIs that require one. */
tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
