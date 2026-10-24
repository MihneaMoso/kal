package com.kal.calendar.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The install decision that used to be Android's: the caller staged an APK and
 * the package installer answered "app not installed as package appears to be
 * invalid", naming neither the file nor which of the three reasons it was.
 */
class InstallCheckTest {

    private val mine = ApkIdentity(
        packageName = "com.kal.calendar",
        versionCode = 2000,
        signer = "c7ede5aa50d5e0f46a36d8cdbf51e643fee6cb275940fb5e13d5187c219789b6",
    )

    /** A staged APK of this app, same signer, newer build: installable. */
    @Test
    fun aNewerBuildOfThisAppWithTheSameSignerInstalls() {
        assertNull(installBlockReason(mine.copy(versionCode = 2001), mine))
    }

    /** Reinstalling the same version is allowed (that is how a repair works). */
    @Test
    fun theSameVersionCodeInstalls() {
        assertNull(installBlockReason(mine, mine))
    }

    @Test
    fun aLowerVersionCodeIsADowngrade() {
        assertEquals(
            InstallBlock.Older,
            installBlockReason(mine.copy(versionCode = 1), mine),
        )
    }

    /**
     * The reported failure: the published APK was built and signed elsewhere
     * (CI's own debug key), so the installer refuses it whatever the version.
     */
    @Test
    fun aDifferentSignerIsRefusedEvenWhenNewer() {
        assertEquals(
            InstallBlock.OtherSigner,
            installBlockReason(mine.copy(versionCode = 9999, signer = "3ffe0323"), mine),
        )
    }

    @Test
    fun anotherPackageIsNeverAnUpdate() {
        assertEquals(
            InstallBlock.OtherApp,
            installBlockReason(mine.copy(packageName = "com.example.other"), mine),
        )
    }

    @Test
    fun aFileThatIsNotAnApkIsReportedAsSuch() {
        assertEquals(
            InstallBlock.NotAnApp,
            installBlockReason(ApkIdentity(null, 0, null), mine),
        )
    }

    @Test
    fun anUnsignedApkIsReportedAsSuch() {
        assertEquals(
            InstallBlock.Unsigned,
            installBlockReason(mine.copy(signer = null), mine),
        )
    }

    /** The order matters: a non-APK is reported before its zero version code is. */
    @Test
    fun unreadableBeatsOlderAndOtherSigner() {
        assertEquals(
            InstallBlock.NotAnApp,
            installBlockReason(ApkIdentity(null, 0, "deadbeef"), mine),
        )
    }

    @Test
    fun theBlockedLineNamesTheVersionAndTheReason() {
        assertEquals(
            "Update ready: v0.3.0 \u2014 cannot install: it is signed with a different key than this install",
            blockedUpdateLine(InstallBlock.OtherSigner, "0.3.0"),
        )
    }

    @Test
    fun everyBlockHasItsOwnMessage() {
        val messages = InstallBlock.entries.map(::installBlockMessage)
        assertEquals(messages.size, messages.toSet().size)
        assertEquals(messages.size, InstallBlock.entries.size)
    }
}
