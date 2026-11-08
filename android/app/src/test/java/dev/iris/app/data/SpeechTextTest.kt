package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SpeechTextTest {
    /** Streams [text] in pieces of [size] characters and collects every sentence, ending with a flush. */
    private fun speak(text: String, size: Int = 3): List<String> {
        val s = SpeechStream()
        val out = ArrayList<String>()
        text.chunked(size).forEach { out += s.push(it) }
        out += s.flush()
        return out
    }

    // ---- splitting ----

    @Test
    fun splitsOnFullStopsQuestionsAndExclamations() {
        assertEquals(listOf("It is sunny.", "Do you want a coat?", "Take one!"), speak("It is sunny. Do you want a coat? Take one!"))
    }

    @Test
    fun theGreekQuestionMarkEndsASentence() {
        assertEquals(listOf("Τι ώρα είναι;", "Είναι πέντε."), speak("Τι ώρα είναι; Είναι πέντε."))
    }

    @Test
    fun theAnoTeleiaEndsASentence() {
        assertEquals(listOf("Πρώτα το ένα·", "μετά το άλλο."), speak("Πρώτα το ένα· μετά το άλλο."))
    }

    @Test
    fun decimalsAreNotSplit() {
        assertEquals(listOf("It is 3.5 degrees.", "Cold."), speak("It is 3.5 degrees. Cold."))
        assertEquals(listOf("Έχει 21,5 βαθμούς."), speak("Έχει 21,5 βαθμούς."))
    }

    @Test
    fun urlsAreNotSplitAndAreNotRead() {
        assertEquals(listOf("See for details.", "Done."), speak("See https://example.com/a.b for details. Done."))
    }

    @Test
    fun abbreviationsAreNotSplit() {
        assertEquals(listOf("Dr. Smith will call."), speak("Dr. Smith will call."))
        assertEquals(listOf("Use a tool, e.g. the browser.", "Then wait."), speak("Use a tool, e.g. the browser. Then wait."))
        assertEquals(listOf("Ρώτα π.χ. τον Γιώργο."), speak("Ρώτα π.χ. τον Γιώργο."))
    }

    @Test
    fun aNumberedListMarkerIsNotASentence() {
        assertEquals(listOf("First thing.", "Second thing."), speak("1. First thing.\n2. Second thing."))
    }

    @Test
    fun aLineBreakEndsASentenceEvenWithoutAFullStop() {
        assertEquals(listOf("On it", "Give me a minute."), speak("On it\nGive me a minute."))
    }

    @Test
    fun ellipsisAndMixedMarksCountOnce() {
        assertEquals(listOf("Hmm...", "Really?!", "Yes."), speak("Hmm... Really?! Yes."))
    }

    @Test
    fun quotesAfterTheMarkStayWithTheSentence() {
        assertEquals(listOf("He said \"go.\"", "I went."), speak("He said \"go.\" I went."))
    }

    // ---- streaming ----

    @Test
    fun theSameSentencesComeOutHoweverTheTextIsCutUp() {
        val text = "Γεια σου! The weather is 14.5 degrees, e.g. cloudy. Θες κάτι άλλο; Ναι\nΟκ."
        val whole = speak(text, text.length)
        for (size in listOf(1, 2, 3, 5, 7, 11)) assertEquals("size $size", whole, speak(text, size))
    }

    @Test
    fun aSentenceIsReleasedAsSoonAsTheNextWordStarts() {
        val s = SpeechStream()
        assertEquals(emptyList<String>(), s.push("Hello there."))
        // Only now is it clear the full stop isn't part of a number.
        assertEquals(listOf("Hello there."), s.push(" Next"))
        assertEquals(listOf("Next one."), s.push(" one.") + s.flush())
    }

    @Test
    fun aFullStopAtTheEndWaitsForAFlush() {
        val s = SpeechStream()
        assertEquals(emptyList<String>(), s.push("Done."))
        assertEquals(listOf("Done."), s.flush())
    }

    @Test
    fun aFlushSaysWhatIsLeftEvenWithoutAFullStop() {
        val s = SpeechStream()
        s.push("On it, give me a minute")
        assertEquals(listOf("On it, give me a minute"), s.flush())
    }

    @Test
    fun aSecondRoundStartsFresh() {
        val s = SpeechStream()
        val first = s.push("On it") + s.flush()
        val second = s.push("Found it. Here you go.") + s.flush()
        assertEquals(listOf("On it"), first)
        assertEquals(listOf("Found it.", "Here you go."), second)
    }

    // ---- what is spoken ----

    @Test
    fun markdownIsStrippedAndCodeIsNotRead() {
        assertEquals(
            listOf("Here is the result.", "(code)", "Hope that helps."),
            speak("**Here** is the *result*.\n```kotlin\nval x = 1\nprintln(x)\n```\nHope that helps."),
        )
    }

    @Test
    fun linksKeepTheirTextButNotTheirAddress() {
        assertEquals(listOf("Read the docs now."), speak("Read [the docs](https://example.com/docs) now."))
    }

    @Test
    fun anOnlyPunctuationPieceIsDropped() {
        assertEquals(listOf("Done."), speak("---\nDone."))
    }

    // ---- the [details] cut-off ----

    @Test
    fun nothingAfterADetailsLineIsSpoken() {
        val said = speak("The weather is fine. Take a walk.\n[details]\n| Day | Temp |\n|---|---|\n| Mon | 14 |\nMore text here.")
        assertEquals(listOf("The weather is fine.", "Take a walk."), said)
    }

    @Test
    fun theDetailsLineWorksWhenItArrivesInTinyPieces() {
        for (size in 1..9) {
            assertEquals("size $size", listOf("Short answer."), speak("Short answer.\n[details]\nlong report. with sentences.", size))
        }
    }

    @Test
    fun theStreamKnowsItStopped() {
        val s = SpeechStream()
        s.push("Hi.\n[details]\nrest")
        assertTrue(s.stopped)
        assertEquals(emptyList<String>(), s.push("more. text."))
        assertEquals(emptyList<String>(), s.flush())
    }

    @Test
    fun theWordDetailsInsideALineIsJustText() {
        assertEquals(listOf("Put [details] on its own line."), speak("Put [details] on its own line."))
    }

    @Test
    fun aBracketLineThatIsNotTheMarkerIsStillSpoken() {
        assertEquals(listOf("Sources", "Done."), speak("[1] Sources\nDone.").map { it.removePrefix("[1] ") }.filter { it.isNotBlank() })
    }

    // ---- language ----

    @Test
    fun greekLettersChooseTheGreekVoice() {
        assertEquals("el", languageOf("Καλημέρα!"))
        assertEquals("el", languageOf("Άνοιξα το Gmail"))
        assertEquals("el", languageOf("Ὅμηρος")) // polytonic Greek
    }

    @Test
    fun everythingElseIsEnglish() {
        assertEquals("en", languageOf("Good morning!"))
        assertEquals("en", languageOf("It is 21.5 degrees"))
        assertEquals("en", languageOf(""))
    }

    @Test
    fun aMixedParagraphGetsTheRightVoicePerSentence() {
        val sentences = speak("Γεια σου! The weather in London is cloudy. Θες να σε ενημερώσω; Just say the word.")
        assertEquals(listOf("el", "en", "el", "en"), sentences.map(::languageOf))
    }

    @Test
    fun sentencesInAnotherScriptAreNeverSpoken() {
        assertEquals(listOf("Sure.", "Done."), speak("Sure. Спасибо за просмотр. Done."))
    }

    @Test
    fun veryLongSentencesAreCutIntoPieces() {
        val long = (1..80).joinToString(" ") { "word$it" } + "."
        val pieces = speak(long, 50)
        assertTrue(pieces.size > 1)
        assertTrue(pieces.all { it.length <= 300 })
        assertEquals(long.replace(Regex("\\s+"), " "), pieces.joinToString(" "))
    }
}
