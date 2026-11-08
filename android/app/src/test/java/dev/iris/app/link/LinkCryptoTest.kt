package dev.iris.app.link

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The phone and the engine must derive the same keys and read each other's frames.
 * The numbers below come from the engine's own code (the `interop_vector` test in
 * crates/engine/src/link.rs uses the same inputs and the same expected values), so if
 * either side changes how it derives keys or seals frames, a test fails on that side.
 */
class LinkCryptoTest {
    private fun hex(s: String) = s.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private fun hex(b: ByteArray) = b.joinToString("") { "%02x".format(it) }

    private val ss = ByteArray(32) { 0x11 }
    private val ee = ByteArray(32) { 0x22 }
    private val clientPub = byteArrayOf(4) + ByteArray(64) { 0x33 }
    private val serverPub = byteArrayOf(4) + ByteArray(64) { 0x44 }

    private val okm = LinkCrypto.hkdf(ss + ee, "iris-link-v1".toByteArray(), clientPub + serverPub, 64)
    private val c2s = okm.copyOfRange(0, 32)
    private val s2c = okm.copyOfRange(32, 64)

    @Test
    fun keysMatchTheEngines() {
        assertEquals("0cdb65aeff6df44c02ced2402ca043a1e57926b163d9eeb5d3c82e8f36b6fe96", hex(c2s))
        assertEquals("7f64d85a2a88571cf19aa236978c4f8a11a2337da191f0124f76b4f9ad76398d", hex(s2c))
    }

    @Test
    fun readsAFrameTheEngineSealed() {
        // The engine's first frame to a phone ({"s":1,"b":…}), sealed with the s2c key.
        val plain = LinkCrypto.open(s2c, 1, "s2c", LinkCrypto.unb64("FlzAP5+iFnkx3hL10dk7eQhQ/9vIXWLwHMQ="))
        assertEquals("hello iris", plain.decodeToString())
    }

    @Test
    fun sealsAFrameTheEngineCanRead() {
        // What the engine's test expects for the phone's first frame.
        val sealed = LinkCrypto.seal(c2s, 1, "c2s", "hi engine".toByteArray())
        assertEquals("D3dakGfWTbToWcf1QwsqhzW8XMqxM8vYHQ==", LinkCrypto.b64(sealed))
    }

    @Test
    fun aTamperedOrMisdirectedFrameIsRefused() {
        val frame = LinkCrypto.unb64("FlzAP5+iFnkx3hL10dk7eQhQ/9vIXWLwHMQ=")
        assertThrows(Exception::class.java) { LinkCrypto.open(s2c, 1, "c2s", frame) } // wrong direction
        assertThrows(Exception::class.java) { LinkCrypto.open(s2c, 2, "s2c", frame) } // wrong counter
        val bad = frame.copyOf().also { it[0] = (it[0].toInt() xor 1).toByte() }
        assertThrows(Exception::class.java) { LinkCrypto.open(s2c, 1, "s2c", bad) } // changed byte
        assertArrayEquals("hello iris".toByteArray(), LinkCrypto.open(s2c, 1, "s2c", frame))
    }
}
