package os.elastos.hey.social

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Self-heal layer #2: after a reboot, bring the carrier back online so DMs/feed
 * keep arriving without the user having to open the app. Pairs with the
 * foreground service (which holds the live connection) and the sender-side
 * outbox (which retries until we're reachable). No Google, no FCM.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        // BOOT_COMPLETED (reboot/OS-upgrade) + MY_PACKAGE_REPLACED (app update) +
        // LOCKED_BOOT_COMPLETED (FBE pre-unlock) — in every case bring the carrier
        // back so DMs/feed resume WITHOUT a manual app open.
        when (intent?.action) {
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_LOCKED_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED ->
                runCatching { RuntimeService.start(context.applicationContext) }
        }
    }
}
