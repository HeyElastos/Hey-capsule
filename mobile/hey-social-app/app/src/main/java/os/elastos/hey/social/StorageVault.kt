package os.elastos.hey.social

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Storage data-encryption key (DEK) — makes EVERY on-device key + datum at rest
 * hardware-encrypted.
 *
 * The runtime seals every persisted file (the BIP39 seed/identity, the
 * Double-Ratchet PRIVATE keys, conversation plaintext, contacts, pinned peer
 * keys) under a 32-byte DEK. We generate that DEK once, WRAP it with a
 * hardware-backed AES-GCM Keystore key (the KEK — StrongBox where present, else
 * the TEE/TrustZone keystore), and persist only the wrapped DEK. At startup the
 * unwrapped DEK is handed to Rust via [HeyApi.hey_set_storage_key] BEFORE the
 * runtime touches disk, so nothing at rest is ever plaintext.
 *
 * The KEK is deliberately NOT auth-gated (no setUserAuthenticationRequired): the
 * always-on background-delivery service must unwrap the DEK without the user
 * present. Hardware-non-exportability is the at-rest protection — the wrapped DEK
 * is useless without THIS device's TEE, which defeats the real threats: ADB /
 * cloud backup extraction, a forensic image of /data, or another app reading the
 * sandbox. The separate, auth-gated [AppLock] + [IdentityVault] remain the
 * biometric/PIN gate for opening the app, revealing the phrase, and spends.
 *
 * StrongBox on a TEE-less device silently downgrades to the TEE keystore (still
 * hardware). On a device with no keystore at all, [dekBase64] returns null and
 * the runtime logs loudly that storage is plaintext (it never silently pretends).
 */
object StorageVault {
    private const val PREFS = "hey"
    private const val KEK_ALIAS = "hey_storage_dek_kek"
    private const val K_DEK_IV = "storage_dek_iv"
    private const val K_DEK_CT = "storage_dek_ct"

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private fun ks() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    private fun ensureKek(strongbox: Boolean): SecretKey {
        (ks().getKey(KEK_ALIAS, null) as? SecretKey)?.let { return it }
        val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(
            KEK_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            // NOT setUserAuthenticationRequired: background delivery must unwrap
            // the DEK without the user present. Auth gating lives in AppLock.
            .apply {
                if (strongbox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
                // H2 (partial): require the device to have been unlocked at least once
                // since boot before the KEK can unwrap the DEK. Non-breaking — the FGS
                // boots device-unlocked (BootReceiver fires after the first unlock), so
                // background delivery is unaffected; it just denies KEK use on a phone
                // seized at the powered-on-but-never-unlocked lock screen (Cellebrite BFU).
                if (Build.VERSION.SDK_INT >= 28) setUnlockedDeviceRequired(true)
            }
            .build()
        kg.init(spec)
        return kg.generateKey()
    }

    /** StrongBox KEK if the device has it, else the TEE keystore. */
    private fun kek(): SecretKey = runCatching { ensureKek(true) }
        .recoverCatching { runCatching { ks().deleteEntry(KEK_ALIAS) }; ensureKek(false) }
        .getOrThrow()

    /**
     * The 32-byte storage DEK as Base64 (NO_WRAP), creating + hardware-wrapping it
     * on first call. Pass to [HeyApi.hey_set_storage_key] BEFORE hey_init. Returns
     * null only when the device has no usable keystore at all — in which case the
     * runtime stays plaintext and says so in the log.
     */
    fun dekBase64(ctx: Context): String? = runCatching {
        val p = prefs(ctx)
        val ivS = p.getString(K_DEK_IV, null)
        val ctS = p.getString(K_DEK_CT, null)
        val dek: ByteArray = if (ivS != null && ctS != null) {
            val iv = Base64.decode(ivS, Base64.NO_WRAP)
            val ct = Base64.decode(ctS, Base64.NO_WRAP)
            val c = Cipher.getInstance("AES/GCM/NoPadding")
                .apply { init(Cipher.DECRYPT_MODE, kek(), GCMParameterSpec(128, iv)) }
            c.doFinal(ct)
        } else {
            val fresh = ByteArray(32).also { SecureRandom().nextBytes(it) }
            val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, kek()) }
            val ct = c.doFinal(fresh)
            p.edit()
                .putString(K_DEK_IV, Base64.encodeToString(c.iv, Base64.NO_WRAP))
                .putString(K_DEK_CT, Base64.encodeToString(ct, Base64.NO_WRAP))
                .apply()
            fresh
        }
        Base64.encodeToString(dek, Base64.NO_WRAP)
    }.getOrNull()

    /** True once a wrapped DEK exists (storage is encrypted at rest). */
    fun isActive(ctx: Context): Boolean = prefs(ctx).contains(K_DEK_CT)
}
