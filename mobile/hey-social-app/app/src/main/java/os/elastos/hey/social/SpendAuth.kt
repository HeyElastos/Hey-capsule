package os.elastos.hey.social

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/**
 * Hardware-bound spend authorization (closes the guard.rs gap: an in-process
 * caller minting its own spend grant). A P-256 signing key lives in StrongBox/TEE
 * and requires a fresh fingerprint/PIN for EVERY use (per-operation auth). To
 * authorize a transfer the user signs the runtime's one-time challenge bound to
 * exactly (kind,to,amount) inside a BiometricPrompt CryptoObject; the Rust guard
 * verifies that signature before minting the grant.
 *
 * FAIL-SAFE: [enroll] flips the binding on ONLY after a full sign→verify
 * round-trip succeeds on this device, so a broken signer can never lock the user
 * out of spending. Until enrolled, [authorizeSpend] falls back to the existing
 * UI-biometric + plain mint, so behaviour is unchanged.
 *
 * NOT yet wired into the send dialog — wire [authorizeSpend] in place of the
 * direct HeyApi.authorize*Send calls AFTER an on-device round-trip test.
 */
object SpendAuth {
    private const val PREFS = "hey"
    private const val ALIAS = "hey_spend_auth"
    private const val K_ENROLLED = "spend_hw_enrolled"
    /** Marks that the one-time default auto-enroll (H3) was already offered, so we
     *  don't re-prompt every boot — whether the user accepted, cancelled, or the
     *  self-test failed (a broken signer must never nag or brick). */
    private const val K_AUTOENROLL_DONE = "spend_hw_autoenroll_done"
    private const val SELFTEST_CHALLENGE = "hey-spend-selftest-v1"
    private const val AUTH = BiometricManager.Authenticators.BIOMETRIC_STRONG or
        BiometricManager.Authenticators.DEVICE_CREDENTIAL

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private fun ks() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    fun isEnrolled(ctx: Context): Boolean = prefs(ctx).getBoolean(K_ENROLLED, false)
    fun available(ctx: Context): Boolean = runCatching {
        BiometricManager.from(ctx).canAuthenticate(AUTH) == BiometricManager.BIOMETRIC_SUCCESS
    }.getOrDefault(false)

    private fun ensureKey(strongbox: Boolean): PrivateKey {
        (ks().getKey(ALIAS, null) as? PrivateKey)?.let { return it }
        val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(java.security.spec.ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setUserAuthenticationRequired(true)
            .apply {
                if (Build.VERSION.SDK_INT >= 30) {
                    // 0s validity = require a fresh auth for EVERY signature.
                    setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                } else {
                    @Suppress("DEPRECATION") setUserAuthenticationValidityDurationSeconds(-1)
                }
                if (strongbox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
            }
            .build()
        kpg.initialize(spec)
        kpg.generateKeyPair()
        return ks().getKey(ALIAS, null) as PrivateKey
    }

    private fun privateKey(): PrivateKey = runCatching { ensureKey(true) }
        .recoverCatching { runCatching { ks().deleteEntry(ALIAS) }; ensureKey(false) }
        .getOrThrow()

    /** SEC1 uncompressed pubkey (0x04 || X(32) || Y(32)) the Rust guard enrolls. */
    private fun publicKeySec1(): ByteArray {
        val pub = ks().getCertificate(ALIAS).publicKey as ECPublicKey
        fun f32(b: java.math.BigInteger): ByteArray {
            var a = b.toByteArray()
            if (a.size > 32) a = a.copyOfRange(a.size - 32, a.size) // drop sign byte
            if (a.size < 32) a = ByteArray(32 - a.size) + a          // left-pad
            return a
        }
        return byteArrayOf(0x04) + f32(pub.w.affineX) + f32(pub.w.affineY)
    }

    private fun b64(b: ByteArray) = android.util.Base64.encodeToString(b, android.util.Base64.NO_WRAP)
    private fun hex(b: ByteArray) = b.joinToString("") { "%02x".format(it) }

    /** Sign `challenge\0kind\0to\0amount` inside a BiometricPrompt CryptoObject. */
    private fun signBound(
        activity: FragmentActivity, challenge: String, kind: String, to: String, amount: String,
        onSig: (String?) -> Unit,
    ) {
        val priv = runCatching { privateKey() }.getOrNull() ?: return onSig(null)
        val sig = runCatching { Signature.getInstance("SHA256withECDSA").apply { initSign(priv) } }.getOrNull() ?: return onSig(null)
        val prompt = BiometricPrompt(
            activity, ContextCompat.getMainExecutor(activity),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val s = result.cryptoObject?.signature ?: return onSig(null)
                    onSig(runCatching {
                        s.update("$challenge\u0000$kind\u0000$to\u0000$amount".toByteArray(Charsets.UTF_8))
                        hex(s.sign())
                    }.getOrNull())
                }
                override fun onAuthenticationError(code: Int, msg: CharSequence) = onSig(null)
            }
        )
        val (title, subtitle) = when (kind) {
            "seed.reveal" -> "Reveal recovery phrase" to "Verify it's you to show your 12 words"
            "spend.unenroll" -> "Turn off hardware confirmation" to "Verify it's you to disable spend protection"
            else -> "Confirm transfer" to "$amount to ${to.take(16)}…"
        }
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle(subtitle)
            .setAllowedAuthenticators(AUTH)
            .build()
        runCatching { prompt.authenticate(info, BiometricPrompt.CryptoObject(sig)) }.onFailure { onSig(null) }
    }

