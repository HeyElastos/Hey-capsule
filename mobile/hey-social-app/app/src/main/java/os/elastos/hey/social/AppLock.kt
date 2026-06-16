package os.elastos.hey.social

import android.content.Context
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity

/**
 * OPTIONAL app lock — off by default. When enabled, Hey asks for the device's
 * hardware-backed biometric (fingerprint/face, verified in the TEE/StrongBox on
 * Pixel) or device PIN before opening. This is a layer ON TOP of two things that
 * are always true:
 *   • Android sandboxes Hey's storage so other apps can't read it.
 *   • Android encrypts all app data at rest (File-Based Encryption), keyed to
 *     the device lock.
 * So the lock guards against someone with your unlocked phone in hand; the data
 * itself is already isolated and encrypted by the OS.
 */
object AppLock {
    private const val PREFS = "hey"
    private const val FLAG = "app_lock"
    private const val AUTHENTICATORS =
        BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL

    /** Can this device do a strong biometric or device-credential check? */
    fun available(ctx: Context): Boolean =
        BiometricManager.from(ctx).canAuthenticate(AUTHENTICATORS) == BiometricManager.BIOMETRIC_SUCCESS

    fun enabled(ctx: Context): Boolean =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(FLAG, false)

    fun setEnabled(ctx: Context, on: Boolean) =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(FLAG, on).apply()

    /** Prompt for unlock. [onResult] is called with true on success. */
    fun prompt(activity: FragmentActivity, onResult: (Boolean) -> Unit) {
        val prompt = BiometricPrompt(
            activity, ContextCompat.getMainExecutor(activity),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = onResult(true)
                override fun onAuthenticationError(code: Int, msg: CharSequence) = onResult(false)
            }
        )
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle("Unlock Hey")
            .setSubtitle("Verify it's you to open your data")
            .setAllowedAuthenticators(AUTHENTICATORS)
            .build()
        runCatching { prompt.authenticate(info) }.onFailure { onResult(false) }
    }
}
