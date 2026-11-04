package dev.iris.app.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import dev.iris.app.link.LinkCredentials
import dev.iris.app.link.LinkCrypto
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Remembers the pairing. The whole record (relay, engine key, device token) is sealed with
 * an AES key that lives in the Android Keystore and never leaves it, so the token isn't
 * readable from a backup or a copy of the app's files.
 */
class CredentialStore(context: Context) {
    private val prefs = context.getSharedPreferences("iris", Context.MODE_PRIVATE)

    fun load(): LinkCredentials? = runCatching {
        val sealed = prefs.getString(KEY_LINK, null) ?: return null
        val bytes = LinkCrypto.unb64(sealed)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12))
        val plain = cipher.doFinal(bytes, 12, bytes.size - 12)
        irisJson.decodeFromString(LinkCredentials.serializer(), plain.decodeToString())
    }.getOrNull()

    fun save(creds: LinkCredentials) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.iv + cipher.doFinal(irisJson.encodeToString(LinkCredentials.serializer(), creds).toByteArray())
        prefs.edit().putString(KEY_LINK, LinkCrypto.b64(sealed)).apply()
    }

    fun forget() {
        prefs.edit().remove(KEY_LINK).apply()
    }

    /** Another small secret sealed with the same Keystore key (the push keys). */
    fun secret(name: String): String? = runCatching {
        val bytes = LinkCrypto.unb64(prefs.getString("secret.$name", null) ?: return null)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12))
        cipher.doFinal(bytes, 12, bytes.size - 12).decodeToString()
    }.getOrNull()

    fun putSecret(name: String, value: String?) {
        if (value == null) return prefs.edit().remove("secret.$name").apply()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        prefs.edit().putString("secret.$name", LinkCrypto.b64(cipher.iv + cipher.doFinal(value.toByteArray()))).apply()
    }

    /** The last relay address used, to prefill the sign-in form. */
    var lastRelay: String
        get() = prefs.getString(KEY_RELAY, "") ?: ""
        set(value) = prefs.edit().putString(KEY_RELAY, value).apply()

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .build(),
            )
            generateKey()
        }
    }

    private companion object {
        const val ALIAS = "iris-link"
        const val KEY_LINK = "link"
        const val KEY_RELAY = "relay"
    }
}