    /** Enroll the hardware spend-binding. Flips it on ONLY after a verified
     *  sign→Rust-verify round-trip. onResult(true) = active; false = unchanged. */
    fun enroll(activity: FragmentActivity, onResult: (Boolean) -> Unit) {
        if (!available(activity)) return onResult(false)
        signBound(activity, SELFTEST_CHALLENGE, "selftest", "selftest", "selftest") { sigHex ->
            if (sigHex == null) return@signBound onResult(false)
            val sec1 = runCatching { publicKeySec1() }.getOrNull() ?: return@signBound onResult(false)
            val ok = HeyApi.hey_spend_selftest(b64(sec1), SELFTEST_CHALLENGE, sigHex) == 0 &&
                HeyApi.hey_enroll_spend_key(b64(sec1)) == 0
            if (ok) prefs(activity).edit().putBoolean(K_ENROLLED, true).apply()
            onResult(ok)
        }
    }

    /**
     * Authorize a spend, returning the one-shot grant token to pass to the signer.
     * When enrolled: hardware-bound (BiometricPrompt CryptoObject → Rust verify).
     * When not: the caller should fall back to the existing UI-biometric + plain
     * HeyApi.authorize*Send. onToken(null) on cancel/failure.
     */
    fun authorizeSpend(
        activity: FragmentActivity, kind: String, to: String, amount: String,
        onToken: (String?) -> Unit,
    ) = authorizeSpend(activity, kind, to, amount, "", onToken)

    /** As [authorizeSpend] but binds a MAX network fee (wei) into the grant when
     *  `maxFeeWei` is non-empty (max-fee hardening; EVM native send). */
    fun authorizeSpend(
        activity: FragmentActivity, kind: String, to: String, amount: String, maxFeeWei: String,
        onToken: (String?) -> Unit,
    ) {
        if (!isEnrolled(activity)) return onToken(null) // caller uses the legacy path
        val challenge = runCatching { HeyApi.hey_spend_challenge() }.getOrNull()?.takeIf { it.isNotBlank() }
            ?: return onToken(null)
        signBound(activity, challenge, kind, to, amount) { sigHex ->
            if (sigHex == null) return@signBound onToken(null)
            val token = runCatching {
                val json = if (maxFeeWei.isNotBlank())
                    HeyApi.hey_authorize_spend_fee_hw(kind, to, amount, maxFeeWei, sigHex)
                else HeyApi.hey_authorize_spend_hw(kind, to, amount, sigHex)
                org.json.JSONObject(json).optString("token")
            }.getOrNull()?.takeIf { it.isNotBlank() }
            onToken(token)
        }
    }

