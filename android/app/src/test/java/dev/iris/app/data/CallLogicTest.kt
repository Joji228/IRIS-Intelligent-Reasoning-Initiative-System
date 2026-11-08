package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A detector that answers from a script, one answer per 20 ms frame. */
private class Scripted(private val voice: BooleanArray) : VoiceDetector {
    var i = 0
    override fun isSpeech(frame: ShortArray) = voice.getOrElse(i++) { false }
    override fun close() {}
}

class TurnDetectorTest {
    private fun frames(n: Int, first: Int = 0) = List(n) { ShortArray(Recorder.FRAME_SAMPLES) { _ -> (first + it).toShort() } }

    /** Runs a script of "S" (speech) and "." (quiet) frames; returns the event at each frame (null = none). */
    private fun run(script: String, setup: TurnDetector.() -> Unit = {}): List<TurnEvent?> {
        val d = TurnDetector(Scripted(BooleanArray(script.length) { script[it] == 'S' }))
        d.setup()
        return script.indices.map { d.feed(ShortArray(Recorder.FRAME_SAMPLES) { _ -> it.toShort() }) }
    }

    @Test
    fun silenceNeverStartsATurn() {
        assertTrue(run(".".repeat(200)).all { it == null })
    }

    @Test
    fun aTurnStartsAfterAQuarterSecondOfSpeech() {
        // 250 ms = 13 frames: the 13th is the one that starts it.
        val events = run("S".repeat(12) + ".")
        assertTrue(events.all { it == null })
        val started = run("S".repeat(13))
        assertEquals(TurnEvent.Started, started[12])
        assertTrue(started.take(12).all { it == null })
    }

    @Test
    fun aShortBlipIsNotATurn() {
        assertTrue(run("SSSS" + ".".repeat(60) + "SSSS" + ".".repeat(60)).all { it == null })
    }

    @Test
    fun aTurnEndsAfterTheWaitSilence() {
        // 800 ms of quiet = 40 frames.
        val script = "S".repeat(20) + ".".repeat(40)
        val events = run(script)
        assertTrue(events[19] == TurnEvent.Started || events.take(20).contains(TurnEvent.Started))
        assertTrue(events.last() is TurnEvent.Ended)
        assertNull(events[script.length - 2])
    }

    @Test
    fun theWaitCanBeShorterOrLonger() {
        val short = run("S".repeat(20) + ".".repeat(25)) { endSilenceMs = 500 }
        assertTrue(short.last() is TurnEvent.Ended)
        val long = run("S".repeat(20) + ".".repeat(45)) { endSilenceMs = 1200 }
        assertTrue(long.none { it is TurnEvent.Ended })
    }

    @Test
    fun aPauseShorterThanTheWaitDoesNotEndTheTurn() {
        val events = run("S".repeat(20) + ".".repeat(30) + "S".repeat(20) + ".".repeat(40))
        assertEquals(1, events.count { it is TurnEvent.Ended })
        val ended = events.filterIsInstance<TurnEvent.Ended>().single()
        // Both bursts are in the one clip: 20 + 30 + 20 frames, plus the quiet kept at the end.
        assertTrue(ended.pcm.size >= 70 * Recorder.FRAME_SAMPLES)
    }

    @Test
    fun firstWordsAreKeptWithAPreRoll() {
        // 30 quiet frames, then speech: the clip starts about 300 ms (15 frames) before the speech.
        val script = ".".repeat(30) + "S".repeat(20) + ".".repeat(40)
        val ended = run(script).filterIsInstance<TurnEvent.Ended>().single()
        // The first kept frame's first sample is its index in the script: 30 - 15 = 15.
        assertEquals(15, ended.pcm[0].toInt())
        assertTrue(ended.pcm.size >= (15 + 20) * Recorder.FRAME_SAMPLES)
    }

    @Test
    fun quietAtTheEndIsTrimmedToTwoHundredMilliseconds() {
        val script = "S".repeat(20) + ".".repeat(40)
        val ended = run(script).filterIsInstance<TurnEvent.Ended>().single()
        // 20 speech frames + 10 quiet ones (200 ms), nothing before them to pre-roll.
        assertEquals(30 * Recorder.FRAME_SAMPLES, ended.pcm.size)
    }

    @Test
    fun aSingleQuietFrameInsideTheStartIsForgiven() {
        val events = run("SSSS.SSSS.SSSS.SSSS")
        assertTrue(events.contains(TurnEvent.Started))
    }

    @Test
    fun strictModeNeedsFortyMillisecondsMoreAndNoGaps() {
        val strict: TurnDetector.() -> Unit = { startSpeechMs = 400; tolerant = false }
        // With gaps, the strict rule never sees enough steady speech.
        assertTrue(run("SSSS.SSSS.SSSS.SSSS.SSSS.SSSS", strict).none { it == TurnEvent.Started })
        // 400 ms = 20 steady frames.
        val events = run("S".repeat(20), strict)
        assertEquals(TurnEvent.Started, events[19])
        assertTrue(events.take(19).all { it == null })
    }

