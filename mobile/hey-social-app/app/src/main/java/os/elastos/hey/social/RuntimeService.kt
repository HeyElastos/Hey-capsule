package os.elastos.hey.social

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import kotlin.concurrent.thread

/**
 * Keeps the Hey process — and therefore the in-process iroh carrier + the DM/feed
 * receivers — ALIVE in the background, and turns carrier-delivered events into
 * LOCAL Android notifications. This is Hey's own push engine: no Firebase, no
 * Google Play Services, so it works on GrapheneOS. Delivery is peer-to-peer over
 * the carrier; this service just surfaces it.
 *
 * STAYING CONNECTED WHEN THE APP IS CLOSED:
 *   1. foregroundServiceType = specialUse on Android 14+ — the only un-capped type
 *      (dataSync/mediaProcessing get killed after ~6h/day, dropping peers). On API<34
 *      the type is optional, so we promote with the plain 2-arg startForeground and
 *      need no dataSync type/permission at all.
 *   2. Battery-optimization exemption (asked in-app). Without it Doze suspends the
 *      network while the screen is off and the carrier can't hold its peers.
 *   3. A network-change callback re-asserts the runtime the instant connectivity
 *      returns (Wi-Fi↔cellular, airplane mode, Doze wake) so peers re-form fast.
 *   4. The in-process receiver loop already re-joins topics + re-dials followed
 *      peers every 2s, so as long as this process lives, peers self-heal — no
 *      silent loss. The ongoing notification surfaces the live peer count.
 *   5. START_STICKY + a ~15-min Doze-capable alarm + a boot receiver resurrect the
 *      service if the OS ever kills it anyway.
 */
class RuntimeService : Service() {
    @Volatile private var polling = false
    private var lastUnread = 0
    private var lastInbound = -1L  // baseline for generic locked-mode notifications
    private var deferredNotified = false // coalesce: at most ONE locked-mode alert per locked session
    private var netCb: ConnectivityManager.NetworkCallback? = null
    // WiFi MulticastLock for mDNS LAN discovery — Android drops inbound multicast by
    // default, so without this the carrier sends mDNS but never hears a same-network
    // peer. Held ONLY while on WiFi/Ethernet (see onCapabilitiesChanged), released on
    // cellular to save battery. Not reference-counted: a single held/released state.
    private var mcLock: android.net.wifi.WifiManager.MulticastLock? = null
    // WifiLock: with the screen off, Android power-saves the WiFi radio and DROPS/defers
    // unsolicited inbound packets — so a peer pushing a DM (and a call offer rides the DM
    // transport) never reaches us while locked, even though our outbound net_report still
    // works. A held WifiLock keeps the radio awake enough to receive. Held on WiFi/Ethernet,
    // released on cellular. This is THE fix for "no DM/call while the phone is locked".
    private var wifiLock: android.net.wifi.WifiManager.WifiLock? = null

    private fun setWifiLock(on: Boolean) {
        runCatching {
            if (on) {
                if (wifiLock?.isHeld == true) return
                val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager ?: return
                @Suppress("DEPRECATION")
                wifiLock = wifi.createWifiLock(android.net.wifi.WifiManager.WIFI_MODE_FULL_HIGH_PERF, "hey-carrier")
                    .apply { setReferenceCounted(false); acquire() }
            } else {
                wifiLock?.let { if (it.isHeld) it.release() }
                wifiLock = null
            }
        }
    }

