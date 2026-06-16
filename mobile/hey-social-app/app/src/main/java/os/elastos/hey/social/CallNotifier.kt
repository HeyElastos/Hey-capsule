package os.elastos.hey.social

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Person
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator

/**
 * Incoming-call alerting that works when the phone is LOCKED or Hey is backgrounded —
 * Hey's own ringer, no FCM (GrapheneOS-safe). On an incoming call signal (carrier →
 * [CallManager.onIncomingCall]) we:
 *   1. Post a high-importance **CallStyle** notification with a **full-screen intent**
 *      so Android shows the ringing UI OVER the lock screen, plus Answer/Decline.
 *   2. Drive a LOOPING ringtone + vibration ourselves (a channel sound plays once, not
 *      looping) so we can stop it the instant the call is answered/declined/ended.
 * Dismissed via [CallManager.onCallEnded]. When Hey is already on-screen the in-app
 * overlay is the UI, so we ring but skip the notification.
 */
object CallNotifier {
    const val EXTRA_INCOMING_CALL = "hey.incoming_call"
    const val EXTRA_ANSWER_CALL = "hey.answer_call"
    const val ACTION_DECLINE = "os.elastos.hey.social.CALL_DECLINE"

    private const val CH = "hey_calls"
    private const val ID = 7

    @Volatile private var ringtone: Ringtone? = null
    @Volatile private var vibrator: Vibrator? = null

    private fun mgr(ctx: Context) =
        ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private fun ensureChannel(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        // IMPORTANCE_HIGH so the full-screen intent + heads-up fire. SILENT — we play a
        // LOOPING ringtone ourselves and can stop it the moment the call ends.
        val ch = NotificationChannel(CH, "Incoming calls", NotificationManager.IMPORTANCE_HIGH).apply {
            setSound(null, null)
            enableVibration(false)
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
            setShowBadge(false)
            runCatching { setBypassDnd(true) }
        }
        mgr(ctx).createNotificationChannel(ch)
    }

    /** Ring (always) and — unless [foreground] (the in-app overlay is showing) — post a
     *  full-screen CallStyle notification that rings over the lock screen. */
    fun incoming(ctx: Context, name: String, video: Boolean, foreground: Boolean) {
        android.util.Log.i("heycall", "CallNotifier.incoming name=$name video=$video foreground=$foreground")
        ensureChannel(ctx)
        startRing(ctx)
        if (foreground) {
            android.util.Log.i("heycall", "foreground: ring only, skipping notification")
            return // overlay shows the incoming UI; just ring
        }
        wakeScreen(ctx) // light the screen up like a real phone ring (FSI may be denied on 14+)

        val title = name.ifBlank { "Hey" }
        val text = if (video) "Incoming video call" else "Incoming voice call"

        // Full-screen + Answer launch the activity directly (a notification-driven
        // activity launch is always allowed, even from the background/lock screen).
        val fullScreen = activityPending(ctx, 20, EXTRA_INCOMING_CALL)
        val answer = activityPending(ctx, 21, EXTRA_ANSWER_CALL)
        val decline = PendingIntent.getBroadcast(
            ctx, 22,
            Intent(ctx, CallActionReceiver::class.java).setAction(ACTION_DECLINE).setPackage(ctx.packageName),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val b = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
            Notification.Builder(ctx, CH) else @Suppress("DEPRECATION") Notification.Builder(ctx)
        b.setSmallIcon(android.R.drawable.sym_action_call)
            .setContentTitle(title)
            .setContentText(text)
            .setCategory(Notification.CATEGORY_CALL)
            .setOngoing(true)
            .setAutoCancel(false)
            .setFullScreenIntent(fullScreen, true)

        if (Build.VERSION.SDK_INT >= 31) {
            val caller = Person.Builder().setName(title).setImportant(true).build()
            b.setStyle(Notification.CallStyle.forIncomingCall(caller, decline, answer))
        } else {
            b.setContentIntent(fullScreen)
            @Suppress("DEPRECATION") b.setPriority(Notification.PRIORITY_MAX)
            @Suppress("DEPRECATION") b.addAction(android.R.drawable.ic_menu_close_clear_cancel, "Decline", decline)
            @Suppress("DEPRECATION") b.addAction(android.R.drawable.sym_action_call, "Answer", answer)
        }
        runCatching { mgr(ctx).notify(ID, b.build()) }
            .onSuccess { android.util.Log.i("heycall", "call notification POSTED (fullScreen)") }
            .onFailure { android.util.Log.e("heycall", "notify FAILED", it) }
    }

    fun dismiss(ctx: Context) {
        runCatching { mgr(ctx).cancel(ID) }
        stopRing()
    }

    private fun activityPending(ctx: Context, rc: Int, extra: String): PendingIntent {
        val i = Intent(ctx, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(extra, true)
        return PendingIntent.getActivity(
            ctx, rc, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    private fun startRing(ctx: Context) {
        if (ringtone?.isPlaying == true) return
        runCatching {
            val uri = RingtoneManager.getActualDefaultRingtoneUri(ctx, RingtoneManager.TYPE_RINGTONE)
                ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            ringtone = RingtoneManager.getRingtone(ctx, uri)?.apply {
                runCatching {
                    audioAttributes = AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build()
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) runCatching { isLooping = true }
                play()
            }
        }
        runCatching {
            val v = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                (ctx.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as android.os.VibratorManager).defaultVibrator
            } else {
                @Suppress("DEPRECATION") ctx.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            }
            vibrator = v
            val pattern = longArrayOf(0, 700, 600, 700, 1400) // ring · pause · ring · gap, repeat
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                v.vibrate(VibrationEffect.createWaveform(pattern, 0))
            } else {
                @Suppress("DEPRECATION") v.vibrate(pattern, 0)
            }
        }
    }

    /** Turn the screen ON for an incoming call (like a real phone). The full-screen
     *  intent only auto-wakes when USE_FULL_SCREEN_INTENT is granted (restricted on
     *  Android 14+); this wakelock wakes the screen regardless. Auto-released. */
    private fun wakeScreen(ctx: Context) {
        runCatching {
            val pm = ctx.getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
            @Suppress("DEPRECATION")
            val wl = pm.newWakeLock(
                android.os.PowerManager.FULL_WAKE_LOCK or
                    android.os.PowerManager.ACQUIRE_CAUSES_WAKEUP or
                    android.os.PowerManager.ON_AFTER_RELEASE,
                "hey:incoming-call",
            )
            wl.acquire(12_000L) // ring window; auto-released
        }
    }

    private fun stopRing() {
        runCatching { ringtone?.stop() }
        ringtone = null
        runCatching { vibrator?.cancel() }
        vibrator = null
    }
}

/** Decline from the call notification → reject + stop the ring. (Answer is a direct
 *  activity launch handled in MainActivity so it works from the lock screen.) */
class CallActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == CallNotifier.ACTION_DECLINE) {
            runCatching { CallManager.decline() }
        }
        CallNotifier.dismiss(context)
    }
}