    @Test
    fun aTurnIsCutAtThirtySeconds() {
        val events = run("S".repeat(1600))
        assertTrue(events.any { it is TurnEvent.Ended })
    }

    @Test
    fun resetForgetsATurnInProgress() {
        val script = "S".repeat(30)
        val d = TurnDetector(Scripted(BooleanArray(script.length) { true }))
        script.indices.forEach { d.feed(ShortArray(Recorder.FRAME_SAMPLES)) }
        assertTrue(d.inTurn)
        d.reset()
        assertEquals(false, d.inTurn)
    }
}

class CallPieceTest {
    @Test
    fun speakingWinsThenYouThenThinking() {
        assertEquals(CallPhase.Speaking, callPhase(speaking = true, userSpeaking = true, busy = true))
        assertEquals(CallPhase.YouSpeaking, callPhase(false, true, true))
        assertEquals(CallPhase.Thinking, callPhase(false, false, true))
        assertEquals(CallPhase.Listening, callPhase(false, false, false))
    }

    @Test
    fun tinyResultsAreDropped() {
        assertEquals(false, isUsableTranscript(""))
        assertEquals(false, isUsableTranscript("  . "))
        assertEquals(false, isUsableTranscript("ok"))
        assertEquals(false, isUsableTranscript("Hm."))
        assertEquals(true, isUsableTranscript("Ναι"))
        assertEquals(true, isUsableTranscript("What's the weather?"))
    }

    @Test
    fun problemsAreToldBriefly() {
        assertEquals(CallProblem.SpeechLimit, classifyProblem(ApiException(429, "Groq's free limit is used up")))
        assertEquals(CallProblem.Offline, classifyProblem(ApiException(0, "Can't reach the I.R.I.S. engine")))
        assertEquals(CallProblem.Offline, classifyProblem(ApiException(0, "Not connected to the server")))
        assertEquals(CallProblem.Other, classifyProblem(ApiException(502, "couldn't reach the speech service")))
        assertEquals("I've hit the free speech limit; give me a minute.", CallProblem.SpeechLimit.spoken)
        assertEquals("I lost the connection to the server.", CallProblem.Offline.spoken)
    }

    @Test
    fun theLoudspeakerSwitchWinsThenBluetoothThenTheEarpiece() {
        assertEquals(AudioRoute.Speaker, chooseRoute(loudspeaker = true, bluetooth = true))
        assertEquals(AudioRoute.Speaker, chooseRoute(true, false))
        assertEquals(AudioRoute.Bluetooth, chooseRoute(false, true))
        assertEquals(AudioRoute.Earpiece, chooseRoute(false, false))
    }

    @Test
    fun progressPhrasesAreSpeakable() {
        assertEquals("Searching the web", progressPhrase("web_search"))
        assertEquals("Reading a page", progressPhrase("web_fetch"))
        assertEquals("Working on it", progressPhrase("some_new_tool"))
    }

    @Test
    fun progressIsRateLimitedAndNeverOverASentence() {
        val g = ProgressGate(gapMs = 8_000)
        assertEquals("Searching the web", g.next(0, "web_search", enabled = true, speakingNow = false))
        // Too soon for another one.
        assertNull(g.next(3_000, "web_fetch", true, false))
        // Eight seconds later, a new step is announced.
        assertEquals("Reading a page", g.next(9_000, "web_fetch", true, false))
        // Never over a sentence of the reply, and never when it is switched off.
        assertNull(g.next(20_000, "read_file", true, speakingNow = true))
        assertNull(g.next(20_000, "read_file", enabled = false, speakingNow = false))
    }

    @Test
    fun plainThinkingAndWaitingAreNotAnnounced() {
        val g = ProgressGate()
        assertNull(g.next(0, "thinking", true, false))
        assertNull(g.next(0, "waiting", true, false))
        assertNull(g.next(0, null, true, false))
    }

    @Test
    fun theSameStepIsNotRepeated() {
        val g = ProgressGate(gapMs = 1_000)
        assertEquals("Searching the web", g.next(0, "web_search", true, false))
        assertNull(g.next(5_000, "web_search", true, false))
        g.reset()
        assertEquals("Searching the web", g.next(10_000, "web_search", true, false))
    }
}

class PhantomAndNoiseTest {
    private fun tone(frames: Int, level: Int) = ShortArray(frames * Recorder.FRAME_SAMPLES) { i -> if (i % 2 == 0) level.toShort() else (-level).toShort() }

    @Test
    fun whispersInventedClosingLinesAreDropped() {
        for (s in listOf("Thank you.", "thanks for watching!", "Thank you. Thank you.", "Ευχαριστώ.", "Υπότιτλοι AUTHORWAVE", "Bye.", "You")) {
            assertTrue(s, isPhantomPhrase(s))
        }
    }

    @Test
    fun realSentencesThatStartLikeThemAreKept() {
        for (s in listOf("Thank you for the summary, now check my mail", "Ευχαριστώ, τι ώρα είναι;", "you know what", "", "Call Iris", "Bye the way, what's the weather")) {
            assertEquals(s, false, isPhantomPhrase(s))
        }
    }

