package os.elastos.hey.social

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Identity vault — encrypts the Hey seed at rest with a HARDWARE-BACKED key:
 * StrongBox (Pixel Titan M / Samsung Knox Vault) where present, else the TEE /
 * TrustZone keystore. With the vault ON, identity.json is DELETED and the seed
 * exists ONLY as ciphertext behind a fresh, Hey-initiated biometric.
 *
 * H2.1 — the seal/unseal Cipher ops are CRYPTOGRAPHICALLY BOUND to a BiometricPrompt
 * CryptoObject (mirroring SpendAuth.signBound): the unseal cipher is built, wrapped in
 * a BiometricPrompt.CryptoObject(Cipher), and the plaintext is produced INSIDE the
 * prompt's success callback (result.cryptoObject.cipher.doFinal). The key uses 0s /
 * per-use validity, so there is NO time-window — a rooted device can no longer unwrap
 * the seed within 30s of ANY device auth; the seed only decrypts under a fresh,
 * Hey-initiated biometric op authenticated against THIS cipher.
 *
 * H2.3 — the key is created with setInvalidatedByBiometricEnrollment(false), so adding
 * a fingerprint does NOT brick the seal (don't rely on OEM DEVICE_CREDENTIAL behaviour).
 *
 * Brick safety: a device-credential is an allowed authenticator; a device-LOCK change
 * still permanently invalidates the key (unseal fails → caller routes to RESTORE), which
 * is why default-on onboarding/migration MUST first prove the recovery phrase is recorded.
 * The caller round-trip-verifies a seal before deleting the plaintext seed.
 */
object IdentityVault {
    private const val PREFS = "hey"
    private const val ALIAS = "hey_identity_vault"
    private const val K_ON = "vault_on"
    private const val K_IV = "vault_iv"
    private const val K_CT = "vault_ct"
    private const val AUTH = BiometricManager.Authenticators.BIOMETRIC_STRONG or
        BiometricManager.Authenticators.DEVICE_CREDENTIAL

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private fun ks() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    /** Can we hardware-wrap here? Needs an enrolled biometric or device credential.
     *  On a very old phone with no secure keystore/lock this is false → stay plaintext. */
    fun available(ctx: Context): Boolean = runCatching {
        BiometricManager.from(ctx).canAuthenticate(AUTH) == BiometricManager.BIOMETRIC_SUCCESS
    }.getOrDefault(false)

    fun isOn(ctx: Context): Boolean = prefs(ctx).getBoolean(K_ON, false)
    fun hasSealed(ctx: Context): Boolean = prefs(ctx).contains(K_CT)
    fun setOn(ctx: Context, on: Boolean) = prefs(ctx).edit().putBoolean(K_ON, on).apply()

    private fun ensureKey(strongbox: Boolean): SecretKey {
        (ks().getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            .apply {
                if (Build.VERSION.SDK_INT >= 30) {
                    // H2.1: 0s validity = PER-USE auth. Every seal/unseal needs a fresh
                    // auth bound to THAT BiometricPrompt's CryptoObject — no time-window.
                    setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                } else {
                    // Pre-30: -1 = require auth for every use (the CryptoObject is still
                    // the binding; the legacy duration API can't express per-op otherwise).
                    @Suppress("DEPRECATION") setUserAuthenticationValidityDurationSeconds(-1)
                }
                // H2.3: adding a new fingerprint must NOT invalidate the seal (don't leave
                // this to OEM DEVICE_CREDENTIAL behaviour). Available API 24+.
                if (Build.VERSION.SDK_INT >= 24) setInvalidatedByBiometricEnrollment(false)
                if (strongbox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
            }
            .build()
        kg.init(spec)
        return kg.generateKey()
    }

    /** StrongBox (Titan M / Knox Vault) if available, else TEE. */
    private fun key(): SecretKey = runCatching { ensureKey(true) }
        .recoverCatching { runCatching { ks().deleteEntry(ALIAS) }; ensureKey(false) }
        .getOrThrow()

    /**
     * H2.1 — SEAL bound to a fresh biometric. Builds an ENCRYPT cipher, wraps it in a
     * BiometricPrompt.CryptoObject, and performs doFinal INSIDE the prompt's success
     * callback so the encryption is gated by THAT prompt. onResult(true) on success
     * (the ciphertext + Keystore-chosen IV are persisted), false on cancel/error.
     * The plaintext is the bare recovery phrase (hey_unlock does from_mnemonic).
     */
    fun sealAuthed(activity: FragmentActivity, plaintext: String, onResult: (Boolean) -> Unit) {
        val cipher = runCatching {
            Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        }.getOrNull() ?: return onResult(false)
        promptCipher(activity, cipher, seal = true) { authedCipher ->
            if (authedCipher == null) return@promptCipher onResult(false)
            onResult(runCatching {
                val ct = authedCipher.doFinal(plaintext.toByteArray(Charsets.UTF_8))
                prefs(activity).edit()
                    .putString(K_IV, Base64.encodeToString(authedCipher.iv, Base64.NO_WRAP))
                    .putString(K_CT, Base64.encodeToString(ct, Base64.NO_WRAP))
                    .apply()
                true
            }.getOrDefault(false))
        }
    }

    /**
     * H2.1 — UNSEAL bound to a fresh biometric. Builds a DECRYPT cipher from the stored
     * IV, wraps it in a BiometricPrompt.CryptoObject, and performs doFinal INSIDE the
     * prompt's success callback. No 30s-stale device auth can drive this — only a fresh
     * op on THIS cipher.
     *
     * Outcomes (the caller needs to tell a CANCEL from a DEAD KEY):
     *   • onResult(plaintext)              — success.
     *   • onResult(null), deadKey = false  — the user cancelled / a prompt error: stay locked.
     *   • onResult(null), deadKey = true   — the Keystore key is permanently invalidated
     *     (the device LOCK was changed/removed) OR there is no seal: the caller clears the
     *     dead seal and routes to RESTORE (the seed is recoverable from the phrase).
     */
    fun unsealAuthed(activity: FragmentActivity, onResult: (plaintext: String?, deadKey: Boolean) -> Unit) {
        val k = runCatching { ks().getKey(ALIAS, null) as? SecretKey }.getOrNull() ?: return onResult(null, true)
        val ivB64 = prefs(activity).getString(K_IV, null) ?: return onResult(null, true)
        val ctB64 = prefs(activity).getString(K_CT, null) ?: return onResult(null, true)
        val iv = runCatching { Base64.decode(ivB64, Base64.NO_WRAP) }.getOrNull() ?: return onResult(null, true)
        val ct = runCatching { Base64.decode(ctB64, Base64.NO_WRAP) }.getOrNull() ?: return onResult(null, true)
        // init throws KeyPermanentlyInvalidatedException when the device lock changed —
        // that is a DEAD KEY (route to restore), distinct from a user cancel below.
        val cipher = runCatching {
            Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, iv)) }
        }.getOrNull() ?: return onResult(null, true)
        promptCipher(activity, cipher, seal = false) { authedCipher ->
            if (authedCipher == null) return@promptCipher onResult(null, false) // cancelled / prompt error
            // A doFinal failure here (GCM tag) is corruption, not a lock change — treat as
            // dead so the user can recover via restore rather than being stuck.
            val pt = runCatching { String(authedCipher.doFinal(ct), Charsets.UTF_8) }.getOrNull()
            onResult(pt, pt == null)
        }
    }

    /** Run a BiometricPrompt over a CryptoObject(cipher); hand the authenticated cipher
     *  (or null on cancel/error) to [onCipher]. Mirrors SpendAuth.signBound. */
    private fun promptCipher(
        activity: FragmentActivity, cipher: Cipher, seal: Boolean, onCipher: (Cipher?) -> Unit,
    ) {
        val prompt = BiometricPrompt(
            activity, ContextCompat.getMainExecutor(activity),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    onCipher(result.cryptoObject?.cipher)
                }
                override fun onAuthenticationError(code: Int, msg: CharSequence) = onCipher(null)
            }
        )
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(if (seal) "Protect your recovery phrase" else "Unlock Hey")
            .setSubtitle(if (seal) "Verify it's you to seal your keys in hardware" else "Verify it's you to open your data")
            .setAllowedAuthenticators(AUTH)
            .build()
        runCatching { prompt.authenticate(info, BiometricPrompt.CryptoObject(cipher)) }.onFailure { onCipher(null) }
    }

    fun clear(ctx: Context) {
        runCatching { ks().deleteEntry(ALIAS) }
        prefs(ctx).edit().remove(K_IV).remove(K_CT).putBoolean(K_ON, false).apply()
    }
}
