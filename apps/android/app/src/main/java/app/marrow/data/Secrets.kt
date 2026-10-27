package app.marrow.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.marrow.core.SecretStore
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Sync passwords, the sync passphrase and AI keys, encrypted with a key that
 * lives in the phone's Android Keystore (it never leaves it). Without the
 * Keystore nothing is stored, rather than storing it readable.
 */
class KeystoreSecrets(context: Context) : SecretStore {
    private val prefs = context.getSharedPreferences("secrets", Context.MODE_PRIVATE)

    private fun key(): SecretKey? = try {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey) ?: KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .build(),
            )
            generateKey()
        }
    } catch (e: Exception) {
        null
    }

    override fun get(id: String): String? {
        val stored = prefs.getString(id, null) ?: return null
        val k = key() ?: return null
        return try {
            val bytes = Base64.decode(stored, Base64.NO_WRAP)
            val c = Cipher.getInstance(TRANSFORM)
            c.init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, bytes, 0, 12))
            String(c.doFinal(bytes, 12, bytes.size - 12), Charsets.UTF_8)
        } catch (e: Exception) {
            null
        }
    }

    override fun set(id: String, value: String?) {
        if (value == null) {
            prefs.edit().remove(id).apply()
            return
        }
        val k = key() ?: return
        try {
            val c = Cipher.getInstance(TRANSFORM)
            c.init(Cipher.ENCRYPT_MODE, k)
            val out = c.iv + c.doFinal(value.toByteArray(Charsets.UTF_8))
            prefs.edit().putString(id, Base64.encodeToString(out, Base64.NO_WRAP)).apply()
        } catch (e: Exception) {
            // Not kept; the person is asked again next time.
        }
    }

    private companion object {
        const val ALIAS = "marrow-secrets"
        const val TRANSFORM = "AES/GCM/NoPadding"
    }
}

/** Secrets kept in memory only (screenshot tests). */
class MemorySecrets : SecretStore {
    private val map = mutableMapOf<String, String>()
    override fun get(id: String): String? = map[id]
    override fun set(id: String, value: String?) {
        if (value == null) map.remove(id) else map[id] = value
    }
}
