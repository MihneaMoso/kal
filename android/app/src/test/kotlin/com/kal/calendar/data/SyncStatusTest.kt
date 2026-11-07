package com.kal.calendar.data

import com.kal.calendar.bridge.Payloads
import com.kal.calendar.bridge.SyncStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Tests for sync status handling and chain management.
 */
class SyncStatusTest {

    @Test
    fun `sync_status_is_unpaired_on_fresh_install`() {
        val status = SyncStatus(paired = false, fingerprint = null)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_is_paired_with_valid_fingerprint`() {
        val fingerprint = "abc123def456"
        val status = SyncStatus(paired = true, fingerprint = fingerprint)

        assertTrue(status.paired)
        assertEquals(fingerprint, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_paired_is_unpaired`() {
        val json = """{"paired":false,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_paired_true_sets_fingerprint`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)

        assertTrue(status.paired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_json_returns_unpaired`() {
        val json = """not valid json"""
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_returns_unpaired`() {
        val json: String? = null
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_missing_fields_uses_defaults`() {
        val json = """{"paired":true}"""
        val status = Payloads.parseSyncStatus(json)

        assertTrue(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_missing_paired_defaults_to_false`() {
        val json = """{"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_paired_boolean_defaults_to_false`() {
        val json = """{"paired":"invalid","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_fingerprint_string_is_still_set`() {
        val json = """{"paired":true,"fingerprint":"invalid-fingerprint"}"""
        val status = Payloads.parseSyncStatus(json)

        assertTrue(status.paired)
        assertEquals("invalid-fingerprint", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_only_returns_unpaired`() {
        val json = "   "
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_string_returns_unpaired`() {
        val json = ""
        val status = Payloads.parseSyncStatus(json)

        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_malformed_fingerprint_object_returns_unpaired`() {
        val json = """{"paired":true,"fingerprint":{}}"""
        val status = Payloads.parseSyncStatus(json)

        // Invalid fingerprint type throws exception -> unpaired
        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_fingerprint_array_returns_unpaired`() {
        val json = """{"paired":true,"fingerprint":["abc"]}"""
        val status = Payloads.parseSyncStatus(json)

        // Invalid fingerprint type throws exception -> unpaired
        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_numeric_fingerprint_returns_unpaired`() {
        val json = """{"paired":true,"fingerprint":123456}"""
        val status = Payloads.parseSyncStatus(json)

        // Invalid fingerprint type throws exception -> unpaired
        assertFalse(status.paired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_equality_and_hash_code`() {
        val status1 = SyncStatus(paired = true, fingerprint = "abc123")
        val status2 = SyncStatus(paired = true, fingerprint = "abc123")
        val status3 = SyncStatus(paired = false, fingerprint = null)

        assertEquals(status1, status2)
        assertEquals(status1.hashCode(), status2.hashCode())
        assertFalse(status1 == status3)
    }
}