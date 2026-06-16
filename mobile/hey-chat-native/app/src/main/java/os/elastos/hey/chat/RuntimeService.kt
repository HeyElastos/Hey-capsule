package os.elastos.hey.chat

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder

/**
 * Foreground service whose only job is to keep the app process (and therefore
 * the in-process carrier's iroh endpoint + gossip subscriptions) alive while
 * Hey Chat is backgrounded, so DMs/feed keep meshing and arrive promptly
 * instead of only when the user reopens the app. Same pattern the remote-window
 * shell uses for its push listener.
 */
class RuntimeService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // The runtime is already running in-process (started by MainActivity);
        // ensureStarted is idempotent, so this is safe if the service is the
        // first to run (e.g. restarted by the system).
        HeyRuntime.ensureStarted(applicationContext)
        startForeground(NOTIFICATION_ID, buildNotification())
        return START_STICKY
    }

    private fun buildNotification(): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Hey Chat",
                NotificationManager.IMPORTANCE_LOW
            ).apply { setShowBadge(false) }
            (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(channel)
        }
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setContentTitle("Hey Chat")
            .setContentText("Connected — messages sync in the background")
            .setSmallIcon(android.R.drawable.ic_dialog_email)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val CHANNEL_ID = "hey_runtime"
        private const val NOTIFICATION_ID = 1
    }
}
