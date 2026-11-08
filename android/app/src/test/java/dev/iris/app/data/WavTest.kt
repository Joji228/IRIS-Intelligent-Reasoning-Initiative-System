package dev.iris.app.data

import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

class WavTest {
    @Test
    fun theHeaderDescribesSixteenKilohertzMonoAudio() {
        val wav = Wav.encode(shortArrayOf(1, -2, 300), 16_000)
        val b = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        assertEquals("RIFF", String(wav, 0, 4))
        assertEquals("WAVE", String(wav, 8, 4))
        assertEquals("fmt ", String(wav, 12, 4))
        assertEquals(1, b.getShort(20).toInt()) // PCM
        assertEquals(1, b.getShort(22).toInt()) // mono
        assertEquals(16_000, b.getInt(24))
        assertEquals(32_000, b.getInt(28)) // bytes per second
        assertEquals(16, b.getShort(34).toInt())
        assertEquals("data", String(wav, 36, 4))
    }

    @Test
    fun sizesMatchTheSamples() {
        val wav = Wav.encode(ShortArray(160), 16_000)
        val b = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        assertEquals(44 + 320, wav.size)
        assertEquals(36 + 320, b.getInt(4)) // file size minus the first 8 bytes
        assertEquals(320, b.getInt(40)) // data size
    }

    @Test
    fun samplesAreLittleEndian() {
        val wav = Wav.encode(shortArrayOf(0x0102, -1), 16_000)
        assertEquals(0x02, wav[44].toInt())
        assertEquals(0x01, wav[45].toInt())
        assertEquals(-1, wav[46].toInt())
        assertEquals(-1, wav[47].toInt())
    }

    @Test
    fun anEmptyClipIsJustTheHeader() {
        assertEquals(44, Wav.encode(ShortArray(0)).size)
    }
}
