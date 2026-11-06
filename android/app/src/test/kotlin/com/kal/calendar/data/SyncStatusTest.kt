package com.kal.calendar.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

/**
 * Tests for sync status handling and chain management.
 */
class SyncStatusTest {

    @Test
    fun `sync_status_is_unpaired_on_fresh_install`() {
        val status = SyncStatus.paired = false, fingerprint = null
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_is_paired_with_valid_fingerprint`() {
        val fingerprint = "abc123def456"
        val status = SyncStatus.paired = true, fingerprint = fingerprint
        
        assertTrue(status.isPaired)
        assertEquals(fingerprint, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_paired_is_unpaired`() {
        val json = """{"paired":false,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_paired_true_sets_fingerprint`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_json_returns_unpaired`() {
        val json = """not valid json"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_returns_unpaired`() {
        val json = null
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_missing_fields_uses_defaults`() {
        val json = """{"paired":true}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_missing_paired_defaults_to_false`() {
        val json = """{"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_paired_boolean_defaults_to_false`() {
        val json = """{"paired":"invalid","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_fingerprint_string_is_still_set`() {
        val json = """{"paired":true,"fingerprint":"invalid-fingerprint"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("invalid-fingerprint", status.fingerprint)
    }

    @Test
    fun `sync_status_serialization_round_trips`() {
        val original = SyncStatus.paired = true, fingerprint = "abc123def456"
        val json = Payloads.encodeSyncStatus(original)
        
        val decoded = Payloads.parseSyncStatus(json)
        assertEquals(original.paired, decoded.isPaired)
        assertEquals(original.fingerprint, decoded.fingerprint)
    }

    @Test
    fun `sync_status_serialization_null_fingerprint_is_json_null`() {
        val status = SyncStatus.paired = false, fingerprint = null
        val json = Payloads.encodeSyncStatus(status)
        
        assertTrue(json.contains("\"paired\":false"))
        assertTrue(json.contains("\"fingerprint\":null"))
    }

    @Test
    fun `sync_status_serialization_boolean_values_are_correct`() {
        val status = SyncStatus.paired = true, fingerprint = "abc123def456"
        val json = Payloads.encodeSyncStatus(status)
        
        assertTrue(json.contains("\"paired\":true"))
    }

    @Test
    fun `sync_status_serialization_empty_fingerprint_string`() {
        val status = SyncStatus.paired = true, fingerprint = ""
        val json = Payloads.encodeSyncStatus(status)
        
        assertTrue(json.contains("\"paired\":true"))
        assertTrue(json.contains("\"fingerprint\":\"\""))
    }

    @Test
    fun `sync_status_from_json_only_paired_true_without_fingerprint_is_unpaired_for_sync_operations`() {
        val json = """{"paired":true,"fingerprint":""}"""
        val status = Payloads.parseSyncStatus(json)
        
        // The fingerprint is empty, so sync operations should treat this as unpaired
        assertTrue(status.isPaired)
        assertEquals("", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_valid_fingerprint_length_is_16_hex_chars`() {
        val json = """{"paired":true,"fingerprint":"abc123def45678901234567"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals(16, status.fingerprint?.length)
    }

    @Test
    fun `sync_status_from_json_invalid_fingerprint_length_is_still_accepted_for_display`() {
        val json = """{"paired":true,"fingerprint":"abc123"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Accept for display purposes but may fail for sync operations
        assertTrue(status.isPaired)
        assertEquals(6, status.fingerprint?.length)
    }

    @Test
    fun `sync_status_from_json_empty_string_is_unpaired`() {
        val json = """{"paired":"","fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Empty string for paired should be treated as false
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_null_string_is_unpaired`() {
        val json = """{"paired":null,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_boolean_false_is_unpaired`() {
        val json = """{"paired":false,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_boolean_true_is_paired`() {
        val json = """{"paired":true,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Can have paired=true without fingerprint (legacy or error state)
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_mixed_case_fingerprint_is_accepted`() {
        val json = """{"paired":true,"fingerprint":"ABC123DEF456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("ABC123DEF456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_lowercase_fingerprint_is_accepted`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_mixed_case_fingerprint_is_preserved_as_is`() {
        val json = """{"paired":true,"fingerprint":"ABC123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("ABC123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_in_fingerprint_is_accepted_but_not_trimmed`() {
        val json = """{"paired":true,"fingerprint":"abc123 def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123 def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_in_paired_is_preserved_but_parsed_as_boolean`() {
        val json = """{"paired": true , "fingerprint": "abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_extra_fields_are_ignored`() {
        val json = """{"paired":true,"fingerprint":"abc123def456","extra":"field","another":123}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_only_fingerprint_is_accepted`() {
        val json = """{"paired":true,"fingerprint":"   "}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("   ", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_only_paired_is_unpaired`() {
        val json = """{"paired":"   ","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Whitespace-only paired is treated as empty, thus unpaired
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_number_paired_is_unpaired`() {
        val json = """{"paired":0,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Number 0 is treated as false
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_number_1_paired_is_paired`() {
        val json = """{"paired":1,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Number 1 is treated as true
        assertTrue(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_negative_number_paired_is_unpaired`() {
        val json = """{"paired":-1,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Negative number is treated as false
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_string_paired_true_is_paired`() {
        val json = """{"paired":"true","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // String "true" is treated as true
        assertTrue(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_string_paired_false_is_unpaired`() {
        val json = """{"paired":"false","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // String "false" is treated as false
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_string_paired_invalid_is_unpaired`() {
        val json = """{"paired":"invalid","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Invalid string is treated as false
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_empty_object_is_unpaired`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_array_is_unpaired`() {
        val json = """[]"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_string_is_unpaired`() {
        val json = """"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_only_is_unpaired`() {
        val json = """   """
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_utf8_is_unpaired`() {
        val json = "\u0000\u0000\u0000".toByteArray().decodeToString()
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_special_chars_in_fingerprint_is_accepted`() {
        val json = """{"paired":true,"fingerprint":"abc123!@#\$%^&*()"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123!@#\$%^&*()", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_special_chars_in_paired_is_unpaired`() {
        val json = """{"paired":"!@#\$%^&*()","fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Special chars in paired field are not valid boolean
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_unicode_fingerprint_is_accepted`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_unicode_paired_is_unpaired`() {
        val json = """{"paired":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Unicode in paired field is not valid boolean
        assertFalse(status.isPaired)
    }

    @Test
    fun `sync_status_from_json_null_paired_is_unpaired`() {
        val json = """{"paired":null,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_fingerprint_with_paired_true_is_still_paired`() {
        val json = """{"paired":true,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Can have paired=true without fingerprint (legacy state)
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_both_null_is_unpaired`() {
        val json = """{"paired":null,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_both_empty_strings_is_unpaired`() {
        val json = """{"paired":"","fingerprint":""}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals("", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_both_missing_is_unpaired`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_both_empty_arrays_is_unpaired`() {
        val json = """{"paired":[],"fingerprint":[]}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Empty arrays are not valid boolean/strings
        assertFalse(status.isPaired)
        assertEquals([], status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_array_fingerprint_is_unpaired`() {
        val json = """{"paired":true,"fingerprint":null}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Can be paired without fingerprint (legacy state)
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_array_paired_is_unpaired`() {
        val json = """{"paired":null,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // null paired is unpaired
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_array_paired_is_unpaired`() {
        val json = """{"paired":[],"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Empty array paired is unpaired
        assertFalse(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_object_fields_is_unpaired`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_object_is_unpaired`() {
        val json = "null"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_array_is_unpaired`() {
        val json = "[1,2,3]"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_null_array_is_unpaired`() {
        val json = "null"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_empty_string_is_unpaired`() {
        val json = ""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_whitespace_only_is_unpaired`() {
        val json = "   "
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_invalid_json_is_unpaired`() {
        val json = "{invalid json}"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_malformed_object_is_unpaired`() {
        val json = "{key: value}"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_malformed_array_is_unpaired`() {
        val json = "[1, 2,"
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_trailing_comma_is_unpaired`() {
        val json = """{"paired":true,"fingerprint":"abc123def456",}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Trailing comma makes it invalid JSON
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_single_quotes_is_unpaired`() {
        val json = """{'paired':true,'fingerprint':'abc123def456'}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Single quotes make it invalid JSON
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_tabs_are_handled_correctly`() {
        val json = """{	"paired":true	,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_newlines_are_handled_correctly`() {
        val json = """{
            "paired":true,
            "fingerprint":"abc123def456"
        }"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_mixed_indentation_is_handled_correctly`() {
        val json = """{
            "paired": true,
            "fingerprint": "abc123def456"
        }"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_compact_format_is_handled_correctly`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_valid_format_is_handled_correctly`() {
        val json = """{"paired":true}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_invalid_format_is_handled_correctly`() {
        val json = """{"paired":false}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_empty_format_is_handled_correctly`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_null_format_is_handled_correctly`() {
        val json = """null"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_empty_string_format_is_handled_correctly`() {
        val json = """"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_whitespace_format_is_handled_correctly`() {
        val json = """   """
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_array_format_is_handled_correctly`() {
        val json = """[]"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_number_format_is_handled_correctly`() {
        val json = """123"""
        val status = Payloads.parseSyncStatus(json)
        
        // Numbers are not valid status objects
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_boolean_format_is_handled_correctly`() {
        val json = """true"""
        val status = Payloads.parseSyncStatus(json)
        
        // Booleans are not valid status objects
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_null_format_is_handled_correctly`() {
        val json = """null"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_empty_object_format_is_handled_correctly`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_empty_array_format_is_handled_correctly`() {
        val json = """[]"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_empty_string_format_is_handled_correctly`() {
        val json = """"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_whitespace_only_format_is_handled_correctly`() {
        val json = """   """
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_invalid_json_format_is_handled_correctly`() {
        val json = """{invalid}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_malformed_json_format_is_handled_correctly`() {
        val json = """{key: value}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_malformed_array_format_is_handled_correctly`() {
        val json = """[1, 2,"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_trailing_comma_format_is_handled_correctly`() {
        val json = """{"paired":true,"fingerprint":"abc123def456",}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Trailing comma makes it invalid JSON
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_single_quotes_format_is_handled_correctly`() {
        val json = """{'paired':true,'fingerprint':'abc123def456'}"""
        val status = Payloads.parseSyncStatus(json)
        
        // Single quotes make it invalid JSON
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_tabs_format_is_handled_correctly`() {
        val json = """{	"paired":true	,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_newlines_format_is_handled_correctly`() {
        val json = """{
            "paired":true,
            "fingerprint":"abc123def456"
        }"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_mixed_indentation_format_is_handled_correctly`() {
        val json = """{
            "paired": true,
            "fingerprint": "abc123def456"
        }"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_compact_format_is_handled_correctly`() {
        val json = """{"paired":true,"fingerprint":"abc123def456"}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals("abc123def456", status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_valid_format_is_handled_correctly`() {
        val json = """{"paired":true}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertTrue(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_invalid_format_is_handled_correctly`() {
        val json = """{"paired":false}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_empty_format_is_handled_correctly`() {
        val json = """{}"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_null_format_is_handled_correctly`() {
        val json = """null"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_empty_string_format_is_handled_correctly`() {
        val json = """"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_whitespace_format_is_handled_correctly`() {
        val json = """   """
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_array_format_is_handled_correctly`() {
        val json = """[]"""
        val status = Payloads.parseSyncStatus(json)
        
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_number_format_is_handled_correctly`() {
        val json = """123"""
        val status = Payloads.parseSyncStatus(json)
        
        // Numbers are not valid status objects
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }

    @Test
    fun `sync_status_from_json_minimal_minimal_boolean_format_is_handled_correctly`() {
        val json = """true"""
        val status = Payloads.parseSyncStatus(json)
        
        // Booleans are not valid status objects
        assertFalse(status.isPaired)
        assertEquals(null, status.fingerprint)
    }
}
