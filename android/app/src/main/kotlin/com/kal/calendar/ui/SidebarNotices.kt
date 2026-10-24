package com.kal.calendar.ui

/**
 * The sidebar's Import/Export notices.
 *
 * Upstream reported these only through `eprintln!` — a failed import looked like
 * a no-op — so the rewrite surfaces the native reason instead (the same
 * deliberate deviation the editor's rejected saves made). Pure functions, so the
 * exact copy is pinned by tests.
 */

/** A rejected import: the native parse/store reason, verbatim. */
fun importFailure(reason: String?): String = "Import failed: ${reason ?: UNKNOWN_REASON}"

/** A rejected export: the native write reason, verbatim. */
fun exportFailure(reason: String?): String = "Export failed: ${reason ?: UNKNOWN_REASON}"

/** The picked document could not be read at all (I/O, permission, gone). */
fun unreadableFile(): String = "Could not read the selected file"

private const val UNKNOWN_REASON = "unknown error"