    /**
     * Priority 2 — LAZY first-send enrollment in ONE scan. When NO hardware binding is
     * enrolled yet but the device CAN do hardware spends, the FIRST wallet/BEAM send both
     * ENROLLS the P-256 spend key AND AUTHORIZES this transfer with a SINGLE biometric:
     *   1. generate/ensure the Keystore key (no auth),
     *   2. issue the spend challenge,
     *   3. ONE BiometricPrompt CryptoObject signs `challenge\0kind\0to\0amount`,
     *   4. enroll the PUBLIC key in Rust (no biometric — just registers the SEC1 pubkey),
     *   5. mint the hardware-bound grant (`hey_authorize_spend(_fee)_hw`) verifying that
     *      same signature against the now-enrolled key.
     * Marks K_ENROLLED only AFTER a working authorize round-trip, so a broken signer can
     * never lock the user out (the send just fails and stays unenrolled — retryable). If
     * the user cancels after step 4 registered the pubkey, the Rust binding is active for
     * THIS process (the SAFE direction — hardware proof required); it reverts on next
     * launch because K_ENROLLED stays false (reenroll won't re-arm it). onToken(null) on
     * cancel/failure (the caller ABORTS — there is NO silent no-biometric fallback once
     * the device is hardware-capable).
     */
    private fun enrollAndAuthorize(
        activity: FragmentActivity, kind: String, to: String, amount: String, maxFeeWei: String,
        onToken: (String?) -> Unit,
    ) {
        val sec1 = runCatching { publicKeySec1() }.getOrNull() ?: return onToken(null)
        val challenge = runCatching { HeyApi.hey_spend_challenge() }.getOrNull()?.takeIf { it.isNotBlank() }
            ?: return onToken(null)
        signBound(activity, challenge, kind, to, amount) { sigHex ->
            if (sigHex == null) return@signBound onToken(null)
            // Register the pubkey (no biometric) so the SAME signature verifies on authorize.
            val enrolled = runCatching { HeyApi.hey_enroll_spend_key(b64(sec1)) == 0 }.getOrDefault(false)
            if (!enrolled) return@signBound onToken(null)
            val token = runCatching {
                val json = if (maxFeeWei.isNotBlank())
                    HeyApi.hey_authorize_spend_fee_hw(kind, to, amount, maxFeeWei, sigHex)
                else HeyApi.hey_authorize_spend_hw(kind, to, amount, sigHex)
                org.json.JSONObject(json).optString("token")
            }.getOrNull()?.takeIf { it.isNotBlank() }
            // Persist the binding only once a real spend authorize succeeded end-to-end.
            if (token != null) prefs(activity).edit().putBoolean(K_ENROLLED, true).apply()
            onToken(token)
        }
    }

    /**
     * H5: HARDWARE-VERIFIED seed reveal. When the binding is enrolled, signs the
     * Rust reveal-challenge inside a BiometricPrompt CryptoObject and returns the
     * mnemonic only after the Rust guard verifies that signature. Returns null on
     * cancel/failure. When NOT enrolled, the caller uses the legacy gate
     * (requireAuth → hey_recovery_phrase). MUST run with the activity present.
     */
    suspend fun revealSeed(activity: FragmentActivity?): String? {
        if (activity == null || !isEnrolled(activity)) return null // caller uses the legacy path
        val challenge = runCatching { HeyApi.hey_reveal_challenge() }.getOrNull()?.takeIf { it.isNotBlank() }
            ?: return null
        return suspendCancellableCoroutine { cont ->
            signBound(activity, challenge, "seed.reveal", "seed.reveal", "seed.reveal") { sigHex ->
                if (sigHex == null) { cont.resumeWith(Result.success(null)); return@signBound }
                val phrase = runCatching { HeyApi.hey_recovery_phrase_hw(sigHex) }.getOrNull()?.takeIf { it.isNotBlank() }
                cont.resumeWith(Result.success(phrase))
            }
        }
    }

