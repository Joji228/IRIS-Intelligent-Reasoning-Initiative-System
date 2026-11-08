package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Test

class VoiceSettingsTest {
    @Test
    fun defaultsMatchThePlan() {
        val s = VoiceSettings()
        assertEquals("", s.language) // auto-detect
        assertEquals(true, s.interrupt)
        assertEquals(true, s.sayProgress)
        assertEquals(false, s.loudspeaker)
        assertEquals(800, s.endOfTurnMs)
        assertEquals(1.0f, s.speechRate)
    }

    @Test
    fun waitChoicesAreHalfAndEightTenthsAndOnePointTwoSeconds() {
        assertEquals(500, VoiceSettings(wait = "short").endOfTurnMs)
        assertEquals(800, VoiceSettings(wait = "normal").endOfTurnMs)
        assertEquals(1200, VoiceSettings(wait = "long").endOfTurnMs)
        assertEquals(800, VoiceSettings(wait = "nonsense").endOfTurnMs)
    }

    @Test
    fun speedChoices() {
        assertEquals(0.85f, VoiceSettings(speed = "slow").speechRate)
        assertEquals(1.2f, VoiceSettings(speed = "fast").speechRate)
    }

    @Test
    fun theVoiceFollowsTheLanguage() {
        val s = VoiceSettings(voiceEn = "en-x", voiceEl = "el-x")
        assertEquals("el-x", s.voiceFor("el"))
        assertEquals("en-x", s.voiceFor("en"))
    }

    @Test
    fun theResetIsToldInPlainWords() {
        val now = 1_000L
        fun text(remaining: Long, inSeconds: Long) = describeReset(VoiceLimits(2000, remaining, now + inSeconds), now)
        assertEquals("The full allowance is available.", text(2000, 3600))
        assertEquals("The full allowance is available.", text(10, -5))
        assertEquals("Fills up again in about a minute.", text(10, 45))
        assertEquals("Fills up again in 12 min.", text(10, 12 * 60))
        assertEquals("Fills up again in 5 h 12 min.", text(10, 5 * 3600 + 12 * 60))
        assertEquals("Fills up again in 3 h.", text(10, 3 * 3600))
        assertEquals("Fills up again in 1 d 2 h.", text(10, 26 * 3600))
    }

    @Test
    fun installedOfflineVoicesComeFirst() {
        val online = VoiceChoice("b-online", "", offline = false, installed = true)
        val offline = VoiceChoice("c-offline", "", offline = true, installed = true)
        val missing = VoiceChoice("a-missing", "", offline = true, installed = false)
        assertEquals(listOf(offline, online, missing), sortVoices(listOf(missing, online, offline)))
    }
}
