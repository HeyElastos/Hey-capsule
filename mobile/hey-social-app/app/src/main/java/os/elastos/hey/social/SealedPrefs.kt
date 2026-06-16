package os.elastos.hey.social

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * At-rest seal for NON-secret-but-sensitive Kotlin-side state — the local
 * transaction history (M1). Same threat model + key shape as [StorageVault]: a
 * hardware-backed (StrongBox where present, else TEE) AES-256-GCM key that is NOT
 * auth-gated, so writes after a send and reads when the wallet opens work without a
 * biometric, while a forensic /data image yields only ciphertext useless without
 * THIS device's TEE. (Contrast the plaintext `hey.xml` the history used to live in,
 * which re-exposed the financial+social trail the team deliberately sealed in the
 * Rust audit log.)
 *
 * Not for actual key material — that stays under the runtime's ChaCha20-Poly1305
 * at-rest seal. This is the Kotlin-side at-rest layer for app metadata.
 */
object SealedPrefs {
    private const val PREFS = "hey_sealed"
    private const val KEY_ALIAS = "hey_sealed_prefs_key"

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private fun ks() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    private fun ensureKey(strongbox: Boolean): SecretKey {
        (ks().getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            // NOT auth-gated (writes happen after a send / reads on wallet open).
            // Non-exportability is the at-rest protection — defeats a /data image.
            .apply {
                if (strongbox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
                // Boot-time + on-write only after first unlock (non-breaking, same as
                // the DEK KEK in H2): the wallet UI is only reachable post-unlock.
                if (Build.VERSION.SDK_INT >= 28) setUnlockedDeviceRequired(true)
            }
            .build()
        kg.init(spec)
        return kg.generateKey()
    }

    private fun key(): SecretKey = runCatching { ensureKey(true) }
        .recoverCatching { runCatching { ks().deleteEntry(KEY_ALIAS) }; ensureKey(false) }
        .getOrThrow()

    /** Encrypt + persist `value` under `name` (IV||CT, Base64). Best-effort: on a
     *  device with no keystore at all, falls back to clearing the slot (no plaintext). */
    fun put(ctx: Context, name: String, value: String) {
        runCatching {
            val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
            val ct = c.doFinal(value.toByteArray(Charsets.UTF_8))
            val packed = Base64.encodeToString(c.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(ct, Base64.NO_WRAP)
            prefs(ctx).edit().putString(name, packed).apply()
        }
    }

    /** Decrypt the sealed `name`, or `default` if absent/undecryptable. */
    fun get(ctx: Context, name: String, default: String): String = runCatching {
        val packed = prefs(ctx).getString(name, null) ?: return default
        val parts = packed.split(":", limit = 2)
        if (parts.size != 2) return default
        val iv = Base64.decode(parts[0], Base64.NO_WRAP)
        val ct = Base64.decode(parts[1], Base64.NO_WRAP)
        val k = ks().getKey(KEY_ALIAS, null) as? SecretKey ?: return default
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, iv)) }
        String(c.doFinal(ct), Charsets.UTF_8)
    }.getOrDefault(default)
}