    private fun setMulticast(on: Boolean) {
        runCatching {
            if (on) {
                if (mcLock?.isHeld == true) return
                val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager ?: return
                mcLock = wifi.createMulticastLock("hey-mdns").apply { setReferenceCounted(false); acquire() }
            } else {
                mcLock?.let { if (it.isHeld) it.release() }
                mcLock = null
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Promote to foreground FIRST, synchronously — basicOngoing() touches only
        // PowerManager (battery state), never the runtime — so we satisfy the
        // startForegroundService ~5s deadline in milliseconds even on a COLD start
        // (boot receiver / heartbeat alarm), where hey_init would otherwise block
        // the main thread booting the runtime + iroh carrier and risk a crash.
        startForegroundTyped(basicOngoing(false))
        wireCallRinger()
        watchNetwork()
        scheduleHeartbeat()
        // The blocking, network-bound runtime boot happens off the main thread.
        // ensureStartedIfProvisioned starts the runtime ONLY if an identity is
        // already provisioned (never auto-creates one before the new-vs-restore
        // choice) and refuses when the vault is locked. ensureStarted is
        // @Synchronized + idempotent, so a concurrent onAvailable call is safe.
        thread(name = "hey-boot") {
            val running = runCatching { HeyApi.ensureStartedIfProvisioned(applicationContext) }.getOrDefault(false)
            android.util.Log.i("heycall", "RuntimeService boot: running=$running")
            if (running) startPolling()
            // Drain incoming-CALL signals in the BACKGROUND too (not just from the app
            // root): without this, a locked/backgrounded phone never sees the "offer", so
            // it never rings or posts the full-screen call notification. Idempotent — the
            // in-app start (MainActivity) is a no-op once this is running.
            if (running) runCatching { CallManager.startPolling() }
                .onFailure { android.util.Log.e("heycall", "CallManager.startPolling threw", it) }
            runCatching { refreshOngoing() } // re-post with the live carrier state
        }
        return START_STICKY
    }

    /** Promote to foreground. On Android 14+ the connection-holding node MUST be the
     *  un-capped `specialUse` type (dataSync/mediaProcessing are time-limited and get
     *  killed). On API<34 the type is optional, so we use the plain 2-arg form — no
     *  dataSync type/permission needed, which also keeps us off the Android-15
     *  dataSync timeout + BOOT_COMPLETED restrictions. */
    private fun startForegroundTyped(notif: Notification) {
        runCatching {
            if (Build.VERSION.SDK_INT >= 34)
                startForeground(ONGOING_ID, notif, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
            else
                startForeground(ONGOING_ID, notif)
        }.onFailure {
            // OEM rejects the type, OR FGS-start-from-boot is blocked (Android 12+
            // background start of a non-exempt specialUse FGS throws
            // ForegroundServiceStartNotAllowedException). Try plain foreground; if
            // even that is blocked, schedule the heartbeat alarm to promote us
            // later (instead of silently failing the boot/update resurrect).
            runCatching { startForeground(ONGOING_ID, notif) }
                .onFailure { runCatching { scheduleHeartbeat() } }
        }
    }

    /** API 34+: the OS contract requires us to STOP the FGS promptly when timed out
     *  (or it crashes us). specialUse shouldn't be timed out, but if policy ever
     *  changes we tear down here and let the ~15-min heartbeat alarm + the network
     *  callback resurrect the service — never re-promote the same instance in-place. */
    override fun onTimeout(startId: Int) {
        runCatching { scheduleHeartbeat() }
        runCatching { stopForeground(STOP_FOREGROUND_REMOVE) }
        runCatching { stopSelf() }
    }

    /** Route carrier-delivered call signals to Hey's own ringer (full-screen CallStyle
     *  notification over the lock screen + looping ringtone). Set on the long-lived
     *  service so an incoming call rings even when the app is closed/backgrounded. The
     *  ring fires regardless of foreground; the notification is skipped when a Hey
     *  screen is up (the in-app overlay is then the UI). */
    private fun wireCallRinger() {
        val app = applicationContext
        CallManager.onIncomingCall = { _, name, _, video ->
            runCatching { CallNotifier.incoming(app, name, video, appForeground) }
        }
        CallManager.onCallEnded = {
            runCatching { CallNotifier.dismiss(app) }
        }
    }

    /** Re-assert the runtime the moment the network returns, so dropped peers
     *  re-form without waiting for the next heartbeat. Event-driven (no polling),
     *  so it costs nothing while connectivity is stable. */
    private fun watchNetwork() {
        if (netCb != null) return
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                // Restarts the runtime if the process was killed; a no-op if it's alive.
                runCatching { HeyApi.ensureStartedIfProvisioned(applicationContext) }
                // Internet returned → re-probe iroh + re-join gossip topics NOW so peers re-form
                // immediately, instead of waiting for the carrier's ~10s self-heal poll.
                runCatching { HeyApi.hey_net_changed() }
            }
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                // ADAPTIVE mDNS: the carrier's LAN-direct path (mDNS) only helps on a
                // shared local segment (WiFi / Ethernet). Hold the WiFi MulticastLock
                // THERE so the phone actually RECEIVES peers' mDNS announces; drop it on
                // cellular (no LAN → no benefit, saves battery). iroh itself already
                // picks LAN-direct vs hole-punch vs relay per-peer — this just makes the
                // LAN option available exactly when the network can use it.
                val lan = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)
                setMulticast(lan)
                setWifiLock(lan) // keep the WiFi radio awake for inbound delivery while locked
            }
            override fun onLost(network: Network) {
                setMulticast(false)
                setWifiLock(false)
            }
        }
        runCatching {
            cm.registerNetworkCallback(
                NetworkRequest.Builder().addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).build(),
                cb,
            )
            netCb = cb
        }
    }

    /** Self-heal #3: a ~15-min alarm re-asserts the service if Android killed it
     *  (Doze/low-memory). Re-armed on every onStartCommand (so it self-repeats). Uses
     *  setAndAllowWhileIdle — unlike setInexactRepeating it IS delivered during Doze
     *  maintenance windows, needs no exact-alarm permission, and stays light on battery.
     *  The foreground service holds the live connection; this just resurrects it. */
    private fun scheduleHeartbeat() {
        runCatching {
            val am = getSystemService(Context.ALARM_SERVICE) as android.app.AlarmManager
            val pi = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
                PendingIntent.getForegroundService(this, 7, Intent(this, RuntimeService::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            else PendingIntent.getService(this, 7, Intent(this, RuntimeService::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            val next = android.os.SystemClock.elapsedRealtime() + 15 * 60 * 1000L
            am.setAndAllowWhileIdle(android.app.AlarmManager.ELAPSED_REALTIME_WAKEUP, next, pi)
        }
    }

    private fun startPolling() {
        if (polling) return
        polling = true
        // Notification drain: surfaces DMs/posts/mentions queued by the receiver.
        // Kept free of the health() probe so a slow loopback call can never delay
        // a message notification.
        thread(name = "hey-notify") {
            while (polling) {
                runCatching {
                    // Social events (new posts/followers/mentions) queued by the receiver.
                    // key = a per-event discriminator so two distinct events of the same
                    // kind from one sender don't collapse onto one notification id.
                    for (n in HeyApi.drainNotifs()) {
                        notifyEvent(
                            n.optString("title").ifBlank { "Hey" },
                            n.optString("body"),
                            (n.optString("kind") + n.optString("did") + n.optString("key")).hashCode(),
                        )
                    }
                    if (HeyApi.processingDeferred()) {
                        // DEFERRED (storage locked OR headless seed-sealed): the receiver
                        // buffers but can't decrypt, so we can't show the message itself.
                        // inboundCount now ticks ONLY for real DMs (the carrier excludes
                        // handshakes/control/fragments), so a rise means a genuine message
                        // is waiting. Coalesce to a SINGLE alert per locked session — a
                        // burst (or a re-pair) must not stack a pile of identical entries.
                        val inb = HeyApi.inboundCount()
                        if (lastInbound >= 0 && inb > lastInbound && !deferredNotified) {
                            notifyEvent("New message", "Unlock Hey to read", DM_ID)
                            deferredNotified = true
                        }
                        lastInbound = inb
                    } else {
                        deferredNotified = false // unlocked → re-arm the locked-session alert
                        // New DMs (incl. tips) via the unread delta when unlocked.
                        val u = HeyApi.hey_total_unread()
                        if (u > lastUnread && u > 0) {
                            notifyEvent("New messages", "$u unread message${if (u == 1) "" else "s"}", DM_ID)
                        }
                        lastUnread = u
                        lastInbound = HeyApi.inboundCount() // keep the baseline fresh
                        // sync-on-tip: an incoming BEAM tip notice (flagged in hey-core on the
                        // carrier receive path) → auto quick-sync BEAM so the payment shows up
                        // without opening the wallet. Event-driven, no polling. Quicksync only +
                        // we're in the unlocked branch, so the seed is available; idempotent/off-main.
                        if (BeamApi.available && HeyApi.hey_beam_tip_pending() &&
                            HeyApi.beamNodeMode(this@RuntimeService) == "quicksync") {
                            runCatching { HeyApi.beamSyncStart(this@RuntimeService) }
                        }
                    }
                }
                Thread.sleep(4000)
            }
        }
        // Ongoing-notification refresh on its OWN cadence: shows the live peer count
        // (no silent loss). Isolated because health() does a loopback HTTP call that
        // can stall during a transport flap — it must never block message delivery.
        thread(name = "hey-status") {
            while (polling) {
                runCatching { refreshOngoing() }
                Thread.sleep(12000)
            }
        }
    }

    override fun onDestroy() {
        polling = false // stops both worker loops; refreshOngoing() re-checks before posting
        runCatching { stopForeground(STOP_FOREGROUND_REMOVE) }
        runCatching { manager().cancel(ONGOING_ID) }
        runCatching {
            netCb?.let { (getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager)?.unregisterNetworkCallback(it) }
        }
        netCb = null
        setMulticast(false)
        setWifiLock(false)
        super.onDestroy()
    }

    // ── notifications ─────────────────────────────────────────────────────────

    private fun manager() = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private fun ensureChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val m = manager()
        m.createNotificationChannel(
            NotificationChannel(RUNNING_CH, "Hey running", NotificationManager.IMPORTANCE_MIN).apply { setShowBadge(false) }
        )
        m.createNotificationChannel(
            NotificationChannel(EVENTS_CH, "Messages & activity", NotificationManager.IMPORTANCE_HIGH)
        )
    }

    private fun openAppIntent(): PendingIntent {
        val i = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(this, 0, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** Tapping the ongoing notification when background isn't allowed jumps straight
     *  to the battery-exemption request — the one switch that keeps Hey connected. */
    private fun batteryIntent(): PendingIntent {
        val i = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return PendingIntent.getActivity(this, 3, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** Health-free text for the main-thread initial foreground (no network call). */
    private fun basicOngoing(running: Boolean): Notification {
        val exempt = BatteryHelper.isExempt(this)
        val text = when {
            !exempt -> "Tap to allow always-on delivery (DMs, mentions, tips)"
            running -> "Connected — peer-to-peer, no servers"
            else -> "Locked — open Hey to receive messages"
        }
        return buildOngoing(text, exempt)
    }

    /** Re-post the ongoing notification with the LIVE carrier state. Called from the
     *  hey-status worker thread only (health() does a loopback HTTP call). Guards on
     *  `polling` so an in-flight call can't re-post a stale notification after the
     *  service is destroyed. */
    private fun refreshOngoing() {
        if (!polling) return
        val exempt = BatteryHelper.isExempt(this)
        val text = if (!exempt) {
            "Tap to allow always-on delivery (DMs, mentions, tips)"
        } else {
            val h = runCatching { HeyApi.health() }.getOrNull()
            val online = h?.optBoolean("online", false) ?: false
            val peers = h?.optInt("peer_count", 0) ?: 0
            when {
                online && peers > 0 -> "Connected · $peers peer${if (peers == 1) "" else "s"}"
                online -> "Connected · finding peers…"
                else -> "Reconnecting…"
            }
        }
        if (!polling) return // re-check after the (possibly slow) health() call
        runCatching { manager().notify(ONGOING_ID, buildOngoing(text, exempt)) }
    }

    private fun buildOngoing(text: String, exempt: Boolean): Notification {
        ensureChannels()
        val b = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) Notification.Builder(this, RUNNING_CH)
        else @Suppress("DEPRECATION") Notification.Builder(this)
        return b.setContentTitle("Hey")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_dialog_email)
            .setOngoing(true)
            .setContentIntent(if (!exempt) batteryIntent() else openAppIntent())
            .build()
    }

    private fun notifyEvent(title: String, body: String, id: Int) {
        // Don't buzz the user about something they're already looking at — when a Hey
        // screen is foreground the in-app UI surfaces it. (The poll loop still updates
        // its baselines, so nothing re-notifies after backgrounding either.)
        if (appForeground) return
        ensureChannels()
        val b = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) Notification.Builder(this, EVENTS_CH)
        else @Suppress("DEPRECATION") Notification.Builder(this)
        val n = b.setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.ic_dialog_email)
            .setAutoCancel(true)
            .setContentIntent(openAppIntent())
            .build()
        runCatching { manager().notify(id, n) }
    }

    companion object {
        /** True while a Hey screen is in the foreground — set by MainActivity's lifecycle
         *  observer. When true, event notifications are SUPPRESSED (the user is already
         *  looking at the app; the in-app UI shows the message/post/call). The ongoing
         *  FGS notification is unaffected. */
        @Volatile var appForeground = false
        private const val RUNNING_CH = "hey_running"
        private const val EVENTS_CH = "hey_events"
        private const val ONGOING_ID = 1
        private const val DM_ID = 2
        fun start(ctx: Context) {
            val i = Intent(ctx, RuntimeService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) ctx.startForegroundService(i) else ctx.startService(i)
        }
    }
}
