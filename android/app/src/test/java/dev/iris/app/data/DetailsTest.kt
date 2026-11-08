package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DetailsTest {
    @Test
    fun noDividerMeansEverythingIsSpoken() {
        val p = splitDetails("Sure, it's sunny.")
        assertEquals("Sure, it's sunny.", p.spoken)
        assertNull(p.details)
    }

    @Test
    fun theDividerLineSplitsTheReply() {
        val p = splitDetails("Here is the summary.\n[details]\n| a | b |\n|---|---|")
        assertEquals("Here is the summary.", p.spoken)
        assertEquals("| a | b |\n|---|---|", p.details)
    }

    @Test
    fun theMarkerInsideALineIsNotADivider() {
        assertNull(splitDetails("Put [details] after it").details)
    }

    @Test
    fun aDividerAtTheEndGivesEmptyDetails() {
        val p = splitDetails("Done.\n[details]")
        assertEquals("Done.", p.spoken)
        assertEquals("", p.details)
    }
}