    @Test
    fun speechLevelIsTheLoudnessOfTheLoudestFrames() {
        val clip = tone(10, 100) + tone(10, 2000) + tone(10, 100)
        assertEquals(2000.0, speechLevel(clip, 10), 1.0)
        assertEquals(0.0, speechLevel(ShortArray(0), 5), 0.0)
    }

    @Test
    fun aQuietHumIsNotWorthSending() {
        // 1 s of speech-like frames but at a level of 80 (a hum, or the dregs of an echo).
        assertEquals(false, isWorthTranscribing(tone(50, 80), 50))
    }

    @Test
    fun aClickIsNotWorthSending() {
        // Loud, but only 100 ms.
        val clip = tone(5, 3000) + tone(40, 20)
        assertEquals(false, isWorthTranscribing(clip, 5))
    }

    @Test
    fun aSpokenPhraseIsWorthSending() {
        val clip = tone(10, 100) + tone(30, 1500) + tone(10, 100)
        assertEquals(true, isWorthTranscribing(clip, 30))
    }

    @Test
    fun aClipThatIsMostlyQuietIsNotWorthSending() {
        // 20 speech frames lost in 4 seconds of otherwise quiet audio.
        val clip = tone(20, 1500) + tone(180, 10)
        assertEquals(false, isWorthTranscribing(clip, 20))
    }

    @Test
    fun theAgentsOwnVoiceComingBackIsRecognised() {
        val said = listOf("I found three flights for tomorrow.", "Do you want me to book the cheapest one?")
        assertEquals(true, looksLikeEcho("book the cheapest one", said))
        assertEquals(true, looksLikeEcho("Thank you", listOf("Thank you. Anything else?")))
        assertEquals(false, looksLikeEcho("Yes please do", said))
        assertEquals(false, looksLikeEcho("ok", said))
    }

    @Test
    fun otherScriptsAreNotEnglishOrGreek() {
        assertEquals(true, hasForeignScript("Спасибо за просмотр"))
        assertEquals(true, hasForeignScript("ご視聴ありがとうございました"))
        assertEquals(false, hasForeignScript("Καλημέρα, how are you?"))
        assertEquals(false, hasForeignScript("It is 21.5 degrees"))
        assertEquals(false, hasForeignScript("42"))
    }

    @Test
    fun aTurnReportsHowMuchOfItWasSpeech() {
        val script = "S".repeat(20) + ".".repeat(40)
        val d = TurnDetector(Scripted(BooleanArray(script.length) { script[it] == 'S' }))
        val ended = script.indices.mapNotNull { d.feed(ShortArray(Recorder.FRAME_SAMPLES)) }.filterIsInstance<TurnEvent.Ended>().single()
        assertEquals(20, ended.speechFrames)
    }
}

class ReplySpeechTest {
    @Test
    fun deltasBecomeSentences() {
        val r = ReplySpeech()
        val said = ArrayList<String>()
        for (piece in listOf("It is su", "nny. Take a ", "coat.")) said += r.onDelta("m1", 0, piece)
        said += r.onEnd()
        assertEquals(listOf("It is sunny.", "Take a coat."), said)
    }

    @Test
    fun aNewRoundSaysWhatWasLeftOfTheEarlierOne() {
        val r = ReplySpeech()
        val said = ArrayList<String>()
        said += r.onDelta("m1", 0, "On it, give me a minute")
        said += r.onDelta("m1", 1, "Found it. All good.")
        said += r.onEnd()
        assertEquals(listOf("On it, give me a minute", "Found it.", "All good."), said)
    }

    @Test
    fun aNewMessageStartsFresh() {
        val r = ReplySpeech()
        val said = ArrayList<String>()
        said += r.onDelta("m1", 0, "First answer")
        said += r.onDelta("m2", 0, "Second answer.")
        said += r.onEnd()
        assertEquals(listOf("First answer", "Second answer."), said)
    }

    @Test
    fun nothingAfterDetailsIsSaid() {
        val r = ReplySpeech()
        val said = ArrayList<String>()
        said += r.onDelta("m1", 0, "Short answer.\n[details]\nLong report. More.")
        said += r.onEnd()
        assertEquals(listOf("Short answer."), said)
    }

    @Test
    fun talkingOverAReplySilencesTheRestOfIt() {
        val r = ReplySpeech()
        val said = ArrayList<String>()
        said += r.onDelta("m1", 0, "One. Tw")
        r.silence()
        said += r.onDelta("m1", 0, "o. Three. Four.")
        said += r.onEnd()
        // The next reply is spoken again.
        said += r.onDelta("m2", 0, "Fresh start.") + r.onEnd()
        assertEquals(listOf("One.", "Fresh start."), said)
    }

    @Test
    fun emptyDeltasAreIgnored() {
        assertEquals(emptyList<String>(), ReplySpeech().onDelta("m1", 0, ""))
    }
}
