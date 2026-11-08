package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateTest {
    private val sha = "6c75e71fcd5259ab4e843d8f7c15817abbfe92288fdfe9bc6bce2abd1495d8fc"

    private fun status(version: String, url: String? = null, sha256: String? = sha, apk: Boolean = true): UpdateStatus {
        val asset = if (apk) UpdateAsset("IRIS-Android-v$version.apk", url ?: AppUpdate.expectedUrl(version), 3_500_000, sha256) else null
        return UpdateStatus(current = "0.2.0", latest = UpdateRelease(version = version, android = asset))
    }

    @Test
    fun versionsCompareByNumber() {
        assertTrue(AppUpdate.isNewer("0.2.1", "0.2.0"))
        assertTrue(AppUpdate.isNewer("v0.10.0", "0.9.9"))
        assertFalse(AppUpdate.isNewer("0.2.0", "0.2.0"))
        assertFalse(AppUpdate.isNewer("0.1.9", "0.2.0"))
        assertFalse(AppUpdate.isNewer("0.2.1-beta", "0.2.0"))
        assertFalse(AppUpdate.isNewer("garbage", "0.2.0"))
    }

    @Test
    fun offersOnlyANewerCheckableApkFromTheProjectsOwnAddress() {
        assertNotNull(AppUpdate.offer(status("0.2.1"), "0.2.0"))
        assertNull("not newer", AppUpdate.offer(status("0.2.0"), "0.2.0"))
        assertNull("no APK on the release", AppUpdate.offer(status("0.2.1", apk = false), "0.2.0"))
        assertNull("no checksum, so it can't be checked", AppUpdate.offer(status("0.2.1", sha256 = null), "0.2.0"))
        assertNull("a different address", AppUpdate.offer(status("0.2.1", url = "https://evil.example/IRIS-Android-v0.2.1.apk"), "0.2.0"))
        assertNull("another repository", AppUpdate.offer(status("0.2.1", url = "https://github.com/someone/else/releases/download/v0.2.1/IRIS-Android-v0.2.1.apk"), "0.2.0"))
        assertNull("nothing known yet", AppUpdate.offer(null, "0.2.0"))
    }

    @Test
    fun checksumIsLowercaseHexOfTheBytes() {
        assertEquals("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", AppUpdate.sha256Hex(ByteArray(0)))
        assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", AppUpdate.sha256Hex("abc".toByteArray()))
    }

    @Test
    fun readsTheEnginesAnswer() {
        val json = """{"current":"0.2.0","available":true,"checked_at":1,"auto_check":true,"last_update":null,
            "install":{"phase":"","percent":0,"error":null},
            "latest":{"version":"0.2.1","notes":"What's new","published_at":"2026-11-08T16:00:00Z","page":"https://github.com/x/y/releases/tag/v0.2.1",
            "windows":null,"android":{"name":"IRIS-Android-v0.2.1.apk","url":"${AppUpdate.expectedUrl("0.2.1")}","size":3500000,"sha256":"$sha"}}}"""
        val s = irisJson.decodeFromString(UpdateStatus.serializer(), json)
        assertEquals("0.2.1", s.latest?.version)
        assertEquals(sha, s.latest?.android?.sha256)
        assertNotNull(AppUpdate.offer(s, "0.2.0"))
    }
}
