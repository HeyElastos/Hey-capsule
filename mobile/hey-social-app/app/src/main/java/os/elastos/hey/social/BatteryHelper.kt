package os.elastos.hey.social

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.provider.Settings

/**
 * Battery-optimization exemption. For reliable background delivery on Android —
 * especially GrapheneOS, whose Doze is aggressive — the user must let Hey run
 * unrestricted, otherwise the OS throttles the carrier connection while the
 * screen is off and DMs/feed stop arriving until the app is reopened.
 */
object BatteryHelper {
    /** True if Hey is already exempt from battery optimization. */
    fun isExempt(ctx: Context): Boolean = runCatching {
        val pm = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
        pm.isIgnoringBatteryOptimizations(ctx.packageName)
    }.getOrDefault(false)

    /** Ask the user to exempt Hey. Falls back to the system battery-optimization
     *  list if the direct request dialog is unavailable. */
    @SuppressLint("BatteryLife")
    fun request(ctx: Context) {
        val direct = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${ctx.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { ctx.startActivity(direct) }.onFailure {
            runCatching {
                ctx.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
        }
    }
}
