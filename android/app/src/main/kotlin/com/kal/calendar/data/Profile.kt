package com.kal.calendar.data

import java.util.Base64
import org.json.JSONObject

/**
 * The sync-chain user profile: a display name plus an optional avatar picture.
 *
 * Persisted as one JSON row under the `profile` setting key — the same shape the
 * Dioxus app wrote with serde (`username`, `avatar_mime`, `avatar_b64`), so an
 * APK swap in either direction keeps the picture. Avatar bytes ride the JSON
 * base64-encoded with their MIME type.
 *
 * Decision C's default applies: the validation that used to be native
 * (`set_avatar` in `app/src/profile.rs`) is a pure function here, so the bytes
 * cross the bridge only once, inside the settings JSON.
 */
data class UserProfile(
    val username: String = "",
    val avatarMime: String? = null,
    val avatarB64: String? = null,
) {
    /** The name to show: the trimmed username, or upstream's fallback. */
    val displayName: String get() = username.trim().ifEmpty { DEFAULT_USERNAME }

    val hasAvatar: Boolean get() = !avatarB64.isNullOrEmpty()

    /** One or two letters: first letter of the first and last word, upper-cased. */
    val initials: String
        get() {
            val words = displayName.split(' ', '\t', '\n', '\r').filter { it.isNotEmpty() }
            val first = words.firstOrNull()?.firstOrNull() ?: return "?"
            val last = words.getOrNull(1)?.let { words.last().firstOrNull() }
            return "$first${last ?: ""}".uppercase()
        }

    /** The decoded avatar, or null when there is none or it is not decodable. */
    fun avatarBytes(): ByteArray? =
        avatarB64?.takeIf { it.isNotEmpty() }?.let {
            runCatching { Base64.getDecoder().decode(it) }.getOrNull()
        }

    /**
     * Attach an uploaded image, rejecting anything over the size cap or that is
     * not an image by magic bytes. A missing MIME type becomes `image/png`,
     * which is what the picker's files usually are and what the Dioxus build
     * stored when the Android picker returned no type.
     */
    fun withAvatar(mime: String?, bytes: ByteArray): Result<UserProfile> {
        if (bytes.size > MAX_AVATAR_BYTES) {
            return Result.failure(AvatarRejected(AvatarRejected.TOO_LARGE))
        }
        if (!looksLikeImage(bytes)) {
            return Result.failure(AvatarRejected(AvatarRejected.NOT_AN_IMAGE))
        }
        return Result.success(
            copy(
                avatarMime = mime?.takeIf { it.isNotEmpty() } ?: DEFAULT_AVATAR_MIME,
                avatarB64 = Base64.getEncoder().encodeToString(bytes),
            ),
        )
    }

    fun withoutAvatar(): UserProfile = copy(avatarMime = null, avatarB64 = null)

    /** The `profile` row: the same three fields serde writes, nulls included. */
    fun toJson(): String = JSONObject()
        .put("username", username)
        .put("avatar_mime", avatarMime ?: JSONObject.NULL)
        .put("avatar_b64", avatarB64 ?: JSONObject.NULL)
        .toString()

    companion object {
        /** Shown when no username has been set (upstream's fallback). */
        const val DEFAULT_USERNAME = "You"

        /** MIME type stored when the picked file has none (upstream's fallback). */
        const val DEFAULT_AVATAR_MIME = "image/png"

        /** Size cap for avatar uploads (base64 storage is ~1.33× the raw size). */
        const val MAX_AVATAR_BYTES = 4 * 1024 * 1024

        /** Read the stored profile; anything unreadable becomes the empty one. */
        fun from(json: String?): UserProfile {
            val obj = runCatching { json?.let(::JSONObject) }.getOrNull() ?: return UserProfile()
            return UserProfile(
                username = obj.optString("username"),
                avatarMime = obj.stringOrNull("avatar_mime"),
                avatarB64 = obj.stringOrNull("avatar_b64"),
            )
        }

        /** The magic-byte check the native `looks_like_image` performed. */
        fun looksLikeImage(bytes: ByteArray): Boolean {
            val head = bytes.copyOfRange(0, minOf(bytes.size, 12))
            if (head.size >= 8 &&
                head[0] == 0x89.toByte() && head[1] == 'P'.code.toByte() &&
                head[2] == 'N'.code.toByte() && head[3] == 'G'.code.toByte()
            ) {
                return true
            }
            if (head.size >= 3 &&
                head[0] == 0xFF.toByte() && head[1] == 0xD8.toByte() && head[2] == 0xFF.toByte()
            ) {
                return true
            }
            val gif = byteArrayOf('G'.code.toByte(), 'I'.code.toByte(), 'F'.code.toByte(), '8'.code.toByte())
            if (head.size >= 4 && head.copyOfRange(0, 4).contentEquals(gif)) return true
            val riff = byteArrayOf('R'.code.toByte(), 'I'.code.toByte(), 'F'.code.toByte(), 'F'.code.toByte())
            return head.size >= 4 && head.copyOfRange(0, 4).contentEquals(riff)
        }
    }
}

/** Why an upload was refused; the messages are upstream's, shown verbatim. */
class AvatarRejected(message: String) : Exception(message) {
    companion object {
        const val TOO_LARGE = "Image too large (max 4 MB)"
        const val NOT_AN_IMAGE = "Selected file is not an image"
    }
}

/** `org.json` has no nullable string accessor; `JSONObject.NULL` is not a String. */
internal fun JSONObject.stringOrNull(key: String): String? =
    if (isNull(key)) null else optString(key).takeIf { it.isNotEmpty() }
