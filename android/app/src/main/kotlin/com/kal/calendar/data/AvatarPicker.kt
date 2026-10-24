package com.kal.calendar.data

import android.content.Context
import android.net.Uri
import java.io.ByteArrayOutputStream

/**
 * Read a picked image for the profile avatar.
 *
 * Stops one byte past [UserProfile.MAX_AVATAR_BYTES] so an oversized pick is
 * refused by the size check without pulling the whole file into memory (the
 * Dioxus picker read everything and rejected it afterwards). Returns the
 * document's MIME type (often null, which the profile normalises) and the bytes,
 * or null when the document cannot be opened or read.
 */
fun readAvatar(context: Context, uri: Uri): Pair<String?, ByteArray>? = runCatching {
    val stream = context.contentResolver.openInputStream(uri) ?: return@runCatching null
    stream.use { input ->
        val limit = UserProfile.MAX_AVATAR_BYTES + 1
        val out = ByteArrayOutputStream(minOf(limit, 64 * 1024))
        val chunk = ByteArray(16 * 1024)
        var total = 0
        while (total < limit) {
            val read = input.read(chunk, 0, minOf(chunk.size, limit - total))
            if (read <= 0) break
            out.write(chunk, 0, read)
            total += read
        }
        context.contentResolver.getType(uri) to out.toByteArray()
    }
}.getOrNull()
