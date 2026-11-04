package dev.iris.app.link

import java.math.BigInteger
import java.nio.ByteBuffer
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.PublicKey
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.security.spec.ECParameterSpec
import java.security.spec.ECPoint
import java.security.spec.ECPublicKeySpec
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyAgreement
import javax.crypto.Mac
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * The cryptography of I.R.I.S. Link (crates/engine/src/link.rs, app/src/lib/link.ts):
 * ECDH P-256 with keys as SEC1 points, HKDF-SHA256, and AES-256-GCM with a counter nonce.
 * Everything here is in the platform's own JCA, so nothing is bundled.
 */
object LinkCrypto {
    private val curve: ECParameterSpec by lazy { (newKeyPair().public as ECPublicKey).params }

    fun newKeyPair(): KeyPair =
        KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()

    /** The uncompressed SEC1 point (0x04 ‖ X ‖ Y), what WebCrypto exports as "raw". */
    fun rawPublic(key: PublicKey): ByteArray {
        val w = (key as ECPublicKey).w
        return byteArrayOf(4) + fixed32(w.affineX) + fixed32(w.affineY)
    }

    fun publicFromRaw(raw: ByteArray): PublicKey {
        require(raw.size == 65 && raw[0] == 4.toByte()) { "not an uncompressed P-256 key" }
        val point = ECPoint(BigInteger(1, raw.copyOfRange(1, 33)), BigInteger(1, raw.copyOfRange(33, 65)))
        return KeyFactory.getInstance("EC").generatePublic(ECPublicKeySpec(point, curve))
    }

    fun ecdh(mine: PrivateKey, theirs: PublicKey): ByteArray =
        KeyAgreement.getInstance("ECDH").run {
            init(mine)
            doPhase(theirs, true)
            generateSecret()
        }

    fun hkdf(ikm: ByteArray, salt: ByteArray, info: ByteArray, length: Int): ByteArray {
        val prk = hmac(salt, ikm)
        val out = ByteArray(length)
        var t = ByteArray(0)
        var done = 0
        var i = 1
        while (done < length) {
            t = hmac(prk, t + info + byteArrayOf(i.toByte()))
            val n = minOf(t.size, length - done)
            t.copyInto(out, done, 0, n)
            done += n
            i++
        }
        return out
    }

    /** Seals one frame: the nonce is 4 zero bytes and the counter (big endian). */
    fun seal(key: ByteArray, counter: Long, aad: String, plain: ByteArray): ByteArray =
        gcm(Cipher.ENCRYPT_MODE, key, counter, aad).doFinal(plain)

    fun open(key: ByteArray, counter: Long, aad: String, sealed: ByteArray): ByteArray =
        gcm(Cipher.DECRYPT_MODE, key, counter, aad).doFinal(sealed)

    private fun gcm(mode: Int, key: ByteArray, counter: Long, aad: String): Cipher =
        Cipher.getInstance("AES/GCM/NoPadding").apply {
            val nonce = ByteBuffer.allocate(12).putInt(0).putLong(counter).array()
            init(mode, SecretKeySpec(key, "AES"), GCMParameterSpec(128, nonce))
            updateAAD(aad.toByteArray())
        }

    private fun hmac(key: ByteArray, data: ByteArray): ByteArray =
        Mac.getInstance("HmacSHA256").run {
            init(SecretKeySpec(key, "HmacSHA256"))
            doFinal(data)
        }

    private fun fixed32(n: BigInteger): ByteArray {
        val b = n.toByteArray()
        return when {
            b.size == 32 -> b
            b.size > 32 -> b.copyOfRange(b.size - 32, b.size)
            else -> ByteArray(32 - b.size) + b
        }
    }

    fun b64url(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    fun b64(bytes: ByteArray): String = Base64.getEncoder().encodeToString(bytes)

    /** Decodes standard or URL-safe base64, with or without padding. */
    fun unb64(s: String): ByteArray = Base64.getDecoder().decode(
        s.replace('-', '+').replace('_', '/').let { it + "=".repeat((4 - it.length % 4) % 4) },
    )
}