    /** Re-establish the (process-global, in-memory) Rust spend binding at app
     *  start from the persisted Keystore key, if the user left hardware
     *  confirmation ON. Reads only the PUBLIC key (no biometric). Idempotent. */
    fun reenroll(ctx: Context) {
        if (!isEnrolled(ctx)) return
        val sec1 = runCatching { publicKeySec1() }.getOrNull() ?: return
        runCatching { HeyApi.hey_enroll_spend_key(b64(sec1)) }
    }

    /** H4: turn hardware confirmation OFF — but NOT disarmable in-process. When a
     *  binding is active the Rust kill switch refuses a bare call; the user must
     *  prove a fresh hardware signature over the disable-challenge (same StrongBox/TEE
     *  P-256 op as a spend). Only on a verified disable do we flip the pref + Rust
     *  flag. onResult(true) = turned off; false = cancelled/failed (stays ON).
     *  The enrolled Keystore key is kept, so toggling back on is instant. */
    fun unenroll(activity: FragmentActivity, onResult: (Boolean) -> Unit) {
        if (!isEnrolled(activity)) { runCatching { HeyApi.hey_unenroll_spend_key() }; return onResult(true) }
        val challenge = runCatching { HeyApi.hey_unenroll_challenge() }.getOrNull()?.takeIf { it.isNotBlank() }
            ?: return onResult(false)
        signBound(activity, challenge, "spend.unenroll", "spend.unenroll", "spend.unenroll") { sigHex ->
            if (sigHex == null) return@signBound onResult(false)
            val ok = runCatching { HeyApi.hey_unenroll_spend_key_hw(sigHex) == 0 }.getOrDefault(false)
            if (ok) prefs(activity).edit().putBoolean(K_ENROLLED, false).apply()
            onResult(ok)
        }
    }

    /** Obtain a spend-grant token for (kind,to,amount). Enrolled -> per-operation
     *  HARDWARE biometric (StrongBox P-256 sig the Rust guard verifies); returns
     *  null if the user cancels (caller must ABORT). Not enrolled -> the legacy
     *  UI-biometric mint, off-main. `kind`/`amount` MUST be the canonical strings
     *  the guard redeems (e.g. "evm:esc" + wei-hex), matching `legacyMint`. */
    suspend fun spendGrant(
        activity: FragmentActivity?, kind: String, to: String, amount: String,
        legacyMint: () -> String,
    ): String? = spendGrant(activity, kind, to, amount, "", legacyMint)

    /** As [spendGrant] but binds a MAX network fee (wei) into the grant when
     *  `maxFeeWei` is non-empty (max-fee hardening; EVM native send). `legacyMint`
     *  must mint the SAME fee-bound grant in the no-binding path.
     *
     *  Priority 2 — EVERY wallet/BEAM send is hardware-gated when the device CAN do it:
     *   • already enrolled            → per-spend hardware biometric (authorizeSpend),
     *   • not enrolled BUT capable    → LAZY first-send enroll+authorize in ONE scan
     *                                   (enrollAndAuthorize), so the binding turns on
     *                                   the first time the user spends, with no extra
     *                                   prompt at app-open,
     *   • no activity / no secure lock → the legacy UI-biometric + plain mint (the only
     *                                   devices that ever reach the no-hardware path). */
    suspend fun spendGrant(
        activity: FragmentActivity?, kind: String, to: String, amount: String, maxFeeWei: String,
        legacyMint: () -> String,
    ): String? {
        // No activity, or the device can't do hardware-bound spends at all → legacy path.
        if (activity == null || !available(activity)) {
            return withContext(Dispatchers.IO) { legacyMint() }.ifBlank { null }
        }
        return suspendCancellableCoroutine { cont ->
            if (isEnrolled(activity)) {
                authorizeSpend(activity, kind, to, amount, maxFeeWei) { token -> cont.resumeWith(Result.success(token)) }
            } else {
                // First spend on a hardware-capable device: enroll + authorize in one scan.
                enrollAndAuthorize(activity, kind, to, amount, maxFeeWei) { token -> cont.resumeWith(Result.success(token)) }
            }
        }
    }
}
