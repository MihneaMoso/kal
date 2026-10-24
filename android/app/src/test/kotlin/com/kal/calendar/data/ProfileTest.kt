package com.kal.calendar.data

import java.util.Base64
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the profile row: its serde-compatible JSON shape, the display-name and
 * initials fallbacks, and the avatar validation that replaced native
 * `set_avatar` (Decision C).
 */
class ProfileTest {

    private val png = byteArrayOf(
        0x89.toByte(), 'P'.code.toByte(), 'N'.code.toByte(), 'G'.code.toByte(),
        0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0,
    )

    private val jpeg = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xE0.toByte())

    @Test
    fun the_stored_row_has_the_fields_serde_writes() {
        val row = JSONObject(UserProfile(username = "Ada").toJson())

        assertEquals(setOf("username", "avatar_mime", "avatar_b64"), row.keySet())
        assertEquals("Ada", row.getString("username"))
        // Option::None serialises as null; the Kotlin row must too, so the Rust
        // loader reads it back as None rather than failing.
        assertTrue(row.isNull("avatar_mime"))
        assertTrue(row.isNull("avatar_b64"))
    }

    @Test
    fun a_stored_row_round_trips() {
        val profile = UserProfile(
            username = "Ada Lovelace",
            avatarMime = "image/png",
            avatarB64 = Base64.getEncoder().encodeToString(png),
        )

        assertEquals(profile, UserProfile.from(profile.toJson()))
    }

    @Test
    fun an_unreadable_row_degrades_to_the_empty_profile() {
        assertEquals(UserProfile(), UserProfile.from(null))
        assertEquals(UserProfile(), UserProfile.from(""))
        assertEquals(UserProfile(), UserProfile.from("not json"))
        // A legacy JSON-quoted wrapper is not a row this build writes.
        assertEquals(UserProfile(), UserProfile.from("\"{\\\"username\\\":\\\"Ada\\\"}\""))
    }

    @Test
    fun an_empty_username_displays_as_you() {
        assertEquals("You", UserProfile().displayName)
        assertEquals("You", UserProfile(username = "   ").displayName)
        assertEquals("Ada", UserProfile(username = "  Ada  ").displayName)
    }

    @Test
    fun initials_use_the_first_and_last_word() {
        // Upstream's initials(): the first word's letter, then the last word's
        // when there is more than one word — upper-cased.
        assertEquals("AL", UserProfile(username = "ada lovelace").initials)
        assertEquals("AB", UserProfile(username = "a b").initials)
        assertEquals("G", UserProfile(username = "grace").initials)
        assertEquals("Y", UserProfile().initials)
        // The display name never renders empty, so the initials never do either.
        assertFalse(UserProfile(username = "   ").initials.isEmpty())
    }

    @Test
    fun an_image_is_stored_as_base64_with_its_mime() {
        val result = UserProfile().withAvatar("image/jpeg", jpeg)

        val profile = result.getOrThrow()
        assertEquals("image/jpeg", profile.avatarMime)
        assertArrayEqualsBytes(jpeg, profile.avatarBytes())
        assertTrue(profile.hasAvatar)
    }

    @Test
    fun a_missing_mime_type_becomes_png() {
        assertEquals("image/png", UserProfile().withAvatar(null, png).getOrThrow().avatarMime)
        assertEquals("image/png", UserProfile().withAvatar("", png).getOrThrow().avatarMime)
    }

    @Test
    fun an_oversized_image_is_refused() {
        val oversized = ByteArray(UserProfile.MAX_AVATAR_BYTES + 1) { png[it % png.size] }

        val result = UserProfile().withAvatar("image/png", oversized)

        assertEquals(
            "Image too large (max 4 MB)",
            result.exceptionOrNull()?.message,
        )
        assertTrue(result.isFailure)
    }

    @Test
    fun a_file_that_is_not_an_image_is_refused() {
        val result = UserProfile().withAvatar("text/plain", "hello".toByteArray())

        assertEquals("Selected file is not an image", result.exceptionOrNull()?.message)
    }

    @Test
    fun the_four_image_families_pass_the_magic_byte_check() {
        assertTrue(UserProfile.looksLikeImage(png))
        assertTrue(UserProfile.looksLikeImage(jpeg))
        assertTrue(UserProfile.looksLikeImage("GIF89a".toByteArray()))
        assertTrue(UserProfile.looksLikeImage("RIFF....WEBP".toByteArray()))
        assertFalse(UserProfile.looksLikeImage(ByteArray(0)))
        assertFalse(UserProfile.looksLikeImage("%PDF-1.7".toByteArray()))
    }

    @Test
    fun removing_the_picture_clears_both_fields() {
        val withPicture = UserProfile().withAvatar("image/png", png).getOrThrow()

        val cleared = withPicture.withoutAvatar()

        assertNull(cleared.avatarMime)
        assertNull(cleared.avatarB64)
        assertFalse(cleared.hasAvatar)
        assertEquals(withPicture.username, cleared.username)
    }

    @Test
    fun an_undecodable_avatar_yields_no_bytes() {
        assertNull(UserProfile(avatarB64 = "%%%not base64%%%").avatarBytes())
        assertNull(UserProfile().avatarBytes())
    }

    @Test
    fun a_rejection_is_an_exception_the_screen_can_show() {
        val rejected = AvatarRejected(AvatarRejected.NOT_AN_IMAGE)

        assertSame("Selected file is not an image", rejected.message)
    }

    private fun assertArrayEqualsBytes(expected: ByteArray, actual: ByteArray?) {
        assertTrue("avatar bytes are missing", actual != null)
        assertTrue("avatar bytes differ", expected.contentEquals(actual))
    }
}
